//! OpenCode session adapter (OpenCode 1.x storage).
//!
//! All knowledge about the OpenCode format lives in this crate only. Format findings come from
//! inspecting OpenCode 1.18 in 2026-11; see `docs/ADAPTERS.md`.
//!
//! Unlike the file-per-session agents, OpenCode keeps every session in one SQLite database
//! (`opencode.db`, WAL mode). The adapter copies it into a private cache before opening it, lists
//! one [`SourceFile`] per session with a content [`SourceFile::revision`], and reparses a whole
//! session whenever its revision changes (events are deduplicated by ID downstream).

use std::path::PathBuf;
use std::sync::Mutex;

use aj_adapter::{Compatibility, ParseOutput, SourceAdapter, SourceFile, SourceProbe};
use aj_ir::AgentKind;
use anyhow::Result;
use rusqlite::Connection;

mod compat;
mod db;
mod map;
mod snapshot;

pub struct OpenCodeAdapter {
    /// Directory holding `opencode.db`, normally `~/.local/share/opencode`.
    root: PathBuf,
    /// Lantai-owned directory the database is copied into.
    cache_dir: PathBuf,
    /// Connection to the copy taken by the last `discover`, reused by `parse`.
    conn: Mutex<Option<Connection>>,
}

impl OpenCodeAdapter {
    pub fn new(root: PathBuf, cache_dir: PathBuf) -> Self {
        Self {
            root,
            cache_dir,
            conn: Mutex::new(None),
        }
    }

    fn db_path(&self) -> PathBuf {
        self.root.join(snapshot::DB_FILE)
    }

    /// Copy the database (if it changed) and open the copy.
    fn refresh(&self) -> Result<Option<Connection>> {
        match snapshot::take(&self.root, &self.cache_dir)? {
            Some(copy) => Ok(Some(db::open(&copy)?)),
            None => Ok(None),
        }
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<Option<T>> {
        let mut guard = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = self.refresh()?;
        }
        match guard.as_ref() {
            Some(conn) => f(conn).map(Some),
            None => Ok(None),
        }
    }
}

impl SourceAdapter for OpenCodeAdapter {
    fn agent(&self) -> AgentKind {
        AgentKind::OpenCode
    }

    fn discover(&self) -> Result<Vec<SourceFile>> {
        // Every scan starts from a fresh copy.
        *self.conn.lock().unwrap_or_else(|e| e.into_inner()) = self.refresh()?;
        let path = self.db_path();
        let sessions = self.with_conn(db::sessions)?.unwrap_or_default();
        Ok(sessions
            .into_iter()
            .map(|s| SourceFile {
                path: path.clone(),
                size: 0,
                mtime: s.updated_ms / 1000,
                revision: Some(s.revision),
                source_session_id: s.id,
            })
            .collect())
    }

    fn parse(&self, file: &SourceFile, _from_offset: u64) -> Result<ParseOutput> {
        let mut out = ParseOutput::default();
        self.with_conn(|conn| {
            let Some(session) = db::session(conn, &file.source_session_id)? else {
                return Ok(());
            };
            let messages = db::messages(conn, &session.id)?;
            let parts = db::parts(conn, &session.id)?;
            map::session_events(&session, &messages, &parts, &mut out);
            Ok(())
        })?;
        // Always a whole-session parse; there is no byte offset to resume from.
        out.new_offset = 0;
        Ok(out)
    }

    fn probe(&self) -> Result<SourceProbe> {
        *self.conn.lock().unwrap_or_else(|e| e.into_inner()) = self.refresh()?;
        Ok(self
            .with_conn(|conn| Ok(compat::assess(&db::shape(conn)?)))?
            // No database: nothing to judge.
            .unwrap_or(SourceProbe {
                compatibility: Compatibility::Unchecked,
                ..Default::default()
            }))
    }
}
