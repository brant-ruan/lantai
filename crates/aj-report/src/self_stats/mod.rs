//! Lantai's own state for the dashboard: which days, weeks and months have reports and
//! whether they are out of date, and how much of the slice digest cache can be reused.
//!
//! Collection reuses the daily material pipeline, so "reusable" and "stale" mean exactly what
//! they mean when a report is generated. No model is called.

mod tally;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use aj_ir::AgentId;
use aj_store::{ReportSummaryRow, Store};
use anyhow::Result;
use chrono::{NaiveDate, TimeZone};

use crate::material::{self, MaterialContext};
use crate::monthly;
use crate::period::{Month, Week};
use crate::prompt_library::{KIND_DAILY, KIND_MONTHLY, KIND_WEEKLY};
use crate::report_io;

pub use tally::{
    agent_rows, cell_status, digest_state, kind_totals, AgentDigest, CellStatus, DayDigest,
    DigestState, DigestTally, KindTotals, PeriodCell, SelfStats,
};

/// Compute Lantai's own state for the days from `since` (or the first active day) to `until`.
///
/// Weeks and months are those containing a day of the window; a month owns the weeks whose
/// Thursday falls in it, as for monthly reports. Each day is collected once, and only while
/// its week is being looked at, so memory stays bounded by one week of material.
pub fn load_self_stats<Tz: TimeZone>(
    store: &Store,
    tz: &Tz,
    since: Option<NaiveDate>,
    until: NaiveDate,
    ctx: &MaterialContext,
) -> Result<SelfStats>
where
    Tz::Offset: std::fmt::Display,
{
    // Slices taking part in reports, for every day with any slice (ignored ones included).
    let active_by_day: BTreeMap<NaiveDate, u64> = store
        .day_counts(true)?
        .into_iter()
        .filter_map(|d| Some((d.local_date.parse().ok()?, d.slices)))
        .collect();
    let in_window = |d: &NaiveDate| since.is_none_or(|s| *d >= s) && *d <= until;
    let weeks: BTreeSet<Week> = active_by_day
        .keys()
        .filter(|d| in_window(d))
        .map(|d| Week::containing(*d))
        .collect();
    let months: BTreeSet<Month> = weeks.iter().map(Week::month).collect();
    let active_in = |dates: &mut dyn Iterator<Item = NaiveDate>| -> u64 {
        dates.filter_map(|d| active_by_day.get(&d)).sum()
    };

    let since_key = since.map(|d| d.to_string());
    let until_key = until.to_string();
    let daily_rows =
        by_key(store.report_summaries(KIND_DAILY, since_key.as_deref(), Some(&until_key))?);
    let weekly_rows = by_key(store.report_summaries(KIND_WEEKLY, None, None)?);
    let monthly_rows = by_key(store.report_summaries(KIND_MONTHLY, None, None)?);

    let mut days = Vec::new();
    let mut digest_days = Vec::new();
    let mut digest = DigestTally::default();
    let mut agents: HashMap<AgentId, DigestTally> = HashMap::new();
    let mut split_slices = 0;
    let mut incremental_updates = 0;
    let mut week_cells = Vec::with_capacity(weeks.len());

    for &week in &weeks {
        let key = week.key();
        let row = weekly_rows.get(&key);
        let check_week = row.is_some_and(|r| r.has_manifest);
        let mut week_materials = Vec::new();

        for date in week.dates() {
            let in_range = in_window(&date);
            if !active_by_day.contains_key(&date) || !(in_range || check_week) {
                continue;
            }
            let date_key = date.to_string();
            let collected = material::collect(store, tz, &date_key, None, ctx)?;
            if in_range {
                let mut tally = DigestTally {
                    ignored: collected.ignored as u64,
                    ..Default::default()
                };
                for m in &collected.materials {
                    let state = digest_state(
                        m.digest.is_some(),
                        m.prior.is_some() || m.slice.digest_json.is_some(),
                    );
                    tally.add(state);
                    agents
                        .entry(m.session.agent.clone())
                        .or_default()
                        .add(state);
                    split_slices += u64::from(!m.cached_parts.is_empty());
                    incremental_updates += u64::from(m.slice.digest_update_count);
                }
                // Ignored slices are attributed to no agent: the collection does not return them.
                digest.merge(&tally);

                let day_row = daily_rows.get(&date_key);
                let staleness = match day_row {
                    Some(r) if r.has_manifest => report_io::staleness(
                        store,
                        KIND_DAILY,
                        &date_key,
                        None,
                        &collected.materials,
                    )?,
                    _ => None,
                };
                days.push(cell(
                    date_key.clone(),
                    (date, date),
                    day_row,
                    tally.active(),
                    staleness,
                ));
                digest_days.push(DayDigest {
                    date: date_key,
                    tally,
                });
            }
            if check_week {
                week_materials.extend(collected.materials);
            }
        }

        let staleness = if check_week {
            report_io::staleness(store, KIND_WEEKLY, &key, None, &week_materials)?
        } else {
            None
        };
        let active = active_in(&mut week.dates().into_iter());
        week_cells.push(cell(
            key,
            (week.monday(), week.sunday()),
            row,
            active,
            staleness,
        ));
    }

    let mut month_cells = Vec::with_capacity(months.len());
    for &month in &months {
        let key = month.key();
        let row = monthly_rows.get(&key);
        let staleness = match row {
            Some(r) if r.has_manifest => monthly::staleness(store, month, None)?,
            _ => None,
        };
        let active = active_in(&mut month.weeks().into_iter().flat_map(|w| w.dates()));
        let weeks = month.weeks();
        let span = match (weeks.first(), weeks.last()) {
            (Some(a), Some(b)) => (a.monday(), b.sunday()),
            _ => (month.first_day(), month.last_day()),
        };
        month_cells.push(cell(key, span, row, active, staleness));
    }

    let from = since
        .or_else(|| active_by_day.keys().next().copied())
        .unwrap_or(until)
        .min(until);
    Ok(SelfStats {
        from,
        to: until,
        daily: kind_totals(&days),
        weekly: kind_totals(&week_cells),
        monthly: kind_totals(&month_cells),
        days,
        weeks: week_cells,
        months: month_cells,
        digest_days,
        digest,
        agents: agent_rows(agents.into_iter().collect()),
        split_slices,
        incremental_updates,
        project_briefs: store.count_project_briefs(since_key.as_deref(), Some(&until_key))?,
    })
}

