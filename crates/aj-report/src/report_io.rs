//! Pieces every report kind shares around generation: which inputs a version was made
//! from (the manifest), whether the current version is out of date, the base an update
//! builds on, and asking the model for sections with one structural repair attempt.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use aj_core::slice::union_minutes;
use aj_core::INTERNAL_TASK_FINGERPRINT;
use aj_store::Store;
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};

use crate::engine::Engine;
use crate::lang::Language;
use crate::manifest::{self, project_id_of, project_key, CurrentInputs, Manifest, ProjectEntry};
use crate::material::{Material, ProjectMaterial, UNASSIGNED};
use crate::progress::Progress;
use crate::stage1::is_cancelled;
use crate::structure::{self, ProjectSection};
use crate::summarize::Budgets;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReportMode {
    /// Write the whole report again.
    #[default]
    Regenerate,
    /// Write only the projects whose material changed since the current version. Falls
    /// back to a full regeneration when there is no version to update.
    Update,
}

impl ReportMode {
    /// [`ReportMode::Update`] when `update` is set, else [`ReportMode::Regenerate`].
    pub fn from_update(update: bool) -> Self {
        if update {
            ReportMode::Update
        } else {
            ReportMode::Regenerate
        }
    }
}

/// Concurrent model calls when the caller does not choose.
pub const DEFAULT_CONCURRENCY: usize = 3;

/// What generating a report takes, the same for every kind but the period it covers: a local
/// date (`YYYY-MM-DD`) for [`crate::DailyOptions`], a [`crate::period::Week`] or a
/// [`crate::period::Month`].
#[derive(Debug, Clone)]
pub struct ReportOptions<P> {
    pub period: P,
    /// Only include the project with this name.
    pub project: Option<String>,
    /// Name of the prompt to use; `None` uses the language's default prompt for the kind.
    pub prompt: Option<String>,
    /// Language the report is written in.
    pub language: &'static Language,
    /// Concurrent model calls, for slice digests and for project sections.
    pub concurrency: usize,
    /// Only assemble the material; call no model and save nothing.
    pub dry_run: bool,
    pub budgets: Budgets,
    /// Optional cancellation flag, checked between model calls. A run cancelled while a
    /// call is in flight finishes that call but saves no report version.
    pub cancel: Option<Arc<AtomicBool>>,
    pub mode: ReportMode,
    /// Current time, which decides which slices are still live.
    pub now: DateTime<Utc>,
}

/// How the current material differs from what the current report version was generated from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Staleness {
    pub new_slices: usize,
    pub changed_slices: usize,
    pub removed_slices: usize,
    /// Projects whose sections an update would write again or drop.
    pub projects: Vec<String>,
}

/// The report row a report for `project` (or the whole period) is stored under.
pub fn report_project_id(store: &Store, project: Option<&str>) -> Result<Option<i64>> {
    Ok(match project {
        Some(name) => store.find_project(name)?.map(|p| p.id),
        None => None,
    })
}

/// The material in manifest form: per project, each slice's content hash.
pub fn current_inputs(materials: &[Material]) -> CurrentInputs {
    let mut out = CurrentInputs::new();
    for m in materials {
        let entry = out
            .entry(project_key(m.project_id))
            .or_insert_with(|| ProjectEntry {
                name: m.project_name.clone(),
                ..ProjectEntry::default()
            });
        entry.slices.insert(m.slice.id, m.content_hash.clone());
    }
    out
}

/// The current name of the project behind a manifest key, for headings.
pub fn project_name(store: &Store, key: &str, entry: &ProjectEntry) -> Result<String> {
    Ok(match project_id_of(key) {
        Some(id) => store
            .project(id)?
            .map(|p| p.name)
            .unwrap_or_else(|| entry.name.clone()),
        None => UNASSIGNED.into(),
    })
}

/// The current version of a report, parsed for an update.
pub struct Base {
    pub sections: Vec<ProjectSection>,
    pub manifest: Manifest,
    pub is_final: bool,
    /// The language the version is written in.
    pub language: &'static Language,
}

pub fn load_base(
    store: &Store,
    kind: &str,
    period: &str,
    project_id: Option<i64>,
) -> Result<Option<Base>> {
    let Some(version) = store.current_report_version(kind, period, project_id)? else {
        return Ok(None);
    };
    let Some(manifest) = version.input_manifest.as_deref().and_then(Manifest::parse) else {
        return Ok(None);
    };
    let Some(language) = Language::find(&version.language) else {
        return Ok(None);
    };
    let known = project_order(store)?;
    let Ok(parsed) = structure::validate_user_edit(
        &version.markdown,
        &structure::report_title(kind, period, language),
        &known,
    ) else {
        return Ok(None);
    };
    let is_final = store
        .report_meta(kind, period, project_id)?
        .is_some_and(|m| m.status == "final");
    Ok(Some(Base {
        sections: parsed.sections,
        manifest,
        is_final,
        language,
    }))
}

