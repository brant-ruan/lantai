//! IR storage layer: SQLite persistence.
//!
//! Events ([`aj_ir::IrEvent`]) are merged here into sessions, turns and the items inside a turn.
//! Ingestion is idempotent: writing the same event twice never produces duplicates.

use std::path::Path;

use aj_ir::{AgentKind, IrEvent, ToolKind};
use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};

mod backup;
mod chunks;
mod human;
mod projects;
mod prompts;
mod reports;
mod schema;
mod search;
mod stats;

pub use backup::{file_schema_version, SCHEMA_VERSION};
pub use chunks::{ChunkDigestRow, ProjectBriefRow};
pub use human::{
    HumanCountRow, HumanNodeRow, HumanPosition, HumanRelocation, NewHumanRoot, PROJECT_PERIOD,
};
pub use projects::{Candidate, DeleteImpact, NewRule, ProjectSummary, RuleRow};
pub use prompts::{BuiltinPrompt, PromptRow};
pub use reports::{
    NewReportVersion, PeriodReportRow, ReportMeta, ReportSummaryRow, ReportVersionRow,
};
pub use search::{
    like_pattern, ReportTextRow, SearchField, SliceFilter, SliceRef, SliceSearchRow, TurnSearchRow,
};
pub use stats::{StatsSliceRow, TokenAggregateRow, ToolAggregateRow};

pub struct Store {
    conn: Connection,
}

// ---------------------------------------------------------------------------
// Row types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CursorRow {
    pub current_path: String,
    pub size: u64,
    pub mtime: i64,
    pub byte_offset: u64,
    pub head_hash: String,
}

#[derive(Debug, Clone)]
pub struct SourceHealth {
    pub agent: String,
    pub files: u64,
    pub error_count: u64,
    pub unknown_count: u64,
    pub last_error: Option<String>,
    pub agent_versions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: i64,
    pub agent: AgentKind,
    pub source_session_id: String,
    pub project_id: Option<i64>,
    /// `manual` | `rule` | `unassigned`
    pub assignment_source: String,
    pub suggested_project_id: Option<i64>,
    /// `rule` | `llm`
    pub suggestion_source: Option<String>,
    pub suggestion_reason: Option<String>,
    pub git_root: Option<String>,
    pub git_remote: Option<String>,
    pub detected_cwd: Option<String>,
    pub git_branch: Option<String>,
    pub title: Option<String>,
    pub user_note: Option<String>,
    pub ignored: bool,
    pub first_active_at: Option<DateTime<Utc>>,
    pub last_active_at: Option<DateTime<Utc>>,
}

impl SessionRow {
    /// Whether this session can receive a project suggestion from the model:
    /// it has no project, was not explicitly assigned to "none", and has no suggestion yet.
    pub fn needs_llm_suggestion(&self) -> bool {
        self.project_id.is_none()
            && self.assignment_source != "manual"
            && self.suggested_project_id.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct TurnRow {
    pub id: i64,
    pub session_id: i64,
    pub turn_index: i64,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub user_prompt: String,
}

#[derive(Debug, Clone)]
pub enum TurnItem {
    Message {
        timestamp: DateTime<Utc>,
        text: String,
    },
    Tool {
        timestamp: DateTime<Utc>,
        kind: ToolKind,
        tool_name: String,
        target: Option<String>,
        summary: Option<String>,
        is_error: Option<bool>,
        added: Option<u32>,
        removed: Option<u32>,
        output_summary: Option<String>,
    },
}

impl TurnItem {
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            TurnItem::Message { timestamp, .. } | TurnItem::Tool { timestamp, .. } => *timestamp,
        }
    }
}

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
    /// `inherit` | `ignored` | `included`
    pub ignore_state: String,
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
}

#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    /// Archived projects keep their data, but their rules are paused and they are not offered
    /// for new assignments.
    pub archived: bool,
}

impl SliceRow {
    /// The project this slice actually belongs to: its own override first, otherwise the session's.
    pub fn effective_project(&self, session: &SessionRow) -> Option<i64> {
        self.project_id_override.or(session.project_id)
    }

