//! Weekly report generation: one request per project clusters its week into a few
//! threads of work (docs/SPEC.md, "Weekly and monthly reports").
//!
//! The material of a project is its week day by day: each day is that day's slice
//! digests, or the day's task clusters when the day is busy or the week too large (the
//! cache is shared with daily reports). Statistics are computed here and written into the
//! report by the program. Versions, manifests and updates work as for daily reports.

mod overview;

pub use overview::{group_days, week_overviews, WeekOverview};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use aj_store::{NewReportVersion, Store};
use anyhow::{bail, Result};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use sha2::{Digest as _, Sha256};

use crate::anchors::{self, AnchorCheck};
use crate::brief::{self, CachedBrief};
use crate::digest;
use crate::engine::Engine;
use crate::lang::Language;
use crate::manifest::project_key;
use crate::material::{self, Material, MaterialContext, ProjectMaterial};
use crate::period::Week;
use crate::progress::Progress;
use crate::prompt_library::{self, PromptVars, KIND_WEEKLY};
use crate::report_io::{self, Base, ReportMode, Staleness};
use crate::stage1::{self, is_cancelled, Stage1Options, Stage1Stats};
use crate::structure::{self, ProjectSection};
use crate::summarize::Budgets;

#[derive(Debug, Clone)]
pub struct WeeklyOptions {
    pub week: Week,
    /// Only include the project with this name.
    pub project: Option<String>,
    /// Name of the prompt to use; `None` uses the language's default weekly prompt.
    pub prompt: Option<String>,
    /// Language the report is written in.
    pub language: &'static Language,
    /// Concurrent model calls, both for digests and for project sections.
    pub concurrency: usize,
    /// Only assemble the material; call no model and save nothing.
    pub dry_run: bool,
    pub budgets: Budgets,
    pub cancel: Option<Arc<AtomicBool>>,
    pub mode: ReportMode,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WeeklyEstimate {
    pub slices_used: usize,
    pub slices_ignored: usize,
    pub digests_cached: usize,
    pub digests_pending: usize,
    /// Of the pending ones, those with a digest from another model.
    pub digests_other_model: usize,
    /// `None` when the report is up to date or has no version to compare with.
    pub staleness: Option<Staleness>,
}

#[derive(Debug, Clone, Default)]
pub struct WeeklyResult {
    pub markdown: String,
    pub slices_used: usize,
    pub slices_ignored: usize,
    pub stage1: Stage1Stats,
    /// Project-days whose material was replaced by that day's task clusters.
    pub days_condensed: usize,
    pub briefs_cached: usize,
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

struct Collected {
    materials: Vec<Material>,
    ignored: usize,
}

fn collect_week<Tz: TimeZone>(
    store: &Store,
    tz: &Tz,
    week: Week,
    project: Option<&str>,
    ctx: &MaterialContext,
) -> Result<Collected>
where
    Tz::Offset: std::fmt::Display,
{
    let mut out = Collected {
        materials: Vec::new(),
        ignored: 0,
    };
    for date in week.dates() {
        let day = material::collect(store, tz, &date.to_string(), project, ctx)?;
        out.materials.extend(day.materials);
        out.ignored += day.ignored;
    }
    Ok(out)
}

/// Preview the week's slice counts and whether the current report is out of date,
/// without calling any model.
pub fn estimate_weekly<Tz: TimeZone>(
    store: &Store,
    tz: &Tz,
    week: Week,
    project: Option<&str>,
    ctx: &MaterialContext,
) -> Result<WeeklyEstimate>
where
    Tz::Offset: std::fmt::Display,
{
    let c = collect_week(store, tz, week, project, ctx)?;
    let cached = c.materials.iter().filter(|m| m.digest.is_some()).count();
    Ok(WeeklyEstimate {
        slices_used: c.materials.len(),
        slices_ignored: c.ignored,
        digests_cached: cached,
        digests_pending: c.materials.len() - cached,
        digests_other_model: material::pending_from_other_engine(&c.materials, ctx.engine_name),
        staleness: report_io::staleness(store, KIND_WEEKLY, &week.key(), project, &c.materials)?,
    })
}

/// One project's week, ready for clustering and writing.
struct ProjectWeek {
    name: String,
    /// The days with activity, each rendered like a project in a daily report.
    days: Vec<(NaiveDate, ProjectMaterial)>,
    /// Cached clusters of those days, for this project.
    cached: Vec<CachedBrief>,
    hints: String,
    stats: String,
    /// The update part of the request (empty for a full regeneration).
    update: String,
}

/// What a project's worker produced.
struct Written {
    name: String,
    prompt_chars: usize,
    section: Result<ProjectSection>,
    warnings: Vec<String>,
}

/// Same session on several days: the slices belong together.
fn session_hints(items: &[&Material]) -> String {
    let mut by_session: BTreeMap<i64, Vec<&Material>> = BTreeMap::new();
    for m in items {
        by_session.entry(m.session.id).or_default().push(m);
    }
    let mut out = String::new();
    for slices in by_session.values().filter(|v| v.len() > 1) {
        let list: Vec<String> = slices
            .iter()
            .map(|m| format!("S{} ({})", m.slice.id, &m.slice.local_date[5..]))
            .collect();
        let _ = writeln!(
            out,
            "- {}: one session continued across days",
            list.join(", ")
        );
    }
    if out.is_empty() {
        return out;
    }
    format!("\nSame-session hints (computed, not inferred):\n{out}")
}

fn render_week(name: &str, days: &[(NaiveDate, ProjectMaterial)], hints: &str) -> ProjectMaterial {
    let mut text = String::new();
    for (date, m) in days {
        let _ = write!(
            text,
            "\n=== {} ({}) ===\n{}\n",
            date,
            date.format("%a"),
            m.text.trim_end()
        );
    }
    text.push_str(hints);
    ProjectMaterial {
        project: name.to_string(),
        text,
        slice_count: days.iter().map(|(_, m)| m.slice_count).sum(),
        identity: String::new(),
    }
}

/// Cluster busy days (or the largest days of an oversized week) of one project.
fn cluster_project(
    engine: &dyn Engine,
    engine_name: &str,
    lang: &Language,
    pw: &mut ProjectWeek,
    budget: usize,
) -> brief::Condensed {
    let mut day_materials: Vec<ProjectMaterial> = pw.days.iter().map(|(_, m)| m.clone()).collect();
    let condensed = brief::condense_if_needed(
        engine,
        engine_name,
        lang,
        &mut day_materials,
        &pw.cached,
        budget,
    );
    for (day, m) in pw.days.iter_mut().zip(day_materials) {
        day.1 = m;
    }
    condensed
}

/// Write one project's section from its (already clustered) week.
fn write_project(
    engine: &dyn Engine,
    instructions: &str,
    title: &str,
    pw: ProjectWeek,
    opts: &WeeklyOptions,
) -> Written {
    let mut warnings = Vec::new();
    let material = render_week(&pw.name, &pw.days, &pw.hints);
    let mut prompt = report_io::build_prompt(instructions, &[material], "Project");
    prompt.push_str(&pw.update);
    let section = if is_cancelled(opts.cancel.as_ref()) {
        Err(anyhow::anyhow!("report generation was cancelled"))
    } else {
        report_io::write_sections(
            engine,
            &prompt,
            title,
            std::slice::from_ref(&pw.name),
            opts.cancel.as_ref(),
            &mut warnings,
            &mut |_| {},
        )
        .and_then(|mut v| {
            v.pop()
                .ok_or_else(|| anyhow::anyhow!("the model wrote no section"))
        })
        .map(|s| report_io::with_stats(s, &pw.stats))
    };
    Written {
        name: pw.name,
        prompt_chars: prompt.chars().count(),
        section,
        warnings,
    }
}

pub fn generate_weekly<Tz: TimeZone>(
    store: &mut Store,
    engine: &dyn Engine,
    tz: &Tz,
    opts: &WeeklyOptions,
    progress: &mut dyn FnMut(&Progress),
) -> Result<WeeklyResult>
where
    Tz::Offset: std::fmt::Display,
{
    let mut result = WeeklyResult::default();
    let engine_name = engine.name();
    let period = opts.week.key();
    let title = opts.week.title();
    let lang = opts.language;
    let prompt_row = prompt_library::resolve(store, KIND_WEEKLY, lang, opts.prompt.as_deref())?;
    result.prompt_name = prompt_row.name.clone();

    // ---- Collect material --------------------------------------------------
    let ctx = MaterialContext {
        engine_name: &engine_name,
        budgets: &opts.budgets,
        now: opts.now,
        language: lang,
    };
    let collected = collect_week(store, tz, opts.week, opts.project.as_deref(), &ctx)?;
    let materials = collected.materials;
    result.slices_ignored = collected.ignored;
    if materials.is_empty() {
        bail!(
            "{period} has no usable session slices ({} ignored)",
            result.slices_ignored
        );
    }
    result.slices_used = materials.len();
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
    let current = report_io::current_inputs(&materials);

    // ---- Decide what to write ------------------------------------------------
    let base: Option<Base> = match opts.mode {
        ReportMode::Update if !opts.dry_run => {
            let base = report_io::load_base(store, KIND_WEEKLY, &period, report_pid)?;
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
    let (mut targets, others): (Vec<Material>, Vec<Material>) = materials
        .into_iter()
        .partition(|m| write_keys.contains(&project_key(m.project_id)));

    // ---- Stage 1 ---------------------------------------------------------------
    stage1::run(
        store,
        engine,
        &engine_name,
        &mut targets,
        &Stage1Options {
            concurrency: opts.concurrency,
            dry_run: opts.dry_run,
            budgets: &opts.budgets,
            cancel: opts.cancel.as_ref(),
        },
        &mut result.stage1,
        progress,
    )?;
    result.warnings.append(&mut result.stage1.warnings);

    // ---- Project weeks (needs the store, so on this thread) --------------------
    let mut by_project: BTreeMap<String, Vec<&Material>> = BTreeMap::new();
    for m in &targets {
        by_project
            .entry(project_key(m.project_id))
            .or_default()
            .push(m);
    }
    let mut weeks: Vec<ProjectWeek> = Vec::new();
    for (key, items) in &by_project {
        let mut by_day: BTreeMap<&str, Vec<&Material>> = BTreeMap::new();
        for m in items {
            by_day
                .entry(m.slice.local_date.as_str())
                .or_default()
                .push(m);
        }
        let mut days = Vec::new();
        let mut cached = Vec::new();
        for (date, day_items) in by_day {
            let Some(rendered) = material::render_by_project(&day_items, tz).pop() else {
                continue;
            };
            cached.extend(CachedBrief::usable(store.project_briefs(date)?, lang));
            days.push((date.parse::<NaiveDate>()?, rendered));
        }
        let update = match &base {
            Some(b) => {
                report_io::update_instructions(store, b, &BTreeSet::from([key.clone()]), &current)?
            }
            None => String::new(),
        };
        weeks.push(ProjectWeek {
            name: items[0].project_name.clone(),
            days,
            cached,
            hints: session_hints(items),
            stats: report_io::stats_line(items, lang),
            update,
        });
    }
    let expected: Vec<String> = weeks.iter().map(|w| w.name.clone()).collect();

    if opts.dry_run {
        let mut out = String::new();
        for pw in &weeks {
            let material = render_week(&pw.name, &pw.days, &pw.hints);
            out.push_str(&report_io::build_prompt(
                &instructions,
                &[material],
                "Project",
            ));
            let _ = writeln!(out, "\n[statistics written by the program] {}\n", pw.stats);
        }
        result.stage2_prompt_chars = out.chars().count();
        result.markdown = out;
        return Ok(result);
    }

    // ---- Cluster busy days of all projects at once, and save them right away -------
    // Saved before writing, so a cancelled or failed run keeps the clusters it paid for.
    if is_cancelled(opts.cancel.as_ref()) {
        bail!("report generation was cancelled");
    }
    progress(&Progress::ClusteringDays);
    let budget = opts.budgets.stage2_chars;
    let clustered: Vec<brief::Condensed> = std::thread::scope(|scope| {
        let handles: Vec<_> = weeks
            .iter_mut()
            .map(|pw| {
                let engine_name = &engine_name;
                scope.spawn(move || cluster_project(engine, engine_name, lang, pw, budget))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("clustering thread panicked"))
            .collect()
    });
    for (pw, c) in weeks.iter().zip(clustered) {
        for (i, b) in &c.to_save {
            store.save_project_brief(&pw.days[*i].0.to_string(), &b.to_row(lang, &engine_name))?;
        }
        result.days_condensed += c.projects;
        result.briefs_cached += c.cached;
        result.warnings.extend(
            c.failures
                .iter()
                .map(|f| format!("day clustering failed, full material kept: {f}")),
        );
    }

    // ---- Write each project, in parallel ---------------------------------------------
    progress(&Progress::WritingProjects {
        count: weeks.len(),
        concurrency: opts.concurrency.max(1),
    });
    let mut written: Vec<Written> = Vec::new();
    let mut queue = weeks.into_iter();
    loop {
        if is_cancelled(opts.cancel.as_ref()) {
            bail!("report generation was cancelled");
        }
        let batch: Vec<ProjectWeek> = queue.by_ref().take(opts.concurrency.max(1)).collect();
        if batch.is_empty() {
            break;
        }
        let done: Vec<Written> = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .into_iter()
                .map(|pw| {
                    let (instructions, title) = (&instructions, &title);
                    scope.spawn(move || write_project(engine, instructions, title, pw, opts))
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("weekly writer thread panicked"))
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

    // ---- Assemble, verify and save --------------------------------
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
    let all_materials: Vec<Material> = targets.into_iter().chain(others).collect();
    let (markdown, check) = anchors::verify(
        &structure::compose_report(&title, &all_sections),
        &material::valid_anchors(&all_materials),
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
        &engine_name,
    );
    if is_cancelled(opts.cancel.as_ref()) {
        bail!("report generation was cancelled");
    }
    let input_hash = digest::hex(&Sha256::digest(result.markdown.as_bytes()));
    result.version_id = Some(store.save_report_version(&NewReportVersion {
        report_type: KIND_WEEKLY,
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
        store.set_report_status(KIND_WEEKLY, &period, report_pid, "draft")?;
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
