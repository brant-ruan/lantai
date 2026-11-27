//! The database check before the interface loads, and the upgrade the person agrees to
//! (docs/SPEC.md, "Upgrading the database"). The front end shows the dialog; the backup and the
//! migration are `aj_backup::upgrade`.

use std::path::PathBuf;

use aj_backup::BACKUPS_DIR;
use aj_report::prompt_library;
use aj_store::DatabaseState;
use chrono::Utc;
use serde::Serialize;

use crate::error::CmdResult;
use crate::pickers;
use crate::state::{data_files, AppState, DB_FILE};

#[derive(Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum DatabaseStateDto {
    Ready,
    Outdated { from: i64, to: i64 },
    TooOld { found: i64 },
    TooNew { found: i64, known: i64 },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStatusDto {
    pub database: DatabaseStateDto,
    /// The interface language setting, read from `config.toml` alone, so the dialog can be shown
    /// in the right language without opening the database.
    pub ui_language: String,
    /// Where the backup goes unless another folder is chosen.
    pub backup_dir: String,
    pub data_dir: String,
    /// Where a newer Lantai can be downloaded.
    pub release_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradedDto {
    pub from: i64,
    pub to: i64,
    pub backup: String,
}

/// Run once the database is at this build's schema (a new one is created here): built-in
/// prompts follow this build.
fn prepare(state: &AppState) -> anyhow::Result<()> {
    prompt_library::sync_builtins(&mut state.open_store()?)
}

/// What the front end needs before loading: whether the database can be used as it is. A ready
/// database is prepared on the way.
#[tauri::command(async)]
pub fn database_status(state: tauri::State<'_, AppState>) -> CmdResult<DatabaseStatusDto> {
    let database = match aj_store::database_state(&state.data_dir.join(DB_FILE))? {
        DatabaseState::Ready => {
            prepare(&state)?;
            DatabaseStateDto::Ready
        }
        DatabaseState::Outdated { from, to } => DatabaseStateDto::Outdated { from, to },
        DatabaseState::TooOld { found } => DatabaseStateDto::TooOld { found },
        DatabaseState::TooNew { found } => DatabaseStateDto::TooNew {
            found,
            known: aj_store::SCHEMA_VERSION,
        },
    };
    let ui_language = state
        .load_config()
        .map(|c| c.ui_language().to_string())
        .unwrap_or_else(|_| "auto".to_string());
    Ok(DatabaseStatusDto {
        database,
        ui_language,
        backup_dir: state
            .data_dir
            .join(BACKUPS_DIR)
            .to_string_lossy()
            .into_owned(),
        data_dir: state.data_dir.to_string_lossy().into_owned(),
        release_url: format!(
            "https://github.com/{}/releases/latest",
            aj_update::REPOSITORY
        ),
    })
}

/// Back up the data into `backup_dir` (the default when absent), then upgrade the database.
#[tauri::command(async)]
pub fn upgrade_database(
    state: tauri::State<'_, AppState>,
    backup_dir: Option<String>,
) -> CmdResult<UpgradedDto> {
    let backup_dir = backup_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| state.data_dir.join(BACKUPS_DIR));
    let up = aj_backup::upgrade(
        &data_files(&state.data_dir),
        &backup_dir,
        env!("CARGO_PKG_VERSION"),
        Utc::now(),
    )?;
    prepare(&state)?;
    Ok(UpgradedDto {
        from: up.from,
        to: up.to,
        backup: up.backup.to_string_lossy().into_owned(),
    })
}

/// Ask for a folder to keep the backup in; `None` when the user cancels.
#[tauri::command]
pub async fn choose_backup_folder(prompt: String, start: String) -> CmdResult<Option<String>> {
    let start = pickers::existing_dir(&PathBuf::from(start));
    Ok(pickers::choose_folder(prompt, start)
        .await?
        .map(|p| p.to_string_lossy().into_owned()))
}

/// Show a file in Finder.
#[tauri::command(async)]
pub fn reveal_in_finder(path: String) -> CmdResult<()> {
    let path = PathBuf::from(path);
    if !path.is_absolute() || !path.exists() {
        return Err(format!("{} does not exist", path.display()).into());
    }
    let status = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(&path)
        .status()
        .map_err(|e| format!("failed to show {}: {e}", path.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to show {}: {status}", path.display()).into())
    }
}

/// Quit Lantai (the dialog's way out when the database cannot be used).
#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}
