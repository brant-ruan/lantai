//! Stage 1 for one slice: decide how to summarize it and do so.
//!
//! - A short slice passes through: its text goes into stage 2 as is.
//! - A slice within the budget is summarized in one request.
//! - A longer slice is split at turn boundaries, summarized part by part, and the part
//!   digests are merged. Parts are cached individually.
//! - A slice that is still live and only gained turns since its cached digest is updated
//!   incrementally: the old digest plus the new turns, instead of the whole record.
//!
//! Nothing here touches the database: cached parts come in with the input, and the
//! parts to persist go out with the outcome.

use aj_core::dehydrate::{join_turns, truncate_middle, DehydratedTurn};

use crate::chunk::plan_chunks;
use crate::digest::{
    build_merge_prompt, build_prompt, build_update_prompt, hash_parts, parse_digest,
    CandidateProject, Digest, DigestContext, StoredDigest, DIGEST_PROMPT_VERSION,
};
use crate::engine::Engine;
use crate::lang::Language;

/// Size limits (in characters) and the limits on incremental work for live slices.
#[derive(Debug, Clone)]
pub struct Budgets {
    /// Slices shorter than this skip the model entirely.
    pub passthrough_chars: usize,
    /// Largest record sent in one stage 1 request; longer slices are split into parts.
    pub chunk_chars: usize,
    /// Largest material sent in one stage 2 request; beyond it projects are condensed first.
    pub stage2_chars: usize,
    /// A slice whose last activity is more recent than this is live: its session may still grow.
    /// Live slices are not pre-digested in the background.
    pub quiet_minutes: i64,
    /// Incremental digest updates allowed before a full digest is required again.
    pub max_incremental_updates: u32,
    /// Maximum daily reports generated concurrently in a sidebar batch.
    pub batch_concurrency: usize,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            passthrough_chars: 800,
            chunk_chars: 40_000,
            stage2_chars: 60_000,
            quiet_minutes: 20,
            max_incremental_updates: 3,
            batch_concurrency: 2,
        }
    }
}

/// A cached digest of the same slice that covers a prefix of its current turns.
#[derive(Debug, Clone)]
pub struct PriorDigest {
    pub digest: Digest,
    /// Last turn index the digest covers.
    pub covered_turn: i64,
    /// Incremental updates already applied since the last full digest.
    pub update_count: u32,
}

/// The digest of one part of a slice, as cached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartDigest {
    pub start_turn: i64,
    pub end_turn: i64,
    pub input_hash: String,
    pub digest_json: String,
}

pub struct SliceInput<'a> {
    /// Report language the digest is written in. Not part of any hash: caches record
    /// their language separately.
    pub language: &'a Language,
    pub turns: &'a [DehydratedTurn],
    pub user_note: Option<&'a str>,
    pub previous_digest: Option<&'a str>,
    /// Part digests cached by an earlier run.
    pub cached_parts: &'a [PartDigest],
    /// Existing projects, supplied only when the session still needs a project suggestion.
    pub candidate_projects: &'a [CandidateProject],
    /// A cached digest covering a prefix of `turns`, usable for an incremental update.
    pub prior: Option<&'a PriorDigest>,
    /// Whether the session may still be growing (see [`Budgets::quiet_minutes`]).
    pub live: bool,
}

#[derive(Debug, Clone)]
pub struct SliceOutcome {
    pub stored: StoredDigest,
    /// Whether `stored` may be cached as the slice digest.
    pub cacheable: bool,
    /// Set when something failed and a fallback was used.
    pub error: Option<String>,
    /// The parts of this slice to persist (empty when the slice was not split).
    pub parts: Vec<PartDigest>,
    pub parts_generated: usize,
    pub parts_cached: usize,
    /// Whether `stored` came from an incremental update of the prior digest.
    pub incremental: bool,
    /// Incremental updates `stored` has had since its last full digest.
    pub update_count: u32,
}

impl SliceOutcome {
    fn simple(stored: StoredDigest, cacheable: bool, error: Option<String>) -> Self {
        Self {
            stored,
            cacheable,
            error,
            parts: vec![],
            parts_generated: 0,
            parts_cached: 0,
            incremental: false,
            update_count: 0,
        }
    }
}

/// Hash of everything that determines the digest of a slice.
///
/// The previous day's digest is background only: whether there is one counts, its text
/// does not, so that redoing one day does not invalidate every later day of the session.
pub fn slice_hash(input: &SliceInput, engine_name: &str, budgets: &Budgets) -> String {
    turns_hash(input.turns, input, engine_name, budgets)
}

