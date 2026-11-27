//! Project, rule, and session assignment commands.
//!
//! Wraps `aj-store` and `aj-core::project` so users can create projects, manage
//! rules, accept suggestions, and assign sessions or slices from the UI.

use std::collections::HashMap;

use aj_adapter::SourceAdapter;
use aj_core::project::matcher::RULE_REGEX;
use aj_core::project::{
    evaluate_all, uncovered_path, validate_rule, validate_rules, RuleInput, ValidRule,
};
use aj_core::scan::normalized_cwd;
use aj_store::NewRule;
use serde::Serialize;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub archived: bool,
    pub slices: u64,
    pub suggested: u64,
    pub rules: u64,
}

impl From<&aj_store::ProjectSummary> for ProjectDto {
    fn from(p: &aj_store::ProjectSummary) -> Self {
        Self {
            id: p.project.id,
            name: p.project.name.clone(),
            description: p.project.description.clone(),
            archived: p.project.archived,
            slices: p.slices,
            suggested: p.suggested,
            rules: p.rules,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteImpactDto {
    pub sessions: u64,
    pub reports: u64,
    pub rules: u64,
    pub notes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleDto {
    pub id: i64,
    pub project_id: i64,
    pub project_name: String,
    pub rule_type: String,
    pub target_field: Option<String>,
    pub pattern: String,
    pub auto_apply: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateDto {
    pub root: String,
    pub git_remote: Option<String>,
    pub sessions: u64,
    pub last_active_at: Option<String>,
    pub sample_titles: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsOverviewDto {
    pub projects: Vec<ProjectDto>,
    pub rules: Vec<RuleDto>,
    pub candidates: Vec<CandidateDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignOutcomeDto {
    pub uncovered_path: Option<String>,
}

fn reevaluate(state: &AppState, store: &aj_store::Store) -> CmdResult<()> {
    let adapters = state.adapters()?;
    let refs: Vec<&dyn SourceAdapter> = adapters.iter().map(|a| a.as_ref()).collect();
    evaluate_all(store, &|s| normalized_cwd(&refs, s))?;
    Ok(())
}

#[tauri::command(async)]
pub fn list_projects_and_rules(
    state: tauri::State<'_, AppState>,
) -> CmdResult<ProjectsOverviewDto> {
    let store = state.open_store()?;
    let projects = store.project_summaries()?;
    let names: HashMap<i64, String> = projects
        .iter()
        .map(|p| (p.project.id, p.project.name.clone()))
        .collect();
    let rules = store
        .rules()?
        .into_iter()
        .map(|r| RuleDto {
            id: r.id,
            project_id: r.project_id,
            project_name: names.get(&r.project_id).cloned().unwrap_or_default(),
            rule_type: r.rule_type,
            target_field: r.target_field,
            pattern: r.pattern,
            auto_apply: r.auto_apply,
        })
        .collect();
    let candidates = store
        .project_candidates()?
        .into_iter()
        .map(|c| CandidateDto {
            root: c.root,
            git_remote: c.git_remote,
            sessions: c.sessions,
            last_active_at: c.last_active_at,
            sample_titles: c.sample_titles,
        })
        .collect();
    Ok(ProjectsOverviewDto {
        projects: projects.iter().map(ProjectDto::from).collect(),
        rules,
        candidates,
    })
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub fn create_project(
    state: tauri::State<'_, AppState>,
    name: String,
    description: Option<String>,
    paths: Option<Vec<String>>,
    remotes: Option<Vec<String>>,
    regexes: Option<Vec<String>>,
    regex_field: Option<String>,
    auto_apply: Option<bool>,
) -> CmdResult<i64> {
    // The form sends a line per pattern; empty lines are not rules.
    let given = |v: Option<Vec<String>>| -> Vec<String> {
        v.unwrap_or_default()
            .into_iter()
            .filter(|p| !p.trim().is_empty())
            .collect()
    };
    let (paths, remotes, regexes) = (given(paths), given(remotes), given(regexes));
    let rules = validate_rules(&RuleInput {
        paths: &paths,
        remotes: &remotes,
        regexes: &regexes,
        regex_field: regex_field.as_deref().unwrap_or("any"),
        auto_apply: auto_apply.unwrap_or(false),
    })?;
    let new: Vec<NewRule> = rules.iter().map(ValidRule::as_new).collect();
    let desc = description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty());
    let mut store = state.open_store()?;
    let (id, rule_ids) = store.create_project_with_rules(&name, desc, &new)?;
    if !rule_ids.is_empty() {
        reevaluate(&state, &store)?;
    }
    Ok(id)
}

#[tauri::command(async)]
pub fn rename_project(
    state: tauri::State<'_, AppState>,
    id: i64,
    new_name: String,
) -> CmdResult<()> {
    let mut store = state.open_store()?;
    store
        .rename_project(id, &new_name, aj_report::structure::rename_project_heading)
        .map(|_| ())
        .map_err(CmdError::from)
}

/// Save the user's project display order (also used to order daily report sections).
#[tauri::command(async)]
pub fn reorder_projects(state: tauri::State<'_, AppState>, ids: Vec<i64>) -> CmdResult<()> {
    let mut store = state.open_store()?;
    store.reorder_projects(&ids).map_err(CmdError::from)
}

#[tauri::command(async)]
pub fn describe_project(
    state: tauri::State<'_, AppState>,
    id: i64,
    description: Option<String>,
) -> CmdResult<()> {
    let store = state.open_store()?;
    let desc = description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty());
    store
        .set_project_description(id, desc)
        .map_err(CmdError::from)
}

/// Archive or restore a project, then re-evaluate assignments (its rules pause or resume).
#[tauri::command(async)]
pub fn set_project_archived(
    state: tauri::State<'_, AppState>,
    id: i64,
    archived: bool,
) -> CmdResult<()> {
    let mut store = state.open_store()?;
    store.set_project_archived(id, archived)?;
    reevaluate(&state, &store)
}

#[tauri::command(async)]
pub fn project_delete_impact(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> CmdResult<DeleteImpactDto> {
    let store = state.open_store()?;
    let i = store.project_delete_impact(id)?;
    Ok(DeleteImpactDto {
        sessions: i.sessions,
        reports: i.reports,
        rules: i.rules,
        notes: i.notes,
    })
}

#[tauri::command(async)]
pub fn delete_project(state: tauri::State<'_, AppState>, id: i64) -> CmdResult<()> {
    let hour = state.core_config()?.day_boundary_hour;
    let today = aj_core::slice::local_date(chrono::Utc::now(), &chrono::Local, hour).to_string();
    let mut store = state.open_store()?;
    store.delete_project(id, &today)?;
    reevaluate(&state, &store)
}

#[tauri::command(async)]
pub fn add_rule(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    rule_type: String,
    target_field: Option<String>,
    pattern: String,
    auto_apply: bool,
) -> CmdResult<i64> {
    let store = state.open_store()?;
    let field = if rule_type == RULE_REGEX {
        Some(target_field.as_deref().unwrap_or("any"))
    } else {
        None
    };
    let normalized = validate_rule(&rule_type, field, &pattern)?;
    let id = store.add_rule(
        project_id,
        &NewRule {
            rule_type: &rule_type,
            target_field: field,
            pattern: &normalized,
            auto_apply,
        },
    )?;
    reevaluate(&state, &store)?;
    Ok(id)
}

#[tauri::command(async)]
pub fn delete_rule(state: tauri::State<'_, AppState>, id: i64) -> CmdResult<()> {
    let store = state.open_store()?;
    store.delete_rule(id)?;
    reevaluate(&state, &store)
}

#[tauri::command(async)]
pub fn set_rule_auto(
    state: tauri::State<'_, AppState>,
    id: i64,
    auto_apply: bool,
) -> CmdResult<()> {
    let store = state.open_store()?;
    store.set_rule_auto_apply(id, auto_apply)?;
    reevaluate(&state, &store)
}

#[tauri::command(async)]
pub fn assign_session(
    state: tauri::State<'_, AppState>,
    session_id: i64,
    project_id: Option<i64>,
) -> CmdResult<AssignOutcomeDto> {
    let store = state.open_store()?;
    store.assign_manual(session_id, project_id)?;
    let uncovered = if project_id.is_some() {
        let rules = store.rules()?;
        store
            .session(session_id)?
            .and_then(|s| uncovered_path(&rules, &s))
    } else {
        None
    };
    Ok(AssignOutcomeDto {
        uncovered_path: uncovered,
    })
}

#[tauri::command(async)]
pub fn assign_slice(
    state: tauri::State<'_, AppState>,
    slice_id: i64,
    project_id: Option<i64>,
) -> CmdResult<()> {
    let store = state.open_store()?;
    store
        .set_slice_project_override(slice_id, project_id)
        .map_err(CmdError::from)
}

#[tauri::command(async)]
pub fn unassign_session(state: tauri::State<'_, AppState>, session_id: i64) -> CmdResult<()> {
    let store = state.open_store()?;
    store.clear_manual(session_id)?;
    reevaluate(&state, &store)
}

#[tauri::command(async)]
pub fn accept_suggestions(
    state: tauri::State<'_, AppState>,
    session_ids: Vec<i64>,
) -> CmdResult<u64> {
    let store = state.open_store()?;
    let mut accepted = 0u64;
    for id in session_ids {
        if store.accept_suggestion(id)? {
            accepted += 1;
        }
    }
    Ok(accepted)
}
