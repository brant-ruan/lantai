//! Queries against the (copied) OpenCode 1.x database.
//!
//! Only the `session`, `message` and `part` tables are read; their `data` columns are JSON text
//! that `map` interprets. OpenCode 2.x tables (`session_v2`, `session_message`) are not read yet.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

/// One `session` row plus the content version that tells whether it changed.
#[derive(Debug, Clone)]
pub(crate) struct SessionRow {
    pub id: String,
    pub parent_id: Option<String>,
    pub directory: String,
    pub title: String,
    pub version: String,
    /// Changes whenever the session, any of its messages or any of its parts is written.
    pub revision: String,
    /// Latest write to the session or its content (Unix milliseconds).
    pub updated_ms: i64,
}

/// A `message` or `part` row: its ID, owning message (parts only) and JSON payload.
#[derive(Debug, Clone)]
pub(crate) struct DataRow {
    pub id: String,
    pub message_id: String,
    pub data: String,
}

pub(crate) fn open(path: &Path) -> Result<Connection> {
    // The copy belongs to Lantai, so SQLite may create its `-shm` next to it.
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open {}", path.display()))
}

/// Whether the 1.x tables exist (a store created by OpenCode 2.x may not have them).
fn has_v1_tables(conn: &Connection) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name IN ('session', 'message', 'part')",
        [],
        |r| r.get(0),
    )?;
    Ok(n == 3)
}

const SESSION_SELECT: &str = "
    WITH m AS (SELECT session_id, count(*) AS n, max(time_updated) AS t FROM message GROUP BY session_id),
         p AS (SELECT session_id, count(*) AS n, max(time_updated) AS t FROM part GROUP BY session_id)
    SELECT s.id, s.parent_id, s.directory, s.title, s.version, s.time_updated,
           coalesce(m.n, 0), coalesce(m.t, 0), coalesce(p.n, 0), coalesce(p.t, 0)
    FROM session s
    LEFT JOIN m ON m.session_id = s.id
    LEFT JOIN p ON p.session_id = s.id";

fn session_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    let updated: i64 = r.get(5)?;
    let (messages, message_t): (i64, i64) = (r.get(6)?, r.get(7)?);
    let (parts, part_t): (i64, i64) = (r.get(8)?, r.get(9)?);
    Ok(SessionRow {
        id: r.get(0)?,
        parent_id: r.get(1)?,
        directory: r.get(2)?,
        title: r.get(3)?,
        version: r.get(4)?,
        revision: format!("{updated}:{messages}:{parts}:{message_t}:{part_t}"),
        updated_ms: updated.max(message_t).max(part_t),
    })
}

pub(crate) fn sessions(conn: &Connection) -> Result<Vec<SessionRow>> {
    if !has_v1_tables(conn)? {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(&format!("{SESSION_SELECT} ORDER BY s.id"))?;
    let rows = stmt.query_map([], session_row)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub(crate) fn session(conn: &Connection, id: &str) -> Result<Option<SessionRow>> {
    if !has_v1_tables(conn)? {
        return Ok(None);
    }
    let mut stmt = conn.prepare(&format!("{SESSION_SELECT} WHERE s.id = ?1"))?;
    Ok(stmt.query_row([id], session_row).optional()?)
}

/// Messages of a session in conversation order.
pub(crate) fn messages(conn: &Connection, session_id: &str) -> Result<Vec<DataRow>> {
    let mut stmt = conn
        .prepare("SELECT id, data FROM message WHERE session_id = ?1 ORDER BY time_created, id")?;
    let rows = stmt.query_map([session_id], |r| {
        Ok(DataRow {
            id: r.get(0)?,
            message_id: String::new(),
            data: r.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Parts of a session; IDs are time ordered, so ordering by ID keeps each message's parts in order.
pub(crate) fn parts(conn: &Connection, session_id: &str) -> Result<Vec<DataRow>> {
    let mut stmt =
        conn.prepare("SELECT id, message_id, data FROM part WHERE session_id = ?1 ORDER BY id")?;
    let rows = stmt.query_map([session_id], |r| {
        Ok(DataRow {
            id: r.get(0)?,
            message_id: r.get(1)?,
            data: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// What the database holds, for the compatibility check.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct StoreShape {
    /// The 1.x tables (`session`, `message`, `part`) exist.
    pub v1: bool,
    /// Distinct `session.version` values in the 1.x tables.
    pub versions: Vec<String>,
    /// The 2.x session table exists.
    pub v2: bool,
    /// Sessions that exist only in the 2.x table (not readable yet).
    pub v2_only: u64,
}

pub(crate) fn shape(conn: &Connection) -> Result<StoreShape> {
    let mut shape = StoreShape {
        v1: has_v1_tables(conn)?,
        v2: table_exists(conn, "session_v2")?,
        ..Default::default()
    };
    if shape.v1 {
        let mut stmt = conn.prepare("SELECT DISTINCT version FROM session")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        shape.versions = rows.collect::<rusqlite::Result<_>>()?;
    }
    if shape.v2 {
        let sql = if shape.v1 {
            "SELECT count(*) FROM session_v2 WHERE id NOT IN (SELECT id FROM session)"
        } else {
            "SELECT count(*) FROM session_v2"
        };
        shape.v2_only = conn.query_row(sql, [], |r| r.get::<_, i64>(0))? as u64;
    }
    Ok(shape)
}
