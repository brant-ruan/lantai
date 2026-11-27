//! Daily, weekly and monthly report estimation, background generation, versioning,
//! and Markdown export commands.
//!
//! Runs model calls off the UI thread, one job per report scope (several periods may
//! generate at once), streams progress events tagged with the job key, supports
//! per-job cooperative cancellation, and delegates export conflict detection to `aj-report`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use aj_report::edits::ReportEdits;
use aj_report::export::{
    default_daily_export_path, default_monthly_export_path, default_project_export_path,
    default_weekly_export_path, export_markdown, AnchorExportMode, ExportOutcome,
};
use aj_report::material::MaterialContext;
use aj_report::period::{Month, Week};
use aj_report::progress::Progress;
use aj_report::prompt_library::{KIND_DAILY, KIND_MONTHLY, KIND_WEEKLY};
use aj_report::AppConfig;
use aj_report::{
    daily::{estimate_daily, DailyEstimate},
    generate_daily,
    monthly::{estimate_monthly, generate_monthly},
    report_io::Staleness,
    rollup::{project_rollup, Granularity},
    stage1::digest_single_slice,
    versions,
    weekly::{estimate_weekly, generate_weekly},
    DailyResult, Engine, ReportMode, ReportOptions, StoredDigest, DEFAULT_CONCURRENCY,
};
use aj_store::Store;
use anyhow::Context;
use chrono::{Local, Utc};
use serde::Serialize;
use tauri::Emitter;

