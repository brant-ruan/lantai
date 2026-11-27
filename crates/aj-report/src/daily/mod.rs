//! Daily report generation: orchestrates stage 1 (slice digests) and stage 2 (the report).
//!
//! The pieces live elsewhere: [`crate::material`] gathers and renders the slices,
//! [`crate::summarize`] digests one slice, [`crate::brief`] clusters projects,
//! [`crate::manifest`] records and compares what a version was generated from, and
//! [`crate::anchors`] verifies the result.
//!
//! A report is either regenerated in full or updated: an update writes only the projects
//! whose material changed since the current version and keeps every other section
//! (user edits included) exactly as it is.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use aj_store::{NewReportVersion, ReportOrigin, ReportStatus, Store};
use anyhow::{bail, Result};
use chrono::TimeZone;
use sha2::{Digest as _, Sha256};

use crate::anchors::{self, AnchorCheck};
use crate::brief::{self, CachedBrief};
use crate::digest::{self, StoredDigest};
use crate::engine::Engine;
use crate::manifest::project_key;
use crate::material::{self, Material, MaterialContext, ProjectMaterial};
use crate::progress::Progress;
use crate::prompt_library::{self, PromptVars, KIND_DAILY};
use crate::report_io::{self, ReportMode, Staleness};
use crate::stage1::{self, Stage1Options, Stage1Stats};
use crate::structure::{self, ProjectSection};

/// Options of a daily report; the period is a local date, `YYYY-MM-DD`.
pub type DailyOptions = report_io::ReportOptions<String>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DailyEstimate {
    pub slices_used: usize,
    pub slices_ignored: usize,
    pub digests_cached: usize,
    pub digests_pending: usize,
    /// Of the pending ones, those with a digest from another model.
    pub digests_other_model: usize,
    pub estimated_chars: usize,
    /// `None` when the report is up to date or has no version to compare with.
    pub staleness: Option<Staleness>,
}

#[derive(Debug, Clone, Default)]
pub struct DailyResult {
    pub markdown: String,
    pub slices_used: usize,
    pub slices_ignored: usize,
    pub stage1: Stage1Stats,
    /// Projects whose material was condensed into task clusters before writing.
    pub projects_condensed: usize,
    /// Task clusters reused from cache.
    pub briefs_cached: usize,
    /// Whether this run updated the previous version instead of regenerating everything.
    pub updated: bool,
    /// Projects whose sections were written in this run.
    pub projects_written: Vec<String>,
    /// Sections carried over unchanged from the previous version (updates only).
    pub sections_kept: usize,
    /// Things that went wrong but were worked around, with the reason.
    pub warnings: Vec<String>,
    pub anchors: AnchorCheck,
    pub stage2_prompt_chars: usize,
    pub version_id: Option<i64>,
    /// Name of the prompt the report was generated with.
    pub prompt_name: String,
}

fn is_cancelled(flag: &Option<Arc<AtomicBool>>) -> bool {
    stage1::is_cancelled(flag.as_ref())
}

/// Preview the slice counts, the estimated input scale and whether the current report
/// is out of date, without calling any model.
pub fn estimate_daily<Tz: TimeZone>(
    store: &Store,
    tz: &Tz,
    date: &str,
    project: Option<&str>,
    ctx: &MaterialContext,
) -> Result<DailyEstimate>
where
    Tz::Offset: std::fmt::Display,
{
    let collected = material::collect(store, tz, date, project, ctx)?;
    let slices_used = collected.materials.len();
    let digests_cached = collected
        .materials
        .iter()
        .filter(|m| m.digest.is_some())
        .count();
    let estimated_chars: usize = collected
        .materials
        .iter()
        .map(|m| match &m.digest {
            Some(StoredDigest::Summary(_)) => m
                .slice
                .digest_json
                .as_deref()
                .map(|j| j.chars().count())
                .unwrap_or(0),
            Some(StoredDigest::Raw { text }) => text.chars().count(),
            None => m.turns.iter().map(|t| t.text.chars().count()).sum(),
        })
        .sum();
    Ok(DailyEstimate {
        slices_used,
        slices_ignored: collected.ignored,
        digests_cached,
        digests_pending: slices_used.saturating_sub(digests_cached),
        digests_other_model: material::pending_from_other_engine(
            &collected.materials,
            ctx.engine_name,
        ),
        estimated_chars,
        staleness: report_io::staleness(store, KIND_DAILY, date, project, &collected.materials)?,
    })
}