/// [`slice_hash`] as it was when the slice ended at `covered_turn`. `None` when the slice
/// has no turn with that index.
pub fn prefix_hash(
    input: &SliceInput,
    covered_turn: i64,
    engine_name: &str,
    budgets: &Budgets,
) -> Option<String> {
    let end = input.turns.iter().position(|t| t.index == covered_turn)?;
    Some(turns_hash(
        &input.turns[..=end],
        input,
        engine_name,
        budgets,
    ))
}

fn turns_hash(
    turns: &[DehydratedTurn],
    input: &SliceInput,
    engine_name: &str,
    budgets: &Budgets,
) -> String {
    hash_parts(&[
        DIGEST_PROMPT_VERSION,
        engine_name,
        &join_turns(turns),
        input.user_note.unwrap_or(""),
        previous_marker(input.previous_digest),
        // The split points change the result, so the budget is part of the identity.
        &budgets.chunk_chars.to_string(),
    ])
}

/// Hash of what the slice says, independent of the model and the prompts: changes only
/// when its turns or the user's note change. Reports record it to detect new material.
pub fn content_hash(input: &SliceInput) -> String {
    hash_parts(&[&join_turns(input.turns), input.user_note.unwrap_or("")])
}

fn previous_marker(previous_digest: Option<&str>) -> &'static str {
    if previous_digest.is_some() {
        "previous"
    } else {
        "first"
    }
}

/// The turns appended since `prior`, when an incremental update is allowed: the slice is
/// live, the prior digest has update budget left, and the new turns are small next to the
/// covered ones (otherwise a full digest gives a better result for about the same cost).
fn incremental_tail<'a>(
    turns: &'a [DehydratedTurn],
    prior: &PriorDigest,
    live: bool,
    budgets: &Budgets,
) -> Option<&'a [DehydratedTurn]> {
    if !live || prior.update_count >= budgets.max_incremental_updates {
        return None;
    }
    let end = turns.iter().position(|t| t.index == prior.covered_turn)?;
    let (covered, tail) = turns.split_at(end + 1);
    let chars = |ts: &[DehydratedTurn]| ts.iter().map(|t| t.text.chars().count()).sum::<usize>();
    (!tail.is_empty() && chars(tail) * 2 <= chars(covered)).then_some(tail)
}

