//! Stored sessions and their turns: reading them back, and the per-session settings the user
//! changes (ignored, note). Writing them is ingestion's job (`ingest.rs`).

use aj_ir::{AgentId, ToolKind};
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, OptionalExtension, Row};

use crate::sql::{agent_at, opt_ts, parse_ts};
use crate::{AssignmentSource, Store, SuggestionSource};

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: i64,
    pub agent: AgentId,
    pub source_session_id: String,
    pub project_id: Option<i64>,
    pub assignment_source: AssignmentSource,
    pub suggested_project_id: Option<i64>,
    pub suggestion_source: Option<SuggestionSource>,
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
            && self.assignment_source != AssignmentSource::Manual
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

/// The columns [`session_from_row`] reads, in its order.
pub(crate) const SESSION_COLS: &str = "id, agent, source_session_id, project_id, assignment_source, detected_cwd, \
     git_branch, COALESCE(title, fallback_title), user_note, ignored, first_active_at, last_active_at, \
     suggested_project_id, suggestion_source, suggestion_reason, git_root, git_remote";

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

impl Store {
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
    pub fn session_counts_by_agent(&self) -> Result<Vec<(AgentId, u64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT agent, COUNT(*) FROM sessions GROUP BY agent ORDER BY agent")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (agent, n) = row?;
            let kind = AgentId::parse(&agent)?;
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

    // ---- Turns -------------------------------------------------------------

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
}

#[cfg(test)]
mod tests {
    use crate::test_events::sample_events;
    use crate::Store;
    use aj_ir::AgentId;

    #[test]
    fn session_counts_are_grouped_by_agent() {
        let mut store = Store::open_in_memory().unwrap();
        let events = sample_events();
        store.ingest(&AgentId::CLAUDE, "s1", &events).unwrap();
        store.ingest(&AgentId::CLAUDE, "s2", &events).unwrap();
        store.ingest(&AgentId::OPENCODE, "s1", &events).unwrap();
        assert_eq!(
            store.session_counts_by_agent().unwrap(),
            [(AgentId::CLAUDE, 2), (AgentId::OPENCODE, 1)]
        );
    }

    #[test]
    fn unregistered_agents_are_stored_and_read_back() {
        let mut store = Store::open_in_memory().unwrap();
        let custom = AgentId::parse("x-mytool").unwrap();
        let id = store
            .ingest(&custom, "s1", &sample_events())
            .unwrap()
            .session_id;
        assert_eq!(store.session(id).unwrap().unwrap().agent, custom);
        assert_eq!(store.session_counts_by_agent().unwrap(), [(custom, 1)]);
    }
}