    /// Whether this slice is excluded from reports: its own setting first, otherwise the session's.
    pub fn is_ignored(&self, session: &SessionRow) -> bool {
        match self.ignore_state.as_str() {
            "ignored" => true,
            "included" => false,
            _ => session.ignored,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayOverview {
    pub local_date: String,
    pub slices: u64,
    pub ignored_slices: u64,
    pub active_minutes: i64,
    pub unassigned_slices: u64,
    pub report_status: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct IngestStats {
    pub session_id: i64,
    pub new_turns: u64,
    pub new_items: u64,
    pub unknown: u64,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) fn ts(t: DateTime<Utc>) -> String {
    // Fixed-format UTC strings, which compare correctly as plain text.
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn parse_ts(s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn opt_ts(s: Option<String>) -> rusqlite::Result<Option<DateTime<Utc>>> {
    s.as_deref().map(parse_ts).transpose()
}

pub(crate) const SESSION_COLS: &str = "id, agent, source_session_id, project_id, assignment_source, detected_cwd, \
     git_branch, COALESCE(title, fallback_title), user_note, ignored, first_active_at, last_active_at, \
     suggested_project_id, suggestion_source, suggestion_reason, git_root, git_remote";

/// Read an agent key column, failing on values this build does not know.
pub(crate) fn agent_at(r: &Row, idx: usize) -> rusqlite::Result<AgentKind> {
    let agent: String = r.get(idx)?;
    AgentKind::parse(&agent).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            idx,
            rusqlite::types::Type::Text,
            format!("unknown agent kind: {agent}").into(),
        )
    })
}

pub(crate) fn session_from_row(r: &Row) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: r.get(0)?,
        agent: agent_at(r, 1)?,
        source_session_id: r.get(2)?,
        project_id: r.get(3)?,
        assignment_source: r.get(4)?,
        detected_cwd: r.get(5)?,
        git_branch: r.get(6)?,
        title: r.get(7)?,
        user_note: r.get(8)?,
        ignored: r.get(9)?,
        first_active_at: opt_ts(r.get(10)?)?,
        last_active_at: opt_ts(r.get(11)?)?,
        suggested_project_id: r.get(12)?,
        suggestion_source: r.get(13)?,
        suggestion_reason: r.get(14)?,
        git_root: r.get(15)?,
        git_remote: r.get(16)?,
    })
}

const SLICE_COLS: &str = "id, session_id, local_date, start_turn_index, end_turn_index, slice_start_at, \
     slice_end_at, active_minutes, files_modified_json, ignore_state, digest_json, digest_input_hash, \
     project_id_override, digest_covered_turn, digest_update_count, active_intervals_json, digest_language";

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
    })
}