use crate::state::{cmd_err, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyEstimateDto {
    pub slices_used: usize,
    pub slices_ignored: usize,
    pub digests_cached: usize,
    pub digests_pending: usize,
    /// Of the pending digests, those another model wrote before (a model switch redoes them).
    pub digests_other_model: usize,
    pub estimated_chars: usize,
    /// Set when the current version was generated from different material than there is now.
    pub staleness: Option<StalenessDto>,
    /// Active weeks in a month that have no weekly report yet (monthly reports only).
    pub unreported_weeks: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StalenessDto {
    pub new_slices: usize,
    pub changed_slices: usize,
    pub removed_slices: usize,
    pub projects: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportMetaDto {
    pub status: String,
    pub current_version_id: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportVersionDto {
    pub id: i64,
    pub number: u64,
    pub origin: String,
    pub markdown: String,
    pub engine: Option<String>,
    /// Who wrote the sections: one entry, or several after an Update with another model.
    pub writers: Vec<SectionWriterDto>,
    pub prompt_name: Option<String>,
    pub prompt_snapshot: Option<String>,
    /// The language the version is written in.
    pub language: String,
    pub created_at: String,
    pub is_current: bool,
    /// Lines the user wrote rather than the model; only on the current version, and only
    /// when its manifest allows telling them apart.
    pub edits: Option<ReportEditsDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionWriterDto {
    /// `None` for sections kept from a version that did not record its model.
    pub engine: Option<String>,
    /// The model aliases that produce `engine` now (empty when none does).
    pub aliases: Vec<String>,
    /// Current names of the projects whose sections it wrote; empty when not recorded.
    pub projects: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportEditsDto {
    pub lines: Vec<LineEditDto>,
    pub removed: Vec<RemovedLinesDto>,
}

/// A line of the displayed Markdown (0-based, split on `\n`) the user added or rewrote.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LineEditDto {
    pub line: usize,
    /// The generated line it rewrote; absent for an added line.
    pub original: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovedLinesDto {
    pub heading_line: usize,
    pub lines: Vec<String>,
}

fn edits_dto(e: ReportEdits) -> ReportEditsDto {
    ReportEditsDto {
        lines: e
            .lines
            .into_iter()
            .map(|l| LineEditDto {
                line: l.line,
                original: l.original,
            })
            .collect(),
        removed: e
            .removed
            .into_iter()
            .map(|r| RemovedLinesDto {
                heading_line: r.heading_line,
                lines: r.lines,
            })
            .collect(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyReportStateDto {
    pub estimate: DailyEstimateDto,
    pub meta: Option<ReportMetaDto>,
    pub versions: Vec<ReportVersionDto>,
    pub default_export_path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorCheckDto {
    pub total: usize,
    pub invalid: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyResultDto {
    pub markdown: String,
    pub slices_used: usize,
    pub slices_ignored: usize,
    pub digests_cached: usize,
    pub digests_generated: usize,
    pub digests_incremental: usize,
    pub digests_passthrough: usize,
    pub slices_split: usize,
    pub parts_generated: usize,
    pub parts_cached: usize,
    pub projects_condensed: usize,
    pub briefs_cached: usize,
    pub suggestions_made: usize,
    pub updated: bool,
    pub projects_written: Vec<String>,
    pub sections_kept: usize,
    pub warnings: Vec<String>,
    pub anchors: AnchorCheckDto,
    pub stage2_prompt_chars: usize,
    pub version_id: Option<i64>,
    pub prompt_name: String,
}

/// The outcome of a weekly report run; the report itself is read back with its state.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyResultDto {
    pub slices_used: usize,
    pub projects_written: Vec<String>,
    pub sections_kept: usize,
    pub updated: bool,
    pub warnings: Vec<String>,
    pub anchors: AnchorCheckDto,
    pub version_id: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResultDto {
    pub status: String,
    pub path: String,
    pub bytes: Option<usize>,
    pub existing_markdown: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRollupEntryDto {
    /// The period key (a date or a week key) and its readable label.
    pub period: String,
    pub label: String,
    pub status: String,
    pub markdown: String,
    pub updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRollupDto {
    pub project_id: i64,
    pub project_name: String,
    pub period_key: String,
    /// `daily` or `weekly`: which reports the entries come from.
    pub granularity: String,
    pub entries: Vec<ProjectRollupEntryDto>,
    pub unreported_periods: Vec<String>,
    pub combined_markdown: String,
    pub default_export_path: String,
}

fn resolve_project_id(
    store: &Store,
    project_id: Option<i64>,
    project: Option<&str>,
) -> Result<Option<i64>, String> {
    if let Some(id) = project_id {
        return Ok(Some(id));
    }
    project
        .map(|name| store.require_project(name).map(|p| p.id))
        .transpose()
        .map_err(cmd_err)
}

fn staleness_dto(s: Staleness) -> StalenessDto {
    StalenessDto {
        new_slices: s.new_slices,
        changed_slices: s.changed_slices,
        removed_slices: s.removed_slices,
        projects: s.projects,
    }
}

fn daily_estimate_dto(est: DailyEstimate) -> DailyEstimateDto {
    DailyEstimateDto {
        slices_used: est.slices_used,
        slices_ignored: est.slices_ignored,
        digests_cached: est.digests_cached,
        digests_pending: est.digests_pending,
        digests_other_model: est.digests_other_model,
        estimated_chars: est.estimated_chars,
        staleness: est.staleness.map(staleness_dto),
        unreported_weeks: Vec::new(),
    }
}

fn build_report_state(
    store: &Store,
    cfg: &AppConfig,
    report_type: &str,
    period_key: &str,
    project_id: Option<i64>,
    estimate: DailyEstimateDto,
    default_export_path: PathBuf,
) -> Result<DailyReportStateDto, String> {
    let meta = store
        .report_meta(report_type, period_key, project_id)
        .map_err(cmd_err)?
        .map(|m| ReportMetaDto {
            status: m.status,
            current_version_id: m.current_version_id,
        });
    let versions = versions::shown_versions(store, report_type, period_key, project_id)
        .map_err(cmd_err)?
        .into_iter()
        .map(|s| ReportVersionDto {
            id: s.version.id,
            number: s.version.number,
            origin: s.version.origin,
            markdown: s.markdown,
            engine: s.version.engine,
            writers: s
                .writers
                .into_iter()
                .map(|w| SectionWriterDto {
                    aliases: w
                        .engine
                        .as_deref()
                        .map(|e| cfg.aliases_for_engine(e))
                        .unwrap_or_default(),
                    engine: w.engine,
                    projects: w.projects,
                })
                .collect(),
            prompt_name: s.version.prompt_name,
            prompt_snapshot: s.version.prompt_snapshot,
            language: s.version.language,
            created_at: s.version.created_at,
            is_current: s.version.is_current,
            edits: s.edits.map(edits_dto),
        })
        .collect();

    Ok(DailyReportStateDto {
        estimate,
        meta,
        versions,
        default_export_path: default_export_path.to_string_lossy().into_owned(),
    })
}

/// Identifies one report generation job: the period (a date or a week key), plus the
/// project when scoped.
fn job_key(period: &str, project: Option<&str>) -> String {
    match project {
        Some(p) => format!("{period}|{p}"),
        None => period.to_string(),
    }
}

/// Registers a job and returns its cancel flag; fails if the same scope is already running.
fn start_job(state: &AppState, key: &str) -> Result<Arc<AtomicBool>, String> {
    let mut jobs = state
        .report_jobs
        .lock()
        .map_err(|_| "internal lock error".to_string())?;
    if jobs.contains_key(key) {
        return Err(format!("a report for {key} is already being generated"));
    }
    let flag = Arc::new(AtomicBool::new(false));
    jobs.insert(key.to_string(), flag.clone());
    Ok(flag)
}

fn finish_job(state: &AppState, key: &str) {
    if let Ok(mut jobs) = state.report_jobs.lock() {
        jobs.remove(key);
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportProgressDto {
    pub key: String,
    pub progress: Progress,
}

fn to_result_dto(r: DailyResult) -> DailyResultDto {
    DailyResultDto {
        markdown: r.markdown,
        slices_used: r.slices_used,
        slices_ignored: r.slices_ignored,
        digests_cached: r.stage1.digests_cached,
        digests_generated: r.stage1.digests_generated,
        digests_incremental: r.stage1.digests_incremental,
        digests_passthrough: r.stage1.digests_passthrough,
        slices_split: r.stage1.slices_split,
        parts_generated: r.stage1.parts_generated,
        parts_cached: r.stage1.parts_cached,
        projects_condensed: r.projects_condensed,
        briefs_cached: r.briefs_cached,
        suggestions_made: r.stage1.suggestions_made,
        updated: r.updated,
        projects_written: r.projects_written,
        sections_kept: r.sections_kept,
        warnings: r.warnings,
        anchors: AnchorCheckDto {
            total: r.anchors.total,
            invalid: r.anchors.invalid,
        },
        stage2_prompt_chars: r.stage2_prompt_chars,
        version_id: r.version_id,
        prompt_name: r.prompt_name,
    }
}

#[tauri::command(async)]
pub fn get_daily_report_state(
    state: tauri::State<'_, AppState>,
    date: String,
    project: Option<String>,
    engine: Option<String>,
) -> Result<DailyReportStateDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let cfg = state.load_config().map_err(cmd_err)?;
    let engine_name = cfg
        .estimate_engine(engine.as_deref(), &state.data_dir.join("runner"))
        .map_err(cmd_err)?
        .name();
    let budgets = cfg.budgets();
    let language = state.report_language(&[]).map_err(cmd_err)?;
    let est = estimate_daily(
        &store,
        &Local,
        &date,
        project.as_deref(),
        &MaterialContext {
            engine_name: &engine_name,
            budgets: &budgets,
            now: Utc::now(),
            language,
        },
    )
    .map_err(cmd_err)?;
    let project_id = resolve_project_id(&store, None, project.as_deref())?;
    build_report_state(
        &store,
        &cfg,
        KIND_DAILY,
        &date,
        project_id,
        daily_estimate_dto(est),
        default_daily_export_path(&state.export_dir, &date),
    )
}

/// The weekly report of an ISO week (`2026-W47`): estimate, staleness and versions.
#[tauri::command(async)]
pub fn get_weekly_report_state(
    state: tauri::State<'_, AppState>,
    week: String,
    engine: Option<String>,
) -> Result<DailyReportStateDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let cfg = state.load_config().map_err(cmd_err)?;
    let week = Week::parse(&week).map_err(cmd_err)?;
    let engine_name = cfg
        .estimate_engine(engine.as_deref(), &state.data_dir.join("runner"))
        .map_err(cmd_err)?
        .name();
    let budgets = cfg.budgets();
    let language = state.report_language(&[]).map_err(cmd_err)?;
    let est = estimate_weekly(
        &store,
        &Local,
        week,
        None,
        &MaterialContext {
            engine_name: &engine_name,
            budgets: &budgets,
            now: Utc::now(),
            language,
        },
    )
    .map_err(cmd_err)?;
    let key = week.key();
    build_report_state(
        &store,
        &cfg,
        KIND_WEEKLY,
        &key,
        None,
        DailyEstimateDto {
            slices_used: est.slices_used,
            slices_ignored: est.slices_ignored,
            digests_cached: est.digests_cached,
            digests_pending: est.digests_pending,
            digests_other_model: est.digests_other_model,
            // Weekly material size depends on per-day condensing; not estimated.
            estimated_chars: 0,
            staleness: est.staleness.map(staleness_dto),
            unreported_weeks: Vec::new(),
        },
        default_weekly_export_path(&state.export_dir, &key),
    )
}

/// The monthly report of a calendar month (`2026-11`): weekly coverage, staleness and versions.
#[tauri::command(async)]
pub fn get_monthly_report_state(
    state: tauri::State<'_, AppState>,
    month: String,
    engine: Option<String>,
) -> Result<DailyReportStateDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let cfg = state.load_config().map_err(cmd_err)?;
    let month = Month::parse(&month).map_err(cmd_err)?;
    let engine_name = cfg
        .estimate_engine(engine.as_deref(), &state.data_dir.join("runner"))
        .map_err(cmd_err)?
        .name();
    let budgets = cfg.budgets();
    let language = state.report_language(&[]).map_err(cmd_err)?;
    let est = estimate_monthly(
        &store,
        &Local,
        month,
        None,
        &MaterialContext {
            engine_name: &engine_name,
            budgets: &budgets,
            now: Utc::now(),
            language,
        },
    )
    .map_err(cmd_err)?;
    let key = month.key();
    build_report_state(
        &store,
        &cfg,
        KIND_MONTHLY,
        &key,
        None,
        DailyEstimateDto {
            slices_used: est.slices_used,
            slices_ignored: est.slices_ignored,
            digests_cached: est.weeks_reported,
            digests_pending: est.unreported_weeks.len(),
            digests_other_model: 0,
            estimated_chars: est.estimated_chars,
            staleness: est.staleness.map(staleness_dto),
            unreported_weeks: est.unreported_weeks,
        },
        default_monthly_export_path(&state.export_dir, &key),
    )
}

/// A project's progress taken from its sections in the daily or weekly reports.
#[tauri::command(async)]
pub fn get_project_rollup(
    state: tauri::State<'_, AppState>,
    project_id: i64,
    period_key: String,
    granularity: String,
    since_date: Option<String>,
    until_date: Option<String>,
) -> Result<ProjectRollupDto, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let granularity = Granularity::parse(&granularity).map_err(cmd_err)?;
    let project = store
        .project(project_id)
        .map_err(cmd_err)?
        .with_context(|| format!("no project with ID {project_id}"))
        .map_err(cmd_err)?;
    let rollup = project_rollup(
        &store,
        project_id,
        &project.name,
        granularity,
        since_date.as_deref(),
        until_date.as_deref(),
        &period_key,
    )
    .map_err(cmd_err)?;
    let default_export_path =
        default_project_export_path(&state.export_dir, &project.name, &period_key)
            .to_string_lossy()
            .into_owned();

    Ok(ProjectRollupDto {
        project_id,
        project_name: project.name,
        period_key,
        granularity: granularity.report_kind().to_string(),
        entries: rollup
            .entries
            .into_iter()
            .map(|e| ProjectRollupEntryDto {
                period: e.period_key,
                label: e.label,
                status: e.status,
                markdown: e.markdown,
                updated_at: e.updated_at,
            })
            .collect(),
        unreported_periods: rollup.unreported,
        combined_markdown: rollup.combined_markdown,
        default_export_path,
    })
}

#[tauri::command]
pub async fn digest_slice_now(
    state: tauri::State<'_, AppState>,
    slice_id: i64,
    engine: Option<String>,
) -> Result<StoredDigest, String> {
    let cfg = state.load_config().map_err(cmd_err)?;
    let language = state.report_language(&[]).map_err(cmd_err)?;
    let data_dir = state.data_dir.clone();

    tauri::async_runtime::spawn_blocking(move || {
        let built_engine = cfg
            .build_engine(engine.as_deref(), &data_dir.join("runner"))
            .map_err(cmd_err)?;
        let budgets = cfg.budgets();
        let mut store = Store::open(&data_dir.join(crate::state::DB_FILE)).map_err(cmd_err)?;
        digest_single_slice(
            &mut store,
            built_engine.as_ref(),
            &Local,
            slice_id,
            &budgets,
            Utc::now(),
            language,
        )
        .map_err(cmd_err)
    })
    .await
    .map_err(|e| format!("slice digest task failed: {e}"))?
}

/// Runs one report job on its own thread, streaming progress events tagged with `key`.
///
/// A cancel returns at once, even while a model call is in flight; the abandoned run
/// stops at its next check and saves nothing.
async fn run_report_job<T, F>(
    window: tauri::Window,
    state: &AppState,
    key: String,
    work: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(Arc<AtomicBool>, &mut dyn FnMut(&Progress)) -> Result<T, String> + Send + 'static,
{
    let cancel_flag = start_job(state, &key)?;
    let progress_key = key.clone();
    let progress_flag = cancel_flag.clone();
    let worker_flag = cancel_flag.clone();
    let progress_window = window.clone();
    let (tx, rx) = std::sync::mpsc::channel::<Result<T, String>>();
    std::thread::spawn(move || {
        let mut progress = |p: &Progress| {
            eprintln!("[report {progress_key}] {p}");
            // Once cancelled, the job is gone from the UI; stay quiet.
            if progress_flag.load(Ordering::Relaxed) {
                return;
            }
            let _ = progress_window.emit(
                "report-progress",
                ReportProgressDto {
                    key: progress_key.clone(),
                    progress: p.clone(),
                },
            );
        };
        let _ = tx.send(work(worker_flag, &mut progress));
    });
    let res = tauri::async_runtime::spawn_blocking(move || loop {
        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(r) => return r,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if cancel_flag.load(Ordering::Relaxed) {
                    return Err("report generation was cancelled".to_string());
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err("report task stopped unexpectedly".to_string())
            }
        }
    })
    .await
    .map_err(|e| format!("report task failed: {e}"))
    .and_then(|r| r);

    finish_job(state, &key);
    let _ = window.emit("report-job-finished", key);
    res
}

/// What the front end asks for when generating a report of any kind.
struct GenerateRequest<P> {
    period: P,
    project: Option<String>,
    prompt: Option<String>,
    engine: Option<String>,
    update: Option<bool>,
}

/// The signature `generate_daily`, `generate_weekly` and `generate_monthly` share.
type GenerateFn<P, R> = fn(
    &mut Store,
    &dyn Engine,
    &Local,
    &ReportOptions<P>,
    &mut dyn FnMut(&Progress),
) -> anyhow::Result<R>;

/// Generates a report as job `key`: builds the engine and options from the request and the
/// config, runs `generate` on the job's thread and converts its result with `to_dto`.
async fn run_generation<P, R, T>(
    window: tauri::Window,
    state: &AppState,
    key: String,
    req: GenerateRequest<P>,
    generate: GenerateFn<P, R>,
    to_dto: fn(R) -> T,
) -> Result<T, String>
where
    P: Send + 'static,
    R: 'static,
    T: Send + 'static,
{
    let cfg = state.load_config().map_err(cmd_err)?;
    let language = state.report_language(&[]).map_err(cmd_err)?;
    let data_dir = state.data_dir.clone();
    run_report_job(window, state, key, move |cancel, progress| {
        let engine = cfg
            .build_engine(req.engine.as_deref(), &data_dir.join("runner"))
            .map_err(cmd_err)?;
        let mut store = Store::open(&data_dir.join(crate::state::DB_FILE)).map_err(cmd_err)?;
        let opts = ReportOptions {
            language,
            period: req.period,
            project: req.project,
            prompt: req.prompt,
            concurrency: DEFAULT_CONCURRENCY,
            dry_run: false,
            budgets: cfg.budgets(),
            cancel: Some(cancel),
            mode: ReportMode::from_update(req.update.unwrap_or(false)),
            now: Utc::now(),
        };
        let r = generate(&mut store, engine.as_ref(), &Local, &opts, progress).map_err(cmd_err)?;
        Ok(to_dto(r))
    })
    .await
}

#[tauri::command]
pub async fn generate_daily_report(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    date: String,
    project: Option<String>,
    prompt: Option<String>,
    engine: Option<String>,
    update: Option<bool>,
) -> Result<DailyResultDto, String> {
    let key = job_key(&date, project.as_deref());
    let req = GenerateRequest {
        period: date,
        project,
        prompt,
        engine,
        update,
    };
    run_generation(window, &state, key, req, generate_daily, to_result_dto).await
}

/// Generates the weekly report of an ISO week (`2026-W47`); the job key is the week key.
#[tauri::command]
pub async fn generate_weekly_report(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    week: String,
    prompt: Option<String>,
    engine: Option<String>,
    update: Option<bool>,
) -> Result<WeeklyResultDto, String> {
    let week = Week::parse(&week).map_err(cmd_err)?;
    let key = week.key();
    let req = GenerateRequest {
        period: week,
        project: None,
        prompt,
        engine,
        update,
    };
    run_generation(window, &state, key, req, generate_weekly, |r| {
        WeeklyResultDto {
            slices_used: r.slices_used,
            projects_written: r.projects_written,
            sections_kept: r.sections_kept,
            updated: r.updated,
            warnings: r.warnings,
            anchors: AnchorCheckDto {
                total: r.anchors.total,
                invalid: r.anchors.invalid,
            },
            version_id: r.version_id,
        }
    })
    .await
}

/// Generates the monthly report of a calendar month (`2026-11`); the job key is the month key.
#[tauri::command]
pub async fn generate_monthly_report(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    month: String,
    prompt: Option<String>,
    engine: Option<String>,
    update: Option<bool>,
) -> Result<WeeklyResultDto, String> {
    let month = Month::parse(&month).map_err(cmd_err)?;
    let key = month.key();
    let req = GenerateRequest {
        period: month,
        project: None,
        prompt,
        engine,
        update,
    };
    run_generation(window, &state, key, req, generate_monthly, |r| {
        WeeklyResultDto {
            slices_used: r.slices_used,
            projects_written: r.projects_written,
            sections_kept: r.sections_kept,
            updated: r.updated,
            warnings: r.warnings,
            anchors: AnchorCheckDto {
                total: r.anchors.total,
                invalid: r.anchors.invalid,
            },
            version_id: r.version_id,
        }
    })
    .await
}

/// Requests cancellation of the job for this scope (a date or a week key, plus the
/// project when scoped); a no-op when nothing is running.
#[tauri::command(async)]
pub fn cancel_report_generation(
    state: tauri::State<'_, AppState>,
    period: String,
    project: Option<String>,
) {
    let key = job_key(&period, project.as_deref());
    if let Ok(jobs) = state.report_jobs.lock() {
        if let Some(flag) = jobs.get(&key) {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

/// Keys of the report jobs currently running.
#[tauri::command(async)]
pub fn running_report_jobs(state: tauri::State<'_, AppState>) -> Vec<String> {
    state
        .report_jobs
        .lock()
        .map(|jobs| jobs.keys().cloned().collect())
        .unwrap_or_default()
}

#[tauri::command(async)]
pub fn save_report_edit(
    state: tauri::State<'_, AppState>,
    report_type: Option<String>,
    period_key: String,
    project_id: Option<i64>,
    project: Option<String>,
    markdown: String,
) -> Result<i64, String> {
    let mut store = state.open_store().map_err(cmd_err)?;
    let kind = report_type.as_deref().unwrap_or(KIND_DAILY);
    let pid = resolve_project_id(&store, project_id, project.as_deref())?;
    let language = state.report_language(&[]).map_err(cmd_err)?;
    versions::save_user_edit(&mut store, kind, &period_key, pid, &markdown, language)
        .map_err(cmd_err)
}

#[tauri::command(async)]
pub fn set_report_status(
    state: tauri::State<'_, AppState>,
    report_type: Option<String>,
    period_key: String,
    project_id: Option<i64>,
    project: Option<String>,
    status: String,
) -> Result<bool, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let kind = report_type.as_deref().unwrap_or(KIND_DAILY);
    let pid = resolve_project_id(&store, project_id, project.as_deref())?;
    store
        .set_report_status(kind, &period_key, pid, &status)
        .map_err(cmd_err)
}

#[tauri::command(async)]
pub fn restore_report_version(
    state: tauri::State<'_, AppState>,
    report_type: Option<String>,
    period_key: String,
    project_id: Option<i64>,
    project: Option<String>,
    version_id: i64,
) -> Result<bool, String> {
    let store = state.open_store().map_err(cmd_err)?;
    let kind = report_type.as_deref().unwrap_or(KIND_DAILY);
    let pid = resolve_project_id(&store, project_id, project.as_deref())?;
    store
        .set_current_report_version(kind, &period_key, pid, version_id)
        .map_err(cmd_err)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub fn export_daily_report(
    state: tauri::State<'_, AppState>,
    report_type: Option<String>,
    period_key: String,
    project_name: Option<String>,
    markdown: String,
    custom_path: Option<String>,
    anchor_mode: String,
    force: bool,
) -> Result<ExportResultDto, String> {
    let mode = AnchorExportMode::parse(&anchor_mode).map_err(cmd_err)?;
    let target = match custom_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        Some(p) => PathBuf::from(p),
        None if report_type.as_deref() == Some("project") => default_project_export_path(
            &state.export_dir,
            project_name.as_deref().unwrap_or("project"),
            &period_key,
        ),
        None if report_type.as_deref() == Some(KIND_WEEKLY) => {
            default_weekly_export_path(&state.export_dir, &period_key)
        }
        None if report_type.as_deref() == Some(KIND_MONTHLY) => {
            default_monthly_export_path(&state.export_dir, &period_key)
        }
        None => default_daily_export_path(&state.export_dir, &period_key),
    };
    match export_markdown(&state.data_dir, &target, &markdown, mode, force).map_err(cmd_err)? {
        ExportOutcome::Written { path, bytes } => Ok(ExportResultDto {
            status: "written".into(),
            path: path.to_string_lossy().into_owned(),
            bytes: Some(bytes),
            existing_markdown: None,
        }),
        ExportOutcome::Conflict {
            path,
            existing_markdown,
        } => Ok(ExportResultDto {
            status: "conflict".into(),
            path: path.to_string_lossy().into_owned(),
            bytes: None,
            existing_markdown: Some(existing_markdown),
        }),
    }
}

#[tauri::command]
pub async fn pick_export_directory(default_dir: Option<String>) -> Result<Option<String>, String> {
    let start = default_dir.map(|raw| PathBuf::from(raw.trim()));
    let picked = crate::pickers::choose_folder("Select Export Folder".to_string(), start).await?;
    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}
