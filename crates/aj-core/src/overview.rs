//! Per-day overview rows for browsing, with active time deduplicated across parallel sessions
//! and the number of notes people wrote that day.
//!
//! The store sums each slice's own active minutes; this module replaces that sum with the
//! union of the day's active intervals, matching the statistics dashboard and report stat lines.

use std::collections::HashMap;

use aj_store::{DayOverview, HumanCountRow, Store};
use anyhow::Result;

use crate::slice::union_minutes;

/// One day of the browsing list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayRow {
    pub day: DayOverview,
    /// Top-level notes and annotations written for the day.
    pub notes: u64,
}

/// Every day with agent activity or notes, newest first. `active_minutes` counts overlapping
/// time once.
pub fn day_rows(store: &Store, include_ignored: bool) -> Result<Vec<DayRow>> {
    Ok(with_notes(
        day_overviews(store, include_ignored)?,
        store.human_counts("day")?,
    ))
}

/// Add note counts to the days, and days that only have notes, keeping newest first.
fn with_notes(days: Vec<DayOverview>, counts: Vec<HumanCountRow>) -> Vec<DayRow> {
    let mut notes: HashMap<String, u64> = counts
        .into_iter()
        .map(|c| (c.period_key, c.notes))
        .collect();
    let mut rows: Vec<DayRow> = days
        .into_iter()
        .map(|day| DayRow {
            notes: notes.remove(&day.local_date).unwrap_or(0),
            day,
        })
        .collect();
    rows.extend(notes.into_iter().map(|(local_date, notes)| DayRow {
        day: DayOverview {
            local_date,
            slices: 0,
            ignored_slices: 0,
            active_minutes: 0,
            unassigned_slices: 0,
            report_status: None,
        },
        notes,
    }));
    rows.sort_by(|a, b| b.day.local_date.cmp(&a.day.local_date));
    rows
}

/// Every active day, newest first. `active_minutes` counts overlapping time once.
pub fn day_overviews(store: &Store, include_ignored: bool) -> Result<Vec<DayOverview>> {
    let mut days = store.day_overviews(include_ignored)?;
    // Days are ordered newest first.
    let (Some(newest), Some(oldest)) = (days.first(), days.last()) else {
        return Ok(days);
    };
    let mut by_day: HashMap<String, Vec<_>> = HashMap::new();
    for s in store.stats_slices(Some(&oldest.local_date), Some(&newest.local_date), false)? {
        by_day
            .entry(s.local_date)
            .or_default()
            .extend(s.active_intervals);
    }
    for d in &mut days {
        d.active_minutes = by_day.remove(&d.local_date).map_or(0, union_minutes);
    }
    Ok(days)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(date: &str, slices: u64) -> DayOverview {
        DayOverview {
            local_date: date.into(),
            slices,
            ignored_slices: 0,
            active_minutes: 0,
            unassigned_slices: 0,
            report_status: None,
        }
    }

    #[test]
    fn notes_join_their_days_and_add_note_only_days_in_order() {
        let rows = with_notes(
            vec![day("2026-11-20", 3), day("2026-11-18", 1)],
            vec![
                HumanCountRow {
                    period_key: "2026-11-19".into(),
                    notes: 2,
                },
                HumanCountRow {
                    period_key: "2026-11-20".into(),
                    notes: 1,
                },
            ],
        );
        let got: Vec<_> = rows
            .iter()
            .map(|r| (r.day.local_date.as_str(), r.day.slices, r.notes))
            .collect();
        assert_eq!(
            got,
            vec![
                ("2026-11-20", 3, 1),
                ("2026-11-19", 0, 2),
                ("2026-11-18", 1, 0)
            ]
        );
    }
}
