//! Where scanning stands for each source file (its cursor), and the health of each agent's
//! sources: files read, records that failed or were unknown, the agent versions seen.

use aj_ir::AgentId;
use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use crate::Store;

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

impl Store {
    pub fn cursor(&self, agent: &AgentId, source_session_id: &str) -> Result<Option<CursorRow>> {
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
        agent: &AgentId,
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
}
