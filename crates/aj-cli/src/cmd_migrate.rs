//! `aj migrate`: back up and upgrade a database left by an older Lantai, and what other commands
//! say when the database cannot be opened as it is (docs/SPEC.md, "Upgrading the database").

use std::path::{Path, PathBuf};

use aj_backup::{DataFiles, BACKUPS_DIR};
use aj_core::data_files::{CONFIG_FILES, DATABASE};
use aj_store::{DatabaseState, NotReady, SCHEMA_VERSION};
use anyhow::{anyhow, Result};
use chrono::Utc;

fn data_files(dir: &Path) -> DataFiles<'_> {
    DataFiles {
        dir,
        database: DATABASE,
        files: CONFIG_FILES,
    }
}

pub(crate) fn cmd_migrate(data_dir: &Path, backup_dir: Option<PathBuf>) -> Result<()> {
    match aj_store::database_state(&data_dir.join(DATABASE))? {
        DatabaseState::Ready => {
            println!("The database is up to date (schema {SCHEMA_VERSION}).");
            return Ok(());
        }
        DatabaseState::Outdated { from, to } => {
            println!("Backing up the database, then upgrading it from schema {from} to {to}.")
        }
        state => return Err(NotReady(state).into()),
    }
    let backup_dir = backup_dir.unwrap_or_else(|| data_dir.join(BACKUPS_DIR));
    let up = aj_backup::upgrade(
        &data_files(data_dir),
        &backup_dir,
        env!("CARGO_PKG_VERSION"),
        Utc::now(),
    )?;
    println!("Backup: {}", up.backup.display());
    println!("The database is now at schema {}.", up.to);
    Ok(())
}

/// Add what to do to the error of opening a database that is not ready; other errors pass as
/// they are.
pub(crate) fn explain_open_error(err: anyhow::Error) -> anyhow::Error {
    match err.downcast_ref::<NotReady>() {
        Some(NotReady(DatabaseState::Outdated { .. })) => anyhow!(
            "{err}.\nRun `aj migrate` to back it up and upgrade it, or open the Lantai app, which \
             offers the same."
        ),
        _ => err,
    }
}
