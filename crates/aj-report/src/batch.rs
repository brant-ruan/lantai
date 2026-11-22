//! On-demand planning for batch daily report operations in the sidebar.
//!
//! Computes for a selected set of dates whether each day has no report, an up-to-date
//! report, or a stale report relative to its current slices, and chooses the smart
//! batch action (`generate`, `update`, or `skip`) without running any model calls.

use std::collections::HashSet;

use aj_store::Store;
use anyhow::Result;
use chrono::{TimeZone, Utc};

use crate::lang::Language;
use crate::material::{self, MaterialContext};
use crate::prompt_library::KIND_DAILY;
use crate::report_io;
use crate::summarize::Budgets;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayReportStatus {
    None,
    Draft,
    Final,
}

impl DayReportStatus {
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
pub struct BatchDayPlan {
    pub date: String,
    pub slices_used: usize,
    pub status: DayReportStatus,
    pub stale: bool,
    pub stale_projects: Vec<String>,
    pub action: BatchAction,
    pub can_regenerate: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchPlan {
    pub days: Vec<BatchDayPlan>,
    pub missing_count: usize,
    pub stale_count: usize,
    pub regenerate_count: usize,
    pub batch_concurrency: usize,
}

/// Decide the smart batch action for one day from its slice count, report status and staleness.
pub fn decide_action(
    slices_used: usize,
    status: DayReportStatus,
    stale: bool,
    include_final: bool,
) -> BatchAction {
    if slices_used == 0 {
        return BatchAction::Skip;
    }
    match status {
        DayReportStatus::None => BatchAction::Generate,
        DayReportStatus::Draft => {
            if stale {
                BatchAction::Update
            } else {
                BatchAction::Skip
            }
        }
        DayReportStatus::Final => {
            if include_final && stale {
                BatchAction::Update
            } else {
                BatchAction::Skip
            }
        }
    }
}

/// Whether `Regenerate all` applies to this day under the current `include_final` policy.
pub fn can_regenerate(slices_used: usize, status: DayReportStatus, include_final: bool) -> bool {
    slices_used > 0 && (status != DayReportStatus::Final || include_final)
}

fn count_active_slices(store: &Store, date: &str) -> Result<usize> {
    let mut count = 0usize;
    for slice in store.slices_for_date(date)? {
        let Some(session) = store.session(slice.session_id)? else {
            continue;
        };
        if !slice.is_ignored(&session) {
            count += 1;
        }
    }
    Ok(count)
}

/// Compute the per-day status, staleness and smart batch action for `dates` (deduplicated
/// in input order). Does not invoke any model.
pub fn plan_batch<Tz: TimeZone>(
    store: &Store,
    tz: &Tz,
    dates: &[String],
    include_final: bool,
    budgets: &Budgets,
    language: &'static Language,
) -> Result<BatchPlan>
where
    Tz::Offset: std::fmt::Display,
{
    let mut seen = HashSet::with_capacity(dates.len());
    let mut days = Vec::with_capacity(dates.len());
    let mut missing_count = 0usize;
    let mut stale_count = 0usize;
    let mut regenerate_count = 0usize;

    let ctx = MaterialContext {
        engine_name: "",
        budgets,
        now: Utc::now(),
        language,
    };

    for date in dates {
        if !seen.insert(date.as_str()) {
            continue;
        }
        let meta = store.report_meta(KIND_DAILY, date, None)?;
        let version = match meta {
            Some(ref m) if m.current_version_id.is_some() => {
                store.current_report_version(KIND_DAILY, date, None)?
            }
            _ => None,
        };
        let status = match (version.as_ref(), meta.as_ref().map(|m| m.status.as_str())) {
            (Some(_), Some("final")) => DayReportStatus::Final,
            (Some(_), _) => DayReportStatus::Draft,
            (None, _) => DayReportStatus::None,
        };

        let has_manifest = version
            .as_ref()
            .and_then(|v| v.input_manifest.as_deref())
            .is_some();

        let (slices_used, stale, stale_projects) =
            if status != DayReportStatus::None && has_manifest {
                let collected = material::collect(store, tz, date, None, &ctx)?;
                let staleness =
                    report_io::staleness(store, KIND_DAILY, date, None, &collected.materials)?;
                let used = collected.materials.len();
                match staleness {
                    Some(s) => (used, true, s.projects),
                    None => (used, false, Vec::new()),
                }
            } else {
                (count_active_slices(store, date)?, false, Vec::new())
            };

        let action = decide_action(slices_used, status, stale, include_final);
        let can_reg = can_regenerate(slices_used, status, include_final);

        match action {
            BatchAction::Generate => missing_count += 1,
            BatchAction::Update => stale_count += 1,
            BatchAction::Skip => {}
        }
        if can_reg {
            regenerate_count += 1;
        }

        days.push(BatchDayPlan {
            date: date.clone(),
            slices_used,
            status,
            stale,
            stale_projects,
            action,
            can_regenerate: can_reg,
        });
    }

    Ok(BatchPlan {
        days,
        missing_count,
        stale_count,
        regenerate_count,
        batch_concurrency: budgets.batch_concurrency.max(1),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testing::MockEngine;
    use crate::prompt_library;
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
    fn decide_action_and_can_regenerate_cover_all_combinations() {
        assert_eq!(
            decide_action(0, DayReportStatus::None, false, false),
            BatchAction::Skip
        );
        assert_eq!(
            decide_action(0, DayReportStatus::Draft, true, true),
            BatchAction::Skip
        );
        assert_eq!(
            decide_action(2, DayReportStatus::None, false, false),
            BatchAction::Generate
        );
        assert_eq!(
            decide_action(2, DayReportStatus::Draft, false, false),
            BatchAction::Skip
        );
        assert_eq!(
            decide_action(2, DayReportStatus::Draft, true, false),
            BatchAction::Update
        );
        assert_eq!(
            decide_action(2, DayReportStatus::Final, false, false),
            BatchAction::Skip
        );
        assert_eq!(
            decide_action(2, DayReportStatus::Final, true, false),
            BatchAction::Skip
        );
        assert_eq!(
            decide_action(2, DayReportStatus::Final, false, true),
            BatchAction::Skip
        );
        assert_eq!(
            decide_action(2, DayReportStatus::Final, true, true),
            BatchAction::Update
        );

        assert!(!can_regenerate(0, DayReportStatus::None, true));
        assert!(can_regenerate(1, DayReportStatus::None, false));
        assert!(can_regenerate(1, DayReportStatus::Draft, false));
        assert!(!can_regenerate(1, DayReportStatus::Final, false));
        assert!(can_regenerate(1, DayReportStatus::Final, true));
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

        let p_default = plan_batch(&store, &tz, &dates, false, &budgets, &crate::lang::EN).unwrap();
        assert_eq!(p_default.days.len(), 5);
        assert_eq!(p_default.batch_concurrency, 3);
        assert_eq!(p_default.missing_count, 1);
        assert_eq!(p_default.stale_count, 1);
        assert_eq!(p_default.regenerate_count, 3); // 16, 17, 18 (15 has 0 slices, 19 is final)
        assert_eq!(p_default.days[0].action, BatchAction::Skip);
        assert_eq!(p_default.days[1].action, BatchAction::Generate);
        assert_eq!(p_default.days[2].action, BatchAction::Skip);
        assert!(!p_default.days[2].stale);
        assert_eq!(p_default.days[3].action, BatchAction::Update);
        assert_eq!(p_default.days[3].stale_projects, vec!["app".to_string()]);
        assert_eq!(p_default.days[4].status, DayReportStatus::Final);
        assert!(p_default.days[4].stale);
        assert_eq!(p_default.days[4].action, BatchAction::Skip);
        assert!(!p_default.days[4].can_regenerate);

        let p_final = plan_batch(&store, &tz, &dates, true, &budgets, &crate::lang::EN).unwrap();
        assert_eq!(p_final.missing_count, 1);
        assert_eq!(p_final.stale_count, 2);
        assert_eq!(p_final.regenerate_count, 4);
        assert_eq!(p_final.days[4].action, BatchAction::Update);
        assert!(p_final.days[4].can_regenerate);
    }
}
