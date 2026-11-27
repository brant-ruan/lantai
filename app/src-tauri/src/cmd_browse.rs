//! Browsing, scanning, slice replay, and health inspection commands.
//!
//! Exposes read/toggle actions for the evening workbench, slice detail drawer,
//! activity calendar, and source diagnostics with an engine summary.

use std::collections::HashMap;

use aj_adapter::SourceAdapter;
use aj_core::dehydrate::dehydrate_slice;
use aj_core::project::uncovered_path;
use aj_core::scan::{scan, ScanOptions};
use aj_core::slice::local_date;
use aj_report::lang::Language;
use aj_report::monthly::month_overviews;
use aj_report::period::{Month, Week};
use aj_report::prompt_library::{self, KIND_DAILY, KIND_MONTHLY, KIND_WEEKLY};
use aj_report::weekly::week_overviews;
use aj_report::StoredDigest;
use aj_store::{SessionRow, SliceRow, Store, TurnItem};
use anyhow::Context;
use chrono::{Datelike, Local, Utc};
use serde::Serialize;

use crate::cmd_project::{ProjectDto, RuleDto};
use crate::state::{cmd_err, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResultDto {
    pub files_seen: u64,
    pub files_parsed: u64,
    pub files_unchanged: u64,
    pub files_failed: u64,
    pub new_turns: u64,
    pub new_items: u64,
    pub assigned_by_rule: u64,
    pub suggestions_pending: u64,
    pub failures: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayOverviewDto {
    pub local_date: String,
    pub slices: u64,
    pub ignored_slices: u64,
    pub active_minutes: i64,
    pub unassigned_slices: u64,
    pub report_status: Option<String>,
    /// Top-level notes and annotations people wrote for the day.
    pub notes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaysResponseDto {
    pub today: String,
    pub days: Vec<DayOverviewDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceCardDto {
    pub id: i64,
    pub session_id: i64,
    pub source_session_id: String,
    pub short_id: String,
    /// Stable agent key (`claude`, `codex`, ...) and its display name.
    pub agent: String,
    pub agent_name: String,
    pub title: String,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub project_override_id: Option<i64>,
    pub assignment_source: String,
    pub suggested_project_id: Option<i64>,
    pub suggested_project_name: Option<String>,
    pub suggestion_source: Option<String>,
    pub suggestion_reason: Option<String>,
    pub uncovered_path: Option<String>,
    pub slice_date: String,
    pub start_time: String,
    pub end_time: String,
    pub active_minutes: i64,
    pub turn_count: i64,
    pub files_modified_count: usize,
    pub user_note: Option<String>,
    pub ignored: bool,
    pub session_ignored: bool,
    pub ignore_state: String,
    pub has_digest: bool,
    /// When the slice's last turn ended (UTC, RFC 3339); decides whether it is still live.
    pub ended_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOptionDto {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub archived: bool,
}

impl From<aj_store::ProjectRow> for ProjectOptionDto {
    fn from(p: aj_store::ProjectRow) -> Self {
        Self {
            id: p.id,
            name: p.name,
            description: p.description,
            archived: p.archived,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptOptionDto {
    pub id: i64,
    pub name: String,
    pub is_default: bool,
    pub is_builtin: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayWorkbenchDto {
    /// The period shown: a date, or a week key (`2026-W47`) for a week workbench.
    pub date: String,
    pub slices: Vec<SliceCardDto>,
    pub projects: Vec<ProjectOptionDto>,
    pub prompts: Vec<PromptOptionDto>,
    /// `None` when no model is configured.
    pub default_engine: Option<String>,
    pub engines: Vec<String>,
    /// Slices that ended more recently than this are live and are not pre-digested.
    pub quiet_minutes: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorkbenchDto {
    pub project: ProjectDto,
    pub slices: Vec<SliceCardDto>,
    pub suggested_slices: Vec<SliceCardDto>,
    pub rules: Vec<RuleDto>,
    pub projects: Vec<ProjectOptionDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDetailDto {
    pub kind: String,
    pub tool_name: String,
    pub target: Option<String>,
    pub summary: Option<String>,
    pub is_error: Option<bool>,
    pub added: Option<u32>,
    pub removed: Option<u32>,
    pub output_summary: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnDetailDto {
    pub turn_index: i64,
    pub time: String,
    pub user_prompt: String,
    pub conclusion: Option<String>,
    pub messages: Vec<String>,
    pub tools: Vec<ToolDetailDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SliceDetailDto {
    pub slice: SliceCardDto,
    pub detected_cwd: Option<String>,
    pub git_root: Option<String>,
    pub git_remote: Option<String>,
    pub git_branch: Option<String>,
    pub files_modified: Vec<String>,
    pub dehydrated_chars: usize,
    pub digest: Option<StoredDigest>,
    /// Which model wrote `digest`; `None` when unknown (written before this was recorded).
    pub digest_model: Option<ModelRefDto>,
    pub turns: Vec<TurnDetailDto>,
}

/// An engine name as stored with a digest, and the model aliases that produce it now.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRefDto {
    pub engine: String,
    /// Empty when no alias produces this engine any more.
    pub aliases: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceHealthDto {
    pub agent: String,
    pub files: u64,
    pub error_count: u64,
    pub unknown_count: u64,
    pub last_error: Option<String>,
    pub agent_versions: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsDto {
    pub data_dir: String,
    pub export_dir: String,
    pub ir_version: String,
    pub sessions: u64,
    pub turns: u64,
    pub sources: Vec<SourceHealthDto>,
}

fn build_slice_card(
    slice: &SliceRow,
    session: &SessionRow,
    project_names: &HashMap<i64, String>,
    rules: &[aj_store::RuleRow],
    lang: &Language,
) -> SliceCardDto {
    let project_id = slice.effective_project(session);
    let project_name = project_id.and_then(|id| project_names.get(&id).cloned());
    let suggested_project_name = session
        .suggested_project_id
        .and_then(|id| project_names.get(&id).cloned());
    let uncovered = if project_id.is_some() {
        uncovered_path(rules, session)
    } else {
        None
    };
    SliceCardDto {
        id: slice.id,
        session_id: session.id,
        source_session_id: session.source_session_id.clone(),
        short_id: session.source_session_id.chars().take(8).collect(),
        agent: session.agent.as_str().to_string(),
        agent_name: session.agent.product_name().to_string(),
        title: session
            .title
            .clone()
            .unwrap_or_else(|| "(untitled)".to_string()),
        project_id,
        project_name,
        project_override_id: slice.project_id_override,
        assignment_source: session.assignment_source.clone(),
        suggested_project_id: session.suggested_project_id,
        suggested_project_name,
        suggestion_source: session.suggestion_source.clone(),
        suggestion_reason: session.suggestion_reason.clone(),
        uncovered_path: uncovered,
        slice_date: slice.local_date.clone(),
        start_time: slice
            .slice_start_at
            .with_timezone(&Local)
            .format("%H:%M")
            .to_string(),
        end_time: slice
            .slice_end_at
            .with_timezone(&Local)
            .format("%H:%M")
            .to_string(),
        active_minutes: slice.active_minutes,
        turn_count: slice.end_turn_index - slice.start_turn_index + 1,
        files_modified_count: slice.files_modified.len(),
        user_note: session.user_note.clone(),
        ignored: slice.is_ignored(session),
        session_ignored: session.ignored,
        ignore_state: slice.ignore_state.clone(),
        has_digest: digest_in(slice, lang).is_some(),
        ended_at: slice.slice_end_at.to_rfc3339(),
    }
}

#[tauri::command]
pub async fn scan_sessions(
    state: tauri::State<'_, AppState>,
    since_days: Option<i64>,
) -> Result<ScanResultDto, String> {
    let data_dir = state.data_dir.clone();
    let app_cfg = state.load_config().map_err(cmd_err)?;
    let cfg = app_cfg.core_config();
    tauri::async_runtime::spawn_blocking(move || {
        let adapters = aj_sources::build_adapters(&data_dir, |kind| app_cfg.agent(kind).enabled)
            .map_err(cmd_err)?;
        let mut store =
            aj_store::Store::open(&data_dir.join(crate::state::DB_FILE)).map_err(cmd_err)?;
        let refs: Vec<&dyn SourceAdapter> = adapters.iter().map(|a| a.as_ref()).collect();
        let r = scan(
            &mut store,
            &refs,
            &cfg,
            &Local,
            &ScanOptions {
                since_days: Some(since_days.unwrap_or(90)),
            },
        )
        .map_err(cmd_err)?;
        Ok(ScanResultDto {
            files_seen: r.files_seen,
            files_parsed: r.files_parsed,
            files_unchanged: r.files_unchanged,
            files_failed: r.files_failed,
            new_turns: r.new_turns,
            new_items: r.new_items,
            assigned_by_rule: r.assignment.assigned,
            suggestions_pending: r.assignment.suggested,
            failures: r.failures.into_iter().take(10).collect(),
        })
    })
    .await
    .map_err(|e| format!("scan task failed: {e}"))?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayBoundaryDto {
    /// Local hour a new day starts.
    pub hour: u32,
    pub max_hour: u32,
}

#[tauri::command(async)]
pub fn get_day_boundary(state: tauri::State<'_, AppState>) -> Result<DayBoundaryDto, String> {
    Ok(DayBoundaryDto {
        hour: state.core_config().map_err(cmd_err)?.day_boundary_hour,
        max_hour: aj_report::MAX_DAY_BOUNDARY_HOUR,
    })
}

/// Save a new day boundary and re-cut every stored session into days with it.
#[tauri::command]
pub async fn set_day_boundary(
    state: tauri::State<'_, AppState>,
    hour: u32,
) -> Result<DayBoundaryDto, String> {
    let cfg = state
        .modify_config(|cfg| cfg.set_day_boundary_hour(hour))
        .map_err(cmd_err)?
        .core_config();
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut store =
            aj_store::Store::open(&data_dir.join(crate::state::DB_FILE)).map_err(cmd_err)?;
        aj_core::slice::reslice_all(&mut store, &cfg, &Local).map_err(cmd_err)?;
        Ok(DayBoundaryDto {
            hour: cfg.day_boundary_hour,
            max_hour: aj_report::MAX_DAY_BOUNDARY_HOUR,
        })
    })
    .await
    .map_err(|e| format!("reslice task failed: {e}"))?
}

#[tauri::command(async)]
pub fn list_days(
    state: tauri::State<'_, AppState>,
    include_ignored: Option<bool>,
) -> Result<DaysResponseDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let rows =
        aj_core::overview::day_rows(&store, include_ignored.unwrap_or(false)).map_err(cmd_err)?;
    let today = local_date(
        Utc::now(),
        &Local,
        state.core_config().map_err(cmd_err)?.day_boundary_hour,
    )
    .to_string();
    Ok(DaysResponseDto {
        today,
        days: rows
            .into_iter()
            .map(|r| DayOverviewDto {
                local_date: r.day.local_date,
                slices: r.day.slices,
                ignored_slices: r.day.ignored_slices,
                active_minutes: r.day.active_minutes,
                unassigned_slices: r.day.unassigned_slices,
                report_status: r.day.report_status,
                notes: r.notes,
            })
            .collect(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekOverviewDto {
    pub week: String,
    pub monday: String,
    pub sunday: String,
    pub active_days: usize,
    pub slices: u64,
    pub active_minutes: i64,
    pub unassigned_slices: u64,
    pub report_status: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeksResponseDto {
    pub current_week: String,
    pub weeks: Vec<WeekOverviewDto>,
}

/// Every week with activity, newest first.
#[tauri::command(async)]
pub fn list_weeks(state: tauri::State<'_, AppState>) -> Result<WeeksResponseDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let today = local_date(
        Utc::now(),
        &Local,
        state.core_config().map_err(cmd_err)?.day_boundary_hour,
    );
    let weeks = week_overviews(&store).map_err(cmd_err)?;
    Ok(WeeksResponseDto {
        current_week: Week::containing(today).key(),
        weeks: weeks
            .into_iter()
            .map(|w| WeekOverviewDto {
                week: w.week.key(),
                monday: w.week.monday().to_string(),
                sunday: w.week.sunday().to_string(),
                active_days: w.active_days,
                slices: w.slices,
                active_minutes: w.active_minutes,
                unassigned_slices: w.unassigned_slices,
                report_status: w.report_status,
            })
            .collect(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthOverviewDto {
    pub month: String,
    pub first_week: String,
    pub last_week: String,
    pub active_weeks: usize,
    pub reported_weeks: usize,
    pub active_days: usize,
    pub slices: u64,
    pub active_minutes: i64,
    pub unassigned_slices: u64,
    pub report_status: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthsResponseDto {
    pub current_month: String,
    pub months: Vec<MonthOverviewDto>,
}

/// Every calendar month with activity, newest first.
#[tauri::command(async)]
pub fn list_months(state: tauri::State<'_, AppState>) -> Result<MonthsResponseDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let today = local_date(
        Utc::now(),
        &Local,
        state.core_config().map_err(cmd_err)?.day_boundary_hour,
    );
    let months = month_overviews(&store).map_err(cmd_err)?;
    Ok(MonthsResponseDto {
        current_month: Month::containing(today).key(),
        months: months
            .into_iter()
            .map(|m| {
                let weeks = m.month.weeks();
                let first_week = weeks
                    .first()
                    .map(|w| format!("W{:02}", w.monday().iso_week().week()))
                    .unwrap_or_default();
                let last_week = weeks
                    .last()
                    .map(|w| format!("W{:02}", w.monday().iso_week().week()))
                    .unwrap_or_default();
                MonthOverviewDto {
                    month: m.month.key(),
                    first_week,
                    last_week,
                    active_weeks: m.active_weeks,
                    reported_weeks: m.reported_weeks,
                    active_days: m.active_days,
                    slices: m.slices,
                    active_minutes: m.active_minutes,
                    unassigned_slices: m.unassigned_slices,
                    report_status: m.report_status,
                }
            })
            .collect(),
    })
}

/// The slice's cached digest, if it is written in `lang` (one in another language is
/// redone on demand).
fn digest_in<'a>(slice: &'a SliceRow, lang: &Language) -> Option<&'a str> {
    match slice.digest_language.as_deref() {
        Some(tag) if tag == lang.tag => slice.digest_json.as_deref(),
        _ => None,
    }
}

fn map_slice_cards(
    store: &Store,
    slices: Vec<SliceRow>,
    project_names: &HashMap<i64, String>,
    rules: &[aj_store::RuleRow],
    lang: &Language,
) -> Result<Vec<SliceCardDto>, String> {
    let mut out = Vec::with_capacity(slices.len());
    for slice in slices {
        if let Some(session) = store.session(slice.session_id).map_err(cmd_err)? {
            out.push(build_slice_card(
                &slice,
                &session,
                project_names,
                rules,
                lang,
            ));
        }
    }
    Ok(out)
}

/// The prompts to pick from for `kind` reports in `lang`, marking its default;
/// a default borrowed from another language is offered too.
fn prompt_options(
    store: &Store,
    kind: &str,
    lang: &Language,
) -> Result<Vec<PromptOptionDto>, String> {
    let default = prompt_library::default_prompt(store, kind, lang).map_err(cmd_err)?;
    let mut rows = prompt_library::offered(store, Some(kind), lang).map_err(cmd_err)?;
    if !rows.iter().any(|p| p.id == default.id) {
        rows.insert(0, default.clone());
    }
    Ok(rows
        .into_iter()
        .map(|p| PromptOptionDto {
            is_default: p.id == default.id,
            id: p.id,
            name: p.name,
            is_builtin: p.is_builtin,
        })
        .collect())
}

#[tauri::command(async)]
pub fn day_workbench(
    state: tauri::State<'_, AppState>,
    date: String,
) -> Result<DayWorkbenchDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let slices = store.slices_for_date(&date).map_err(cmd_err)?;
    build_workbench(&state, &store, date, slices, KIND_DAILY)
}

/// The sessions of an ISO week (`2026-W47`), with the weekly prompts.
#[tauri::command(async)]
pub fn week_workbench(
    state: tauri::State<'_, AppState>,
    week: String,
) -> Result<DayWorkbenchDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let week = Week::parse(&week).map_err(cmd_err)?;
    let mut slices = Vec::new();
    for date in week.dates() {
        slices.extend(store.slices_for_date(&date.to_string()).map_err(cmd_err)?);
    }
    build_workbench(&state, &store, week.key(), slices, KIND_WEEKLY)
}

/// The sessions of a calendar month (`2026-11`, spanning its ISO weeks), with the monthly prompts.
#[tauri::command(async)]
pub fn month_workbench(
    state: tauri::State<'_, AppState>,
    month: String,
) -> Result<DayWorkbenchDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let month = Month::parse(&month).map_err(cmd_err)?;
    let mut slices = Vec::new();
    for week in month.weeks() {
        for date in week.dates() {
            slices.extend(store.slices_for_date(&date.to_string()).map_err(cmd_err)?);
        }
    }
    build_workbench(&state, &store, month.key(), slices, KIND_MONTHLY)
}

fn build_workbench(
    state: &AppState,
    store: &Store,
    period: String,
    slice_rows: Vec<SliceRow>,
    prompt_kind: &str,
) -> Result<DayWorkbenchDto, String> {
    let cfg = state.load_config().map_err(cmd_err)?;
    let project_rows = store.projects().map_err(cmd_err)?;
    let project_names: HashMap<i64, String> = project_rows
        .iter()
        .map(|p| (p.id, p.name.clone()))
        .collect();
    let rules = store.rules().map_err(cmd_err)?;
    let lang = state.report_language(&[]).map_err(cmd_err)?;
    let slices = map_slice_cards(store, slice_rows, &project_names, &rules, lang)?;

    let projects = project_rows
        .into_iter()
        .map(ProjectOptionDto::from)
        .collect();
    let prompts = prompt_options(store, prompt_kind, lang)?;

    Ok(DayWorkbenchDto {
        date: period,
        slices,
        projects,
        prompts,
        default_engine: cfg.default_model_name().map(str::to_string),
        engines: cfg.models.keys().cloned().collect(),
        quiet_minutes: cfg.budgets().quiet_minutes,
    })
}

#[tauri::command(async)]
pub fn project_workbench(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    since_date: Option<String>,
    until_date: Option<String>,
) -> Result<ProjectWorkbenchDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let project_rows = store.project_summaries().map_err(cmd_err)?;
    let project_names: HashMap<i64, String> = project_rows
        .iter()
        .map(|p| (p.project.id, p.project.name.clone()))
        .collect();
    let summary = project_rows
        .iter()
        .find(|p| p.project.id == project_id)
        .with_context(|| format!("no project with ID {project_id}"))
        .map_err(cmd_err)?;
    let project = ProjectDto::from(summary);

    let all_rules = store.rules().map_err(cmd_err)?;
    let lang = state.report_language(&[]).map_err(cmd_err)?;
    let slices = map_slice_cards(
        &store,
        store
            .slices_for_project(project_id, since_date.as_deref(), until_date.as_deref())
            .map_err(cmd_err)?,
        &project_names,
        &all_rules,
        lang,
    )?;
    let suggested_slices = map_slice_cards(
        &store,
        store
            .suggested_slices_for_project(project_id)
            .map_err(cmd_err)?,
        &project_names,
        &all_rules,
        lang,
    )?;
    let rules = all_rules
        .into_iter()
        .filter(|r| r.project_id == project_id)
        .map(|r| RuleDto {
            id: r.id,
            project_id: r.project_id,
            project_name: project.name.clone(),
            rule_type: r.rule_type,
            target_field: r.target_field,
            pattern: r.pattern,
            auto_apply: r.auto_apply,
        })
        .collect();
    let projects = project_rows
        .into_iter()
        .map(|p| ProjectOptionDto::from(p.project))
        .collect();

    Ok(ProjectWorkbenchDto {
        project,
        slices,
        suggested_slices,
        rules,
        projects,
    })
}

#[tauri::command(async)]
pub fn slice_detail(
    state: tauri::State<'_, AppState>,
    slice_id: i64,
) -> Result<SliceDetailDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let slice = store
        .slice(slice_id)
        .map_err(cmd_err)?
        .with_context(|| format!("no slice S{slice_id}"))
        .map_err(cmd_err)?;
    let session = store
        .session(slice.session_id)
        .map_err(cmd_err)?
        .context("the session this slice belongs to does not exist")
        .map_err(cmd_err)?;
    let project_names = store.project_names().map_err(cmd_err)?;
    let rules = store.rules().map_err(cmd_err)?;
    let lang = state.report_language(&[]).map_err(cmd_err)?;
    let cfg = state.load_config().map_err(cmd_err)?;
    let card = build_slice_card(&slice, &session, &project_names, &rules, lang);

    let dehydrated_chars = dehydrate_slice(&store, &session, &slice, &Local)
        .map_err(cmd_err)?
        .text()
        .chars()
        .count();
    let digest = digest_in(&slice, lang).and_then(|j| serde_json::from_str::<StoredDigest>(j).ok());
    let digest_model = digest
        .as_ref()
        .and(slice.digest_engine.as_deref())
        .map(|engine| ModelRefDto {
            engine: engine.to_string(),
            aliases: cfg.aliases_for_engine(engine),
        });

    let mut turns = Vec::new();
    for turn in store
        .turns(session.id, slice.start_turn_index, slice.end_turn_index)
        .map_err(cmd_err)?
    {
        let items = store.turn_items(turn.id).map_err(cmd_err)?;
        let mut messages = Vec::new();
        let mut tools = Vec::new();
        for item in items {
            match item {
                TurnItem::Message { text, .. } => {
                    if !text.trim().is_empty() {
                        messages.push(text);
                    }
                }
                TurnItem::Tool {
                    kind,
                    tool_name,
                    target,
                    summary,
                    is_error,
                    added,
                    removed,
                    output_summary,
                    ..
                } => tools.push(ToolDetailDto {
                    kind: kind.as_str().to_string(),
                    tool_name,
                    target,
                    summary,
                    is_error,
                    added,
                    removed,
                    output_summary,
                }),
            }
        }
        let conclusion = messages.last().cloned();
        turns.push(TurnDetailDto {
            turn_index: turn.turn_index,
            time: turn
                .started_at
                .with_timezone(&Local)
                .format("%m-%d %H:%M")
                .to_string(),
            user_prompt: turn.user_prompt,
            conclusion,
            messages,
            tools,
        });
    }

    Ok(SliceDetailDto {
        slice: card,
        detected_cwd: session.detected_cwd,
        git_root: session.git_root,
        git_remote: session.git_remote,
        git_branch: session.git_branch,
        files_modified: slice.files_modified,
        dehydrated_chars,
        digest,
        digest_model,
        turns,
    })
}

#[tauri::command(async)]
pub fn set_session_note(
    state: tauri::State<'_, AppState>,
    session_id: i64,
    note: Option<String>,
) -> Result<(), String> {
    let store = state.open_store().map_err(cmd_err)?;
    let trimmed = note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string);
    store
        .set_session_note(session_id, trimmed.as_deref())
        .map_err(cmd_err)
}

#[tauri::command(async)]
pub fn set_session_ignored(
    state: tauri::State<'_, AppState>,
    session_id: i64,
    ignored: bool,
) -> Result<(), String> {
    let store = state.open_store().map_err(cmd_err)?;
    store
        .set_session_ignored(session_id, ignored)
        .map_err(cmd_err)
}

#[tauri::command(async)]
pub fn set_slice_ignore_state(
    state: tauri::State<'_, AppState>,
    slice_id: i64,
    ignore_state: String,
) -> Result<(), String> {
    if !matches!(ignore_state.as_str(), "inherit" | "ignored" | "included") {
        return Err(format!("invalid slice ignore state: {ignore_state}"));
    }
    let store = state.open_store().map_err(cmd_err)?;
    store
        .set_slice_ignore_state(slice_id, &ignore_state)
        .map_err(cmd_err)
}

#[tauri::command(async)]
pub fn diagnostics(state: tauri::State<'_, AppState>) -> Result<DiagnosticsDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let (sessions, turns) = store.session_count().map_err(cmd_err)?;
    let sources = store
        .source_health()
        .map_err(cmd_err)?
        .into_iter()
        .map(|h| SourceHealthDto {
            agent: aj_ir::AgentId::parse(&h.agent)
                .map_or(h.agent, |a| a.product_name().to_string()),
            files: h.files,
            error_count: h.error_count,
            unknown_count: h.unknown_count,
            last_error: h.last_error,
            agent_versions: h.agent_versions,
        })
        .collect();
    Ok(DiagnosticsDto {
        data_dir: state.data_dir.to_string_lossy().into_owned(),
        export_dir: state.export_dir.to_string_lossy().into_owned(),
        ir_version: aj_ir::IR_VERSION.to_string(),
        sessions,
        turns,
        sources,
    })
}