/// Compare the current material with the manifest of the current report version.
pub fn staleness(
    store: &Store,
    kind: &str,
    period: &str,
    project: Option<&str>,
    materials: &[Material],
) -> Result<Option<Staleness>> {
    staleness_for_inputs(store, kind, period, project, &current_inputs(materials))
}

/// Compare `current` inputs with the manifest of the current report version.
pub fn staleness_for_inputs(
    store: &Store,
    kind: &str,
    period: &str,
    project: Option<&str>,
    current: &CurrentInputs,
) -> Result<Option<Staleness>> {
    let project_id = report_project_id(store, project)?;
    let Some(version) = store.current_report_version(kind, period, project_id)? else {
        return Ok(None);
    };
    let Some(manifest) = version.input_manifest.as_deref().and_then(Manifest::parse) else {
        return Ok(None);
    };
    let d = manifest::diff(&manifest, current);
    if !d.is_stale() {
        return Ok(None);
    }
    let mut projects = Vec::new();
    for key in d.to_write() {
        projects.push(current[&key].name.clone());
    }
    for key in &d.removed {
        projects.push(project_name(store, key, &manifest.projects[key])?);
    }
    Ok(Some(Staleness {
        new_slices: d.new_slices,
        changed_slices: d.changed_slices,
        removed_slices: d.removed_slices,
        projects,
    }))
}

/// `> active N days · about X hours · M sessions`, in `lang`.
pub fn stats_line(items: &[&Material], lang: &Language) -> String {
    type Intervals = Vec<(DateTime<Utc>, DateTime<Utc>)>;
    let mut by_day: BTreeMap<&str, Intervals> = BTreeMap::new();
    let mut sessions = BTreeSet::new();
    for m in items {
        by_day
            .entry(m.slice.local_date.as_str())
            .or_default()
            .extend(m.slice.active_intervals.iter().copied());
        sessions.insert(m.session.id);
    }
    let minutes: i64 = by_day.values().map(|iv| union_minutes(iv.clone())).sum();
    lang.stats_line(by_day.len(), minutes, sessions.len())
}

/// Remove leading program-written `> ...` statistics lines and blank lines from a section body.
pub fn strip_stats(body: &str) -> String {
    let lines: Vec<&str> = body
        .lines()
        .skip_while(|l| l.trim_start().starts_with("> ") || l.trim().is_empty())
        .collect();
    lines.join("\n").trim().to_string()
}

/// Insert the program's statistics line at the top of a section, replacing any the model wrote.
pub fn with_stats(section: ProjectSection, stats: &str) -> ProjectSection {
    let clean = strip_stats(&section.body);
    let body = if stats.trim().is_empty() {
        clean
    } else if clean.is_empty() {
        stats.to_string()
    } else {
        format!("{stats}\n{clean}")
    };
    ProjectSection {
        project: section.project,
        body,
    }
}

/// Ask for the sections of `expected`, with one structural repair attempt.
pub fn write_sections(
    engine: &dyn Engine,
    prompt: &str,
    title: &str,
    expected: &[String],
    cancel: Option<&Arc<AtomicBool>>,
    warnings: &mut Vec<String>,
    progress: &mut dyn FnMut(&Progress),
) -> Result<Vec<ProjectSection>> {
    let raw = engine.complete(prompt)?;
    let validated = match structure::validate_and_normalize(&raw, title, expected) {
        Ok(v) => v,
        Err(errors) => {
            if is_cancelled(cancel) {
                bail!("report generation was cancelled");
            }
            progress(&Progress::RepairingStructure);
            let repair = build_repair_prompt(prompt, &raw, &errors, expected);
            let retried = engine.complete(&repair)?;
            match structure::validate_and_normalize(&retried, title, expected) {
                Ok(v) => {
                    warnings.push(format!(
                        "report required structural repair ({})",
                        errors.join("; ")
                    ));
                    v
                }
                Err(retry_errors) => bail!(
                    "report failed structural validation: {}",
                    retry_errors.join("; ")
                ),
            }
        }
    };
    Ok(validated.sections)
}