fn by_key(rows: Vec<ReportSummaryRow>) -> HashMap<String, ReportSummaryRow> {
    rows.into_iter()
        .map(|r| (r.period_key.clone(), r))
        .collect()
}

fn cell(
    key: String,
    (first_day, last_day): (NaiveDate, NaiveDate),
    row: Option<&ReportSummaryRow>,
    active_slices: u64,
    staleness: Option<report_io::Staleness>,
) -> PeriodCell {
    PeriodCell {
        key,
        first_day,
        last_day,
        status: cell_status(row.map(|r| r.status.as_str()), active_slices),
        stale: staleness.is_some(),
        stale_projects: staleness.map(|s| s.projects).unwrap_or_default(),
        versions: row.map_or(0, |r| r.versions),
        edited: row.is_some_and(|r| r.edited),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testing::MockEngine;
    use crate::engine::Engine;
    use crate::{generate_daily, prompt_library, Budgets, DailyOptions, ReportMode};
    use aj_core::{slice::reslice_session, Config};
    use aj_ir::IrEvent;
    use chrono::{DateTime, FixedOffset, Utc};

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

    fn generate(store: &mut Store, engine: &MockEngine, tz: &FixedOffset, date: &str) {
        let opts = DailyOptions {
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
        };
        generate_daily(store, engine, tz, &opts, &mut |_| {}).unwrap();
    }

    #[test]
    fn reports_coverage_and_digest_cache_follow_the_generator() {
        let tz = FixedOffset::east_opt(8 * 3600).unwrap();
        let mut store = Store::open_in_memory().unwrap();
        prompt_library::sync_builtins(&mut store).unwrap();
        let app = store.create_project("app", None).unwrap();

        // 11-16: every slice ignored. 11-17: no report. 11-18: final, then new work arrives.
        let ignored = add_session(&mut store, &tz, "s-ign", app, "2026-11-16T02:00:00Z");
        store.set_session_ignored(ignored, true).unwrap();
        add_session(&mut store, &tz, "s-none", app, "2026-11-17T02:00:00Z");
        add_session(&mut store, &tz, "s-done", app, "2026-11-18T02:00:00Z");

        let engine = MockEngine::new(|_| Ok("## app\n- did work [#S1]".into()));
        generate(&mut store, &engine, &tz, "2026-11-18");
        store
            .set_report_status(KIND_DAILY, "2026-11-18", None, "final")
            .unwrap();
        add_session(&mut store, &tz, "s-late", app, "2026-11-18T04:00:00Z");

        let name = engine.name();
        let budgets = Budgets::default();
        let ctx = MaterialContext {
            engine_name: &name,
            budgets: &budgets,
            now: t("2026-11-20T00:00:00Z"),
            language: &crate::lang::ZH_HANS,
        };
        let since = Some("2026-11-16".parse().unwrap());
        let stats =
            load_self_stats(&store, &tz, since, "2026-11-19".parse().unwrap(), &ctx).unwrap();

        let days: Vec<_> = stats
            .days
            .iter()
            .map(|c| (c.key.as_str(), c.status, c.stale))
            .collect();
        assert_eq!(
            days,
            [
                ("2026-11-16", CellStatus::Ignored, false),
                ("2026-11-17", CellStatus::None, false),
                ("2026-11-18", CellStatus::Final, true),
            ]
        );
        assert_eq!(
            stats.daily,
            KindTotals {
                periods: 2,
                reports: 1,
                finals: 1,
                stale: 1,
                edited: 0
            }
        );
        // The generated day's first slice is cached; the late one and 11-17 never were.
        assert_eq!(
            stats.digest,
            DigestTally {
                fresh: 1,
                behind: 0,
                missing: 2,
                ignored: 1
            }
        );
        assert_eq!(stats.agents.len(), 1);
        assert_eq!(stats.agents[0].tally.active(), 3);
        // All three days are in ISO week 47 and in November.
        assert_eq!(stats.weeks.len(), 1);
        assert_eq!(stats.weeks[0].status, CellStatus::None);
        assert_eq!(stats.months[0].key, "2026-11");
    }
}
