//! Thin Tauri commands over `aj_backup`: save the database and the configuration to a zip
//! file, look into such a file, and restore from it. API keys stay in the keychain and are
//! not part of a backup.

use std::path::{Path, PathBuf};

use aj_backup::Manifest;
use aj_report::prompt_library;
use chrono::{Local, Utc};
use serde::Serialize;

use crate::app_icon::{self, BundleSync};
use crate::pickers;
use crate::state::{cmd_err, data_files, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfoDto {
    pub path: String,
    pub app_version: String,
    pub schema_version: i64,
    /// RFC 3339, UTC.
    pub created_at: String,
    /// The configuration files in the backup.
    pub files: Vec<String>,
}

impl BackupInfoDto {
    fn new(path: &Path, m: Manifest) -> Self {
        Self {
            path: path.to_string_lossy().into_owned(),
            app_version: m.app_version,
            schema_version: m.schema_version,
            created_at: m.created_at,
            files: m.files,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoredDto {
    pub backup: BackupInfoDto,
    /// Where the data that was replaced is kept.
    pub previous: String,
}

/// Where pickers start: the Downloads folder, else the home folder.
fn start_dir() -> Option<PathBuf> {
    dirs::download_dir().or_else(dirs::home_dir)
}

/// Ask where to save and write a backup there; `None` when the user cancels.
#[tauri::command]
pub async fn export_backup(
    state: tauri::State<'_, AppState>,
    prompt: String,
) -> Result<Option<BackupInfoDto>, String> {
    let name = format!("lantai-backup-{}.zip", Local::now().format("%Y-%m-%d"));
    let Some(mut path) = pickers::choose_save_file(prompt, name, start_dir()).await? else {
        return Ok(None);
    };
    if path
        .extension()
        .is_none_or(|e| !e.eq_ignore_ascii_case("zip"))
    {
        path.as_mut_os_string().push(".zip");
    }
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        aj_backup::export(
            &data_files(&data_dir),
            &path,
            env!("CARGO_PKG_VERSION"),
            Utc::now(),
        )
        .map(|m| Some(BackupInfoDto::new(&path, m)))
        .map_err(cmd_err)
    })
    .await
    .map_err(|e| format!("backup task failed: {e}"))?
}

/// Ask for a backup file and describe it, so the user can confirm before restoring.
#[tauri::command]
pub async fn pick_backup(prompt: String) -> Result<Option<BackupInfoDto>, String> {
    let picked =
        pickers::choose_file(prompt, "public.zip-archive".to_string(), start_dir()).await?;
    let Some(path) = picked else {
        return Ok(None);
    };
    tauri::async_runtime::spawn_blocking(move || {
        aj_backup::read_manifest(&path)
            .map(|m| Some(BackupInfoDto::new(&path, m)))
            .map_err(cmd_err)
    })
    .await
    .map_err(|e| format!("backup task failed: {e}"))?
}

/// Replace the database and the configuration with the backup at `path`. The interface
/// reloads afterwards, since everything it shows may have changed.
#[tauri::command(async)]
pub fn restore_backup(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<RestoredDto, String> {
    let running = state
        .report_jobs
        .lock()
        .map_err(|_| "report jobs lock poisoned".to_string())?
        .len();
    if running > 0 {
        return Err(format!(
            "{running} report generation(s) are running; wait for them to finish or cancel them, then restore"
        ));
    }
    // No settings change may land in the middle of the restore.
    let _guard = state
        .config_lock
        .lock()
        .map_err(|_| "config lock poisoned".to_string())?;
    let path = PathBuf::from(path);
    let restored =
        aj_backup::restore(&data_files(&state.data_dir), &path, Utc::now()).map_err(cmd_err)?;
    // As at startup: built-in prompts follow this build.
    prompt_library::sync_builtins(&mut state.open_store().map_err(cmd_err)?).map_err(cmd_err)?;
    app_icon::apply(
        &app,
        app_icon::load_pref(&state.data_dir),
        BundleSync::Always,
    );
    Ok(RestoredDto {
        backup: BackupInfoDto::new(&path, restored.manifest),
        previous: restored.previous.to_string_lossy().into_owned(),
    })
}
