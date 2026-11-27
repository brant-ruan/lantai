//! IR storage layer: SQLite persistence.
//!
//! [`Store`] owns the connection; its queries are grouped by what they touch, one module each:
//! ingestion of IR events (`ingest`), sessions and turns (`sessions`), daily slices (`slices`),
//! scan cursors (`sources`), projects, reports, prompts, the human journal, search and
//! statistics. SQL shared between modules lives in `sql`.

use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::Connection;

mod backup;
mod chunks;
mod human;
mod ingest;
mod projects;
mod prompts;
mod reports;
mod schema;
mod search;
mod sessions;
mod slices;
mod sources;
mod sql;
mod stats;

pub use backup::{database_state, file_schema_version, snapshot_file, SCHEMA_VERSION};
pub use chunks::{ChunkDigestRow, ProjectBriefRow};
pub use human::{
    HumanCountRow, HumanNodeRow, HumanPosition, HumanRelocation, NewHumanRoot, PROJECT_PERIOD,
};
pub use ingest::IngestStats;
pub use projects::{Candidate, DeleteImpact, NewRule, ProjectRow, ProjectSummary, RuleRow};
pub use prompts::{BuiltinPrompt, PromptRow};
pub use reports::{
    NewReportVersion, PeriodReportRow, ReportMeta, ReportSummaryRow, ReportVersionRow,
};
pub use schema::{DatabaseState, NotReady};
pub use search::{
    like_pattern, ReportTextRow, SearchField, SliceFilter, SliceRef, SliceSearchRow, TurnSearchRow,
};
pub use sessions::{SessionRow, TurnItem, TurnRow};
pub use slices::{DayCounts, NewSlice, SliceDigestRow, SliceRow};
pub use sources::{CursorRow, SourceHealth};
pub use stats::{StatsSliceRow, TokenAggregateRow, ToolAggregateRow};

/// The database. Every query is a method on it, defined in the module of what it touches.
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Open the database at `path`, creating it (and its folder) when there is none. A database
    /// at another schema version is refused with a [`NotReady`] error; see [`Store::migrate`].
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Self::connect(path)?)
    }

    /// Bring the database at `path` to this build's schema. Callers back it up first
    /// (docs/SPEC.md, "Upgrading the database"); a current database is left as it is.
    pub fn migrate(path: &Path) -> Result<()> {
        if !path.exists() {
            bail!("no database at {}", path.display());
        }
        let conn = Self::connect(path)?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        schema::upgrade(&conn)
    }

    fn connect(path: &Path) -> Result<Connection> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // Several report jobs may write at once; wait for the lock instead of failing.
        conn.busy_timeout(std::time::Duration::from_secs(15))?;
        Ok(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        schema::init(&conn)?;
        Ok(Self { conn })
    }
}

/// Events shared by the tests of several modules.
#[cfg(test)]
pub(crate) mod test_events {
    use aj_ir::{DiffStat, IrEvent, ToolKind};
    use chrono::{DateTime, Utc};

    pub(crate) fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    /// One turn: a prompt, an edit whose result corrects its line counts, and a reply.
    pub(crate) fn sample_events() -> Vec<IrEvent> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_refuses_an_old_database_until_it_is_migrated() {
        let dir = std::env::temp_dir().join(format!("aj-store-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("journal.sqlite");
        assert_eq!(database_state(&path).unwrap(), DatabaseState::Ready);
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(&format!(
                "{}\nPRAGMA user_version = 8;",
                include_str!("../tests/fixtures/schema_v8.sql")
            ))
            .unwrap();
        }
        let outdated = DatabaseState::Outdated {
            from: 8,
            to: schema::SCHEMA_VERSION,
        };
        assert_eq!(database_state(&path).unwrap(), outdated);
        let err = Store::open(&path).err().unwrap();
        assert_eq!(err.downcast_ref::<NotReady>(), Some(&NotReady(outdated)));
        assert_eq!(file_schema_version(&path).unwrap(), 8);

        Store::migrate(&path).unwrap();
        assert_eq!(database_state(&path).unwrap(), DatabaseState::Ready);
        Store::open(&path).unwrap();
        // Migrating a current database changes nothing.
        Store::migrate(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
