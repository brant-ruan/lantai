//! Per-month overview rows for browsing: the week overviews of the store grouped into
//! calendar months, with the status of each month's report.

use std::collections::BTreeMap;

use aj_store::Store;
use anyhow::Result;

use crate::period::Month;
use crate::prompt_library::KIND_MONTHLY;
use crate::weekly::{week_overviews, WeekOverview};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonthOverview {
    pub month: Month,
    pub active_weeks: usize,
    pub reported_weeks: usize,
    pub active_days: usize,
    pub slices: u64,
    pub active_minutes: i64,
    pub unassigned_slices: u64,
    pub report_status: Option<String>,
}

/// Group week overviews into calendar months, newest month first. Pure logic; the
/// report status is left empty.
pub fn group_weeks(weeks: &[WeekOverview]) -> Vec<MonthOverview> {
    let mut months: BTreeMap<Month, MonthOverview> = BTreeMap::new();
    for w in weeks {
        let month = w.week.month();
        let m = months.entry(month).or_insert_with(|| MonthOverview {
            month,
            active_weeks: 0,
            reported_weeks: 0,
            active_days: 0,
            slices: 0,
            active_minutes: 0,
            unassigned_slices: 0,
            report_status: None,
        });
        m.active_weeks += 1;
        if w.report_status.is_some() {
            m.reported_weeks += 1;
        }
        m.active_days += w.active_days;
        m.slices += w.slices;
        m.active_minutes += w.active_minutes;
        m.unassigned_slices += w.unassigned_slices;
    }
    months.into_values().rev().collect()
}

/// Every month with activity, newest first, with its report status.
pub fn month_overviews(store: &Store) -> Result<Vec<MonthOverview>> {
    let mut months = group_weeks(&week_overviews(store)?);
    for m in &mut months {
        m.report_status = store
            .report_meta(KIND_MONTHLY, &m.month.key(), None)?
            .map(|meta| meta.status);
    }
    Ok(months)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::period::Week;

    fn week(key: &str, days: usize, slices: u64, minutes: i64, reported: bool) -> WeekOverview {
        WeekOverview {
            week: Week::parse(key).unwrap(),
            active_days: days,
            slices,
            active_minutes: minutes,
            unassigned_slices: 1,
            report_status: reported.then(|| "draft".into()),
        }
    }

    #[test]
    fn weeks_group_into_months_newest_first_using_thursday_attribution() {
        // 2026-W44 belongs to October, 2026-W45/W47 belong to November, 2026-W49 to December.
        let months = group_weeks(&[
            week("2026-W49", 2, 4, 40, false),
            week("2026-W47", 3, 6, 60, true),
            week("2026-W45", 1, 2, 20, false),
            week("2026-W44", 4, 8, 80, true),
        ]);
        let keys: Vec<String> = months.iter().map(|m| m.month.key()).collect();
        assert_eq!(keys, ["2026-12", "2026-11", "2026-10"]);
        assert_eq!(months[1].active_weeks, 2);
        assert_eq!(months[1].reported_weeks, 1);
        assert_eq!(months[1].active_days, 4);
        assert_eq!(months[1].slices, 8);
        assert_eq!(months[1].active_minutes, 80);
        assert_eq!(months[1].unassigned_slices, 2);
    }
}
