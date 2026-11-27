//! Stage 1 for a set of slices: digest the ones without a valid cached digest, in
//! parallel, persist the results, and apply the project suggestions they carry.
//!
//! Shared by every report kind; how one slice is summarized lives in [`crate::summarize`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use aj_store::{ChunkDigestRow, SliceDigestRow, Store};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, TimeZone, Utc};

use crate::digest::StoredDigest;
use crate::engine::Engine;
use crate::lang::Language;
use crate::material::{self, Material, MaterialContext};
use crate::progress::Progress;
use crate::summarize::{self, Budgets, SliceOutcome};

#[derive(Debug, Clone, Copy)]
pub struct Stage1Options<'a> {
    /// Number of slices digested concurrently.
    pub concurrency: usize,
    /// Call no model: uncached slices get a local preview.
    pub dry_run: bool,
    pub budgets: &'a Budgets,
    /// Checked between batches of model calls.
    pub cancel: Option<&'a Arc<AtomicBool>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stage1Stats {
    pub digests_cached: usize,
    pub digests_generated: usize,
    /// Live slices whose cached digest was updated with their new turns only.
    pub digests_incremental: usize,
    pub digests_passthrough: usize,
    /// Slices that were too long for one request and were digested part by part.
    pub slices_split: usize,
    pub parts_generated: usize,
    pub parts_cached: usize,
    /// Unassigned sessions that received a project suggestion from the model.
    pub suggestions_made: usize,
    /// Slices whose digest failed and fell back to a raw record, with the reason.
    pub warnings: Vec<String>,
}

pub fn is_cancelled(flag: Option<&Arc<AtomicBool>>) -> bool {
    flag.is_some_and(|f| f.load(Ordering::Relaxed))
}

/// Summarize one slice on demand (for the drawer or idle pre-generation) and cache the result.
pub fn digest_single_slice<Tz: TimeZone>(
    store: &mut Store,
    engine: &dyn Engine,
    tz: &Tz,
    slice_id: i64,
    budgets: &Budgets,
    now: DateTime<Utc>,
    language: &'static Language,
) -> Result<StoredDigest>
where
    Tz::Offset: std::fmt::Display,
{
    let slice = store
        .slice(slice_id)?
        .with_context(|| format!("no slice S{slice_id}"))?;
    let session = store
        .session(slice.session_id)?
        .with_context(|| format!("the session for slice S{slice_id} does not exist"))?;
    let engine_name = engine.name();
    let projects = material::candidate_projects(store)?;
    let ctx = MaterialContext {
        engine_name: &engine_name,
        budgets,
        now,
        language,
    };
    let mut m = material::build_one(store, tz, slice, session, &projects, &ctx)?
        .with_context(|| format!("slice S{slice_id} has no content to summarize"))?;
    let mut dummy = Stage1Stats::default();
    if let Some(cached) = m.digest.take() {
        apply_llm_suggestion(store, m.session.id, &cached, &mut dummy)?;
        return Ok(cached);
    }
    let outcome = summarize::summarize_slice(engine, &engine_name, &m.input(), budgets);
    let err = outcome.error.clone();
    let stored = record_outcome(store, &engine_name, &m, outcome, &mut dummy, &mut |_| {})?;
    if let Some(e) = err {
        bail!("{e}");
    }
    Ok(stored)
}

/// Digest every material that has no valid cached digest, `opts.concurrency` at a time,
/// and persist the results. In a dry run, uncached slices get a local preview instead.
pub fn run(
    store: &mut Store,
    engine: &dyn Engine,
    engine_name: &str,
    materials: &mut [Material],
    opts: &Stage1Options,
    result: &mut Stage1Stats,
    progress: &mut dyn FnMut(&Progress),
) -> Result<()> {
    result.digests_cached += materials.iter().filter(|m| m.digest.is_some()).count();
    if !opts.dry_run {
        for m in materials.iter() {
            if let Some(stored) = &m.digest {
                apply_llm_suggestion(store, m.session.id, stored, result)?;
            }
        }
    }
    let pending: Vec<usize> = (0..materials.len())
        .filter(|i| materials[*i].digest.is_none())
        .collect();
    if opts.dry_run {
        for &i in &pending {
            materials[i].digest = Some(summarize::preview(&materials[i].turns));
        }
        return Ok(());
    }
    if pending.is_empty() {
        return Ok(());
    }
    progress(&Progress::DigestsPending {
        pending: pending.len(),
        cached: result.digests_cached,
    });
    // A worker pool: each worker takes the next pending slice as soon as it is free, so
    // one slow request never holds up the others. Results are persisted on this thread.
    let budgets = opts.budgets;
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut done: Vec<(usize, StoredDigest)> = Vec::new();
    let shared: &[Material] = materials;
    std::thread::scope(|scope| -> Result<()> {
        let (tx, rx) = std::sync::mpsc::channel::<(usize, SliceOutcome)>();
        for _ in 0..opts.concurrency.max(1).min(pending.len()) {
            let (tx, next, pending) = (tx.clone(), &next, &pending);
            scope.spawn(move || loop {
                if is_cancelled(opts.cancel) {
                    break;
                }
                let k = next.fetch_add(1, Ordering::SeqCst);
                let Some(&i) = pending.get(k) else { break };
                let outcome =
                    summarize::summarize_slice(engine, engine_name, &shared[i].input(), budgets);
                if tx.send((i, outcome)).is_err() {
                    break;
                }
            });
        }
        drop(tx);
        for (i, outcome) in rx {
            let stored = record_outcome(store, engine_name, &shared[i], outcome, result, progress)?;
            done.push((i, stored));
        }
        Ok(())
    })?;
    for (i, stored) in done {
        materials[i].digest = Some(stored);
    }
    if is_cancelled(opts.cancel) {
        bail!("report generation was cancelled");
    }
    Ok(())
}

