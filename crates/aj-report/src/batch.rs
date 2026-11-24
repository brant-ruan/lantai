//! On-demand planning for daily, weekly and monthly report batches, without model calls.

use std::collections::HashSet;

use aj_store::Store;
use anyhow::Result;
use chrono::{TimeZone, Utc};

use crate::lang::Language;
use crate::material::{self, MaterialContext};
use crate::monthly::estimate_monthly;
use crate::period::{Month, Week};
use crate::report_io;
use crate::rollup::Granularity;
use crate::summarize::Budgets;
use crate::weekly::estimate_weekly;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportStatus {
    None,
    Draft,
    Final,
}

impl ReportStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Draft => "draft",
            Self::Final => "final",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchAction {
    Generate,
    Update,
    Skip,
}

impl BatchAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Generate => "generate",
            Self::Update => "update",
            Self::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchItem {
    pub period_key: String,
    pub has_inputs: bool,
    pub missing_weeks: Vec<String>,
    pub status: ReportStatus,
    pub stale: bool,
    pub action: BatchAction,
    pub can_regenerate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchPlan {
    pub items: Vec<BatchItem>,
    pub missing_count: usize,
    pub stale_count: usize,
    pub regenerate_count: usize,
    pub batch_concurrency: usize,
}

/// Decide the smart action from the period’s available input, report status and staleness.
fn decide_action(
    has_inputs: bool,
    status: ReportStatus,
    stale: bool,
    include_final: bool,
) -> BatchAction {
    if !has_inputs {
        return BatchAction::Skip;
    }
    match status {
        ReportStatus::None => BatchAction::Generate,
        ReportStatus::Draft => {
            if stale {
                BatchAction::Update
            } else {
                BatchAction::Skip
            }
        }
        ReportStatus::Final => {
            if include_final && stale {
                BatchAction::Update
            } else {
                BatchAction::Skip
            }
        }
    }
}

fn has_active_slices(store: &Store, date: &str) -> Result<bool> {
    for slice in store.slices_for_date(date)? {
        let Some(session) = store.session(slice.session_id)? else {
            continue;
        };
        if !slice.is_ignored(&session) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Plan distinct periods in input order. Monthly availability is determined by usable
/// weekly sections, and its staleness is measured against those sections.
pub fn plan_batch<Tz: TimeZone>(
    store: &Store,
    tz: &Tz,
    granularity: Granularity,
    period_keys: &[String],
    include_final: bool,
    budgets: &Budgets,
    language: &'static Language,
) -> Result<BatchPlan>
where
    Tz::Offset: std::fmt::Display,
{
    let kind = granularity.report_kind();
    let mut seen = HashSet::with_capacity(period_keys.len());
    let mut plan = BatchPlan {
        items: Vec::with_capacity(period_keys.len()),
        missing_count: 0,
        stale_count: 0,
        regenerate_count: 0,
        batch_concurrency: budgets.batch_concurrency.max(1),
    };
    let ctx = MaterialContext {
        engine_name: "",
        budgets,
        now: Utc::now(),
        language,
    };

    for key in period_keys {
        let period = match granularity {
            Granularity::Daily => key.clone(),
            Granularity::Weekly => Week::parse(key)?.key(),
            Granularity::Monthly => Month::parse(key)?.key(),
        };
        if !seen.insert(period.clone()) {
            continue;
        }
        let meta = store.report_meta(kind, &period, None)?;
        let status = match meta.filter(|m| m.current_version_id.is_some()) {
            Some(m) if m.status == "final" => ReportStatus::Final,
            Some(_) => ReportStatus::Draft,
            None => ReportStatus::None,
        };

        let (has_inputs, stale, missing_weeks) = match granularity {
            Granularity::Daily => {
                let version = store.current_report_version(kind, &period, None)?;
                if version.is_some_and(|v| v.input_manifest.is_some()) {
                    let collected = material::collect(store, tz, &period, None, &ctx)?;
                    let stale =
                        report_io::staleness(store, kind, &period, None, &collected.materials)?;
                    (!collected.materials.is_empty(), stale.is_some(), Vec::new())
                } else {
                    (has_active_slices(store, &period)?, false, Vec::new())
                }
            }
            Granularity::Weekly => {
                let estimate = estimate_weekly(store, tz, Week::parse(&period)?, None, &ctx)?;
                (
                    estimate.slices_used > 0,
                    estimate.staleness.is_some(),
                    Vec::new(),
                )
            }
            Granularity::Monthly => {
                let estimate = estimate_monthly(store, tz, Month::parse(&period)?, None, &ctx)?;
                (
                    estimate.weeks_reported > 0,
                    estimate.staleness.is_some(),
                    estimate.unreported_weeks,
                )
            }
        };
        let action = decide_action(has_inputs, status, stale, include_final);
        let can_regenerate = has_inputs && (status != ReportStatus::Final || include_final);
        plan.missing_count += usize::from(action == BatchAction::Generate);
        plan.stale_count += usize::from(action == BatchAction::Update);
        plan.regenerate_count += usize::from(can_regenerate);
        plan.items.push(BatchItem {
            period_key: period,
            has_inputs,
            missing_weeks,
            status,
            stale,
            action,
            can_regenerate,
        });
    }

    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testing::MockEngine;
    use crate::monthly::{generate_monthly, MonthlyOptions};
    use crate::prompt_library::{self, KIND_DAILY};
    use crate::weekly::{generate_weekly, WeeklyOptions};
    use crate::{generate_daily, DailyOptions, ReportMode};
    use aj_core::{slice::reslice_session, Config};
    use aj_ir::{AgentId, IrEvent};
    use chrono::{DateTime, FixedOffset};

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn add_session(store: &mut Store, tz: &FixedOffset, sid: &str, project: i64, at: &str) -> i64 {
        let id = store
            .ingest(
                &AgentId::CLAUDE,
                sid,
                &[IrEvent::TurnStart {
                    source_event_id: "u1".into(),
                    timestamp: t(at),
                    user_prompt: "do work".into(),
                }],
            )
            .unwrap()
            .session_id;
        store.assign_manual(id, Some(project)).unwrap();
        reslice_session(store, id, &Config::default(), tz).unwrap();
        id
    }

    fn daily_opts(date: &str) -> DailyOptions {
        DailyOptions {
            date: date.into(),
            project: None,
            prompt: None,
            concurrency: 1,
            dry_run: false,
            budgets: Budgets::default(),
            cancel: None,
            mode: ReportMode::Regenerate,
            now: t("2026-11-20T00:00:00Z"),
            language: &crate::lang::ZH_HANS,
        }
    }

    #[test]
    fn decide_action_respects_availability_staleness_and_final_status() {
        assert_eq!(
            decide_action(false, ReportStatus::None, false, false),
            BatchAction::Skip
        );
        assert_eq!(
            decide_action(false, ReportStatus::Draft, true, true),
            BatchAction::Skip
        );
        assert_eq!(
            decide_action(true, ReportStatus::None, false, false),
            BatchAction::Generate
        );
        assert_eq!(
            decide_action(true, ReportStatus::Draft, false, false),
            BatchAction::Skip
        );
        assert_eq!(
            decide_action(true, ReportStatus::Draft, true, false),
            BatchAction::Update
        );
        assert_eq!(
            decide_action(true, ReportStatus::Final, false, false),
            BatchAction::Skip
        );
        assert_eq!(
            decide_action(true, ReportStatus::Final, true, false),
            BatchAction::Skip
        );
        assert_eq!(
            decide_action(true, ReportStatus::Final, false, true),
            BatchAction::Skip
        );
        assert_eq!(
            decide_action(true, ReportStatus::Final, true, true),
            BatchAction::Update
        );
    }

    #[test]
    fn plan_batch_detects_missing_fresh_stale_and_final_days() {
        let tz = FixedOffset::east_opt(8 * 3600).unwrap();
        let mut store = Store::open_in_memory().unwrap();
        prompt_library::sync_builtins(&mut store).unwrap();
        let app = store.create_project("app", None).unwrap();

        // 2026-11-15: ignored-only -> skip
        let s_ignored = add_session(&mut store, &tz, "s-ign", app, "2026-11-15T02:00:00Z");
        store.set_session_ignored(s_ignored, true).unwrap();

        // 2026-11-16: missing report -> generate
        add_session(&mut store, &tz, "s-miss", app, "2026-11-16T02:00:00Z");

        // 2026-11-17: fresh draft -> skip
        add_session(&mut store, &tz, "s-fresh", app, "2026-11-17T02:00:00Z");

        // 2026-11-18: stale draft -> update
        add_session(&mut store, &tz, "s-stale1", app, "2026-11-18T02:00:00Z");

        // 2026-11-19: stale final -> skip unless include_final
        add_session(&mut store, &tz, "s-fin1", app, "2026-11-19T02:00:00Z");

        let engine = MockEngine::new(|_| Ok("## app\n- did work [#S1]".into()));
        generate_daily(
            &mut store,
            &engine,
            &tz,
            &daily_opts("2026-11-17"),
            &mut |_| {},
        )
        .unwrap();
        generate_daily(
            &mut store,
            &engine,
            &tz,
            &daily_opts("2026-11-18"),
            &mut |_| {},
        )
        .unwrap();
        generate_daily(
            &mut store,
            &engine,
            &tz,
            &daily_opts("2026-11-19"),
            &mut |_| {},
        )
        .unwrap();
        store
            .set_report_status(KIND_DAILY, "2026-11-19", None, "final")
            .unwrap();

        // Add new activity on 18 and 19 to make both stale.
        add_session(&mut store, &tz, "s-stale2", app, "2026-11-18T04:00:00Z");
        add_session(&mut store, &tz, "s-fin2", app, "2026-11-19T04:00:00Z");

        let dates = vec![
            "2026-11-15".into(),
            "2026-11-16".into(),
            "2026-11-17".into(),
            "2026-11-18".into(),
            "2026-11-19".into(),
            "2026-11-16".into(), // duplicate ignored
        ];
        let budgets = Budgets {
            batch_concurrency: 3,
            ..Budgets::default()
        };

        let p_default = plan_batch(
            &store,
            &tz,
            Granularity::Daily,
            &dates,
            false,
            &budgets,
            &crate::lang::EN,
        )
        .unwrap();
        assert_eq!(p_default.items.len(), 5);
        assert_eq!(p_default.batch_concurrency, 3);
        assert_eq!(p_default.missing_count, 1);
        assert_eq!(p_default.stale_count, 1);
        assert_eq!(p_default.regenerate_count, 3); // 16, 17, 18 (15 has 0 slices, 19 is final)
        assert_eq!(p_default.items[0].action, BatchAction::Skip);
        assert_eq!(p_default.items[1].action, BatchAction::Generate);
        assert_eq!(p_default.items[2].action, BatchAction::Skip);
        assert!(!p_default.items[2].stale);
        assert_eq!(p_default.items[3].action, BatchAction::Update);
        assert!(p_default.items[3].stale);
        assert_eq!(p_default.items[4].status, ReportStatus::Final);
        assert!(p_default.items[4].stale);
        assert_eq!(p_default.items[4].action, BatchAction::Skip);
        assert!(!p_default.items[4].can_regenerate);

        let p_final = plan_batch(
            &store,
            &tz,
            Granularity::Daily,
            &dates,
            true,
            &budgets,
            &crate::lang::EN,
        )
        .unwrap();
        assert_eq!(p_final.missing_count, 1);
        assert_eq!(p_final.stale_count, 2);
        assert_eq!(p_final.regenerate_count, 4);
        assert_eq!(p_final.items[4].action, BatchAction::Update);
        assert!(p_final.items[4].can_regenerate);
    }

    #[test]
    fn weekly_batch_plans_from_sessions_without_daily_reports() {
        let tz = FixedOffset::east_opt(0).unwrap();
        let mut store = Store::open_in_memory().unwrap();
        prompt_library::sync_builtins(&mut store).unwrap();
        let project = store.create_project("app", None).unwrap();
        let ignored = add_session(&mut store, &tz, "ignored", project, "2026-11-10T10:00:00Z");
        store.set_session_ignored(ignored, true).unwrap();
        add_session(&mut store, &tz, "active", project, "2026-11-18T10:00:00Z");
        let keys = ["2026-W46".into(), "2026-W47".into(), "2026w47".into()];
        let budgets = Budgets::default();
        let plan = |store: &Store| {
            plan_batch(
                store,
                &tz,
                Granularity::Weekly,
                &keys,
                false,
                &budgets,
                &crate::lang::EN,
            )
            .unwrap()
        };
        let initial = plan(&store);
        assert_eq!(initial.items.len(), 2);
        assert_eq!(initial.items[0].action, BatchAction::Skip);
        assert_eq!(initial.items[1].action, BatchAction::Generate);

        let engine = MockEngine::new(|_| Ok("## app\n- did work [#S2]".into()));
        let opts = WeeklyOptions {
            week: Week::parse("2026-W47").unwrap(),
            project: None,
            prompt: None,
            concurrency: 1,
            dry_run: false,
            budgets: Budgets::default(),
            cancel: None,
            mode: ReportMode::Regenerate,
            now: t("2026-11-22T00:00:00Z"),
            language: &crate::lang::EN,
        };
        generate_weekly(&mut store, &engine, &tz, &opts, &mut |_| {}).unwrap();
        assert_eq!(plan(&store).items[1].action, BatchAction::Skip);
        add_session(&mut store, &tz, "later", project, "2026-11-19T10:00:00Z");
        assert_eq!(plan(&store).items[1].action, BatchAction::Update);
    }

    #[test]
    fn monthly_batch_requires_usable_weekly_sections_and_updates_when_coverage_grows() {
        let tz = FixedOffset::east_opt(0).unwrap();
        let mut store = Store::open_in_memory().unwrap();
        prompt_library::sync_builtins(&mut store).unwrap();
        let project = store.create_project("app", None).unwrap();
        add_session(&mut store, &tz, "early", project, "2026-11-10T10:00:00Z");
        add_session(&mut store, &tz, "late", project, "2026-11-18T10:00:00Z");
        add_session(&mut store, &tz, "dec", project, "2026-12-02T10:00:00Z");
        let keys = ["2026-11".into(), "2026-12".into()];
        let budgets = Budgets::default();
        let plan = |store: &Store| {
            plan_batch(
                store,
                &tz,
                Granularity::Monthly,
                &keys,
                false,
                &budgets,
                &crate::lang::EN,
            )
            .unwrap()
        };
        let save_week = |store: &mut Store, period_key: &str, markdown: &str| {
            store
                .save_report_version(&aj_store::NewReportVersion {
                    report_type: Granularity::Weekly.report_kind(),
                    period_key,
                    project_id: None,
                    origin: "generated",
                    markdown,
                    engine: None,
                    input_hash: None,
                    prompt_id: None,
                    prompt_name: None,
                    prompt_snapshot: None,
                    input_manifest: None,
                    language: "en",
                })
                .unwrap();
        };
        save_week(
            &mut store,
            "2026-W49",
            "# Weekly report\n\n## app\n> Stats only",
        );
        let blocked = plan(&store);
        assert_eq!(blocked.regenerate_count, 0);
        assert_eq!(blocked.items[0].missing_weeks, ["2026-W46", "2026-W47"]);
        assert_eq!(blocked.items[1].missing_weeks, ["2026-W49"]);
        assert_eq!(blocked.items[1].action, BatchAction::Skip);

        save_week(&mut store, "2026-W46", "## app\n- early work [#S1]");
        let partial = plan(&store);
        assert_eq!(partial.items[0].action, BatchAction::Generate);
        assert_eq!(partial.items[0].missing_weeks, ["2026-W47"]);
        assert_eq!(partial.missing_count, 1);

        let engine = MockEngine::new(|_| Ok("## app\n- early work [#S1]".into()));
        let opts = MonthlyOptions {
            month: Month::parse("2026-11").unwrap(),
            project: None,
            prompt: None,
            concurrency: 1,
            dry_run: false,
            budgets: Budgets::default(),
            cancel: None,
            mode: ReportMode::Regenerate,
            now: t("2026-11-30T00:00:00Z"),
            language: &crate::lang::EN,
        };
        generate_monthly(&mut store, &engine, &tz, &opts, &mut |_| {}).unwrap();
        assert_eq!(plan(&store).items[0].action, BatchAction::Skip);
        save_week(&mut store, "2026-W47", "## app\n- late work [#S2]");
        let stale = plan(&store);
        assert!(stale.items[0].missing_weeks.is_empty());
        assert_eq!(stale.items[0].action, BatchAction::Update);

        let marked = store
            .set_reports_status("monthly", &keys, None, "final")
            .unwrap();
        assert_eq!(marked.updated, ["2026-11"]);
        assert_eq!(marked.skipped, ["2026-12"]);
        assert_eq!(plan(&store).items[0].action, BatchAction::Skip);
    }
}
