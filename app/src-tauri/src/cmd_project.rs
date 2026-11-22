//! Project, rule, and session assignment commands.
//!
//! Wraps `aj-store` and `aj-core::project` so users can create projects, manage
//! rules, accept suggestions, and assign sessions or slices from the UI.

use std::collections::HashMap;

use aj_adapter::SourceAdapter;
use aj_core::project::matcher::{RULE_GIT_REMOTE, RULE_PATH_PREFIX, RULE_REGEX};
use aj_core::project::{evaluate_all, uncovered_path, validate_rule};
use aj_core::scan::normalized_cwd;
use aj_store::NewRule;
use serde::Serialize;

use crate::state::{cmd_err, AppState};

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

fn reevaluate(state: &AppState, store: &aj_store::Store) -> Result<(), String> {
    let adapters = state.adapters().map_err(cmd_err)?;
    let refs: Vec<&dyn SourceAdapter> = adapters.iter().map(|a| a.as_ref()).collect();
    evaluate_all(store, &|s| normalized_cwd(&refs, s)).map_err(cmd_err)?;
    Ok(())
}

#[tauri::command(async)]
pub fn list_projects_and_rules(
    state: tauri::State<'_, AppState>,
) -> Result<ProjectsOverviewDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let projects = store.list_projects().map_err(cmd_err)?;
    let names: HashMap<i64, String> = projects
        .iter()
        .map(|p| (p.project.id, p.project.name.clone()))
        .collect();
    let rules = store
        .rules()
        .map_err(cmd_err)?
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
        .project_candidates()
        .map_err(cmd_err)?
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
) -> Result<i64, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let desc = description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty());
    let id = store.create_project(&name, desc).map_err(cmd_err)?;
    let auto = auto_apply.unwrap_or(false);
    let field = regex_field.as_deref().unwrap_or("any");

    let mut added = 0usize;
    let groups: [(&str, Vec<String>, Option<&str>); 3] = [
        (RULE_PATH_PREFIX, paths.unwrap_or_default(), None),
        (RULE_GIT_REMOTE, remotes.unwrap_or_default(), None),
        (RULE_REGEX, regexes.unwrap_or_default(), Some(field)),
    ];
    for (rule_type, patterns, target_field) in groups {
        for raw in patterns {
            if raw.trim().is_empty() {
                continue;
            }
            let pattern = validate_rule(rule_type, target_field, &raw).map_err(cmd_err)?;
            store
                .add_rule(
                    id,
                    &NewRule {
                        rule_type,
                        target_field,
                        pattern: &pattern,
                        auto_apply: auto,
                    },
                )
                .map_err(cmd_err)?;
            added += 1;
        }
    }
    if added > 0 {
        reevaluate(&state, &store)?;
    }
    Ok(id)
}

#[tauri::command(async)]
pub fn rename_project(
    state: tauri::State<'_, AppState>,
    id: i64,
    new_name: String,
) -> Result<(), String> {
    let mut store = state.open_store().map_err(cmd_err)?;
    store
        .rename_project(id, &new_name, aj_report::structure::rename_project_heading)
        .map(|_| ())
        .map_err(cmd_err)
}

/// Save the user's project display order (also used to order daily report sections).
#[tauri::command(async)]
pub fn reorder_projects(state: tauri::State<'_, AppState>, ids: Vec<i64>) -> Result<(), String> {
    let mut store = state.open_store().map_err(cmd_err)?;
    store.reorder_projects(&ids).map_err(cmd_err)
}

#[tauri::command(async)]
pub fn describe_project(
    state: tauri::State<'_, AppState>,
    id: i64,
    description: Option<String>,
) -> Result<(), String> {
    let store = state.open_store().map_err(cmd_err)?;
    let desc = description
        .as_deref()
        .map(str::trim)
        .filter(|d| !d.is_empty());
    store.set_project_description(id, desc).map_err(cmd_err)
}

/// Archive or restore a project, then re-evaluate assignments (its rules pause or resume).
#[tauri::command(async)]
pub fn set_project_archived(
    state: tauri::State<'_, AppState>,
    id: i64,
    archived: bool,
) -> Result<(), String> {
    let mut store = state.open_store().map_err(cmd_err)?;
    store.set_project_archived(id, archived).map_err(cmd_err)?;
    reevaluate(&state, &store)
}

#[tauri::command(async)]
pub fn project_delete_impact(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<DeleteImpactDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let i = store.project_delete_impact(id).map_err(cmd_err)?;
    Ok(DeleteImpactDto {
        sessions: i.sessions,
        reports: i.reports,
        rules: i.rules,
        notes: i.notes,
    })
}

#[tauri::command(async)]
pub fn delete_project(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    let hour = state.core_config().map_err(cmd_err)?.day_boundary_hour;
    let today = aj_core::slice::local_date(chrono::Utc::now(), &chrono::Local, hour).to_string();
    let mut store = state.open_store().map_err(cmd_err)?;
    store.delete_project(id, &today).map_err(cmd_err)?;
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
) -> Result<i64, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let field = if rule_type == RULE_REGEX {
        Some(target_field.as_deref().unwrap_or("any"))
    } else {
        None
    };
    let normalized = validate_rule(&rule_type, field, &pattern).map_err(cmd_err)?;
    let id = store
        .add_rule(
            project_id,
            &NewRule {
                rule_type: &rule_type,
                target_field: field,
                pattern: &normalized,
                auto_apply,
            },
        )
        .map_err(cmd_err)?;
    reevaluate(&state, &store)?;
    Ok(id)
}

#[tauri::command(async)]
pub fn delete_rule(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    let store = state.open_store().map_err(cmd_err)?;
    store.delete_rule(id).map_err(cmd_err)?;
    reevaluate(&state, &store)
}

#[tauri::command(async)]
pub fn set_rule_auto(
    state: tauri::State<'_, AppState>,
    id: i64,
    auto_apply: bool,
) -> Result<(), String> {
    let store = state.open_store().map_err(cmd_err)?;
    store.set_rule_auto_apply(id, auto_apply).map_err(cmd_err)?;
    reevaluate(&state, &store)
}

#[tauri::command(async)]
pub fn assign_session(
    state: tauri::State<'_, AppState>,
    session_id: i64,
    project_id: Option<i64>,
) -> Result<AssignOutcomeDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    store
        .assign_manual(session_id, project_id)
        .map_err(cmd_err)?;
    let uncovered = if project_id.is_some() {
        let rules = store.rules().map_err(cmd_err)?;
        store
            .session(session_id)
            .map_err(cmd_err)?
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
) -> Result<(), String> {
    let store = state.open_store().map_err(cmd_err)?;
    store
        .set_slice_project_override(slice_id, project_id)
        .map_err(cmd_err)
}

#[tauri::command(async)]
pub fn unassign_session(state: tauri::State<'_, AppState>, session_id: i64) -> Result<(), String> {
    let store = state.open_store().map_err(cmd_err)?;
    store.clear_manual(session_id).map_err(cmd_err)?;
    reevaluate(&state, &store)
}

#[tauri::command(async)]
pub fn accept_suggestions(
    state: tauri::State<'_, AppState>,
    session_ids: Vec<i64>,
) -> Result<u64, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let mut accepted = 0u64;
    for id in session_ids {
        if store.accept_suggestion(id).map_err(cmd_err)? {
            accepted += 1;
        }
    }
    Ok(accepted)
}
