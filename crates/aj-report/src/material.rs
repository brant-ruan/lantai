//! The material of a report: which slices take part, and how they are rendered for the model.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;

use aj_core::dehydrate::{dehydrate_slice, DehydratedTurn};
use aj_core::task_link::{comparable_files, link_slices, SliceFacts, TaskLink};
use aj_store::{SessionRow, SliceRow, Store};
use anyhow::Result;
use chrono::{DateTime, Duration, TimeZone, Utc};

use crate::digest::{hash_parts, CandidateProject, Digest, StoredDigest};
use crate::lang::Language;
use crate::summarize::{
    content_hash, prefix_hash, slice_hash, Budgets, PartDigest, PriorDigest, SliceInput,
};

/// Group name used in reports for sessions that belong to no project.
pub const UNASSIGNED: &str = "Unassigned";

/// One slice taking part in a report.
pub struct Material {
    /// Report language the digest is (to be) written in.
    pub language: &'static Language,
    pub slice: SliceRow,
    pub session: SessionRow,
    pub project_id: Option<i64>,
    pub project_name: String,
    pub turns: Vec<DehydratedTurn>,
    pub previous_digest: Option<String>,
    pub cached_parts: Vec<PartDigest>,
    /// Existing projects passed to stage 1 when the session still needs a suggestion.
    pub candidate_projects: Vec<CandidateProject>,
    /// Hash of everything that determines the slice digest.
    pub hash: String,
    /// Hash of the slice's content alone (see [`content_hash`]), recorded in report manifests.
    pub content_hash: String,
    /// Whether the session may still be growing.
    pub live: bool,
    /// A cached digest covering a prefix of the turns, for an incremental update.
    pub prior: Option<PriorDigest>,
    /// The digest, once known (from the cache or freshly generated).
    pub digest: Option<StoredDigest>,
}

impl Material {
    pub fn input(&self) -> SliceInput<'_> {
        SliceInput {
            language: self.language,
            turns: &self.turns,
            user_note: self.session.user_note.as_deref(),
            previous_digest: self.previous_digest.as_deref(),
            cached_parts: &self.cached_parts,
            candidate_projects: &self.candidate_projects,
            prior: self.prior.as_ref(),
            live: self.live,
        }
    }

    /// The last turn this slice contains.
    pub fn last_turn(&self) -> i64 {
        self.turns.last().map(|t| t.index).unwrap_or(-1)
    }

    pub fn title(&self) -> String {
        self.session
            .title
            .clone()
            .unwrap_or_else(|| "(untitled)".into())
    }
}

/// What a material's digest identity depends on besides the slice itself.
#[derive(Debug, Clone, Copy)]
pub struct MaterialContext<'a> {
    pub engine_name: &'a str,
    /// Report language; cached digests and parts in another language are not reused.
    pub language: &'static Language,
    pub budgets: &'a Budgets,
    /// Current time, which decides whether a slice is still live.
    pub now: DateTime<Utc>,
}

pub struct Collected {
    pub materials: Vec<Material>,
    /// Slices left out because they or their session are ignored.
    pub ignored: usize,
}

/// Projects the model may suggest for a session; archived projects are left out.
pub fn candidate_projects(store: &Store) -> Result<Vec<CandidateProject>> {
    Ok(store
        .list_projects()?
        .into_iter()
        .filter(|p| !p.project.archived)
        .map(|p| CandidateProject {
            name: p.project.name,
            description: p.project.description,
        })
        .collect())
}