pub fn summarize_slice(
    engine: &dyn Engine,
    engine_name: &str,
    input: &SliceInput,
    budgets: &Budgets,
) -> SliceOutcome {
    let text = join_turns(input.turns);
    let has_note = input.user_note.is_some_and(|n| !n.trim().is_empty());
    if text.chars().count() < budgets.passthrough_chars
        && !has_note
        && input.previous_digest.is_none()
    {
        return SliceOutcome::simple(StoredDigest::Raw { text }, true, None);
    }

    // A single turn larger than the budget cannot be split further; trim it instead.
    let turns: Vec<DehydratedTurn> = input
        .turns
        .iter()
        .map(|t| DehydratedTurn {
            index: t.index,
            text: truncate_middle(&t.text, budgets.chunk_chars),
        })
        .collect();
    let sizes: Vec<usize> = turns.iter().map(|t| t.text.chars().count()).collect();
    let plan = plan_chunks(&sizes, budgets.chunk_chars);
    let ctx = DigestContext {
        user_note: input.user_note,
        previous_digest: input.previous_digest,
        earlier_parts: None,
        candidate_projects: input.candidate_projects,
    };

    if plan.len() <= 1 {
        if let Some(prior) = input.prior {
            if let Some(tail) = incremental_tail(&turns, prior, input.live, budgets) {
                let prompt =
                    build_update_prompt(&prior.digest, &join_turns(tail), &ctx, input.language);
                // A failed update falls through to a full digest.
                if let Ok(d) = engine.complete(&prompt).and_then(|out| parse_digest(&out)) {
                    let mut outcome = SliceOutcome::simple(StoredDigest::Summary(d), true, None);
                    outcome.incremental = true;
                    outcome.update_count = prior.update_count + 1;
                    return outcome;
                }
            }
        }
        return match engine
            .complete(&build_prompt(&join_turns(&turns), &ctx, input.language))
            .and_then(|out| parse_digest(&out))
        {
            Ok(d) => SliceOutcome::simple(StoredDigest::Summary(d), true, None),
            Err(e) => SliceOutcome::simple(fallback(&text), false, Some(format!("{e:#}"))),
        };
    }

    // ---- Part by part ---------------------------------------------------------
    let mut outcome = SliceOutcome::simple(fallback(&text), false, None);
    let mut digests: Vec<Digest> = Vec::new();
    let mut earlier = String::new();
    for (n, range) in plan.iter().enumerate() {
        let part = &turns[range.clone()];
        let (start_turn, end_turn) = (part[0].index, part[part.len() - 1].index);
        let record = join_turns(part);
        let part_ctx = DigestContext {
            earlier_parts: Some(&earlier),
            candidate_projects: &[],
            ..ctx
        };
        let input_hash = hash_parts(&[
            DIGEST_PROMPT_VERSION,
            engine_name,
            &record,
            input.user_note.unwrap_or(""),
            previous_marker(input.previous_digest),
            &earlier,
        ]);

        let cached = input
            .cached_parts
            .iter()
            .find(|c| {
                c.start_turn == start_turn && c.end_turn == end_turn && c.input_hash == input_hash
            })
            .and_then(|c| serde_json::from_str::<Digest>(&c.digest_json).ok());
        let digest = match cached {
            Some(d) => {
                outcome.parts_cached += 1;
                d
            }
            None => match engine
                .complete(&build_prompt(&record, &part_ctx, input.language))
                .and_then(|out| parse_digest(&out))
            {
                Ok(d) => {
                    outcome.parts_generated += 1;
                    d
                }
                Err(e) => {
                    // Keep the parts that did succeed so the next run only redoes the rest.
                    outcome.error = Some(format!(
                        "part {} of {} (T{start_turn}-T{end_turn}): {e:#}",
                        n + 1,
                        plan.len()
                    ));
                    return outcome;
                }
            },
        };

        earlier.push_str(&format!(
            "Part {} (T{start_turn}-T{end_turn}): {}",
            n + 1,
            digest.goal
        ));
        if !digest.open_items.is_empty() {
            earlier.push_str(&format!(" | open: {}", digest.open_items.join("; ")));
        }
        earlier.push('\n');
        outcome.parts.push(PartDigest {
            start_turn,
            end_turn,
            input_hash,
            digest_json: serde_json::to_string(&digest).unwrap_or_default(),
        });
        digests.push(digest);
    }

    // ---- Merge ----------------------------------------------------------------
    let rendered: String = outcome
        .parts
        .iter()
        .enumerate()
        .map(|(n, p)| {
            format!(
                "Part {} (T{}-T{}):\n{}\n",
                n + 1,
                p.start_turn,
                p.end_turn,
                p.digest_json
            )
        })
        .collect();
    match engine
        .complete(&build_merge_prompt(&rendered, &ctx, input.language))
        .and_then(|out| parse_digest(&out))
    {
        Ok(merged) => {
            outcome.stored = StoredDigest::Summary(merged);
            outcome.cacheable = true;
        }
        Err(e) => {
            // The parts are good; combine them mechanically and retry the merge next time.
            outcome.stored = StoredDigest::Summary(merge_locally(&digests));
            outcome.error = Some(format!(
                "merging {} parts: {e:#}; combined without the model",
                digests.len()
            ));
        }
    }
    outcome
}

fn fallback(text: &str) -> StoredDigest {
    StoredDigest::Raw {
        text: truncate_middle(text, 3_000),
    }
}

/// Text that stands in for a digest when only previewing the material.
pub fn preview(turns: &[DehydratedTurn]) -> StoredDigest {
    StoredDigest::Raw {
        text: truncate_middle(&join_turns(turns), 1_500),
    }
}

