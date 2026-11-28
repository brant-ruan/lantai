//! Daily slices: a session's turns on one local day, with the user's per-day settings (ignore
//! state, project override) and the cached digest; and per-day counts for browsing.
//!
//! Slices are computed in `aj-core`; this module stores and reads them. Active time across
//! slices is not summed here: parallel sessions overlap, and `aj_core` takes the union.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Row};

use crate::sql::{parse_ts, ts, SLICE_INCLUDED, SLICE_PROJECT, SLICE_UNASSIGNED};
use crate::{IgnoreState, ReportStatus, SessionRow, Store};

/// A slice to be written (computed by the core layer).
#[derive(Debug, Clone, PartialEq)]
pub struct NewSlice {
    pub local_date: String,
    pub start_turn_index: i64,
    pub end_turn_index: i64,
    pub slice_start_at: DateTime<Utc>,
    pub slice_end_at: DateTime<Utc>,
    pub active_minutes: i64,
    pub active_intervals_json: String,
    pub files_modified_json: String,
}

#[derive(Debug, Clone)]
pub struct SliceRow {
    pub id: i64,
    pub session_id: i64,
    pub local_date: String,
    pub start_turn_index: i64,
    pub end_turn_index: i64,
    pub slice_start_at: DateTime<Utc>,
    pub slice_end_at: DateTime<Utc>,
    pub active_minutes: i64,
    /// Active intervals (UTC), as computed by slicing.
    pub active_intervals: Vec<(DateTime<Utc>, DateTime<Utc>)>,
    pub files_modified: Vec<String>,
    pub ignore_state: IgnoreState,
    /// Project assignment for this day only; takes precedence over the session's project.
    pub project_id_override: Option<i64>,
    pub digest_json: Option<String>,
    pub digest_input_hash: Option<String>,
    /// Report language the cached digest is written in.
    pub digest_language: Option<String>,
    /// Last turn the cached digest covers (`None` before the first digest).
    pub digest_covered_turn: Option<i64>,
    /// Incremental updates applied to the cached digest since its last full digest.
    pub digest_update_count: u32,
    /// Engine that wrote the cached digest (`None` before schema 12).
    pub digest_engine: Option<String>,
}

impl SliceRow {
    /// The project this slice actually belongs to: its own override first, otherwise the session's.
    pub fn effective_project(&self, session: &SessionRow) -> Option<i64> {
        self.project_id_override.or(session.project_id)
    }

    /// Whether this slice is excluded from reports: its own setting first, otherwise the session's.
    pub fn is_ignored(&self, session: &SessionRow) -> bool {
        match self.ignore_state {
            IgnoreState::Ignored => true,
            IgnoreState::Included => false,
            IgnoreState::Inherit => session.ignored,
        }
    }
}

/// A slice digest to be cached, with what it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceDigestRow<'a> {
    pub digest_json: &'a str,
    pub input_hash: &'a str,
    /// Report language the digest is written in.
    pub language: &'a str,
    pub covered_turn: i64,
    pub update_count: u32,
    /// Engine that wrote the digest.
    pub engine: &'a str,
}

/// Slice counts of one local day, and the status of its daily report. Active time is added by
/// `aj_core::overview`, which deduplicates parallel sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayCounts {
    pub local_date: String,
    /// Slices taking part in reports.
    pub slices: u64,
    pub ignored_slices: u64,
    /// Slices taking part in reports that have no project and whose session was not assigned
    /// to none.
    pub unassigned_slices: u64,
    pub report_status: Option<ReportStatus>,
}

const SLICE_COLS: &str = "id, session_id, local_date, start_turn_index, end_turn_index, slice_start_at, \
     slice_end_at, active_minutes, files_modified_json, ignore_state, digest_json, digest_input_hash, \
     project_id_override, digest_covered_turn, digest_update_count, active_intervals_json, digest_language, \
     digest_engine";

