//! Whole-database copies for backup and restore: a consistent snapshot of the open database
//! into a new file, and replacing its whole content with another database file while other
//! connections may be open (SQLite's online backup, so they see either the old or the new
//! content, never a mix).

use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags};

use crate::{schema, Store};

/// How many times a restore waits 100 ms for other connections to release the database.
const RESTORE_ATTEMPTS: usize = 150;

/// The schema version this build writes.
pub const SCHEMA_VERSION: i64 = schema::SCHEMA_VERSION;

/// The schema version of a database file, read without migrating it.
pub fn file_schema_version(path: &Path) -> Result<i64> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("failed to open database {}", path.display()))?;
    schema::version(&conn)
}

impl Store {
    /// Write a consistent copy of the database to `dest`, which must not exist yet.
    pub fn snapshot_to(&self, dest: &Path) -> Result<()> {
        if dest.exists() {
            bail!("{} already exists", dest.display());
        }
        self.conn
            .execute("VACUUM INTO ?1", [dest.to_string_lossy()])
            .with_context(|| format!("failed to copy the database to {}", dest.display()))?;
        Ok(())
    }

    /// Replace the whole database with the content of the database file `src`, which should
    /// already be at this build's schema (open it with [`Store::open`] first to migrate it).
    pub fn replace_with(&mut self, src: &Path) -> Result<()> {
        let version = file_schema_version(src)?;
        if version != SCHEMA_VERSION {
            bail!("the database to restore is at schema {version}, expected {SCHEMA_VERSION}");
        }
        let source = Connection::open_with_flags(src, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("failed to open database {}", src.display()))?;
        let backup = Backup::new(&source, &mut self.conn)?;
        // All pages in one step, so other connections never see a half-restored database;
        // wait while one of them holds a lock.
        for _ in 0..RESTORE_ATTEMPTS {
            match backup.step(-1).context("failed to restore the database")? {
                StepResult::Done => return Ok(()),
                StepResult::More | StepResult::Busy | StepResult::Locked => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                _ => bail!("failed to restore the database"),
            }
        }
        bail!("the database stayed busy; close other Lantai windows or commands and try again")
    }

    /// Fail unless SQLite finds the database intact.
    pub fn check_integrity(&self) -> Result<()> {
        let result: String = self
            .conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if result != "ok" {
            bail!("the database is damaged: {result}");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_and_replace_round_trip() {
        let dir = std::env::temp_dir().join(format!("aj-store-restore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut live = Store::open(&dir.join("live.sqlite")).unwrap();
        live.create_project("kept", None).unwrap();
        let snapshot = dir.join("snapshot.sqlite");
        live.snapshot_to(&snapshot).unwrap();
        assert!(live.snapshot_to(&snapshot).is_err(), "never overwrites");
        assert_eq!(file_schema_version(&snapshot).unwrap(), SCHEMA_VERSION);

        live.create_project("dropped", None).unwrap();
        // Another connection stays usable across the restore and sees the new content.
        let other = Store::open(&dir.join("live.sqlite")).unwrap();
        live.replace_with(&snapshot).unwrap();
        live.check_integrity().unwrap();
        let names = |s: &Store| -> Vec<String> {
            s.list_projects()
                .unwrap()
                .into_iter()
                .map(|p| p.project.name)
                .collect()
        };
        assert_eq!(names(&live), vec!["kept".to_string()]);
        assert_eq!(names(&other), vec!["kept".to_string()]);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