pub fn generate_daily<Tz: TimeZone>(
    store: &mut Store,
    engine: &dyn Engine,
    tz: &Tz,
    opts: &DailyOptions,
    progress: &mut dyn FnMut(&Progress),
) -> Result<DailyResult>
where
    Tz::Offset: std::fmt::Display,
{
    let mut result = DailyResult::default();
    let engine_name = engine.name();
    let lang = opts.language;
    let prompt_row = prompt_library::resolve(store, KIND_DAILY, lang, opts.prompt.as_deref())?;
    result.prompt_name = prompt_row.name.clone();

    // ---- Collect material --------------------------------------------------
    let ctx = MaterialContext {
        engine_name: &engine_name,
        budgets: &opts.budgets,
        now: opts.now,
        language: lang,
    };
    let collected = material::collect(store, tz, &opts.period, opts.project.as_deref(), &ctx)?;
    let materials = collected.materials;
    result.slices_ignored = collected.ignored;
    if materials.is_empty() {
        bail!(
            "{} has no usable session slices ({} ignored)",
            opts.period,
            result.slices_ignored
        );
    }
    result.slices_used = materials.len();
    let instructions = prompt_library::assemble(
        &prompt_row.body,
        &PromptVars {
            date: Some(&opts.period),
            period: Some(&opts.period),
            project: opts.project.as_deref(),
        },
        lang,
    );
    let prompt_hash = digest::hash_parts(&[&instructions]);
    let instructions = prompt_library::with_language(&instructions, lang);
    let report_pid = report_io::report_project_id(store, opts.project.as_deref())?;
    let current = report_io::current_inputs(&materials);

    // ---- Decide what to write ------------------------------------------------
    let base = match opts.mode {
        ReportMode::Update if !opts.dry_run => {
            let base = report_io::load_base(store, KIND_DAILY, &opts.period, report_pid)?;
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
        bail!("the report for {} is already up to date", opts.period);
    };
    let mut targets: Vec<Material> = materials
        .into_iter()
        .filter(|m| write_keys.contains(&project_key(m.project_id)))
        .collect();

    // ---- Stage 1: digests for the projects being written ----------------------
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

    // ---- Stage 2: aggregate and generate ------------------------------------
    let refs: Vec<&Material> = targets.iter().collect();
    let mut sections = material::render_by_project(&refs, tz);
    if !opts.dry_run {
        condense(
            store,
            engine,
            &engine_name,
            opts,
            &mut sections,
            &mut result,
            progress,
        )?;
    }
    let expected: Vec<String> = sections.iter().map(|s| s.project.clone()).collect();
    let mut prompt = report_io::build_prompt(&instructions, &sections, "Project");
    if let Some(b) = &base {
        prompt.push_str(&report_io::update_instructions(
            store,
            b,
            &write_keys,
            &current,
        )?);
    }
    result.stage2_prompt_chars = prompt.chars().count();
    if opts.dry_run {
        result.markdown = prompt;
        return Ok(result);
    }
    let mut written: Vec<ProjectSection> = if expected.is_empty() {
        Vec::new()
    } else {
        if is_cancelled(&opts.cancel) {
            bail!("report generation was cancelled");
        }
        progress(&Progress::WritingDaily {
            sections: base.is_some().then_some(expected.len()),
            chars: result.stage2_prompt_chars,
        });
        report_io::write_sections(
            engine,
            &prompt,
            &lang.daily_title(&opts.period),
            &expected,
            opts.cancel.as_ref(),
            &mut result.warnings,
            progress,
        )?
    };

    // ---- Verify, assemble and save -----------------------------------------
    for section in &mut written {
        let sources = targets.iter().filter(|m| m.project_name == section.project);
        let (body, check) = anchors::verify(&section.body, &material::valid_anchors(sources));
        section.body = body;
        result.anchors.total += check.total;
        result.anchors.invalid += check.invalid;
    }
    let all_sections = match &base {
        None => written,
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
            structure::splice_sections(&b.sections, &written, &removed_names)
        }
    };
    result.markdown = structure::compose_report(&lang.daily_title(&opts.period), &all_sections);
    result.projects_written = expected;

    let new_manifest = report_io::new_manifest(
        prompt_hash,
        current,
        base.as_ref(),
        &write_keys,
        &result.markdown,
        &engine_name,
    );

    // The caller may have stopped waiting while the last call was in flight.
    if is_cancelled(&opts.cancel) {
        bail!("report generation was cancelled");
    }
    let stage2_hash = digest::hex(&Sha256::digest(prompt.as_bytes()));
    result.version_id = Some(store.save_report_version(&NewReportVersion {
        report_type: KIND_DAILY,
        period_key: &opts.period,
        project_id: report_pid,
        origin: ReportOrigin::Generated,
        markdown: &result.markdown,
        engine: Some(&engine_name),
        input_hash: Some(&stage2_hash),
        prompt_id: Some(prompt_row.id),
        prompt_name: Some(&prompt_row.name),
        prompt_snapshot: Some(&prompt_row.body),
        input_manifest: Some(&new_manifest.to_json()),
        language: lang.tag,
    })?);
    // A finalized report that gained new material needs a fresh review.
    if base.as_ref().is_some_and(|b| b.is_final) {
        store.set_report_status(KIND_DAILY, &opts.period, report_pid, ReportStatus::Draft)?;
    }
    Ok(result)
}

/// Condense projects into task clusters when they have many slices or the day is too large.
fn condense(
    store: &Store,
    engine: &dyn Engine,
    engine_name: &str,
    opts: &DailyOptions,
    sections: &mut [ProjectMaterial],
    result: &mut DailyResult,
    progress: &mut dyn FnMut(&Progress),
) -> Result<()> {
    let total: usize = sections.iter().map(|s| s.text.chars().count()).sum();
    if total > opts.budgets.stage2_chars || sections.iter().any(brief::needs_clustering) {
        progress(&Progress::ClusteringProjects);
    }
    let cached_briefs = CachedBrief::usable(store.project_briefs(&opts.period)?, opts.language);
    let condensed = brief::condense_if_needed(
        engine,
        engine_name,
        opts.language,
        sections,
        &cached_briefs,
        opts.budgets.stage2_chars,
    );
    for (_, b) in &condensed.to_save {
        store.save_project_brief(&opts.period, &b.to_row(opts.language, engine_name))?;
    }
    result.projects_condensed = condensed.projects;
    result.briefs_cached = condensed.cached;
    result.warnings.extend(
        condensed
            .failures
            .into_iter()
            .map(|f| format!("task clustering failed, full material kept: {f}")),
    );
    Ok(())
}

#[cfg(test)]
mod tests;