/// Persist what stage 1 produced for one slice (written by the engine named `engine_name`) and
/// fold it into the counters.
pub(crate) fn record_outcome(
    store: &mut Store,
    engine_name: &str,
    m: &Material,
    outcome: SliceOutcome,
    result: &mut Stage1Stats,
    progress: &mut dyn FnMut(&Progress),
) -> Result<StoredDigest> {
    let split = !outcome.parts.is_empty() || outcome.parts_generated + outcome.parts_cached > 0;
    if split {
        result.slices_split += 1;
        result.parts_generated += outcome.parts_generated;
        result.parts_cached += outcome.parts_cached;
    }
    match (&outcome.stored, &outcome.error) {
        (_, Some(e)) => result.warnings.push(format!("S{}: {e}", m.slice.id)),
        (StoredDigest::Raw { .. }, None) => result.digests_passthrough += 1,
        (StoredDigest::Summary(_), None) if outcome.incremental => result.digests_incremental += 1,
        (StoredDigest::Summary(_), None) => result.digests_generated += 1,
    }

    // Parts are saved even when the slice digest is not cacheable, so a retry only redoes what failed.
    let rows: Vec<ChunkDigestRow> = outcome
        .parts
        .iter()
        .map(|p| ChunkDigestRow {
            start_turn_index: p.start_turn,
            end_turn_index: p.end_turn,
            input_hash: p.input_hash.clone(),
            digest_json: p.digest_json.clone(),
            language: m.language.tag.to_string(),
            engine: Some(engine_name.to_string()),
        })
        .collect();
    store.replace_chunk_digests(m.slice.id, &rows)?;
    if outcome.cacheable {
        store.save_digest(
            m.slice.id,
            &SliceDigestRow {
                digest_json: &serde_json::to_string(&outcome.stored)?,
                input_hash: &m.hash,
                language: m.language.tag,
                covered_turn: m.last_turn(),
                update_count: outcome.update_count,
                engine: engine_name,
            },
        )?;
    }

    progress(&Progress::SliceDigested {
        slice_id: m.slice.id,
        title: m.title(),
        parts: split.then_some(outcome.parts.len()),
        incremental: outcome.incremental,
    });
    apply_llm_suggestion(store, m.session.id, &outcome.stored, result)?;
    Ok(outcome.stored)
}

pub(crate) fn apply_llm_suggestion(
    store: &Store,
    session_id: i64,
    stored: &StoredDigest,
    result: &mut Stage1Stats,
) -> Result<()> {
    let StoredDigest::Summary(d) = stored else {
        return Ok(());
    };
    let Some(raw) = d.suggested_project.as_deref() else {
        return Ok(());
    };
    if !store
        .session(session_id)?
        .is_some_and(|s| s.needs_llm_suggestion())
    {
        return Ok(());
    }
    if let Some(project_id) = resolve_suggested_project(store, raw)? {
        store.set_suggestion(session_id, Some((project_id, "llm", "judged by model")))?;
        result.suggestions_made += 1;
    }
    Ok(())
}

/// Match the model's `suggested_project` text against an existing project, tolerating
/// stray quotes or an echoed description after the name. Archived projects are never
/// suggested (a cached digest may still name one).
fn resolve_suggested_project(store: &Store, raw: &str) -> Result<Option<i64>> {
    fn clean(s: &str) -> &str {
        s.trim()
            .trim_start_matches(['-', '*'])
            .trim()
            .trim_matches(['"', '\'', '`'])
            .trim()
    }
    let name = clean(raw);
    if name.is_empty() || name.eq_ignore_ascii_case("none") || name.eq_ignore_ascii_case("null") {
        return Ok(None);
    }
    let active = |name: &str| -> Result<Option<i64>> {
        Ok(store
            .find_project(name)?
            .filter(|p| !p.archived)
            .map(|p| p.id))
    };
    if let Some(id) = active(name)? {
        return Ok(Some(id));
    }
    for sep in [" — ", ": ", ":", " (", "（"] {
        if let Some((head, _)) = name.split_once(sep) {
            let head = clean(head);
            if !head.is_empty() {
                if let Some(id) = active(head)? {
                    return Ok(Some(id));
                }
            }
        }
    }
    Ok(None)
}
