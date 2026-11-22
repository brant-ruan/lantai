//! Monthly report generation: one request per project merges its weekly report sections
//! across the month into main threads (docs/SPEC.md, "Weekly and monthly reports").
//!
//! Monthly reports consume only the weekly layer (`reports` of kind `weekly`), stripping
//! each week's program-written statistics line and re-computing monthly statistics from
//! the month's slices. Versions, manifests, anchor verification and incremental updates
//! work identically to daily and weekly reports.

mod overview;

pub use overview::{group_weeks, month_overviews, MonthOverview};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use aj_store::{NewReportVersion, Store};
use anyhow::{bail, Result};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use sha2::{Digest as _, Sha256};

use crate::anchors::{self, AnchorCheck};
use crate::digest;
use crate::engine::Engine;
use crate::lang::Language;
use crate::manifest::{self, project_key, CurrentInputs, ProjectEntry};
use crate::material::{self, Material, MaterialContext, ProjectMaterial, UNASSIGNED};
use crate::period::{Month, Week};
use crate::progress::Progress;
use crate::prompt_library::{self, PromptVars, KIND_MONTHLY, KIND_WEEKLY};
use crate::report_io::{self, Base, ReportMode, Staleness};
use crate::stage1::is_cancelled;
use crate::structure::{self, ProjectSection};
use crate::summarize::Budgets;

