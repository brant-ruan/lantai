//! Report prompt management commands.
//!
//! Lists, previews, creates, copies, edits, renames, deletes, and changes defaults
//! for prompts stored in `prompts`. Defaults and previews are relative to the current
//! report language.

use aj_report::lang::Language;
use aj_report::prompt_library::{self, PromptVars, KINDS};
use aj_store::{PromptRow, Store};
use anyhow::Context;
use serde::Serialize;

use crate::state::{cmd_err, AppState};

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

/// The prompts offered for `lang`: its own, plus the built-ins it borrows.
pub fn prompts_for(
    store: &Store,
    kind: Option<&str>,
    lang: &Language,
) -> anyhow::Result<Vec<PromptRow>> {
    let mut rows = store.list_prompts(kind, Some(lang.tag))?;
    if lang.builtin_prompt_language() != lang.tag {
        rows.extend(store.list_prompts(kind, Some(lang.builtin_prompt_language()))?);
    }
    Ok(rows)
}

/// IDs of the default prompt of every kind in `lang`.
pub fn default_ids(store: &Store, lang: &Language) -> anyhow::Result<Vec<i64>> {
    KINDS
        .iter()
        .map(|k| Ok(prompt_library::default_prompt(store, k, lang)?.id))
        .collect()
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
) -> Result<Vec<PromptDto>, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let lang = state.report_language(&[]).map_err(cmd_err)?;
    let rows = if all_languages.unwrap_or(false) {
        store.list_prompts(kind.as_deref(), None)
    } else {
        prompts_for(&store, kind.as_deref(), lang)
    }
    .map_err(cmd_err)?;
    let defaults = default_ids(&store, lang).map_err(cmd_err)?;
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
) -> Result<String, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let lang = state.report_language(&[]).map_err(cmd_err)?;
    let row = store
        .prompt(id)
        .map_err(cmd_err)?
        .with_context(|| format!("no prompt with ID {id}"))
        .map_err(cmd_err)?;
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
) -> Result<i64, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let lang = state.report_language(&[]).map_err(cmd_err)?;
    store
        .create_prompt(&kind, lang.tag, &name, &body)
        .map_err(cmd_err)
}

#[tauri::command(async)]
pub fn update_prompt(
    state: tauri::State<'_, AppState>,
    id: i64,
    body: String,
) -> Result<(), String> {
    let store = state.open_store().map_err(cmd_err)?;
    store.update_prompt_body(id, &body).map_err(cmd_err)
}

#[tauri::command(async)]
pub fn rename_prompt(
    state: tauri::State<'_, AppState>,
    id: i64,
    new_name: String,
) -> Result<(), String> {
    let store = state.open_store().map_err(cmd_err)?;
    store.rename_prompt(id, &new_name).map_err(cmd_err)
}

#[tauri::command(async)]
pub fn delete_prompt(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    let mut store = state.open_store().map_err(cmd_err)?;
    store.delete_prompt(id).map_err(cmd_err)
}

/// Make a prompt (of any language) the default for its kind in the current report language.
#[tauri::command(async)]
pub fn set_default_prompt(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    let mut store = state.open_store().map_err(cmd_err)?;
    let lang = state.report_language(&[]).map_err(cmd_err)?;
    store.set_default_prompt(id, lang.tag).map_err(cmd_err)
}
