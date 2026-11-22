//! Daily slicing of sessions that span several days.

use std::collections::BTreeMap;

use aj_store::{NewSlice, Store, TurnItem};
use anyhow::Result;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};

use crate::Config;

/// The "work day" an instant belongs to: convert to the local time zone, then shift back by the day boundary.
pub fn local_date<Tz: TimeZone>(t: DateTime<Utc>, tz: &Tz, day_boundary_hour: u32) -> NaiveDate {
    (t.with_timezone(tz) - Duration::hours(day_boundary_hour as i64)).date_naive()
}

/// Merge instants into active intervals: neighbours no further apart than `idle_gap` share an interval.
pub fn active_intervals(
    mut points: Vec<DateTime<Utc>>,
    idle_gap: Duration,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    points.sort();
    let mut out: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for p in points {
        match out.last_mut() {
            Some((_, end)) if p - *end <= idle_gap => *end = p,
            _ => out.push((p, p)),
        }
    }
    out
}

/// Total interval length in minutes. Each interval counts at least 1 minute, so a single-message session is never zero.
pub fn total_minutes(intervals: &[(DateTime<Utc>, DateTime<Utc>)]) -> i64 {
    intervals
        .iter()
        .map(|(s, e)| ((*e - *s).num_seconds() as f64 / 60.0).ceil().max(1.0) as i64)
        .sum()
}

/// Minutes covered by the union of `intervals`: overlapping time (parallel sessions) counts once.
pub fn union_minutes(mut intervals: Vec<(DateTime<Utc>, DateTime<Utc>)>) -> i64 {
    intervals.sort();
    let mut merged: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for (s, e) in intervals {
        match merged.last_mut() {
            Some((_, end)) if s <= *end => *end = (*end).max(e),
            _ => merged.push((s, e)),
        }
    }
    total_minutes(&merged)
}

#[derive(Default)]
struct DayAcc {
    start_turn: i64,
    end_turn: i64,
    points: Vec<DateTime<Utc>>,
    files: Vec<String>,
}

/// Recompute and store all slices of a session.
///
/// A whole turn belongs to the day it started on; turns are never split.
pub fn reslice_session<Tz: TimeZone>(
    store: &mut Store,
    session_id: i64,
    cfg: &Config,
    tz: &Tz,
) -> Result<()> {
    let mut days: BTreeMap<NaiveDate, DayAcc> = BTreeMap::new();
    for turn in store.all_turns(session_id)? {
        let date = local_date(turn.started_at, tz, cfg.day_boundary_hour);
        let acc = days.entry(date).or_insert_with(|| DayAcc {
            start_turn: turn.turn_index,
            end_turn: turn.turn_index,
            ..Default::default()
        });
        acc.start_turn = acc.start_turn.min(turn.turn_index);
        acc.end_turn = acc.end_turn.max(turn.turn_index);
        acc.points.push(turn.started_at);
        for item in store.turn_items(turn.id)? {
            acc.points.push(item.timestamp());
            if let TurnItem::Tool {
                kind,
                target: Some(path),
                ..
            } = &item
            {
                if kind.modifies_files() && !acc.files.contains(path) {
                    acc.files.push(path.clone());
                }
            }
        }
    }

    let idle_gap = Duration::minutes(cfg.idle_gap_minutes);
    let slices: Vec<NewSlice> = days
        .into_iter()
        .map(|(date, acc)| {
            let intervals = active_intervals(acc.points, idle_gap);
            let start = intervals.first().map(|i| i.0).unwrap_or_default();
            let end = intervals.last().map(|i| i.1).unwrap_or_default();
            NewSlice {
                local_date: date.to_string(),
                start_turn_index: acc.start_turn,
                end_turn_index: acc.end_turn,
                slice_start_at: start,
                slice_end_at: end,
                active_minutes: total_minutes(&intervals),
                active_intervals_json: serde_json::to_string(&intervals)
                    .unwrap_or_else(|_| "[]".into()),
                files_modified_json: serde_json::to_string(&acc.files)
                    .unwrap_or_else(|_| "[]".into()),
            }
        })
        .collect();
    store.replace_slices(session_id, &slices)
}

