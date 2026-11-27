//! Report prompt management commands.
//!
//! Lists, previews, creates, copies, edits, renames, deletes, and changes defaults
//! for prompts stored in `prompts`. Defaults and previews are relative to the current
//! report language.

use aj_report::lang::Language;
use aj_report::prompt_library::{self, PromptVars};
use anyhow::Context;
use serde::Serialize;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptDto {
    pub id: i64,
    pub kind: String,
    /// The language the prompt is tuned for.
    pub language: String,
    pub name: String,
    pub body: String,
    /// Whether it is the default for its kind in the current report language.
    pub is_default: bool,
    pub is_builtin: bool,
    pub updated_at: String,
    pub assembled_preview: String,
}

/// What is sent for `body` when writing in `lang`.
fn preview(body: &str, vars: &PromptVars, lang: &Language) -> String {
    prompt_library::with_language(&prompt_library::assemble(body, vars, lang), lang)
}

/// Prompts of the current report language, or of every language with `all_languages`.
#[tauri::command(async)]
pub fn list_prompts(
    state: tauri::State<'_, AppState>,
    kind: Option<String>,
    all_languages: Option<bool>,
) -> CmdResult<Vec<PromptDto>> {
    let store = state.open_store()?;
    let lang = state.report_language(&[])?;
    let rows = if all_languages.unwrap_or(false) {
        store.list_prompts(kind.as_deref(), None)
    } else {
        prompt_library::offered(&store, kind.as_deref(), lang)
    }?;
    let defaults = prompt_library::default_ids(&store, lang)?;
    Ok(rows
        .into_iter()
        .map(|p| PromptDto {
            assembled_preview: preview(&p.body, &PromptVars::default(), lang),
            is_default: defaults.contains(&p.id),
            id: p.id,
            kind: p.kind,
            language: p.language,
            name: p.name,
            body: p.body,
            is_builtin: p.is_builtin,
            updated_at: p.updated_at,
        })
        .collect())
}

#[tauri::command(async)]
pub fn preview_prompt(
    state: tauri::State<'_, AppState>,
    id: i64,
    date: Option<String>,
    project: Option<String>,
) -> CmdResult<String> {
    let store = state.open_store()?;
    let lang = state.report_language(&[])?;
    let row = store
        .prompt(id)?
        .with_context(|| format!("no prompt with ID {id}"))?;
    Ok(preview(
        &row.body,
        &PromptVars {
            date: date.as_deref(),
            period: date.as_deref(),
            project: project.as_deref(),
        },
        lang,
    ))
}

/// Create a prompt for the current report language.
#[tauri::command(async)]
pub fn create_prompt(
    state: tauri::State<'_, AppState>,
    kind: String,
    name: String,
    body: String,
) -> CmdResult<i64> {
    let store = state.open_store()?;
    let lang = state.report_language(&[])?;
    store
        .create_prompt(&kind, lang.tag, &name, &body)
        .map_err(CmdError::from)
}

#[tauri::command(async)]
pub fn update_prompt(state: tauri::State<'_, AppState>, id: i64, body: String) -> CmdResult<()> {
    let store = state.open_store()?;
    store.update_prompt_body(id, &body).map_err(CmdError::from)
}

#[tauri::command(async)]
pub fn rename_prompt(
    state: tauri::State<'_, AppState>,
    id: i64,
    new_name: String,
) -> CmdResult<()> {
    let store = state.open_store()?;
    store.rename_prompt(id, &new_name).map_err(CmdError::from)
}

#[tauri::command(async)]
pub fn delete_prompt(state: tauri::State<'_, AppState>, id: i64) -> CmdResult<()> {
    let mut store = state.open_store()?;
    store.delete_prompt(id).map_err(CmdError::from)
}

/// Make a prompt (of any language) the default for its kind in the current report language.
#[tauri::command(async)]
pub fn set_default_prompt(state: tauri::State<'_, AppState>, id: i64) -> CmdResult<()> {
    let mut store = state.open_store()?;
    let lang = state.report_language(&[])?;
    store
        .set_default_prompt(id, lang.tag)
        .map_err(CmdError::from)
}
