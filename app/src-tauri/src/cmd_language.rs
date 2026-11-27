//! Language settings: the interface language and the report language (see
//! docs/SPEC.md, "Languages"). The interface texts themselves live in the frontend.

use aj_report::lang::LANGUAGES;
use serde::Serialize;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportLanguageDto {
    pub tag: String,
    pub native_name: String,
    /// Whether it has its own tuned prompts (otherwise it borrows the English ones).
    pub tuned: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageSettingsDto {
    /// `"auto"` (follow the system) or a language tag.
    pub ui_language: String,
    pub report_language: String,
    pub report_languages: Vec<ReportLanguageDto>,
}

/// The language settings. On the first call of an installation that has not chosen a
/// report language yet, the initial one is picked (from `system_locales`, the system's
/// preferred languages most preferred first) and saved.
#[tauri::command(async)]
pub fn get_language_settings(
    state: tauri::State<'_, AppState>,
    system_locales: Option<Vec<String>>,
) -> CmdResult<LanguageSettingsDto> {
    let report = state.report_language(&system_locales.unwrap_or_default())?;
    let cfg = state.load_config()?;
    Ok(LanguageSettingsDto {
        ui_language: cfg.ui_language().to_string(),
        report_language: report.tag.to_string(),
        report_languages: LANGUAGES
            .iter()
            .map(|l| ReportLanguageDto {
                tag: l.tag.to_string(),
                native_name: l.native_name.to_string(),
                tuned: l.is_tuned(),
            })
            .collect(),
    })
}

#[tauri::command(async)]
pub fn set_ui_language(state: tauri::State<'_, AppState>, language: String) -> CmdResult<()> {
    state
        .modify_config(|c| c.set_ui_language(&language))
        .map(|_| ())
        .map_err(CmdError::from)
}

#[tauri::command(async)]
pub fn set_report_language(state: tauri::State<'_, AppState>, language: String) -> CmdResult<()> {
    state
        .modify_config(|c| c.set_report_language(&language))
        .map(|_| ())
        .map_err(CmdError::from)
}