/// Recompute the slices of every stored session, e.g. after the day boundary changed.
/// Returns the number of sessions processed.
pub fn reslice_all<Tz: TimeZone>(store: &mut Store, cfg: &Config, tz: &Tz) -> Result<usize> {
    let ids: Vec<i64> = store.all_sessions()?.into_iter().map(|s| s.id).collect();
    for &id in &ids {
        reslice_session(store, id, cfg, tz)?;
    }
    Ok(ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aj_ir::{AgentId, IrEvent};
    use chrono::FixedOffset;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    #[test]
    fn day_boundary_moves_early_morning_to_previous_day() {
        let tz = FixedOffset::east_opt(8 * 3600).unwrap();
        // 02:30 on 11-17 in UTC+8 counts as 11-16
        assert_eq!(
            local_date(t("2026-11-16T18:30:00Z"), &tz, 4).to_string(),
            "2026-11-16"
        );
        // 04:00 on 11-17 in UTC+8 counts as 11-17
        assert_eq!(
            local_date(t("2026-11-16T20:00:00Z"), &tz, 4).to_string(),
            "2026-11-17"
        );
        // With a boundary of 0, calendar days are used
        assert_eq!(
            local_date(t("2026-11-16T18:30:00Z"), &tz, 0).to_string(),
            "2026-11-17"
        );
    }

    #[test]
    fn idle_gaps_split_intervals() {
        let pts = vec![
            t("2026-11-16T01:00:00Z"),
            t("2026-11-16T01:10:00Z"),
            t("2026-11-16T03:00:00Z"),
            t("2026-11-16T03:00:20Z"),
        ];
        let iv = active_intervals(pts, Duration::minutes(30));
        assert_eq!(iv.len(), 2);
        assert_eq!(total_minutes(&iv), 10 + 1);
    }

    #[test]
    fn session_spanning_two_days_gets_two_slices() {
        let tz = FixedOffset::east_opt(8 * 3600).unwrap();
        let mut store = Store::open_in_memory().unwrap();
        let turn = |id: &str, ts: &str| IrEvent::TurnStart {
            source_event_id: id.into(),
            timestamp: t(ts),
            user_prompt: id.into(),
        };
        let events = vec![
            turn("a", "2026-11-16T02:00:00Z"), // 10:00 in UTC+8, 11-16
            turn("b", "2026-11-16T15:00:00Z"), // 23:00 in UTC+8, still 11-16
            turn("c", "2026-11-17T02:00:00Z"), // 10:00 in UTC+8, 11-17
        ];
        let sid = store
            .ingest(&AgentId::CLAUDE, "s", &events)
            .unwrap()
            .session_id;
        reslice_session(&mut store, sid, &Config::default(), &tz).unwrap();
        let slices = store.slices_for_session(sid).unwrap();
        assert_eq!(slices.len(), 2);
        assert_eq!(
            (slices[0].start_turn_index, slices[0].end_turn_index),
            (0, 1)
        );
        assert_eq!(
            (slices[1].start_turn_index, slices[1].end_turn_index),
            (2, 2)
        );
        assert_eq!(slices[0].local_date, "2026-11-16");
    }

    #[test]
    fn union_counts_overlapping_time_once() {
        let t = |s: &str| -> DateTime<Utc> { format!("2026-11-17T{s}:00Z").parse().unwrap() };
        let iv = vec![
            (t("10:00"), t("11:00")),
            (t("10:30"), t("11:30")),
            (t("13:00"), t("13:20")),
        ];
        assert_eq!(union_minutes(iv), 110);
        assert_eq!(union_minutes(vec![]), 0);
    }
}