/// [`SLICE_COLS`] qualified with a table alias, for queries that join other tables.
fn slice_cols_of(alias: &str) -> String {
    SLICE_COLS
        .split(',')
        .map(|c| format!("{alias}.{}", c.trim()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn slice_from_row(r: &Row) -> rusqlite::Result<SliceRow> {
    let files: String = r.get(8)?;
    Ok(SliceRow {
        id: r.get(0)?,
        session_id: r.get(1)?,
        local_date: r.get(2)?,
        start_turn_index: r.get(3)?,
        end_turn_index: r.get(4)?,
        slice_start_at: parse_ts(&r.get::<_, String>(5)?)?,
        slice_end_at: parse_ts(&r.get::<_, String>(6)?)?,
        active_minutes: r.get(7)?,
        active_intervals: serde_json::from_str(&r.get::<_, String>(15)?).unwrap_or_default(),
        files_modified: serde_json::from_str(&files).unwrap_or_default(),
        ignore_state: r.get(9)?,
        digest_json: r.get(10)?,
        digest_input_hash: r.get(11)?,
        project_id_override: r.get(12)?,
        digest_covered_turn: r.get(13)?,
        digest_update_count: r.get(14)?,
        digest_language: r.get(16)?,
        digest_engine: r.get(17)?,
    })
}

impl Store {
    /// Replace a session's slices with freshly computed ones.
    /// A slice for the same day keeps its ID, ignore state and digest cache; days that no longer exist are deleted.
    /// A slice on a new day (e.g. after the day boundary changed) takes the ignore state and project
    /// override of the previous slice that shared the most turns with it.
    pub fn replace_slices(&mut self, session_id: i64, slices: &[NewSlice]) -> Result<()> {
        let tx = self.conn.transaction()?;
        // (local_date, start_turn, end_turn, ignore_state, project_id_override) before the change.
        let before: Vec<(String, i64, i64, IgnoreState, Option<i64>)> = {
            let mut stmt = tx.prepare(
                "SELECT local_date, start_turn_index, end_turn_index, ignore_state, project_id_override
                 FROM session_daily_slices WHERE session_id = ?1",
            )?;
            let rows = stmt
                .query_map([session_id], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
                })?
                .collect::<rusqlite::Result<_>>()?;
            rows
        };
        for s in slices {
            tx.execute(
                "INSERT INTO session_daily_slices
                   (session_id, local_date, start_turn_index, end_turn_index, slice_start_at,
                    slice_end_at, active_minutes, active_intervals_json, files_modified_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(session_id, local_date) DO UPDATE SET
                   start_turn_index = excluded.start_turn_index,
                   end_turn_index = excluded.end_turn_index,
                   slice_start_at = excluded.slice_start_at,
                   slice_end_at = excluded.slice_end_at,
                   active_minutes = excluded.active_minutes,
                   active_intervals_json = excluded.active_intervals_json,
                   files_modified_json = excluded.files_modified_json",
                params![
                    session_id,
                    s.local_date,
                    s.start_turn_index,
                    s.end_turn_index,
                    ts(s.slice_start_at),
                    ts(s.slice_end_at),
                    s.active_minutes,
                    s.active_intervals_json,
                    s.files_modified_json,
                ],
            )?;
        }
        let dates: Vec<String> = slices.iter().map(|s| s.local_date.clone()).collect();
        for s in slices {
            if before.iter().any(|b| b.0 == s.local_date) {
                continue;
            }
            let overlap = |b: &&(String, i64, i64, IgnoreState, Option<i64>)| {
                (s.end_turn_index.min(b.2) - s.start_turn_index.max(b.1) + 1).max(0)
            };
            if let Some(src) = before
                .iter()
                .filter(|b| overlap(b) > 0)
                .max_by_key(|b| overlap(b))
            {
                tx.execute(
                    "UPDATE session_daily_slices SET ignore_state = ?3, project_id_override = ?4
                     WHERE session_id = ?1 AND local_date = ?2",
                    params![session_id, s.local_date, src.3, src.4],
                )?;
            }
        }
        let existing: Vec<(i64, String)> = {
            let mut stmt = tx
                .prepare("SELECT id, local_date FROM session_daily_slices WHERE session_id = ?1")?;
            let rows = stmt
                .query_map([session_id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            rows
        };
        for (id, date) in existing {
            if !dates.contains(&date) {
                tx.execute("DELETE FROM session_daily_slices WHERE id = ?1", [id])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn slice(&self, id: i64) -> Result<Option<SliceRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {SLICE_COLS} FROM session_daily_slices WHERE id = ?1"),
                [id],
                slice_from_row,
            )
            .optional()?)
    }

    pub fn slices_for_date(&self, local_date: &str) -> Result<Vec<SliceRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SLICE_COLS} FROM session_daily_slices WHERE local_date = ?1 ORDER BY slice_start_at"
        ))?;
        let rows = stmt
            .query_map([local_date], slice_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn slices_for_session(&self, session_id: i64) -> Result<Vec<SliceRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SLICE_COLS} FROM session_daily_slices WHERE session_id = ?1 ORDER BY local_date"
        ))?;
        let rows = stmt
            .query_map([session_id], slice_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn slices_for_project(
        &self,
        project_id: i64,
        since_date: Option<&str>,
        until_date: Option<&str>,
    ) -> Result<Vec<SliceRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM session_daily_slices sds
             JOIN sessions s ON s.id = sds.session_id
             WHERE {SLICE_PROJECT} = ?1
               AND (?2 IS NULL OR sds.local_date >= ?2)
               AND (?3 IS NULL OR sds.local_date <= ?3)
             ORDER BY sds.local_date DESC, sds.slice_start_at DESC",
            slice_cols_of("sds")
        ))?;
        let rows = stmt
            .query_map(params![project_id, since_date, until_date], slice_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn suggested_slices_for_project(&self, project_id: i64) -> Result<Vec<SliceRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {} FROM session_daily_slices sds
             JOIN sessions s ON s.id = sds.session_id
             WHERE {SLICE_UNASSIGNED}
               AND s.suggested_project_id = ?1
             ORDER BY sds.local_date DESC, sds.slice_start_at DESC",
            slice_cols_of("sds")
        ))?;
        let rows = stmt
            .query_map([project_id], slice_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// The slice right before this one in the same session.
    pub fn previous_slice(&self, slice: &SliceRow) -> Result<Option<SliceRow>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {SLICE_COLS} FROM session_daily_slices
                     WHERE session_id = ?1 AND local_date < ?2 ORDER BY local_date DESC LIMIT 1"
                ),
                params![slice.session_id, slice.local_date],
                slice_from_row,
            )
            .optional()?)
    }

    pub fn set_slice_ignore_state(&self, slice_id: i64, state: IgnoreState) -> Result<()> {
        self.conn.execute(
            "UPDATE session_daily_slices SET ignore_state = ?2 WHERE id = ?1",
            params![slice_id, state],
        )?;
        Ok(())
    }

    pub fn save_digest(&self, slice_id: i64, digest: &SliceDigestRow) -> Result<()> {
        self.conn.execute(
            "UPDATE session_daily_slices SET digest_json = ?2, digest_input_hash = ?3,
                 digest_covered_turn = ?4, digest_update_count = ?5, digest_language = ?6,
                 digest_engine = ?7
             WHERE id = ?1",
            params![
                slice_id,
                digest.digest_json,
                digest.input_hash,
                digest.covered_turn,
                digest.update_count,
                digest.language,
                digest.engine
            ],
        )?;
        Ok(())
    }

    /// Every day with slices, newest first. Without `include_ignored`, days whose slices are
    /// all ignored are left out.
    pub fn day_counts(&self, include_ignored: bool) -> Result<Vec<DayCounts>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT sds.local_date,
                    SUM(CASE WHEN {SLICE_INCLUDED} THEN 1 ELSE 0 END) AS included,
                    SUM(CASE WHEN {SLICE_INCLUDED} THEN 0 ELSE 1 END),
                    SUM(CASE WHEN {SLICE_INCLUDED} AND {SLICE_UNASSIGNED} THEN 1 ELSE 0 END),
                    r.status
             FROM session_daily_slices sds
             JOIN sessions s ON s.id = sds.session_id
             LEFT JOIN reports r ON r.report_type = 'daily' AND r.period_key = sds.local_date AND r.project_id IS NULL
             GROUP BY sds.local_date
             HAVING ?1 != 0 OR included > 0
             ORDER BY sds.local_date DESC",
        ))?;
        let rows = stmt
            .query_map(params![i32::from(include_ignored)], |r| {
                Ok(DayCounts {
                    local_date: r.get(0)?,
                    slices: r.get::<_, i64>(1)? as u64,
                    ignored_slices: r.get::<_, i64>(2)? as u64,
                    unassigned_slices: r.get::<_, i64>(3)? as u64,
                    report_status: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use crate::test_events::{sample_events, t};
    use crate::{
        DayCounts, IgnoreState, NewReportVersion, NewSlice, ReportStatus, SliceDigestRow, Store,
    };
    use aj_ir::{AgentId, IrEvent};

    #[test]
    fn slices_keep_digest_across_replace() {
        let mut store = Store::open_in_memory().unwrap();
        let sid = store
            .ingest(&AgentId::CLAUDE, "s1", &sample_events())
            .unwrap()
            .session_id;
        let mut slice = NewSlice {
            local_date: "2026-11-16".into(),
            start_turn_index: 0,
            end_turn_index: 0,
            slice_start_at: t("2026-11-16T01:00:00Z"),
            slice_end_at: t("2026-11-16T01:01:00Z"),
            active_minutes: 1,
            active_intervals_json: "[]".into(),
            files_modified_json: "[]".into(),
        };
        store.replace_slices(sid, &[slice.clone()]).unwrap();
        let id = store.slices_for_date("2026-11-16").unwrap()[0].id;
        store
            .save_digest(
                id,
                &SliceDigestRow {
                    digest_json: "{}",
                    input_hash: "h",
                    language: "en",
                    covered_turn: 0,
                    update_count: 1,
                    engine: "mock",
                },
            )
            .unwrap();

        slice.end_turn_index = 3;
        store.replace_slices(sid, &[slice]).unwrap();
        let row = store.slice(id).unwrap().unwrap();
        assert_eq!(row.end_turn_index, 3);
        assert_eq!(row.digest_input_hash.as_deref(), Some("h"));
        assert_eq!(row.digest_engine.as_deref(), Some("mock"));
        assert_eq!(
            (row.digest_covered_turn, row.digest_update_count),
            (Some(0), 1)
        );

        store.replace_slices(sid, &[]).unwrap();
        assert!(store.slice(id).unwrap().is_none());
    }

    #[test]
    fn moved_slice_keeps_ignore_state_and_override() {
        let mut store = Store::open_in_memory().unwrap();
        let sid = store
            .ingest(&AgentId::CLAUDE, "s1", &sample_events())
            .unwrap()
            .session_id;
        let slice = |date: &str, start: i64, end: i64| NewSlice {
            local_date: date.into(),
            start_turn_index: start,
            end_turn_index: end,
            slice_start_at: t("2026-11-16T01:00:00Z"),
            slice_end_at: t("2026-11-16T01:01:00Z"),
            active_minutes: 1,
            active_intervals_json: "[]".into(),
            files_modified_json: "[]".into(),
        };
        store
            .replace_slices(sid, &[slice("2026-11-15", 0, 2)])
            .unwrap();
        let old = store.slices_for_session(sid).unwrap()[0].id;
        store
            .set_slice_ignore_state(old, IgnoreState::Ignored)
            .unwrap();

        // Turns 1-2 move to the next day; both resulting slices came from the ignored one,
        // the new day inherits it and the kept day keeps its own state.
        store
            .replace_slices(sid, &[slice("2026-11-15", 0, 0), slice("2026-11-16", 1, 2)])
            .unwrap();
        let rows = store.slices_for_session(sid).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.ignore_state == IgnoreState::Ignored));

        // The whole slice moves to a new day: the old day's state carries over.
        store
            .set_slice_ignore_state(rows[0].id, IgnoreState::Inherit)
            .unwrap();
        store
            .replace_slices(sid, &[slice("2026-11-17", 0, 2)])
            .unwrap();
        let rows = store.slices_for_session(sid).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].ignore_state, IgnoreState::Ignored);
    }

    #[test]
    fn day_counts_exclude_ignored_and_track_unassigned_and_report_status() {
        let mut store = Store::open_in_memory().unwrap();
        let pid = store.create_project("App", None).unwrap();

        let add = |store: &mut Store, sid: &str, date: &str| -> i64 {
            let session_id = store
                .ingest(
                    &AgentId::CLAUDE,
                    sid,
                    &[IrEvent::TurnStart {
                        source_event_id: "u1".into(),
                        timestamp: t(&format!("{date}T01:00:00Z")),
                        user_prompt: "hi".into(),
                    }],
                )
                .unwrap()
                .session_id;
            store
                .replace_slices(
                    session_id,
                    &[NewSlice {
                        local_date: date.into(),
                        start_turn_index: 0,
                        end_turn_index: 0,
                        slice_start_at: t(&format!("{date}T01:00:00Z")),
                        slice_end_at: t(&format!("{date}T01:10:00Z")),
                        active_minutes: 10,
                        active_intervals_json: "[]".into(),
                        files_modified_json: "[]".into(),
                    }],
                )
                .unwrap();
            session_id
        };

        let s1 = add(&mut store, "s1", "2026-11-16");
        store.assign_manual(s1, Some(pid)).unwrap();
        add(&mut store, "s2", "2026-11-16");
        let s3 = add(&mut store, "s3", "2026-11-16");
        store.set_session_ignored(s3, true).unwrap();
        let s4 = add(&mut store, "s4", "2026-11-16");
        store.assign_manual(s4, None).unwrap();
        let s5 = add(&mut store, "s5", "2026-11-15");
        store.set_session_ignored(s5, true).unwrap();

        store
            .save_report_version(&NewReportVersion {
                report_type: "daily",
                period_key: "2026-11-16",
                project_id: None,
                origin: crate::ReportOrigin::Generated,
                markdown: "# Report",
                engine: None,
                input_hash: None,
                prompt_id: None,
                prompt_name: None,
                prompt_snapshot: None,
                input_manifest: None,
                language: "en",
            })
            .unwrap();
        store
            .set_report_status("daily", "2026-11-16", None, ReportStatus::Final)
            .unwrap();

        let busy_day = DayCounts {
            local_date: "2026-11-16".into(),
            slices: 3,
            ignored_slices: 1,
            unassigned_slices: 1,
            report_status: Some(ReportStatus::Final),
        };
        assert_eq!(store.day_counts(false).unwrap(), vec![busy_day.clone()]);
        assert_eq!(
            store.day_counts(true).unwrap(),
            vec![
                busy_day,
                DayCounts {
                    local_date: "2026-11-15".into(),
                    slices: 0,
                    ignored_slices: 1,
                    unassigned_slices: 0,
                    report_status: None,
                },
            ]
        );
    }

    #[test]
    fn project_slice_queries_handle_overrides_ranges_and_suggestions() {
        let mut store = Store::open_in_memory().unwrap();
        let app = store.create_project("App", None).unwrap();
        let other = store.create_project("Other", None).unwrap();

        let add = |store: &mut Store, sid: &str, dates: &[&str]| -> i64 {
            let events: Vec<IrEvent> = dates
                .iter()
                .enumerate()
                .map(|(idx, date)| IrEvent::TurnStart {
                    source_event_id: format!("{sid}-u{idx}"),
                    timestamp: t(&format!("{date}T02:00:00Z")),
                    user_prompt: "work".into(),
                })
                .collect();
            let session_id = store
                .ingest(&AgentId::CLAUDE, sid, &events)
                .unwrap()
                .session_id;
            let slices: Vec<NewSlice> = dates
                .iter()
                .enumerate()
                .map(|(idx, date)| NewSlice {
                    local_date: (*date).into(),
                    start_turn_index: idx as i64,
                    end_turn_index: idx as i64,
                    slice_start_at: t(&format!("{date}T02:00:00Z")),
                    slice_end_at: t(&format!("{date}T02:10:00Z")),
                    active_minutes: 10,
                    active_intervals_json: "[]".into(),
                    files_modified_json: "[]".into(),
                })
                .collect();
            store.replace_slices(session_id, &slices).unwrap();
            session_id
        };

        let s1 = add(&mut store, "s1", &["2026-11-14", "2026-11-16"]);
        store.assign_manual(s1, Some(app)).unwrap();

        // Day-level override moves a slice from Other into App.
        let s2 = add(&mut store, "s2", &["2026-11-15"]);
        store.assign_manual(s2, Some(other)).unwrap();
        let s2_slice = store.slices_for_session(s2).unwrap()[0].id;
        store
            .set_slice_project_override(s2_slice, Some(app))
            .unwrap();

        // Suggested session shows up in suggested_slices_for_project.
        let s3 = add(&mut store, "s3", &["2026-11-16"]);
        store
            .set_suggestion(s3, Some((app, crate::SuggestionSource::Llm, "matched app")))
            .unwrap();

        let all = store.slices_for_project(app, None, None).unwrap();
        assert_eq!(all.len(), 3);
        let ranged = store
            .slices_for_project(app, Some("2026-11-15"), Some("2026-11-16"))
            .unwrap();
        assert_eq!(ranged.len(), 2);
        assert_eq!(ranged[0].local_date, "2026-11-16");

        let suggested = store.suggested_slices_for_project(app).unwrap();
        assert_eq!(suggested.len(), 1);
        assert_eq!(suggested[0].session_id, s3);
    }
}