/// Before an older database is migrated, keep a consistent copy of it next to the original
/// (`journal.sqlite.bak-schema-<old version>`), so that going back to an older build of Lantai
/// stays possible. Made once per old version; an existing copy is left alone.
fn backup_before_migration(conn: &Connection, path: &Path) -> Result<()> {
    let current = schema::version(conn)?;
    if current == 0 || current >= schema::SCHEMA_VERSION {
        return Ok(());
    }
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".bak-schema-{current}"));
    let backup = path.with_file_name(name);
    if backup.exists() {
        return Ok(());
    }
    conn.execute("VACUUM INTO ?1", [backup.to_string_lossy()])
        .with_context(|| format!("failed to back up the database to {}", backup.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database {}", path.display()))?;
        backup_before_migration(&conn, path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // Several report jobs may write at once; wait for the lock instead of failing.
        conn.busy_timeout(std::time::Duration::from_secs(15))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        schema::init(&conn)?;
        Ok(Self { conn })
    }

    // ---- Scan cursors ------------------------------------------------------

    pub fn cursor(&self, agent: AgentKind, source_session_id: &str) -> Result<Option<CursorRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT current_path, size, mtime, byte_offset, head_hash FROM source_files
                 WHERE agent = ?1 AND source_session_id = ?2",
                params![agent.as_str(), source_session_id],
                |r| {
                    Ok(CursorRow {
                        current_path: r.get(0)?,
                        size: r.get::<_, i64>(1)? as u64,
                        mtime: r.get(2)?,
                        byte_offset: r.get::<_, i64>(3)? as u64,
                        head_hash: r.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// Update a cursor. `reset_counters` means the file was reparsed from the start, so error and unknown counts restart.
    #[allow(clippy::too_many_arguments)]
    pub fn save_cursor(
        &self,
        agent: AgentKind,
        source_session_id: &str,
        cursor: &CursorRow,
        reset_counters: bool,
        new_errors: u64,
        new_unknown: u64,
        last_error: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO source_files
               (agent, source_session_id, current_path, size, mtime, byte_offset, head_hash,
                error_count, unknown_count, last_error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(agent, source_session_id) DO UPDATE SET
               current_path = excluded.current_path,
               size = excluded.size,
               mtime = excluded.mtime,
               byte_offset = excluded.byte_offset,
               head_hash = excluded.head_hash,
               error_count = CASE WHEN ?11 THEN 0 ELSE error_count END + excluded.error_count,
               unknown_count = CASE WHEN ?11 THEN 0 ELSE unknown_count END + excluded.unknown_count,
               last_error = COALESCE(excluded.last_error, CASE WHEN ?11 THEN NULL ELSE last_error END)",
            params![
                agent.as_str(),
                source_session_id,
                cursor.current_path,
                cursor.size as i64,
                cursor.mtime,
                cursor.byte_offset as i64,
                cursor.head_hash,
                new_errors as i64,
                new_unknown as i64,
                last_error,
                reset_counters,
            ],
        )?;
        Ok(())
    }

    pub fn source_health(&self) -> Result<Vec<SourceHealth>> {
        let mut stmt = self.conn.prepare(
            "SELECT agent, COUNT(*), SUM(error_count), SUM(unknown_count),
                    (SELECT last_error FROM source_files f2
                      WHERE f2.agent = f.agent AND last_error IS NOT NULL
                      ORDER BY mtime DESC LIMIT 1)
             FROM source_files f GROUP BY agent ORDER BY agent",
        )?;
        let mut out = stmt
            .query_map([], |r| {
                Ok(SourceHealth {
                    agent: r.get(0)?,
                    files: r.get::<_, i64>(1)? as u64,
                    error_count: r.get::<_, i64>(2)? as u64,
                    unknown_count: r.get::<_, i64>(3)? as u64,
                    last_error: r.get(4)?,
                    agent_versions: vec![],
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for h in &mut out {
            let mut stmt = self.conn.prepare(
                "SELECT DISTINCT agent_version FROM sessions
                 WHERE agent = ?1 AND agent_version IS NOT NULL ORDER BY agent_version",
            )?;
            h.agent_versions = stmt
                .query_map([&h.agent], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?;
        }
        Ok(out)
    }

    // ---- Ingestion ---------------------------------------------------------

    /// Merge a batch of events into a session, inside a single transaction.
    pub fn ingest(
        &mut self,
        agent: AgentKind,
        source_session_id: &str,
        events: &[IrEvent],
    ) -> Result<IngestStats> {
        let tx = self.conn.transaction()?;
        let stats = ingest_tx(&tx, agent, source_session_id, events)?;
        tx.commit()?;
        Ok(stats)
    }

    // ---- Sessions ----------------------------------------------------------

    pub fn session(&self, id: i64) -> Result<Option<SessionRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {SESSION_COLS} FROM sessions WHERE id = ?1"),
                [id],
                session_from_row,
            )
            .optional()?)
    }

    /// Find sessions by a prefix of the source session ID (so the CLI accepts short IDs).
    pub fn find_sessions_by_prefix(&self, prefix: &str) -> Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SESSION_COLS} FROM sessions WHERE source_session_id LIKE ?1 || '%' LIMIT 10"
        ))?;
        let rows = stmt
            .query_map([prefix], session_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn session_count(&self) -> Result<(u64, u64)> {
        Ok(self.conn.query_row(
            "SELECT (SELECT COUNT(*) FROM sessions), (SELECT COUNT(*) FROM turns)",
            [],
            |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)? as u64)),
        )?)
    }

    /// Number of stored sessions per agent, including those of agents no longer scanned.
    pub fn session_counts_by_agent(&self) -> Result<Vec<(AgentKind, u64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT agent, COUNT(*) FROM sessions GROUP BY agent ORDER BY agent")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (agent, n) = row?;
            let kind = AgentKind::parse(&agent)
                .with_context(|| format!("unknown agent in database: {agent}"))?;
            out.push((kind, n as u64));
        }
        Ok(out)
    }

    pub fn set_session_ignored(&self, session_id: i64, ignored: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET ignored = ?2 WHERE id = ?1",
            params![session_id, ignored],
        )?;
        Ok(())
    }

    pub fn set_session_note(&self, session_id: i64, note: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET user_note = ?2 WHERE id = ?1",
            params![session_id, note],
        )?;
        Ok(())
    }

    // ---- Turn --------------------------------------------------------------

    pub fn turns(&self, session_id: i64, start_index: i64, end_index: i64) -> Result<Vec<TurnRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, session_id, turn_index, started_at, ended_at, user_prompt FROM turns
             WHERE session_id = ?1 AND turn_index BETWEEN ?2 AND ?3 ORDER BY turn_index",
        )?;
        let rows = stmt
            .query_map(params![session_id, start_index, end_index], |r| {
                Ok(TurnRow {
                    id: r.get(0)?,
                    session_id: r.get(1)?,
                    turn_index: r.get(2)?,
                    started_at: parse_ts(&r.get::<_, String>(3)?)?,
                    ended_at: parse_ts(&r.get::<_, String>(4)?)?,
                    user_prompt: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn all_turns(&self, session_id: i64) -> Result<Vec<TurnRow>> {
        self.turns(session_id, 0, i64::MAX)
    }

    pub fn turn_items(&self, turn_id: i64) -> Result<Vec<TurnItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT item_type, timestamp, text, tool_kind, tool_name, target, summary, is_error,
                    added, removed, output_summary
             FROM turn_items WHERE turn_id = ?1 ORDER BY seq",
        )?;
        let rows = stmt
            .query_map([turn_id], |r| {
                let item_type: String = r.get(0)?;
                let timestamp = parse_ts(&r.get::<_, String>(1)?)?;
                Ok(if item_type == "message" {
                    TurnItem::Message {
                        timestamp,
                        text: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    }
                } else {
                    TurnItem::Tool {
                        timestamp,
                        kind: ToolKind::parse(&r.get::<_, Option<String>>(3)?.unwrap_or_default()),
                        tool_name: r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        target: r.get(5)?,
                        summary: r.get(6)?,
                        is_error: r.get(7)?,
                        added: r.get(8)?,
                        removed: r.get(9)?,
                        output_summary: r.get(10)?,
                    }
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    // ---- Slices ------------------------------------------------------------

    /// Replace a session's slices with freshly computed ones.
    /// A slice for the same day keeps its ID, ignore state and digest cache; days that no longer exist are deleted.
    /// A slice on a new day (e.g. after the day boundary changed) takes the ignore state and project
    /// override of the previous slice that shared the most turns with it.
    pub fn replace_slices(&mut self, session_id: i64, slices: &[NewSlice]) -> Result<()> {
        let tx = self.conn.transaction()?;
        // (local_date, start_turn, end_turn, ignore_state, project_id_override) before the change.
        let before: Vec<(String, i64, i64, String, Option<i64>)> = {
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
            let overlap = |b: &&(String, i64, i64, String, Option<i64>)| {
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
             WHERE COALESCE(sds.project_id_override, s.project_id) = ?1
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
             WHERE COALESCE(sds.project_id_override, s.project_id) IS NULL
               AND s.assignment_source != 'manual'
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

    pub fn set_slice_ignore_state(&self, slice_id: i64, state: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE session_daily_slices SET ignore_state = ?2 WHERE id = ?1",
            params![slice_id, state],
        )?;
        Ok(())
    }

    pub fn save_digest(&self, slice_id: i64, digest: &SliceDigestRow) -> Result<()> {
        self.conn.execute(
            "UPDATE session_daily_slices SET digest_json = ?2, digest_input_hash = ?3,
                 digest_covered_turn = ?4, digest_update_count = ?5, digest_language = ?6
             WHERE id = ?1",
            params![
                slice_id,
                digest.digest_json,
                digest.input_hash,
                digest.covered_turn,
                digest.update_count,
                digest.language
            ],
        )?;
        Ok(())
    }

    pub fn dates_with_activity(&self, limit: u32) -> Result<Vec<(String, u64, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT local_date, COUNT(*), SUM(active_minutes) FROM session_daily_slices
             GROUP BY local_date ORDER BY local_date DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([limit], |r| {
                Ok((r.get(0)?, r.get::<_, i64>(1)? as u64, r.get(2)?))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn day_overviews(&self, include_ignored: bool) -> Result<Vec<DayOverview>> {
        let mut stmt = self.conn.prepare(
            "SELECT sds.local_date,
                    SUM(CASE WHEN (sds.ignore_state = 'included' OR (sds.ignore_state = 'inherit' AND s.ignored = 0)) THEN 1 ELSE 0 END),
                    SUM(CASE WHEN NOT (sds.ignore_state = 'included' OR (sds.ignore_state = 'inherit' AND s.ignored = 0)) THEN 1 ELSE 0 END),
                    SUM(CASE WHEN (sds.ignore_state = 'included' OR (sds.ignore_state = 'inherit' AND s.ignored = 0)) THEN sds.active_minutes ELSE 0 END),
                    SUM(CASE WHEN (sds.ignore_state = 'included' OR (sds.ignore_state = 'inherit' AND s.ignored = 0))
                                  AND COALESCE(sds.project_id_override, s.project_id) IS NULL
                                  AND s.assignment_source != 'manual'
                             THEN 1 ELSE 0 END),
                    r.status
             FROM session_daily_slices sds
             JOIN sessions s ON s.id = sds.session_id
             LEFT JOIN reports r ON r.report_type = 'daily' AND r.period_key = sds.local_date AND r.project_id IS NULL
             GROUP BY sds.local_date
             HAVING (?1 != 0 OR SUM(CASE WHEN (sds.ignore_state = 'included' OR (sds.ignore_state = 'inherit' AND s.ignored = 0)) THEN 1 ELSE 0 END) > 0)
             ORDER BY sds.local_date DESC",
        )?;
        let rows = stmt
            .query_map(params![i32::from(include_ignored)], |r| {
                Ok(DayOverview {
                    local_date: r.get(0)?,
                    slices: r.get::<_, i64>(1)? as u64,
                    ignored_slices: r.get::<_, i64>(2)? as u64,
                    active_minutes: r.get(3)?,
                    unassigned_slices: r.get::<_, i64>(4)? as u64,
                    report_status: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}

// ---------------------------------------------------------------------------
// Event merging
// ---------------------------------------------------------------------------

fn ingest_tx(
    tx: &Transaction,
    agent: AgentKind,
    source_session_id: &str,
    events: &[IrEvent],
) -> Result<IngestStats> {
    tx.execute(
        "INSERT OR IGNORE INTO sessions (agent, source_session_id) VALUES (?1, ?2)",
        params![agent.as_str(), source_session_id],
    )?;
    let session_id: i64 = tx.query_row(
        "SELECT id FROM sessions WHERE agent = ?1 AND source_session_id = ?2",
        params![agent.as_str(), source_session_id],
        |r| r.get(0),
    )?;

    let mut stats = IngestStats {
        session_id,
        ..Default::default()
    };
    // Current turn: the latest TurnStart in this batch; items at the start of a batch belong to the session's last existing turn.
    let mut current_turn: Option<i64> = tx
        .query_row(
            "SELECT id FROM turns WHERE session_id = ?1 ORDER BY turn_index DESC LIMIT 1",
            [session_id],
            |r| r.get(0),
        )
        .optional()?;

    for event in events {
        match event {
            IrEvent::SessionMeta {
                cwd,
                git_branch,
                agent_version,
                parent_session_id,
            } => {
                // The working directory keeps its first value (a mid-session cd does not change ownership); the rest take the latest value.
                tx.execute(
                    "UPDATE sessions SET
                       detected_cwd = COALESCE(detected_cwd, ?2),
                       git_branch = COALESCE(?3, git_branch),
                       agent_version = COALESCE(?4, agent_version),
                       parent_session_id = COALESCE(?5, parent_session_id)
                     WHERE id = ?1",
                    params![
                        session_id,
                        cwd,
                        git_branch,
                        agent_version,
                        parent_session_id
                    ],
                )?;
            }
            IrEvent::SessionTitle { title } => {
                tx.execute(
                    "UPDATE sessions SET title = ?2 WHERE id = ?1",
                    params![session_id, title],
                )?;
            }
            IrEvent::TurnStart {
                source_event_id,
                timestamp,
                user_prompt,
            } => {
                let turn_id = insert_turn(
                    tx,
                    session_id,
                    source_event_id,
                    *timestamp,
                    user_prompt,
                    &mut stats,
                )?;
                current_turn = Some(turn_id);
            }
            IrEvent::AgentMessage {
                source_event_id,
                timestamp,
                text,
                ..
            } => {
                let turn_id = match current_turn {
                    Some(id) => id,
                    None => implicit_turn(tx, session_id, source_event_id, *timestamp, &mut stats)?,
                };
                current_turn = Some(turn_id);
                let inserted = tx.execute(
                    "INSERT OR IGNORE INTO turn_items
                       (session_id, turn_id, seq, source_event_id, timestamp, item_type, text)
                     VALUES (?1, ?2, (SELECT COALESCE(MAX(seq), -1) + 1 FROM turn_items WHERE turn_id = ?2),
                             ?3, ?4, 'message', ?5)",
                    params![session_id, turn_id, source_event_id, ts(*timestamp), text],
                )?;
                if inserted > 0 {
                    stats.new_items += 1;
                    touch(tx, session_id, turn_id, *timestamp)?;
                }
            }
            IrEvent::ToolAction {
                source_event_id,
                timestamp,
                kind,
                tool_name,
                target,
                summary,
                diff_stat,
            } => {
                let turn_id = match current_turn {
                    Some(id) => id,
                    None => implicit_turn(tx, session_id, source_event_id, *timestamp, &mut stats)?,
                };
                current_turn = Some(turn_id);
                let inserted = tx.execute(
                    "INSERT OR IGNORE INTO turn_items
                       (session_id, turn_id, seq, source_event_id, timestamp, item_type,
                        tool_kind, tool_name, target, summary, added, removed)
                     VALUES (?1, ?2, (SELECT COALESCE(MAX(seq), -1) + 1 FROM turn_items WHERE turn_id = ?2),
                             ?3, ?4, 'tool', ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        session_id,
                        turn_id,
                        source_event_id,
                        ts(*timestamp),
                        kind.as_str(),
                        tool_name,
                        target,
                        summary,
                        diff_stat.map(|d| d.added),
                        diff_stat.map(|d| d.removed),
                    ],
                )?;
                if inserted > 0 {
                    stats.new_items += 1;
                    touch(tx, session_id, turn_id, *timestamp)?;
                }
            }
            IrEvent::ToolOutcome {
                tool_source_id,
                is_error,
                diff_stat,
                output_summary,
            } => {
                tx.execute(
                    "UPDATE turn_items SET
                       is_error = ?3,
                       added = COALESCE(?4, added),
                       removed = COALESCE(?5, removed),
                       output_summary = COALESCE(?6, output_summary)
                     WHERE session_id = ?1 AND source_event_id = ?2 AND item_type = 'tool'",
                    params![
                        session_id,
                        tool_source_id,
                        is_error,
                        diff_stat.map(|d| d.added),
                        diff_stat.map(|d| d.removed),
                        output_summary,
                    ],
                )?;
            }
            IrEvent::TokenUsage {
                source_event_id,
                timestamp,
                model,
                usage,
            } => {
                // Usage never opens a turn; before the session's first turn it has nowhere to go.
                let Some(turn_id) = current_turn else {
                    continue;
                };
                tx.execute(
                    "INSERT OR IGNORE INTO token_usage
                       (session_id, turn_id, source_event_id, timestamp, model,
                        input_tokens, output_tokens, cache_read_tokens, cache_write_tokens)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        session_id,
                        turn_id,
                        source_event_id,
                        ts(*timestamp),
                        model,
                        usage.input_tokens as i64,
                        usage.output_tokens as i64,
                        usage.cache_read_tokens as i64,
                        usage.cache_write_tokens as i64,
                    ],
                )?;
            }
            IrEvent::Unknown { .. } => stats.unknown += 1,
        }
    }
    Ok(stats)
}

fn insert_turn(
    tx: &Transaction,
    session_id: i64,
    source_event_id: &str,
    timestamp: DateTime<Utc>,
    user_prompt: &str,
    stats: &mut IngestStats,
) -> Result<i64> {
    let t = ts(timestamp);
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO turns (session_id, turn_index, source_event_id, started_at, ended_at, user_prompt)
         VALUES (?1, (SELECT COALESCE(MAX(turn_index), -1) + 1 FROM turns WHERE session_id = ?1), ?2, ?3, ?3, ?4)",
        params![session_id, source_event_id, t, user_prompt],
    )?;
    let turn_id: i64 = tx.query_row(
        "SELECT id FROM turns WHERE session_id = ?1 AND source_event_id = ?2",
        params![session_id, source_event_id],
        |r| r.get(0),
    )?;
    if inserted > 0 {
        stats.new_turns += 1;
        // Without an explicit title, use the beginning of the first input.
        let first_line = user_prompt
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        let fallback: String = first_line.chars().take(60).collect();
        tx.execute(
            "UPDATE sessions SET fallback_title = COALESCE(fallback_title, NULLIF(?2, '')) WHERE id = ?1",
            params![session_id, fallback],
        )?;
        touch(tx, session_id, turn_id, timestamp)?;
    }
    Ok(turn_id)
}

/// Agent activity appeared before any human input (e.g. the session starts from a compaction summary): add a turn with an empty prompt.
fn implicit_turn(
    tx: &Transaction,
    session_id: i64,
    source_event_id: &str,
    timestamp: DateTime<Utc>,
    stats: &mut IngestStats,
) -> Result<i64> {
    insert_turn(
        tx,
        session_id,
        &format!("implicit:{source_event_id}"),
        timestamp,
        "",
        stats,
    )
}

fn touch(tx: &Transaction, session_id: i64, turn_id: i64, timestamp: DateTime<Utc>) -> Result<()> {
    let t = ts(timestamp);
    tx.execute(
        "UPDATE turns SET ended_at = MAX(ended_at, ?2) WHERE id = ?1",
        params![turn_id, t],
    )?;
    tx.execute(
        "UPDATE sessions SET
           first_active_at = MIN(COALESCE(first_active_at, ?2), ?2),
           last_active_at = MAX(COALESCE(last_active_at, ?2), ?2)
         WHERE id = ?1",
        params![session_id, t],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aj_ir::DiffStat;

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn sample_events() -> Vec<IrEvent> {
        vec![
            IrEvent::SessionMeta {
                cwd: Some("/work/app".into()),
                git_branch: Some("main".into()),
                agent_version: Some("2.1.0".into()),
                parent_session_id: None,
            },
            IrEvent::TurnStart {
                source_event_id: "u1".into(),
                timestamp: t("2026-11-16T01:00:00Z"),
                user_prompt: "fix the login".into(),
            },
            IrEvent::ToolAction {
                source_event_id: "tool1".into(),
                timestamp: t("2026-11-16T01:00:05Z"),
                kind: ToolKind::FileEdit,
                tool_name: "Edit".into(),
                target: Some("/work/app/a.rs".into()),
                summary: None,
                diff_stat: Some(DiffStat {
                    added: 1,
                    removed: 1,
                }),
            },
            IrEvent::ToolOutcome {
                tool_source_id: "tool1".into(),
                is_error: false,
                diff_stat: Some(DiffStat {
                    added: 5,
                    removed: 2,
                }),
                output_summary: None,
            },
            IrEvent::AgentMessage {
                source_event_id: "a1".into(),
                timestamp: t("2026-11-16T01:01:00Z"),
                text: "fixed".into(),
            },
        ]
    }

    #[test]
    fn migrating_keeps_a_backup_of_the_old_database() {
        let dir = std::env::temp_dir().join(format!("aj-store-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(&format!(
                "{}\nPRAGMA user_version = 8;",
                include_str!("../tests/fixtures/schema_v8.sql")
            ))
            .unwrap();
        }
        let version = |p: &Path| -> i64 {
            Connection::open(p)
                .unwrap()
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap()
        };
        drop(Store::open(&path).unwrap());
        let backup = dir.join("journal.sqlite.bak-schema-8");
        assert_eq!(version(&backup), 8);
        assert_eq!(version(&path), schema::SCHEMA_VERSION);
        // A current database is opened without another copy.
        drop(Store::open(&path).unwrap());
        assert_eq!(
            std::fs::read_dir(&dir)
                .unwrap()
                .filter(|e| e
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".bak-"))
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ingest_is_idempotent() {
        let mut store = Store::open_in_memory().unwrap();
        let events = sample_events();
        let first = store.ingest(AgentKind::Claude, "s1", &events).unwrap();
        assert_eq!((first.new_turns, first.new_items), (1, 2));
        let second = store.ingest(AgentKind::Claude, "s1", &events).unwrap();
        assert_eq!((second.new_turns, second.new_items), (0, 0));

        let session = store.session(first.session_id).unwrap().unwrap();
        assert_eq!(session.title.as_deref(), Some("fix the login"));
        assert_eq!(session.detected_cwd.as_deref(), Some("/work/app"));

        let turns = store.all_turns(first.session_id).unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].ended_at, t("2026-11-16T01:01:00Z"));
        let items = store.turn_items(turns[0].id).unwrap();
        assert_eq!(items.len(), 2);
        // The patch line counts from the tool result override the estimate from the arguments.
        match &items[0] {
            TurnItem::Tool {
                added,
                removed,
                is_error,
                ..
            } => {
                assert_eq!(
                    (*added, *removed, *is_error),
                    (Some(5), Some(2), Some(false))
                );
            }
            other => panic!("expected a tool item, got {other:?}"),
        }
    }

    #[test]
    fn session_counts_are_grouped_by_agent() {
        let mut store = Store::open_in_memory().unwrap();
        let events = sample_events();
        store.ingest(AgentKind::Claude, "s1", &events).unwrap();
        store.ingest(AgentKind::Claude, "s2", &events).unwrap();
        store.ingest(AgentKind::OpenCode, "s1", &events).unwrap();
        assert_eq!(
            store.session_counts_by_agent().unwrap(),
            [(AgentKind::Claude, 2), (AgentKind::OpenCode, 1)]
        );
    }

    #[test]
    fn items_in_later_batch_attach_to_last_turn() {
        let mut store = Store::open_in_memory().unwrap();
        let events = sample_events();
        store.ingest(AgentKind::Claude, "s1", &events[..2]).unwrap();
        let stats = store.ingest(AgentKind::Claude, "s1", &events[2..]).unwrap();
        assert_eq!((stats.new_turns, stats.new_items), (0, 2));
        assert_eq!(store.all_turns(stats.session_id).unwrap().len(), 1);
    }

    #[test]
    fn activity_before_any_prompt_gets_implicit_turn() {
        let mut store = Store::open_in_memory().unwrap();
        let events = sample_events();
        let stats = store.ingest(AgentKind::Claude, "s1", &events[4..]).unwrap();
        let turns = store.all_turns(stats.session_id).unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].user_prompt, "");
    }

    #[test]
    fn slices_keep_digest_across_replace() {
        let mut store = Store::open_in_memory().unwrap();
        let sid = store
            .ingest(AgentKind::Claude, "s1", &sample_events())
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
                },
            )
            .unwrap();

        slice.end_turn_index = 3;
        store.replace_slices(sid, &[slice]).unwrap();
        let row = store.slice(id).unwrap().unwrap();
        assert_eq!(row.end_turn_index, 3);
        assert_eq!(row.digest_input_hash.as_deref(), Some("h"));
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
            .ingest(AgentKind::Claude, "s1", &sample_events())
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
        store.set_slice_ignore_state(old, "ignored").unwrap();

        // Turns 1-2 move to the next day; both resulting slices came from the ignored one,
        // the new day inherits it and the kept day keeps its own state.
        store
            .replace_slices(sid, &[slice("2026-11-15", 0, 0), slice("2026-11-16", 1, 2)])
            .unwrap();
        let rows = store.slices_for_session(sid).unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.ignore_state == "ignored"));

        // The whole slice moves to a new day: the old day's state carries over.
        store.set_slice_ignore_state(rows[0].id, "inherit").unwrap();
        store
            .replace_slices(sid, &[slice("2026-11-17", 0, 2)])
            .unwrap();
        let rows = store.slices_for_session(sid).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].ignore_state, "ignored");
    }

    #[test]
    fn day_overviews_exclude_ignored_and_track_unassigned_and_report_status() {
        let mut store = Store::open_in_memory().unwrap();
        let pid = store.create_project("App", None).unwrap();

        let add = |store: &mut Store, sid: &str, date: &str, mins: i64| -> (i64, i64) {
            let session_id = store
                .ingest(
                    AgentKind::Claude,
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
                        active_minutes: mins,
                        active_intervals_json: "[]".into(),
                        files_modified_json: "[]".into(),
                    }],
                )
                .unwrap();
            let slice_id = store.slices_for_session(session_id).unwrap()[0].id;
            (session_id, slice_id)
        };

        let (s1, _) = add(&mut store, "s1", "2026-11-16", 10);
        store.assign_manual(s1, Some(pid)).unwrap();
        let (_s2, _) = add(&mut store, "s2", "2026-11-16", 20);
        let (s3, _) = add(&mut store, "s3", "2026-11-16", 30);
        store.set_session_ignored(s3, true).unwrap();
        let (s4, _) = add(&mut store, "s4", "2026-11-16", 15);
        store.assign_manual(s4, None).unwrap();
        let (s5, _) = add(&mut store, "s5", "2026-11-15", 25);
        store.set_session_ignored(s5, true).unwrap();

        store
            .save_report_version(&NewReportVersion {
                report_type: "daily",
                period_key: "2026-11-16",
                project_id: None,
                origin: "generated",
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
            .set_report_status("daily", "2026-11-16", None, "final")
            .unwrap();

        let days = store.day_overviews(false).unwrap();
        assert_eq!(
            days,
            vec![DayOverview {
                local_date: "2026-11-16".into(),
                slices: 3,
                ignored_slices: 1,
                active_minutes: 45,
                unassigned_slices: 1,
                report_status: Some("final".into()),
            }]
        );

        let all_days = store.day_overviews(true).unwrap();
        assert_eq!(
            all_days,
            vec![
                DayOverview {
                    local_date: "2026-11-16".into(),
                    slices: 3,
                    ignored_slices: 1,
                    active_minutes: 45,
                    unassigned_slices: 1,
                    report_status: Some("final".into()),
                },
                DayOverview {
                    local_date: "2026-11-15".into(),
                    slices: 0,
                    ignored_slices: 1,
                    active_minutes: 0,
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
                .ingest(AgentKind::Claude, sid, &events)
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
            .set_suggestion(s3, Some((app, "llm", "matched app")))
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