/// The part of an update request that says which sections to write, and hands over the
/// sections of those projects that the user edited.
pub fn update_instructions(
    store: &Store,
    base: &Base,
    write_keys: &BTreeSet<String>,
    current: &CurrentInputs,
) -> Result<String> {
    let names: Vec<&str> = write_keys
        .iter()
        .filter_map(|k| current.get(k).map(|e| e.name.as_str()))
        .collect();
    let mut p = format!(
        "\n\nThis request updates an existing report. Write only the sections for these projects: {}. The other sections of the report stay as they are.\n",
        names.join(", ")
    );
    let mut edited = String::new();
    for key in write_keys {
        let Some(old) = base.manifest.projects.get(key) else {
            continue;
        };
        let name = project_name(store, key, old)?;
        let Some(section) = base
            .sections
            .iter()
            .find(|s| s.project.to_lowercase() == name.to_lowercase())
        else {
            continue;
        };
        if manifest::section_hash(&section.body) != old.section_hash {
            edited.push_str(&format!(
                "\n## Current section: {name}\n{}\n",
                section.body.trim()
            ));
        }
    }
    if !edited.is_empty() {
        p.push_str(
            "\nThe user edited the current version of the sections below. Keep the user's wording, corrections and additions wherever the material still supports them, and integrate the new material into them:\n",
        );
        p.push_str(&edited);
    }
    Ok(p)
}

pub fn build_prompt(
    instructions: &str,
    sections: &[ProjectMaterial],
    section_label: &str,
) -> String {
    let mut p = String::new();
    p.push_str(INTERNAL_TASK_FINGERPRINT);
    p.push('\n');
    p.push_str(instructions);
    p.push_str("\n\nMaterial:\n");
    for s in sections {
        p.push_str(&format!(
            "\n## {section_label}: {}\n{}",
            s.project,
            s.text.trim_end()
        ));
        p.push('\n');
    }
    p
}

fn build_repair_prompt(
    original_prompt: &str,
    previous_output: &str,
    errors: &[String],
    expected_projects: &[String],
) -> String {
    format!(
        "{original_prompt}\n\n---\n\nThe previous draft could not be accepted because it violated the required Markdown structure:\n- {}\n\nPrevious draft:\n{}\n\nRewrite the full report, keeping the language of the previous draft, so that:\n1. It starts with a single `# ...` H1 heading.\n2. Every H2 heading (`## <Project>`) uses one of these exact project names: {}.\n3. Every project section uses `- ` bullet points (no standalone prose paragraphs; put any subheadings like `### Completed` inside the project section).",
        errors.join("\n- "),
        previous_output.trim(),
        expected_projects.join(", "),
    )
}

/// The manifest of a new version. Projects written now record their new section; the
/// others keep their old record, so that a user edit to a kept section is still
/// recognized as one later.
pub fn new_manifest(
    prompt_hash: String,
    current: CurrentInputs,
    base: Option<&Base>,
    write_keys: &BTreeSet<String>,
    markdown: &str,
    engine_name: &str,
) -> Manifest {
    let mut out = Manifest {
        prompt_hash,
        projects: Default::default(),
    };
    for (key, entry) in current {
        let (section_hash, engine) = match (base, write_keys.contains(&key)) {
            (Some(b), false) => b
                .manifest
                .projects
                .get(&key)
                .map(|e| (e.section_hash.clone(), e.engine.clone()))
                .unwrap_or_default(),
            _ => (
                structure::extract_project_section(markdown, &entry.name)
                    .map(|body| manifest::section_hash(&body))
                    .unwrap_or_default(),
                Some(engine_name.to_string()),
            ),
        };
        out.projects.insert(
            key,
            ProjectEntry {
                section_hash,
                engine,
                ..entry
            },
        );
    }
    out
}

/// What an update writes: every project when there is no base or the prompt or the
/// language changed, otherwise the changed ones; plus the projects to drop. `None` when
/// nothing changed.
pub fn plan_writes(
    base: Option<&Base>,
    current: &CurrentInputs,
    prompt_hash: &str,
    lang: &Language,
) -> Option<(BTreeSet<String>, Vec<String>)> {
    match base {
        None => Some((current.keys().cloned().collect(), Vec::new())),
        Some(b) => {
            let d = manifest::diff(&b.manifest, current);
            let write: BTreeSet<String> =
                if b.manifest.prompt_hash != prompt_hash || !std::ptr::eq(b.language, lang) {
                    current.keys().cloned().collect()
                } else {
                    d.to_write().into_iter().collect()
                };
            (!(write.is_empty() && d.removed.is_empty())).then_some((write, d.removed))
        }
    }
}

/// The language a stored version is written in (English when the tag is unknown).
pub fn version_language(tag: &str) -> &'static Language {
    Language::find(tag).unwrap_or(&crate::lang::EN)
}

/// Project names in the user's order, for ordering report sections.
pub fn project_order(store: &Store) -> Result<Vec<String>> {
    Ok(store.projects()?.into_iter().map(|p| p.name).collect())
}

/// A stored period report as it is shown: the canonical title in the version's language and
/// project sections in the user's project order. Line numbers handed to the interface (edit
/// marks, annotation placement) refer to this form.
pub fn display_markdown(
    kind: &str,
    period: &str,
    markdown: &str,
    language: &str,
    project_order: &[String],
) -> String {
    structure::with_title(
        &structure::order_project_sections(markdown, project_order),
        &structure::report_title(kind, period, version_language(language)),
    )
}
