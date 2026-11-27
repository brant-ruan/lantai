//! Database schema.
//!
//! Any schema change bumps [`SCHEMA_VERSION`]. How an existing database is carried over (a migration,
//! or asking the user to rebuild) is decided per change by the project owner; see AGENTS.md.
//! Session data can be regenerated from the source files, but projects, rules, prompts and reports cannot.
//!
//! A new database gets [`SCHEMA`] directly. An older one is brought forward one version at a
//! time by [`MIGRATIONS`], but only through [`upgrade`]: opening never migrates (docs/SPEC.md,
//! "Upgrading the database"). A version without a migration cannot be upgraded.

use std::fmt;

use anyhow::{bail, Result};
use rusqlite::Connection;

pub const SCHEMA_VERSION: i64 = 12;

/// Human journal entries: day notes, project notes and annotations on generated report points
/// (see `aj_store::human`), as the full schema creates them.
macro_rules! human_nodes_sql {
    () => {
        r#"
-- What people write in the journal. A row with `attach_json` is an annotation on a point of a
-- generated report; otherwise a row with a project is a project note and one without is a day
-- note. Children hang under `parent_id`, ordered by `sort_key` among siblings. `position` puts a
-- project note at the top or the bottom of its project's section; children copy their root's.
CREATE TABLE human_nodes (
    id          INTEGER PRIMARY KEY,
    period_kind TEXT NOT NULL,
    period_key  TEXT NOT NULL,
    project_id  INTEGER REFERENCES logical_projects(id) ON DELETE SET NULL,
    parent_id   INTEGER REFERENCES human_nodes(id) ON DELETE CASCADE,
    sort_key    TEXT NOT NULL,
    attach_json TEXT,
    position    TEXT NOT NULL DEFAULT 'bottom',
    body        TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX human_nodes_period ON human_nodes (period_kind, period_key);
CREATE INDEX human_nodes_parent ON human_nodes (parent_id);
"#
    };
}

const SCHEMA: &str = concat!(
    r#"
CREATE TABLE logical_projects (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT,
    color       TEXT,
    is_archived INTEGER NOT NULL DEFAULT 0,
    privacy_mode TEXT NOT NULL DEFAULT 'default',
    sort_order  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL
);

CREATE TABLE project_match_rules (
    id         INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES logical_projects(id) ON DELETE CASCADE,
    rule_type  TEXT NOT NULL,
    target_field TEXT,
    pattern    TEXT NOT NULL,
    auto_apply INTEGER NOT NULL DEFAULT 0,
    UNIQUE (rule_type, pattern)
);

CREATE TABLE sessions (
    id                INTEGER PRIMARY KEY,
    agent             TEXT NOT NULL,
    source_session_id TEXT NOT NULL,
    parent_session_id TEXT,
    project_id        INTEGER REFERENCES logical_projects(id) ON DELETE SET NULL,
    assignment_source TEXT NOT NULL DEFAULT 'unassigned',
    suggested_project_id INTEGER REFERENCES logical_projects(id) ON DELETE SET NULL,
    suggestion_source TEXT,
    suggestion_reason TEXT,
    git_root          TEXT,
    git_remote        TEXT,
    detected_cwd      TEXT,
    git_branch        TEXT,
    agent_version     TEXT,
    title             TEXT,
    fallback_title    TEXT,
    user_note         TEXT,
    tags_json         TEXT NOT NULL DEFAULT '[]',
    ignored           INTEGER NOT NULL DEFAULT 0,
    first_active_at   TEXT,
    last_active_at    TEXT,
    UNIQUE (agent, source_session_id)
);

CREATE TABLE turns (
    id              INTEGER PRIMARY KEY,
    session_id      INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn_index      INTEGER NOT NULL,
    source_event_id TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    ended_at        TEXT NOT NULL,
    user_prompt     TEXT NOT NULL,
    UNIQUE (session_id, source_event_id),
    UNIQUE (session_id, turn_index)
);

-- Items inside a turn, in order: agent text (message) or tool call (tool).
CREATE TABLE turn_items (
    id              INTEGER PRIMARY KEY,
    session_id      INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn_id         INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    seq             INTEGER NOT NULL,
    source_event_id TEXT NOT NULL,
    timestamp       TEXT NOT NULL,
    item_type       TEXT NOT NULL,
    text            TEXT,
    tool_kind       TEXT,
    tool_name       TEXT,
    target          TEXT,
    summary         TEXT,
    is_error        INTEGER,
    added           INTEGER,
    removed         INTEGER,
    output_summary  TEXT,
    UNIQUE (session_id, source_event_id)
);
CREATE INDEX turn_items_by_turn ON turn_items (turn_id, seq);

-- Token usage of each model call, attached to the turn current when it was reported.
CREATE TABLE token_usage (
    id                 INTEGER PRIMARY KEY,
    session_id         INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    turn_id            INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    source_event_id    TEXT NOT NULL,
    timestamp          TEXT NOT NULL,
    model              TEXT,
    input_tokens       INTEGER NOT NULL,
    output_tokens      INTEGER NOT NULL,
    cache_read_tokens  INTEGER NOT NULL,
    cache_write_tokens INTEGER NOT NULL,
    UNIQUE (session_id, source_event_id)
);
CREATE INDEX token_usage_by_turn ON token_usage (turn_id);

CREATE TABLE session_daily_slices (
    id                    INTEGER PRIMARY KEY,
    session_id            INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    local_date            TEXT NOT NULL,
    start_turn_index      INTEGER NOT NULL,
    end_turn_index        INTEGER NOT NULL,
    slice_start_at        TEXT NOT NULL,
    slice_end_at          TEXT NOT NULL,
    active_minutes        INTEGER NOT NULL,
    active_intervals_json TEXT NOT NULL,
    files_modified_json   TEXT NOT NULL,
    ignore_state          TEXT NOT NULL DEFAULT 'inherit',
    project_id_override   INTEGER REFERENCES logical_projects(id) ON DELETE SET NULL,
    digest_json           TEXT,
    digest_input_hash     TEXT,
    -- Report language the cached digest is written in; a digest in another language is not reused.
    digest_language       TEXT,
    -- Last turn the digest covers, and how many incremental updates it has had since a full digest.
    digest_covered_turn   INTEGER,
    digest_update_count   INTEGER NOT NULL DEFAULT 0,
    -- Engine that wrote the cached digest (NULL for digests from before schema 12).
    digest_engine         TEXT,
    UNIQUE (session_id, local_date)
);
CREATE INDEX slices_by_date ON session_daily_slices (local_date);

-- Digests of the parts of a slice that was too long to summarize in one request.
CREATE TABLE digest_chunks (
    id               INTEGER PRIMARY KEY,
    slice_id         INTEGER NOT NULL REFERENCES session_daily_slices(id) ON DELETE CASCADE,
    start_turn_index INTEGER NOT NULL,
    end_turn_index   INTEGER NOT NULL,
    input_hash       TEXT NOT NULL,
    digest_json      TEXT NOT NULL,
    language         TEXT NOT NULL,
    engine           TEXT,
    UNIQUE (slice_id, start_turn_index)
);

-- A project's daily material condensed into task clusters (JSON) before the report is written.
CREATE TABLE project_briefs (
    id            INTEGER PRIMARY KEY,
    local_date    TEXT NOT NULL,
    project       TEXT NOT NULL,
    input_hash    TEXT NOT NULL,
    clusters_json TEXT NOT NULL,
    language      TEXT NOT NULL,
    engine        TEXT,
    UNIQUE (local_date, project)
);

CREATE TABLE source_files (
    id                INTEGER PRIMARY KEY,
    agent             TEXT NOT NULL,
    source_session_id TEXT NOT NULL,
    current_path      TEXT NOT NULL,
    size              INTEGER NOT NULL,
    mtime             INTEGER NOT NULL,
    byte_offset       INTEGER NOT NULL,
    head_hash         TEXT NOT NULL,
    error_count       INTEGER NOT NULL DEFAULT 0,
    unknown_count     INTEGER NOT NULL DEFAULT 0,
    last_error        TEXT,
    UNIQUE (agent, source_session_id)
);

CREATE TABLE reports (
    id                 INTEGER PRIMARY KEY,
    report_type        TEXT NOT NULL,
    period_key         TEXT NOT NULL,
    project_id         INTEGER REFERENCES logical_projects(id) ON DELETE CASCADE,
    status             TEXT NOT NULL DEFAULT 'draft',
    current_version_id INTEGER
);
CREATE UNIQUE INDEX reports_key ON reports (report_type, period_key, COALESCE(project_id, 0));

-- Report prompts. Built-in rows mirror the texts shipped with the program; the rest belong to the user.
-- `language` is the report language the prompt is written for.
CREATE TABLE prompts (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,
    language   TEXT NOT NULL,
    name       TEXT NOT NULL COLLATE NOCASE,
    body       TEXT NOT NULL,
    is_builtin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (kind, language, name)
);

-- The prompt the user chose as the default for a report kind and report language.
-- Without a row, the first built-in prompt for that language is the default.
CREATE TABLE prompt_defaults (
    kind      TEXT NOT NULL,
    language  TEXT NOT NULL,
    prompt_id INTEGER NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
    PRIMARY KEY (kind, language)
);

CREATE TABLE report_versions (
    id               INTEGER PRIMARY KEY,
    report_id        INTEGER NOT NULL REFERENCES reports(id) ON DELETE CASCADE,
    origin           TEXT NOT NULL,
    markdown_content TEXT NOT NULL,
    prompt_id        INTEGER REFERENCES prompts(id) ON DELETE SET NULL,
    prompt_name      TEXT,
    prompt_snapshot  TEXT,
    engine           TEXT,
    input_hash       TEXT,
    -- Per-project record of the inputs, for staleness checks and incremental updates.
    input_manifest_json TEXT,
    -- Report language the version is written in.
    language         TEXT NOT NULL,
    created_at       TEXT NOT NULL
);
"#,
    human_nodes_sql!()
);

/// Language of everything written before reports had a language setting.
const LEGACY_LANGUAGE: &str = "zh-Hans";

/// A migration from one schema version to the next.
struct Migration {
    from: i64,
    /// Run with foreign keys off, inside a transaction.
    sql: &'static str,
}

/// 8 -> 9: report language. Prompts get a language (the table is rebuilt for the new unique key)
/// and their defaults move to `prompt_defaults`; versions and caches record their language.
/// Everything existing was written in Chinese.
const MIGRATION_8_TO_9: &str = r#"
CREATE TABLE prompts_new (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,
    language   TEXT NOT NULL,
    name       TEXT NOT NULL COLLATE NOCASE,
    body       TEXT NOT NULL,
    is_builtin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (kind, language, name)
);
INSERT INTO prompts_new (id, kind, language, name, body, is_builtin, created_at, updated_at)
    SELECT id, kind, '{legacy}', name, body, is_builtin, created_at, updated_at FROM prompts;
CREATE TABLE prompt_defaults (
    kind      TEXT NOT NULL,
    language  TEXT NOT NULL,
    prompt_id INTEGER NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
    PRIMARY KEY (kind, language)
);
INSERT INTO prompt_defaults (kind, language, prompt_id)
    SELECT kind, '{legacy}', id FROM prompts WHERE is_default = 1;
DROP TABLE prompts;
ALTER TABLE prompts_new RENAME TO prompts;

ALTER TABLE report_versions ADD COLUMN language TEXT NOT NULL DEFAULT '{legacy}';
ALTER TABLE session_daily_slices ADD COLUMN digest_language TEXT;
UPDATE session_daily_slices SET digest_language = '{legacy}' WHERE digest_json IS NOT NULL;
ALTER TABLE digest_chunks ADD COLUMN language TEXT NOT NULL DEFAULT '{legacy}';
ALTER TABLE project_briefs ADD COLUMN language TEXT NOT NULL DEFAULT '{legacy}';
"#;

/// 9 -> 10: the human journal, as it was first created.
const MIGRATION_9_TO_10: &str = r#"
CREATE TABLE human_nodes (
    id          INTEGER PRIMARY KEY,
    period_kind TEXT NOT NULL,
    period_key  TEXT NOT NULL,
    project_id  INTEGER REFERENCES logical_projects(id) ON DELETE SET NULL,
    parent_id   INTEGER REFERENCES human_nodes(id) ON DELETE CASCADE,
    sort_key    TEXT NOT NULL,
    attach_json TEXT,
    body        TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX human_nodes_period ON human_nodes (period_kind, period_key);
CREATE INDEX human_nodes_parent ON human_nodes (parent_id);
"#;

/// 10 -> 11: a project note sits at the top or the bottom of its section. Every existing note
/// was shown at the bottom.
const MIGRATION_10_TO_11: &str = r#"
ALTER TABLE human_nodes ADD COLUMN position TEXT NOT NULL DEFAULT 'bottom';
"#;

/// 11 -> 12: digests, digest parts and project briefs record the engine that wrote them. Earlier
/// rows have none (shown as unknown).
const MIGRATION_11_TO_12: &str = r#"
ALTER TABLE session_daily_slices ADD COLUMN digest_engine TEXT;
ALTER TABLE digest_chunks ADD COLUMN engine TEXT;
ALTER TABLE project_briefs ADD COLUMN engine TEXT;
"#;

const MIGRATIONS: &[Migration] = &[
    Migration {
        from: 8,
        sql: MIGRATION_8_TO_9,
    },
    Migration {
        from: 9,
        sql: MIGRATION_9_TO_10,
    },
    Migration {
        from: 10,
        sql: MIGRATION_10_TO_11,
    },
    Migration {
        from: 11,
        sql: MIGRATION_11_TO_12,
    },
];

/// The schema version of an existing database; 0 for a new, empty one.
pub fn version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// Whether a database can be used by this build as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseState {
    /// At this build's schema, or no database yet (one is created when opened).
    Ready,
    /// An older schema that [`upgrade`] can bring to `to`.
    Outdated { from: i64, to: i64 },
    /// An older schema without a migration path.
    TooOld { found: i64 },
    /// Written by a newer Lantai.
    TooNew { found: i64 },
}

impl DatabaseState {
    /// The state of a database at schema `version` (0: none yet).
    pub fn of_version(version: i64) -> Self {
        let oldest = MIGRATIONS.first().map_or(SCHEMA_VERSION, |m| m.from);
        match version {
            0 => Self::Ready,
            v if v == SCHEMA_VERSION => Self::Ready,
            v if v > SCHEMA_VERSION => Self::TooNew { found: v },
            v if v < oldest => Self::TooOld { found: v },
            v => Self::Outdated {
                from: v,
                to: SCHEMA_VERSION,
            },
        }
    }
}

/// The error of opening a database that is not [`DatabaseState::Ready`]; callers find it with
/// `anyhow::Error::downcast_ref` to say what to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotReady(pub DatabaseState);

impl fmt::Display for NotReady {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            DatabaseState::Ready => write!(f, "the database is ready"),
            DatabaseState::Outdated { from, to } => write!(
                f,
                "the database was written by an older version of Lantai (schema {from}, this \
                 build uses {to}) and must be upgraded first"
            ),
            DatabaseState::TooOld { found } => write!(
                f,
                "the database was written by an early version of Lantai (schema {found}) that this \
                 build cannot upgrade; it was left untouched"
            ),
            DatabaseState::TooNew { found } => write!(
                f,
                "the database was written by a newer version of Lantai (schema {found}, this build \
                 knows {SCHEMA_VERSION}); update Lantai to open it"
            ),
        }
    }
}

