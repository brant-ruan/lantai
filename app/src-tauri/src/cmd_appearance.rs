//! Appearance commands: list the app icons and switch between them.

use serde::Serialize;

use crate::app_icon::{self, BundleSync};
use crate::error::CmdResult;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppIconDto {
    pub id: String,
    pub label: String,
    pub current: bool,
}

#[tauri::command]
pub fn list_app_icons(state: tauri::State<'_, AppState>) -> Vec<AppIconDto> {
    let current = app_icon::load_pref(&state.data_dir).id;
    app_icon::ICONS
        .iter()
        .map(|i| AppIconDto {
            id: i.id.to_string(),
            label: i.label.to_string(),
            current: i.id == current,
        })
        .collect()
}

#[tauri::command]
pub fn set_app_icon(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: String,
) -> CmdResult<()> {
    let icon = app_icon::find(&id).ok_or_else(|| format!("unknown app icon: {id}"))?;
    app_icon::save_pref(&state.data_dir, icon)?;
    app_icon::apply(&app, icon, BundleSync::Always);
    Ok(())
}