/// Combine part digests without a model: concatenate, deduplicate, take the last state.
pub fn merge_locally(parts: &[Digest]) -> Digest {
    let mut merged = Digest::default();
    for part in parts {
        if merged.goal.is_empty() {
            merged.goal = part.goal.clone();
        }
        merged.key_changes.extend(part.key_changes.iter().cloned());
        merged
            .decisions_and_pitfalls
            .extend(part.decisions_and_pitfalls.iter().cloned());
        for tag in &part.tags {
            if !merged.tags.contains(tag) {
                merged.tags.push(tag.clone());
            }
        }
    }
    if let Some(last) = parts.last() {
        merged.outcome = last.outcome.clone();
        merged.open_items = last.open_items.clone();
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testing::MockEngine;

    fn turns(n: i64, chars_each: usize) -> Vec<DehydratedTurn> {
        (0..n)
            .map(|i| DehydratedTurn {
                index: i,
                text: format!("### T{i}\n{}", "x".repeat(chars_each)),
            })
            .collect()
    }

    fn budgets() -> Budgets {
        Budgets {
            passthrough_chars: 50,
            chunk_chars: 250,
            stage2_chars: 10_000,
            ..Budgets::default()
        }
    }

    /// Answers part requests with a digest naming the first turn of the record, and merge requests with a merged digest.
    fn engine() -> MockEngine {
        MockEngine::new(|prompt| {
            if prompt.contains("Part digests:") {
                return Ok(r#"{"goal":"merged goal","key_changes":[{"text":"all","turns":[0,5]}],"outcome":"completed"}"#.into());
            }
            let record = prompt.split("Session record:").nth(1).unwrap();
            let first = record
                .split("### T")
                .nth(1)
                .unwrap()
                .split('\n')
                .next()
                .unwrap()
                .to_string();
            Ok(format!(
                r#"{{"goal":"goal of T{first}","key_changes":[{{"text":"change","turns":[{first}]}}],"open_items":["todo {first}"],"outcome":"in_progress"}}"#
            ))
        })
    }

    fn input<'a>(turns: &'a [DehydratedTurn], cached: &'a [PartDigest]) -> SliceInput<'a> {
        SliceInput {
            language: &crate::lang::ZH_HANS,
            turns,
            user_note: None,
            previous_digest: None,
            cached_parts: cached,
            candidate_projects: &[],
            prior: None,
            live: false,
        }
    }

    #[test]
    fn short_slice_passes_through_without_a_model_call() {
        let t = turns(1, 10);
        let e = engine();
        let out = summarize_slice(&e, "mock", &input(&t, &[]), &budgets());
        assert!(matches!(out.stored, StoredDigest::Raw { .. }));
        assert!(out.cacheable);
        assert_eq!(e.call_count(), 0);
    }

    #[test]
    fn slice_within_budget_takes_one_call() {
        let t = turns(2, 80);
        let e = engine();
        let out = summarize_slice(&e, "mock", &input(&t, &[]), &budgets());
        assert_eq!(e.call_count(), 1);
        assert!(out.parts.is_empty());
        assert!(matches!(out.stored, StoredDigest::Summary(ref d) if d.goal == "goal of T0"));
    }

    #[test]
    fn long_slice_is_split_summarized_in_order_and_merged() {
        // Six turns of ~107 chars with a budget of 250: three parts of two turns each.
        let t = turns(6, 100);
        let e = engine();
        let out = summarize_slice(&e, "mock", &input(&t, &[]), &budgets());

        assert_eq!(e.call_count(), 4); // three parts and one merge
        assert_eq!((out.parts_generated, out.parts_cached), (3, 0));
        assert_eq!(
            out.parts
                .iter()
                .map(|p| (p.start_turn, p.end_turn))
                .collect::<Vec<_>>(),
            vec![(0, 1), (2, 3), (4, 5)]
        );
        assert!(out.cacheable && out.error.is_none());
        assert!(matches!(out.stored, StoredDigest::Summary(ref d) if d.goal == "merged goal"));

        let prompts = e.prompts.lock().unwrap();
        // Later parts see the key points of the earlier ones, not their records.
        assert!(!prompts[0].contains("Earlier parts:"));
        assert!(prompts[2].contains("Earlier parts:\nPart 1 (T0-T1): goal of T0 | open: todo 0\nPart 2 (T2-T3): goal of T2 | open: todo 2\n"));
        assert!(!prompts[2].contains("### T0"));
        // The merge sees every part digest.
        assert!(prompts[3].contains("Part 3 (T4-T5):"));
    }

    #[test]
    fn appended_turns_only_redo_the_last_part_and_the_merge() {
        let e = engine();
        let first = summarize_slice(&e, "mock", &input(&turns(6, 100), &[]), &budgets());
        let calls_before = e.call_count();

        let longer = turns(7, 100);
        let second = summarize_slice(&e, "mock", &input(&longer, &first.parts), &budgets());
        // Parts 1-3 are unchanged and reused; the new fourth part and the merge are the only calls.
        assert_eq!((second.parts_cached, second.parts_generated), (3, 1));
        assert_eq!(e.call_count() - calls_before, 2);

        // A changed note invalidates every part.
        let noted = SliceInput {
            user_note: Some("note"),
            ..input(&longer, &second.parts)
        };
        let third = summarize_slice(&e, "mock", &noted, &budgets());
        assert_eq!((third.parts_cached, third.parts_generated), (0, 4));
    }

    #[test]
    fn failed_part_keeps_finished_parts_and_is_not_cacheable() {
        let e = MockEngine::new(|prompt| {
            if prompt.contains("### T2") {
                anyhow::bail!("model unavailable")
            }
            Ok(r#"{"goal":"ok","key_changes":[{"text":"c","turns":[0]}]}"#.into())
        });
        let out = summarize_slice(&e, "mock", &input(&turns(6, 100), &[]), &budgets());
        assert!(!out.cacheable);
        assert_eq!(out.parts.len(), 1);
        assert!(out
            .error
            .as_deref()
            .unwrap()
            .starts_with("part 2 of 3 (T2-T3)"));
        assert!(matches!(out.stored, StoredDigest::Raw { .. }));
    }

    #[test]
    fn failed_merge_falls_back_to_a_local_merge() {
        let e = MockEngine::new(|prompt| {
            if prompt.contains("Part digests:") {
                anyhow::bail!("model unavailable")
            }
            let n = if prompt.contains("### T0") { 0 } else { 2 };
            Ok(format!(
                r#"{{"goal":"g{n}","key_changes":[{{"text":"c{n}","turns":[{n}]}}],"open_items":["o{n}"],"outcome":"s{n}","tags":["t","u{n}"]}}"#
            ))
        });
        let out = summarize_slice(&e, "mock", &input(&turns(4, 100), &[]), &budgets());
        assert!(!out.cacheable);
        assert_eq!(out.parts.len(), 2);
        let StoredDigest::Summary(d) = out.stored else {
            panic!("expected a summary")
        };
        assert_eq!(d.goal, "g0");
        assert_eq!(d.key_changes.len(), 2);
        assert_eq!(
            (d.outcome.as_str(), d.open_items.as_slice()),
            ("s2", &["o2".to_string()][..])
        );
        assert_eq!(d.tags, vec!["t", "u0", "u2"]);
    }

    #[test]
    fn oversized_turn_is_trimmed_rather_than_dropped() {
        let t = turns(1, 5_000);
        let e = engine();
        summarize_slice(&e, "mock", &input(&t, &[]), &budgets());
        let prompt = e.prompts.lock().unwrap()[0].clone();
        assert!(prompt.contains("chars omitted"));
    }

    #[test]
    fn slice_hash_depends_on_content_and_budget() {
        let t = turns(2, 80);
        let base = slice_hash(&input(&t, &[]), "mock", &budgets());
        assert_eq!(base, slice_hash(&input(&t, &[]), "mock", &budgets()));
        assert_ne!(
            base,
            slice_hash(&input(&turns(3, 80), &[]), "mock", &budgets())
        );
        assert_ne!(base, slice_hash(&input(&t, &[]), "other", &budgets()));
        let wider = Budgets {
            chunk_chars: 999,
            ..budgets()
        };
        assert_ne!(base, slice_hash(&input(&t, &[]), "mock", &wider));
        let projects = vec![CandidateProject {
            name: "App".into(),
            description: None,
        }];
        let with_candidates = SliceInput {
            candidate_projects: &projects,
            ..input(&t, &[])
        };
        assert_eq!(base, slice_hash(&with_candidates, "mock", &budgets()));
    }

    #[test]
    fn previous_digest_text_is_background_but_its_presence_counts() {
        let t = turns(2, 80);
        let with = |prev: Option<&'static str>| SliceInput {
            previous_digest: prev,
            ..input(&t, &[])
        };
        let a = slice_hash(&with(Some("{\"goal\":\"a\"}")), "mock", &budgets());
        let b = slice_hash(&with(Some("{\"goal\":\"b\"}")), "mock", &budgets());
        assert_eq!(a, b);
        assert_ne!(a, slice_hash(&with(None), "mock", &budgets()));
    }

    #[test]
    fn prefix_hash_matches_the_hash_of_the_shorter_slice() {
        let short = turns(2, 80);
        let long = turns(4, 80);
        let short_hash = slice_hash(&input(&short, &[]), "mock", &budgets());
        assert_eq!(
            prefix_hash(&input(&long, &[]), 1, "mock", &budgets()),
            Some(short_hash)
        );
        assert_eq!(prefix_hash(&input(&long, &[]), 9, "mock", &budgets()), None);
    }

    #[test]
    fn content_hash_ignores_engine_and_previous_digest() {
        let t = turns(2, 80);
        let base = content_hash(&input(&t, &[]));
        let with_prev = SliceInput {
            previous_digest: Some("{}"),
            ..input(&t, &[])
        };
        assert_eq!(base, content_hash(&with_prev));
        let noted = SliceInput {
            user_note: Some("note"),
            ..input(&t, &[])
        };
        assert_ne!(base, content_hash(&noted));
    }

    fn prior(covered_turn: i64, update_count: u32) -> PriorDigest {
        PriorDigest {
            digest: Digest {
                goal: "old goal".into(),
                ..Digest::default()
            },
            covered_turn,
            update_count,
        }
    }

    fn update_engine() -> MockEngine {
        MockEngine::new(|prompt| {
            if prompt.contains("Current digest:") {
                return Ok(r#"{"goal":"updated","key_changes":[{"text":"c","turns":[3]}]}"#.into());
            }
            Ok(r#"{"goal":"full","key_changes":[{"text":"c","turns":[0]}]}"#.into())
        })
    }

    #[test]
    fn live_slice_that_grew_a_little_is_updated_incrementally() {
        // Four turns of ~47 chars fit in one part; the prior digest covers the first three.
        let t = turns(4, 40);
        let p = prior(2, 0);
        let e = update_engine();
        let live = SliceInput {
            prior: Some(&p),
            live: true,
            ..input(&t, &[])
        };
        let out = summarize_slice(&e, "mock", &live, &budgets());
        assert_eq!(e.call_count(), 1);
        assert!(out.incremental && out.cacheable);
        assert_eq!(out.update_count, 1);
        assert!(matches!(out.stored, StoredDigest::Summary(ref d) if d.goal == "updated"));
        let prompt = e.prompts.lock().unwrap()[0].clone();
        assert!(prompt.contains("\"goal\":\"old goal\""));
        let new_turns = prompt.split("New turns:").nth(1).unwrap();
        assert!(new_turns.contains("### T3") && !new_turns.contains("### T2"));
    }

    #[test]
    fn full_digest_when_not_live_out_of_updates_or_the_tail_is_large() {
        let t = turns(4, 40);
        let cases = [
            (prior(2, 0), false), // the session went quiet
            (prior(2, 3), true),  // the update budget is used up
            (prior(0, 0), true),  // three new turns against one covered turn
        ];
        for (p, live) in cases {
            let e = update_engine();
            let i = SliceInput {
                prior: Some(&p),
                live,
                ..input(&t, &[])
            };
            let out = summarize_slice(&e, "mock", &i, &budgets());
            assert!(!out.incremental);
            assert_eq!(out.update_count, 0);
            assert!(matches!(out.stored, StoredDigest::Summary(ref d) if d.goal == "full"));
        }
    }

    #[test]
    fn failed_incremental_update_falls_back_to_a_full_digest() {
        let t = turns(4, 40);
        let p = prior(2, 0);
        let e = MockEngine::new(|prompt| {
            if prompt.contains("Current digest:") {
                anyhow::bail!("model unavailable")
            }
            Ok(r#"{"goal":"full","key_changes":[{"text":"c","turns":[0]}]}"#.into())
        });
        let i = SliceInput {
            prior: Some(&p),
            live: true,
            ..input(&t, &[])
        };
        let out = summarize_slice(&e, "mock", &i, &budgets());
        assert_eq!(e.call_count(), 2);
        assert!(!out.incremental && out.cacheable);
    }

    #[test]
    fn candidate_projects_appear_in_single_and_merge_prompts_only() {
        let projects = vec![CandidateProject {
            name: "Atlas".into(),
            description: Some("paper".into()),
        }];

        let t_single = turns(2, 80);
        let e_single = engine();
        summarize_slice(
            &e_single,
            "mock",
            &SliceInput {
                candidate_projects: &projects,
                ..input(&t_single, &[])
            },
            &budgets(),
        );
        assert!(e_single.prompts.lock().unwrap()[0]
            .contains("Candidate projects:\n- \"Atlas\" — paper"));

        let t_split = turns(6, 100);
        let e_split = engine();
        summarize_slice(
            &e_split,
            "mock",
            &SliceInput {
                candidate_projects: &projects,
                ..input(&t_split, &[])
            },
            &budgets(),
        );
        let prompts = e_split.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 4);
        assert!(!prompts[0].contains("Candidate projects:"));
        assert!(!prompts[1].contains("Candidate projects:"));
        assert!(!prompts[2].contains("Candidate projects:"));
        assert!(prompts[3].contains("Candidate projects:\n- \"Atlas\" — paper"));
    }
}