/// Build one [`Material`] for `slice`, returning `None` when every turn is empty.
pub fn build_one<Tz: TimeZone>(
    store: &Store,
    tz: &Tz,
    slice: SliceRow,
    session: SessionRow,
    projects: &[CandidateProject],
    ctx: &MaterialContext,
) -> Result<Option<Material>>
where
    Tz::Offset: std::fmt::Display,
{
    let project_id = slice.effective_project(&session);
    let project_name = match project_id {
        Some(id) => store
            .project(id)?
            .map(|p| p.name)
            .unwrap_or_else(|| UNASSIGNED.into()),
        None => UNASSIGNED.into(),
    };
    let turns = dehydrate_slice(store, &session, &slice, tz)?.turns;
    if turns.iter().all(|t| t.text.trim().is_empty()) {
        return Ok(None);
    }
    let previous_digest = store.previous_slice(&slice)?.and_then(|p| p.digest_json);
    let cached_parts = store
        .chunk_digests(slice.id)?
        .into_iter()
        .filter(|c| c.language == ctx.language.tag)
        .map(|c| PartDigest {
            start_turn: c.start_turn_index,
            end_turn: c.end_turn_index,
            input_hash: c.input_hash,
            digest_json: c.digest_json,
        })
        .collect();
    let candidate_projects = if session.needs_llm_suggestion() {
        projects.to_vec()
    } else {
        Vec::new()
    };

    let live = ctx.now - slice.slice_end_at < Duration::minutes(ctx.budgets.quiet_minutes);
    let mut material = Material {
        language: ctx.language,
        slice,
        session,
        project_id,
        project_name,
        turns,
        previous_digest,
        cached_parts,
        candidate_projects,
        hash: String::new(),
        content_hash: String::new(),
        live,
        prior: None,
        digest: None,
    };
    material.hash = slice_hash(&material.input(), ctx.engine_name, ctx.budgets);
    material.content_hash = content_hash(&material.input());

    // A digest written in another language is as good as none.
    let same_language = material.slice.digest_language.as_deref() == Some(ctx.language.tag);
    let cached: Option<StoredDigest> = material
        .slice
        .digest_json
        .as_deref()
        .filter(|_| same_language)
        .and_then(|j| serde_json::from_str(j).ok());
    let cached_hash = material
        .slice
        .digest_input_hash
        .clone()
        .filter(|_| same_language);
    // An incrementally updated digest is refreshed in full once its session goes quiet.
    let needs_refresh = material.slice.digest_update_count > 0 && !live;
    if cached_hash.as_deref() == Some(material.hash.as_str()) && !needs_refresh {
        material.digest = cached;
    } else if let (Some(StoredDigest::Summary(digest)), Some(covered), Some(cached_hash)) =
        (cached, material.slice.digest_covered_turn, cached_hash)
    {
        // The cached digest is still valid for the turns it covers: the slice only grew.
        let prefix = prefix_hash(&material.input(), covered, ctx.engine_name, ctx.budgets);
        if prefix.as_deref() == Some(cached_hash.as_str()) {
            material.prior = Some(PriorDigest {
                digest,
                covered_turn: covered,
                update_count: material.slice.digest_update_count,
            });
        }
    }
    Ok(Some(material))
}

/// Gather the slices of a day that take part in the report, with cached digests attached.
pub fn collect<Tz: TimeZone>(
    store: &Store,
    tz: &Tz,
    date: &str,
    only_project: Option<&str>,
    ctx: &MaterialContext,
) -> Result<Collected>
where
    Tz::Offset: std::fmt::Display,
{
    let mut out = Collected {
        materials: Vec::new(),
        ignored: 0,
    };
    let projects = candidate_projects(store)?;
    for slice in store.slices_for_date(date)? {
        let Some(session) = store.session(slice.session_id)? else {
            continue;
        };
        if slice.is_ignored(&session) {
            out.ignored += 1;
            continue;
        }
        let Some(material) = build_one(store, tz, slice, session, &projects, ctx)? else {
            continue;
        };
        if only_project.is_some_and(|p| !p.eq_ignore_ascii_case(&material.project_name)) {
            continue;
        }
        out.materials.push(material);
    }
    Ok(out)
}

/// Slice ID to the turn indexes it contains: the set of anchors a report may use.
pub fn valid_anchors(materials: &[Material]) -> HashMap<i64, BTreeSet<i64>> {
    materials
        .iter()
        .map(|m| (m.slice.id, m.turns.iter().map(|t| t.index).collect()))
        .collect()
}

/// One project's share of the material, rendered for the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMaterial {
    pub project: String,
    pub text: String,
    pub slice_count: usize,
    /// Hash of the slices and their digests: changes only when what the model would
    /// summarize changes, not when display details such as active minutes do.
    pub identity: String,
}

/// Render the material grouped by project, sorted by project name.
pub fn render_by_project<Tz: TimeZone>(materials: &[&Material], tz: &Tz) -> Vec<ProjectMaterial>
where
    Tz::Offset: std::fmt::Display,
{
    let mut grouped: BTreeMap<&str, Vec<&Material>> = BTreeMap::new();
    for m in materials {
        grouped.entry(m.project_name.as_str()).or_default().push(m);
    }
    grouped
        .into_iter()
        .map(|(project, items)| {
            let mut text = String::new();
            let mut identity_parts: Vec<String> = Vec::new();
            let links = same_task_links(&items);
            for m in &items {
                render_slice(&mut text, m, tz);
                identity_parts.push(m.slice.id.to_string());
                identity_parts.push(match &m.digest {
                    Some(d) => serde_json::to_string(d).unwrap_or_default(),
                    None => m.content_hash.clone(),
                });
                identity_parts.push(m.session.user_note.clone().unwrap_or_default());
            }
            let hints = render_links(&links, &items);
            text.push_str(&hints);
            identity_parts.push(hints);
            let refs: Vec<&str> = identity_parts.iter().map(String::as_str).collect();
            ProjectMaterial {
                project: project.to_string(),
                text,
                slice_count: items.len(),
                identity: hash_parts(&refs),
            }
        })
        .collect()
}