#[derive(Debug, Clone)]
pub struct MonthlyOptions {
    pub month: Month,
    /// Only include the project with this name.
    pub project: Option<String>,
    /// Name of the prompt to use; `None` uses the language's default monthly prompt.
    pub prompt: Option<String>,
    /// Language the report is written in.
    pub language: &'static Language,
    /// Concurrent model calls for project sections.
    pub concurrency: usize,
    /// Only assemble the material; call no model and save nothing.
    pub dry_run: bool,
    pub budgets: Budgets,
    pub cancel: Option<Arc<AtomicBool>>,
    pub mode: ReportMode,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MonthlyEstimate {
    pub slices_used: usize,
    pub slices_ignored: usize,
    /// Number of weeks in this month that already have a usable weekly report.
    pub weeks_reported: usize,
    /// Active weeks in this month that still lack a weekly report (in the requested scope).
    pub unreported_weeks: Vec<String>,
    pub estimated_chars: usize,
    /// `None` when the report is up to date or has no version to compare with.
    pub staleness: Option<Staleness>,
}

#[derive(Debug, Clone, Default)]
pub struct MonthlyResult {
    pub markdown: String,
    pub slices_used: usize,
    pub slices_ignored: usize,
    pub weeks_used: usize,
    pub unreported_weeks: Vec<String>,
    pub updated: bool,
    pub projects_written: Vec<String>,
    pub sections_kept: usize,
    pub warnings: Vec<String>,
    pub anchors: AnchorCheck,
    /// Characters of all project requests together.
    pub stage2_prompt_chars: usize,
    pub version_id: Option<i64>,
    pub prompt_name: String,
}

struct ProjectWeeklySections {
    name: String,
    weeks: Vec<(Week, String)>,
}

struct CollectedMonth {
    materials: Vec<Material>,
    ignored: usize,
    by_project: BTreeMap<String, ProjectWeeklySections>,
    current: CurrentInputs,
    weeks_used: usize,
    unreported_weeks: Vec<String>,
    estimated_chars: usize,
}

fn parse_weekly_sections(
    markdown: &str,
    week: Week,
    known_projects: &[String],
) -> Vec<ProjectSection> {
    if let Ok(parsed) = structure::validate_user_edit(markdown, &week.title(), known_projects) {
        return parsed.sections;
    }
    known_projects
        .iter()
        .chain(std::iter::once(&UNASSIGNED.to_string()))
        .filter_map(|name| {
            structure::extract_project_section(markdown, name).map(|body| ProjectSection {
                project: name.clone(),
                body,
            })
        })
        .collect()
}

/// The month's weekly report sections by project, and the inputs a monthly report is built
/// from. Reads only weekly reports: the month's slices do not enter its inputs.
struct MonthSections {
    by_project: BTreeMap<String, ProjectWeeklySections>,
    current: CurrentInputs,
    /// Weeks with at least one usable section.
    reported: BTreeSet<Week>,
    estimated_chars: usize,
}

fn month_sections(store: &Store, month: Month, project: Option<&str>) -> Result<MonthSections> {
    let project_rows = store.list_projects()?;
    let known_projects: Vec<String> = project_rows
        .iter()
        .map(|p| p.project.name.clone())
        .collect();
    let project_ids: BTreeMap<String, i64> = project_rows
        .iter()
        .map(|p| (p.project.name.to_lowercase(), p.project.id))
        .collect();

    let mut by_project: BTreeMap<String, ProjectWeeklySections> = BTreeMap::new();
    let mut reported = BTreeSet::new();
    let mut estimated_chars = 0usize;
    for w in month.weeks() {
        let Some(version) = store.current_report_version(KIND_WEEKLY, &w.key(), None)? else {
            continue;
        };
        for sec in parse_weekly_sections(&version.markdown, w, &known_projects) {
            if project.is_some_and(|p| !p.eq_ignore_ascii_case(&sec.project)) {
                continue;
            }
            let clean_body = report_io::strip_stats(&sec.body);
            if clean_body.is_empty() {
                continue;
            }
            let pid = if sec.project.eq_ignore_ascii_case(UNASSIGNED) {
                None
            } else {
                project_ids.get(&sec.project.to_lowercase()).copied()
            };
            estimated_chars += clean_body.chars().count();
            by_project
                .entry(project_key(pid))
                .or_insert_with(|| ProjectWeeklySections {
                    name: sec.project.clone(),
                    weeks: Vec::new(),
                })
                .weeks
                .push((w, clean_body));
            reported.insert(w);
        }
    }

    let mut current = CurrentInputs::new();
    for (key, proj) in &by_project {
        let slices: BTreeMap<i64, String> = proj
            .weeks
            .iter()
            .map(|(w, body)| {
                (
                    i64::from(w.monday().num_days_from_ce()),
                    manifest::section_hash(body),
                )
            })
            .collect();
        current.insert(
            key.clone(),
            ProjectEntry {
                name: proj.name.clone(),
                slices,
                section_hash: String::new(),
            },
        );
    }
    Ok(MonthSections {
        by_project,
        current,
        reported,
        estimated_chars,
    })
}

fn collect_month<Tz: TimeZone>(
    store: &Store,
    tz: &Tz,
    month: Month,
    project: Option<&str>,
    ctx: &MaterialContext,
) -> Result<CollectedMonth>
where
    Tz::Offset: std::fmt::Display,
{
    let sections = month_sections(store, month, project)?;
    let mut materials = Vec::new();
    let mut ignored = 0usize;
    let mut unreported_weeks = Vec::new();
    for w in month.weeks() {
        let mut week_has_slices = false;
        for date in w.dates() {
            let day = material::collect(store, tz, &date.to_string(), project, ctx)?;
            week_has_slices |= !day.materials.is_empty();
            materials.extend(day.materials);
            ignored += day.ignored;
        }
        if week_has_slices && !sections.reported.contains(&w) {
            unreported_weeks.push(w.key());
        }
    }

    Ok(CollectedMonth {
        materials,
        ignored,
        by_project: sections.by_project,
        current: sections.current,
        weeks_used: sections.reported.len(),
        unreported_weeks,
        estimated_chars: sections.estimated_chars,
    })
}

/// Whether the current monthly report is out of date. Cheap: reads only weekly reports.
pub fn staleness(store: &Store, month: Month, project: Option<&str>) -> Result<Option<Staleness>> {
    let sections = month_sections(store, month, project)?;
    report_io::staleness_for_inputs(
        store,
        KIND_MONTHLY,
        &month.key(),
        project,
        &sections.current,
    )
}

/// Preview the month's weekly report coverage, slice counts and whether the current
/// monthly report is out of date, without calling any model.
pub fn estimate_monthly<Tz: TimeZone>(
    store: &Store,
    tz: &Tz,
    month: Month,
    project: Option<&str>,
    ctx: &MaterialContext,
) -> Result<MonthlyEstimate>
where
    Tz::Offset: std::fmt::Display,
{
    let c = collect_month(store, tz, month, project, ctx)?;
    Ok(MonthlyEstimate {
        slices_used: c.materials.len(),
        slices_ignored: c.ignored,
        weeks_reported: c.weeks_used,
        unreported_weeks: c.unreported_weeks,
        estimated_chars: c.estimated_chars,
        staleness: report_io::staleness_for_inputs(
            store,
            KIND_MONTHLY,
            &month.key(),
            project,
            &c.current,
        )?,
    })
}

/// One project's month, ready to write.
struct ProjectMonth {
    name: String,
    weeks: Vec<(Week, String)>,
    stats: String,
    update: String,
}

/// What a project's worker produced.
struct Written {
    name: String,
    prompt_chars: usize,
    section: Result<ProjectSection>,
    warnings: Vec<String>,
}

fn render_month(name: &str, weeks: &[(Week, String)]) -> ProjectMaterial {
    let mut text = String::new();
    for (w, body) in weeks {
        let _ = write!(
            text,
            "\n=== {} ({} ~ {}) ===\n{}\n",
            w.key(),
            w.monday().format("%m-%d"),
            w.sunday().format("%m-%d"),
            body.trim_end()
        );
    }
    ProjectMaterial {
        project: name.to_string(),
        text,
        slice_count: weeks.len(),
        identity: String::new(),
    }
}

fn write_project(
    engine: &dyn Engine,
    instructions: &str,
    title: &str,
    pm: ProjectMonth,
    opts: &MonthlyOptions,
) -> Written {
    let mut warnings = Vec::new();
    let material = render_month(&pm.name, &pm.weeks);
    let mut prompt = report_io::build_prompt(instructions, &[material], "Project");
    prompt.push_str(&pm.update);
    let section = if is_cancelled(opts.cancel.as_ref()) {
        Err(anyhow::anyhow!("report generation was cancelled"))
    } else {
        report_io::write_sections(
            engine,
            &prompt,
            title,
            std::slice::from_ref(&pm.name),
            opts.cancel.as_ref(),
            &mut warnings,
            &mut |_| {},
        )
        .and_then(|mut v| {
            v.pop()
                .ok_or_else(|| anyhow::anyhow!("the model wrote no section"))
        })
        .map(|s| report_io::with_stats(s, &pm.stats))
    };
    Written {
        name: pm.name,
        prompt_chars: prompt.chars().count(),
        section,
        warnings,
    }
}

pub fn generate_monthly<Tz: TimeZone>(
    store: &mut Store,
    engine: &dyn Engine,
    tz: &Tz,
    opts: &MonthlyOptions,
    progress: &mut dyn FnMut(&Progress),
) -> Result<MonthlyResult>
where
    Tz::Offset: std::fmt::Display,
{
    let mut result = MonthlyResult::default();
    let engine_name = engine.name();
    let period = opts.month.key();
    let title = opts.month.title();
    let lang = opts.language;
    let prompt_row = prompt_library::resolve(store, KIND_MONTHLY, lang, opts.prompt.as_deref())?;
    result.prompt_name = prompt_row.name.clone();

    // ---- Collect weekly sections & slices ------------------------------------
    let ctx = MaterialContext {
        engine_name: &engine_name,
        budgets: &opts.budgets,
        now: opts.now,
        language: lang,
    };
    let collected = collect_month(store, tz, opts.month, opts.project.as_deref(), &ctx)?;
    result.slices_used = collected.materials.len();
    result.slices_ignored = collected.ignored;
    result.weeks_used = collected.weeks_used;
    result.unreported_weeks = collected.unreported_weeks;

    if collected.by_project.is_empty() {
        if !result.unreported_weeks.is_empty() {
            bail!(
                "{period} has no weekly reports yet (active weeks without a weekly report: {}); generate the weekly reports first",
                result.unreported_weeks.join(", ")
            );
        }
        bail!("{period} has no weekly reports with usable project sections");
    }
    if !result.unreported_weeks.is_empty() {
        result.warnings.push(format!(
            "active week(s) without a weekly report were not included: {}",
            result.unreported_weeks.join(", ")
        ));
    }

    let instructions = prompt_library::assemble(
        &prompt_row.body,
        &PromptVars {
            date: None,
            period: Some(&period),
            project: opts.project.as_deref(),
        },
        lang,
    );
    let prompt_hash = digest::hash_parts(&[&instructions]);
    let instructions = prompt_library::with_language(&instructions, lang);
    let report_pid = report_io::report_project_id(store, opts.project.as_deref())?;
    let current = collected.current;

    // ---- Decide what to write ------------------------------------------------
    let base: Option<Base> = match opts.mode {
        ReportMode::Update if !opts.dry_run => {
            let base = report_io::load_base(store, KIND_MONTHLY, &period, report_pid)?;
            if base.is_none() {
                result
                    .warnings
                    .push("no previous version to update; regenerated the whole report".into());
            }
            base
        }
        _ => None,
    };
    let Some((write_keys, removed_keys)) =
        report_io::plan_writes(base.as_ref(), &current, &prompt_hash, lang)
    else {
        bail!("the report for {period} is already up to date");
    };

    let mut materials_by_project: BTreeMap<String, Vec<&Material>> = BTreeMap::new();
    for m in &collected.materials {
        materials_by_project
            .entry(project_key(m.project_id))
            .or_default()
            .push(m);
    }

    let mut months: Vec<ProjectMonth> = Vec::new();
    for key in &write_keys {
        let Some(proj) = collected.by_project.get(key) else {
            continue;
        };
        let items = materials_by_project
            .get(key)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let stats = if items.is_empty() {
            String::new()
        } else {
            report_io::stats_line(items, lang)
        };
        let update = match &base {
            Some(b) => {
                report_io::update_instructions(store, b, &BTreeSet::from([key.clone()]), &current)?
            }
            None => String::new(),
        };
        months.push(ProjectMonth {
            name: proj.name.clone(),
            weeks: proj.weeks.clone(),
            stats,
            update,
        });
    }
    let expected: Vec<String> = months.iter().map(|m| m.name.clone()).collect();

    if opts.dry_run {
        let mut out = String::new();
        for pm in &months {
            let material = render_month(&pm.name, &pm.weeks);
            out.push_str(&report_io::build_prompt(
                &instructions,
                &[material],
                "Project",
            ));
            if !pm.stats.is_empty() {
                let _ = writeln!(out, "\n[statistics written by the program] {}\n", pm.stats);
            }
        }
        result.stage2_prompt_chars = out.chars().count();
        result.markdown = out;
        return Ok(result);
    }

    // ---- Write each project, in parallel -------------------------------------
    progress(&Progress::WritingProjects {
        count: months.len(),
        concurrency: opts.concurrency.max(1),
    });
    let mut written: Vec<Written> = Vec::new();
    let mut queue = months.into_iter();
    loop {
        if is_cancelled(opts.cancel.as_ref()) {
            bail!("report generation was cancelled");
        }
        let batch: Vec<ProjectMonth> = queue.by_ref().take(opts.concurrency.max(1)).collect();
        if batch.is_empty() {
            break;
        }
        let done: Vec<Written> = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .into_iter()
                .map(|pm| {
                    let (instructions, title) = (&instructions, &title);
                    scope.spawn(move || write_project(engine, instructions, title, pm, opts))
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("monthly writer thread panicked"))
                .collect()
        });
        for w in &done {
            progress(&Progress::ProjectWritten {
                project: w.name.clone(),
                ok: w.section.is_ok(),
            });
        }
        written.extend(done);
    }