impl std::error::Error for NotReady {}

/// Create the schema in a new database; accept one at this build's schema; refuse any other.
pub fn init(conn: &Connection) -> Result<()> {
    let current = version(conn)?;
    if current == 0 {
        conn.execute_batch(&format!(
            "BEGIN;\n{SCHEMA}\nPRAGMA user_version = {SCHEMA_VERSION};\nCOMMIT;"
        ))?;
        return Ok(());
    }
    match DatabaseState::of_version(current) {
        DatabaseState::Ready => Ok(()),
        state => Err(NotReady(state).into()),
    }
}

/// Bring an outdated database to this build's schema, one version at a time, each step in its own
/// transaction. A current database is left as it is; any other state is refused.
pub fn upgrade(conn: &Connection) -> Result<()> {
    let mut current = version(conn)?;
    match DatabaseState::of_version(current) {
        DatabaseState::Ready => return Ok(()),
        DatabaseState::Outdated { .. } => {}
        state => return Err(NotReady(state).into()),
    }
    while current < SCHEMA_VERSION {
        let Some(m) = MIGRATIONS.iter().find(|m| m.from == current) else {
            bail!("no migration from schema {current}");
        };
        migrate(conn, m)?;
        current += 1;
    }
    Ok(())
}

/// Apply one migration atomically. Foreign keys are off while tables are rebuilt, as SQLite
/// requires, and checked before committing.
fn migrate(conn: &Connection, m: &Migration) -> Result<()> {
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let result = (|| -> Result<()> {
        let sql = m.sql.replace("{legacy}", LEGACY_LANGUAGE);
        conn.execute_batch(&format!("BEGIN;\n{sql}"))?;
        let violations: i64 =
            conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })?;
        if violations > 0 {
            conn.execute_batch("ROLLBACK;")?;
            bail!(
                "migration from schema {} broke {violations} foreign key(s)",
                m.from
            );
        }
        conn.execute_batch(&format!("PRAGMA user_version = {};\nCOMMIT;", m.from + 1))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = conn.execute_batch("ROLLBACK;");
    }
    conn.pragma_update(None, "foreign_keys", "ON")?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA_V8: &str = include_str!("../tests/fixtures/schema_v8.sql");

    fn columns(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!(
                "SELECT name FROM pragma_table_info('{table}') ORDER BY name"
            ))
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn tables(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    fn v8_database() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", "ON").unwrap();
        conn.execute_batch(&format!("{SCHEMA_V8}\nPRAGMA user_version = 8;"))
            .unwrap();
        conn.execute_batch(
            "INSERT INTO prompts (id, kind, name, body, is_default, is_builtin, created_at, updated_at)
                 VALUES (1, 'daily', 'standard', 'builtin body', 0, 1, 't', 't'),
                        (2, 'daily', 'mine', 'user body', 1, 0, 't', 't'),
                        (3, 'weekly', 'standard', 'weekly body', 1, 1, 't', 't');
             INSERT INTO reports (id, report_type, period_key, status, current_version_id)
                 VALUES (1, 'daily', '2026-11-16', 'draft', 1);
             INSERT INTO report_versions (id, report_id, origin, markdown_content, prompt_id, created_at)
                 VALUES (1, 1, 'generated', '# x', 2, 't');
             INSERT INTO sessions (id, agent, source_session_id) VALUES (1, 'claude', 's1');
             INSERT INTO session_daily_slices (id, session_id, local_date, start_turn_index,
                     end_turn_index, slice_start_at, slice_end_at, active_minutes,
                     active_intervals_json, files_modified_json, digest_json, digest_input_hash)
                 VALUES (1, 1, '2026-11-16', 0, 1, 't', 't', 1, '[]', '[]', '{}', 'h'),
                        (2, 1, '2026-11-17', 2, 3, 't', 't', 1, '[]', '[]', NULL, NULL);
             INSERT INTO digest_chunks (slice_id, start_turn_index, end_turn_index, input_hash, digest_json)
                 VALUES (1, 0, 0, 'c', '{}');
             INSERT INTO project_briefs (local_date, project, input_hash, clusters_json)
                 VALUES ('2026-11-16', 'p', 'b', '{}');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn migrated_database_matches_a_fresh_one() {
        let migrated = v8_database();
        upgrade(&migrated).unwrap();
        let fresh = Connection::open_in_memory().unwrap();
        init(&fresh).unwrap();

        let version = |c: &Connection| -> i64 {
            c.query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(version(&migrated), SCHEMA_VERSION);
        assert_eq!(tables(&migrated), tables(&fresh));
        for table in tables(&fresh) {
            assert_eq!(
                columns(&migrated, &table),
                columns(&fresh, &table),
                "{table}"
            );
        }
        let fk: bool = migrated
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert!(fk, "foreign keys are switched back on");
    }

    #[test]
    fn migration_keeps_prompts_defaults_reports_and_caches_as_chinese() {
        let conn = v8_database();
        upgrade(&conn).unwrap();
        let one = |sql: &str| -> String { conn.query_row(sql, [], |r| r.get(0)).unwrap() };

        assert_eq!(
            one(
                "SELECT group_concat(id || ':' || language || ':' || name, ' ')
                 FROM (SELECT * FROM prompts ORDER BY id)"
            ),
            "1:zh-Hans:standard 2:zh-Hans:mine 3:zh-Hans:standard"
        );
        assert_eq!(
            one(
                "SELECT group_concat(kind || ':' || language || ':' || prompt_id, ' ')
                 FROM (SELECT * FROM prompt_defaults ORDER BY kind)"
            ),
            "daily:zh-Hans:2 weekly:zh-Hans:3"
        );
        // The version still points at its prompt after the table was rebuilt.
        assert_eq!(
            one("SELECT language || ':' || prompt_id FROM report_versions"),
            "zh-Hans:2"
        );
        assert_eq!(
            one(
                "SELECT group_concat(id || ':' || ifnull(digest_language, '-'), ' ')
                 FROM (SELECT * FROM session_daily_slices ORDER BY id)"
            ),
            "1:zh-Hans 2:-"
        );
        assert_eq!(one("SELECT language FROM digest_chunks"), "zh-Hans");
        assert_eq!(one("SELECT language FROM project_briefs"), "zh-Hans");
        // Caches from before schema 12 do not know their engine.
        assert_eq!(
            one(
                "SELECT ifnull(s.digest_engine, '-') || ifnull(c.engine, '-') || ifnull(b.engine, '-')
                 FROM session_daily_slices s, digest_chunks c, project_briefs b WHERE s.id = 1"
            ),
            "---"
        );

        // Deleting a prompt now clears its default through the foreign key.
        conn.execute("DELETE FROM prompts WHERE id = 2", [])
            .unwrap();
        assert_eq!(
            one("SELECT CAST(count(*) AS TEXT) FROM prompt_defaults WHERE kind = 'daily'"),
            "0"
        );
    }

    #[test]
    fn migration_puts_existing_notes_at_the_bottom() {
        let conn = v8_database();
        // Bring it to schema 10 and write a note there, then open it as usual.
        for m in MIGRATIONS.iter().filter(|m| m.from < 10) {
            migrate(&conn, m).unwrap();
        }
        conn.execute_batch(
            "INSERT INTO human_nodes (id, period_kind, period_key, sort_key, body, created_at, updated_at)
                 VALUES (1, 'day', '2026-11-21', 'V', 'kept', 't', 't');",
        )
        .unwrap();
        upgrade(&conn).unwrap();
        let (body, position): (String, String) = conn
            .query_row(
                "SELECT body, position FROM human_nodes WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((body.as_str(), position.as_str()), ("kept", "bottom"));
    }

    fn not_ready(err: anyhow::Error) -> DatabaseState {
        err.downcast_ref::<NotReady>().expect("a NotReady error").0
    }

    #[test]
    fn opening_never_migrates() {
        let conn = v8_database();
        assert_eq!(
            not_ready(init(&conn).unwrap_err()),
            DatabaseState::Outdated {
                from: 8,
                to: SCHEMA_VERSION
            }
        );
        assert_eq!(version(&conn).unwrap(), 8);
    }

    #[test]
    fn unknown_older_and_newer_versions_are_refused() {
        let old = Connection::open_in_memory().unwrap();
        old.execute_batch("CREATE TABLE t (x); PRAGMA user_version = 3;")
            .unwrap();
        assert_eq!(
            not_ready(init(&old).unwrap_err()),
            DatabaseState::TooOld { found: 3 }
        );
        assert_eq!(
            not_ready(upgrade(&old).unwrap_err()),
            DatabaseState::TooOld { found: 3 }
        );

        let newer = Connection::open_in_memory().unwrap();
        newer
            .execute_batch(&format!(
                "CREATE TABLE t (x); PRAGMA user_version = {};",
                SCHEMA_VERSION + 1
            ))
            .unwrap();
        let err = upgrade(&newer).unwrap_err();
        assert!(err.to_string().contains("update Lantai"), "{err}");
        assert_eq!(
            not_ready(err),
            DatabaseState::TooNew {
                found: SCHEMA_VERSION + 1
            }
        );
    }
}