/// Pairs of slices in one project that are probably the same task (see [`aj_core::task_link`]).
fn same_task_links(items: &[&Material]) -> Vec<TaskLink> {
    let files: Vec<BTreeSet<String>> = items
        .iter()
        .map(|m| {
            // The working directory first, so a worktree and the main checkout compare equal.
            let roots: Vec<&str> = [
                m.session.detected_cwd.as_deref(),
                m.session.git_root.as_deref(),
            ]
            .into_iter()
            .flatten()
            .collect();
            comparable_files(&m.slice.files_modified, &roots)
        })
        .collect();
    let facts: Vec<SliceFacts> = items
        .iter()
        .zip(&files)
        .map(|(m, files)| SliceFacts {
            slice_id: m.slice.id,
            git_root: m.session.git_root.as_deref(),
            branch: m.session.git_branch.as_deref(),
            start: m.slice.slice_start_at,
            end: m.slice.slice_end_at,
            files,
        })
        .collect();
    link_slices(&facts)
}

fn render_links(links: &[TaskLink], items: &[&Material]) -> String {
    if links.is_empty() {
        return String::new();
    }
    let agent = |id: i64| {
        items
            .iter()
            .find(|m| m.slice.id == id)
            .map(|m| m.session.agent.product_name())
            .unwrap_or("")
    };
    let mut out = String::from(
        "\nSame-task hints (computed, not inferred: each pair changed the same files on the same branch and is probably one task; merge them into one item when their content agrees):\n",
    );
    for l in links {
        let mut reasons = vec![format!("same branch {}", l.branch)];
        if l.shared_files > 0 {
            reasons.push(format!("{} changed file(s) in common", l.shared_files));
        }
        if l.overlapping() {
            reasons.push("overlapping in time".into());
        } else if l.adjacent() {
            reasons.push(format!("{} min apart", l.gap_minutes));
        }
        let _ = writeln!(
            out,
            "- S{} ({}) and S{} ({}): {}",
            l.a,
            agent(l.a),
            l.b,
            agent(l.b),
            reasons.join(", ")
        );
    }
    out
}

fn render_slice<Tz: TimeZone>(p: &mut String, m: &Material, tz: &Tz)
where
    Tz::Offset: std::fmt::Display,
{
    let start = m.slice.slice_start_at.with_timezone(tz).format("%H:%M");
    let end = m.slice.slice_end_at.with_timezone(tz).format("%H:%M");
    let _ = write!(
        p,
        "\n### Slice S{} ({} session \"{}\", {start}-{end}, ~{} active min",
        m.slice.id,
        m.session.agent.product_name(),
        m.title(),
        m.slice.active_minutes
    );
    if let Some(branch) = m.session.git_branch.as_deref().filter(|b| !b.is_empty()) {
        let _ = write!(p, ", branch {branch}");
    }
    p.push_str(")\n");
    if let Some(note) = m
        .session
        .user_note
        .as_deref()
        .filter(|n| !n.trim().is_empty())
    {
        let _ = writeln!(p, "User note: {}", note.trim());
    }
    match m.digest.as_ref() {
        Some(StoredDigest::Summary(d)) => render_digest(p, d),
        Some(StoredDigest::Raw { text }) => {
            let _ = writeln!(p, "Raw record (T<n> are turn numbers):\n{text}");
        }
        None => {}
    }
}

fn render_digest(p: &mut String, d: &Digest) {
    let turns = |t: &[i64]| {
        if t.is_empty() {
            String::new()
        } else {
            format!(
                " [{}]",
                t.iter()
                    .map(|n| format!("T{n}"))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    };
    if !d.goal.is_empty() {
        let _ = writeln!(p, "Goal: {}", d.goal);
    }
    if !d.key_changes.is_empty() {
        p.push_str("Key changes:\n");
        for c in &d.key_changes {
            let _ = writeln!(p, "- {}{}", c.text, turns(&c.turns));
        }
    }
    if !d.decisions_and_pitfalls.is_empty() {
        p.push_str("Decisions and pitfalls:\n");
        for c in &d.decisions_and_pitfalls {
            let _ = writeln!(p, "- {}{}", c.text, turns(&c.turns));
        }
    }
    // `outcome` and `open_items` stay in the digest for incremental merging, but are
    // not shown to the report stage: reports describe what happened, not progress.
}
