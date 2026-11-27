//! Per-week overview rows for browsing: the day overviews of `aj_core` grouped into
//! ISO weeks, with the status of each week's report.

use std::collections::BTreeMap;

use aj_core::overview::DayOverview;
use aj_store::{ReportStatus, Store};
use anyhow::Result;
use chrono::NaiveDate;

use crate::period::Week;
use crate::prompt_library::KIND_WEEKLY;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeekOverview {
    pub week: Week,
    pub active_days: usize,
    pub slices: u64,
    pub active_minutes: i64,
    pub unassigned_slices: u64,
    pub report_status: Option<ReportStatus>,
}

/// Group day overviews into weeks, newest week first. Days whose date does not parse are
/// skipped. Pure logic; the report status is left empty.
pub fn group_days(days: &[DayOverview]) -> Vec<WeekOverview> {
    let mut weeks: BTreeMap<Week, WeekOverview> = BTreeMap::new();
    for d in days {
        let Ok(date) = d.local_date.parse::<NaiveDate>() else {
            continue;
        };
        let week = Week::containing(date);
        let w = weeks.entry(week).or_insert_with(|| WeekOverview {
            week,
            active_days: 0,
            slices: 0,
            active_minutes: 0,
            unassigned_slices: 0,
            report_status: None,
        });
        w.active_days += 1;
        w.slices += d.slices;
        w.active_minutes += d.active_minutes;
        w.unassigned_slices += d.unassigned_slices;
    }
    weeks.into_values().rev().collect()
}

/// Every week with activity, newest first, with its report status.
pub fn week_overviews(store: &Store) -> Result<Vec<WeekOverview>> {
    let mut weeks = group_days(&aj_core::overview::day_overviews(store, false)?);
    for w in &mut weeks {
        w.report_status = store
            .report_meta(KIND_WEEKLY, &w.week.key(), None)?
            .map(|m| m.status);
    }
    Ok(weeks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str, slices: u64, minutes: i64) -> DayOverview {
        DayOverview {
            local_date: date.into(),
            slices,
            ignored_slices: 0,
            active_minutes: minutes,
            unassigned_slices: 1,
            report_status: None,
        }
    }

    #[test]
    fn days_group_into_iso_weeks_newest_first() {
        let weeks = group_days(&[
            day("2026-11-23", 1, 10),
            day("2026-11-22", 2, 20),
            day("2026-11-16", 3, 30),
            day("2026-11-15", 4, 40),
        ]);
        let keys: Vec<String> = weeks.iter().map(|w| w.week.key()).collect();
        assert_eq!(keys, ["2026-W48", "2026-W47", "2026-W46"]);
        assert_eq!(weeks[1].active_days, 2);
        assert_eq!(weeks[1].slices, 5);
        assert_eq!(weeks[1].active_minutes, 50);
        assert_eq!(weeks[1].unassigned_slices, 2);
    }
}