    // ---- Assemble, verify and save -------------------------------------------
    let mut sections = Vec::new();
    let mut failures = Vec::new();
    for w in written {
        result.stage2_prompt_chars += w.prompt_chars;
        result.warnings.extend(w.warnings);
        match w.section {
            Ok(s) => sections.push(s),
            Err(e) => failures.push(format!("{}: {e:#}", w.name)),
        }
    }
    if !failures.is_empty() {
        bail!(
            "some project sections could not be written: {}",
            failures.join("; ")
        );
    }

    let all_sections = match &base {
        None => sections,
        Some(b) => {
            let mut removed_names = Vec::new();
            for key in &removed_keys {
                removed_names.push(report_io::project_name(
                    store,
                    key,
                    &b.manifest.projects[key],
                )?);
            }
            result.updated = true;
            result.sections_kept = b
                .sections
                .iter()
                .filter(|s| {
                    let same = |n: &String| n.to_lowercase() == s.project.to_lowercase();
                    !expected.iter().any(same) && !removed_names.iter().any(same)
                })
                .count();
            structure::splice_sections(&b.sections, &sections, &removed_names)
        }
    };
    let (markdown, check) = anchors::verify(
        &structure::compose_report(&title, &all_sections),
        &material::valid_anchors(&collected.materials),
    );
    result.anchors = check;
    result.markdown = markdown;
    result.projects_written = expected;

    let new_manifest = report_io::new_manifest(
        prompt_hash,
        current,
        base.as_ref(),
        &write_keys,
        &result.markdown,
    );
    if is_cancelled(opts.cancel.as_ref()) {
        bail!("report generation was cancelled");
    }
    let input_hash = digest::hex(&Sha256::digest(result.markdown.as_bytes()));
    result.version_id = Some(store.save_report_version(&NewReportVersion {
        report_type: KIND_MONTHLY,
        period_key: &period,
        project_id: report_pid,
        origin: "generated",
        markdown: &result.markdown,
        engine: Some(&engine_name),
        input_hash: Some(&input_hash),
        prompt_id: Some(prompt_row.id),
        prompt_name: Some(&prompt_row.name),
        prompt_snapshot: Some(&prompt_row.body),
        input_manifest: Some(&new_manifest.to_json()),
        language: lang.tag,
    })?);
    if base.as_ref().is_some_and(|b| b.is_final) {
        store.set_report_status(KIND_MONTHLY, &period, report_pid, "draft")?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
