use super::*;
use crate::engine::testing::MockEngine;
use crate::lang::{EN, ZH_HANS};
use crate::manifest::Manifest;
use crate::summarize::Budgets;
use aj_core::{slice::reslice_session, Config};
use aj_ir::{AgentId, IrEvent};
use chrono::{DateTime, FixedOffset, Utc};
use std::sync::atomic::Ordering;

fn t(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

fn turn(id: &str, at: &str, prompt: &str) -> IrEvent {
    IrEvent::TurnStart {
        source_event_id: id.into(),
        timestamp: t(at),
        user_prompt: prompt.into(),
    }
}

fn message(id: &str, at: &str, text: String) -> IrEvent {
    IrEvent::AgentMessage {
        source_event_id: id.into(),
        timestamp: t(at),
        text,
    }
}

fn add_session(store: &mut Store, tz: &FixedOffset, sid: &str, project: i64, events: &[IrEvent]) {
    let id = store
        .ingest(&AgentId::CLAUDE, sid, events)
        .unwrap()
        .session_id;
    store.assign_manual(id, Some(project)).unwrap();
    reslice_session(store, id, &Config::default(), tz).unwrap();
}

/// Build a store with two sessions: a long one (needs a digest) and a short one (passthrough).
fn fixture() -> (Store, FixedOffset) {
    let tz = FixedOffset::east_opt(8 * 3600).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    prompt_library::sync_builtins(&mut store).unwrap();
    let pid = store.create_project("app", None).unwrap();

    let long = vec![
        turn(
            "u1",
            "2026-11-16T02:00:00Z",
            "fix the concurrent OAuth refresh",
        ),
        message(
            "m1",
            "2026-11-16T02:05:00Z",
            "Detailed explanation. ".repeat(100),
        ),
    ];
    let short = vec![turn(
        "u1",
        "2026-11-16T03:00:00Z",
        "what does this error mean",
    )];
    add_session(&mut store, &tz, "long", pid, &long);
    add_session(&mut store, &tz, "short", pid, &short);
    (store, tz)
}

fn opts() -> DailyOptions {
    DailyOptions {
        period: "2026-11-16".into(),
        project: None,
        prompt: None,
        concurrency: 2,
        dry_run: false,
        budgets: Budgets::default(),
        cancel: None,
        mode: ReportMode::Regenerate,
        // Long after the fixture's sessions, so no slice is live.
        now: t("2026-11-20T00:00:00Z"),
        language: &crate::lang::ZH_HANS,
    }
}

fn ctx<'a>(engine_name: &'a str, budgets: &'a Budgets) -> MaterialContext<'a> {
    MaterialContext {
        engine_name,
        budgets,
        now: t("2026-11-20T00:00:00Z"),
        language: &crate::lang::ZH_HANS,
    }
}

const DIGEST_REPLY: &str = r#"{"goal":"fix concurrent refresh","key_changes":[{"text":"added a mutex","turns":[0]}],"outcome":"completed"}"#;

fn mock() -> MockEngine {
    MockEngine::new(|prompt| {
        Ok(
            if prompt.contains("Session record:") || prompt.contains("Part digests:") {
                DIGEST_REPLY.into()
            } else {
                "```markdown\n# Daily report 2026-11-16\n\n## app\n- added a mutex [#S1.T0]\n- made-up content [#S77]\n```".into()
            },
        )
    })
}

fn run(store: &mut Store, engine: &MockEngine, tz: &FixedOffset, o: &DailyOptions) -> DailyResult {
    generate_daily(store, engine, tz, o, &mut |_| {}).unwrap()
}

#[test]
fn generates_report_caches_digests_and_verifies_anchors() {
    let (mut store, tz) = fixture();
    let engine = mock();
    let r = run(&mut store, &engine, &tz, &opts());

    assert_eq!(r.slices_used, 2);
    assert_eq!(
        (
            r.stage1.digests_generated,
            r.stage1.digests_passthrough,
            r.stage1.digests_cached
        ),
        (1, 1, 0)
    );
    // One stage 1 call (the short slice passes through without a model call) plus one stage 2 call.
    assert_eq!(engine.call_count(), 2);
    assert_eq!(
        r.anchors,
        AnchorCheck {
            total: 2,
            invalid: 1
        }
    );
    assert_eq!(
        r.markdown,
        "# 2026-11-16 \u{00b7} \u{5468}\u{4e00}\n\n## app\n- added a mutex [#S1.T0]\n- made-up content"
    );

    // The stage 2 material contains the digest, not the long original text.
    let stage2 = engine.prompts.lock().unwrap().last().unwrap().clone();
    assert!(stage2.contains("Key changes:\n- added a mutex [#S1.T0]"));
    assert!(stage2.contains("what does this error mean"));
    assert!(!stage2.contains("Detailed explanation. Detailed explanation."));

    // Second run: every digest is cached, only stage 2 is called; the report gains a version instead of being overwritten.
    let r2 = run(&mut store, &engine, &tz, &opts());
    assert_eq!(
        (r2.stage1.digests_generated, r2.stage1.digests_cached),
        (0, 2)
    );
    assert_eq!(engine.call_count(), 3);
    let versions = store.report_versions("daily", "2026-11-16", None).unwrap();
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[1].prompt_name.as_deref(), Some("standard"));
    assert!(versions[1]
        .prompt_snapshot
        .as_deref()
        .unwrap()
        .contains("{date}"));
}

#[test]
fn note_invalidates_cache_and_ignored_sessions_are_excluded() {
    let (mut store, tz) = fixture();
    let engine = mock();
    run(&mut store, &engine, &tz, &opts());

    let long = store.find_sessions_by_prefix("long").unwrap().remove(0);
    store
        .set_session_note(long.id, Some("switched to a shared promise"))
        .unwrap();
    let short = store.find_sessions_by_prefix("short").unwrap().remove(0);
    store.set_session_ignored(short.id, true).unwrap();

    let r = run(&mut store, &engine, &tz, &opts());
    assert_eq!((r.slices_used, r.slices_ignored), (1, 1));
    assert_eq!(
        (r.stage1.digests_generated, r.stage1.digests_cached),
        (1, 0)
    );
    let prompts = engine.prompts.lock().unwrap();
    assert!(prompts[prompts.len() - 2].contains("User note: switched to a shared promise"));
}

#[test]
fn failed_digest_falls_back_to_raw_and_is_not_cached() {
    let (mut store, tz) = fixture();
    let engine = MockEngine::new(|prompt| {
        if prompt.contains("Session record:") {
            anyhow::bail!("model unavailable")
        }
        Ok("# Daily report\n\n## app\n- worked on app".into())
    });
    let r = run(&mut store, &engine, &tz, &opts());
    assert_eq!(r.warnings.len(), 1);
    let long = store.find_sessions_by_prefix("long").unwrap().remove(0);
    assert!(store.slices_for_session(long.id).unwrap()[0]
        .digest_json
        .is_none());
}

#[test]
fn dry_run_calls_no_model_and_saves_nothing() {
    let (mut store, tz) = fixture();
    let engine = mock();
    let r = run(
        &mut store,
        &engine,
        &tz,
        &DailyOptions {
            dry_run: true,
            ..opts()
        },
    );
    assert_eq!(engine.call_count(), 0);
    assert!(r.markdown.contains("## Project: app"));
    assert!(store
        .report_versions("daily", "2026-11-16", None)
        .unwrap()
        .is_empty());
}

#[test]
fn chosen_prompt_shapes_the_request_and_is_recorded_with_the_version() {
    let (mut store, tz) = fixture();
    store
        .create_prompt(
            "daily",
            "zh-Hans",
            "terse",
            "Summarize {date} in three bullets for {project}.",
        )
        .unwrap();
    let engine = mock();
    let options = DailyOptions {
        prompt: Some("terse".into()),
        project: Some("APP".into()),
        ..opts()
    };
    let r = run(&mut store, &engine, &tz, &options);
    assert_eq!(r.prompt_name, "terse");

    let stage2 = engine.prompts.lock().unwrap().last().unwrap().clone();
    assert!(stage2.contains("Summarize 2026-11-16 in three bullets for APP."));
    // The fixed rules follow the user's text, and the built-in body is not used.
    assert!(stage2.contains("[#S12.T5]"));
    let builtin_first_line = crate::prompts::ZH_HANS
        .daily_standard
        .lines()
        .next()
        .unwrap()
        .replace("{date}", "2026-11-16");
    assert!(!stage2.contains(&builtin_first_line));

    let pid = store.find_project("app").unwrap().unwrap().id;
    let versions = store
        .report_versions("daily", "2026-11-16", Some(pid))
        .unwrap();
    assert_eq!(versions[0].prompt_name.as_deref(), Some("terse"));
    // The snapshot keeps the text as written, placeholders included.
    assert_eq!(
        versions[0].prompt_snapshot.as_deref(),
        Some("Summarize {date} in three bullets for {project}.")
    );

    let missing = DailyOptions {
        prompt: Some("missing".into()),
        ..opts()
    };
    assert!(generate_daily(&mut store, &engine, &tz, &missing, &mut |_| {}).is_err());
}

/// A session of eight turns, each carrying about 600 chars of agent text.
fn long_session(turns: usize) -> Vec<IrEvent> {
    let mut events = Vec::new();
    for i in 0..turns {
        let at = format!("2026-11-16T02:{:02}:00Z", i * 2);
        events.push(turn(&format!("u{i}"), &at, &format!("step {i}")));
        let at = format!("2026-11-16T02:{:02}:30Z", i * 2);
        events.push(message(&format!("m{i}"), &at, "Did some work. ".repeat(40)));
    }
    events
}

#[test]
fn long_slice_is_digested_in_parts_and_parts_are_reused_when_it_grows() {
    let tz = FixedOffset::east_opt(8 * 3600).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    prompt_library::sync_builtins(&mut store).unwrap();
    let pid = store.create_project("app", None).unwrap();
    add_session(&mut store, &tz, "big", pid, &long_session(8));

    // Each turn is ~640 chars, so a budget of 1500 puts two turns in a part: four parts.
    let options = DailyOptions {
        budgets: Budgets {
            chunk_chars: 1_500,
            ..Budgets::default()
        },
        ..opts()
    };
    let engine = mock();
    let r = run(&mut store, &engine, &tz, &options);
    assert_eq!(
        (
            r.stage1.slices_split,
            r.stage1.parts_generated,
            r.stage1.parts_cached
        ),
        (1, 4, 0)
    );
    // Four parts, one merge, one report.
    assert_eq!(engine.call_count(), 6);
    let slice_id = store.slices_for_date("2026-11-16").unwrap()[0].id;
    let parts = store.chunk_digests(slice_id).unwrap();
    assert_eq!(parts.len(), 4);
    // The digest and its parts record the engine that wrote them.
    assert!(parts.iter().all(|p| p.engine.as_deref() == Some("mock")));
    let slice = store.slice(slice_id).unwrap().unwrap();
    assert_eq!(slice.digest_engine.as_deref(), Some("mock"));

    // Unchanged: the slice digest itself is cached, so only the report is regenerated.
    let r = run(&mut store, &engine, &tz, &options);
    assert_eq!((r.stage1.digests_cached, r.stage1.slices_split), (1, 0));
    assert_eq!(engine.call_count(), 7);

    // Two more turns arrive: four old parts are reused, one new part, one merge, one report.
    let more = long_session(10);
    add_session(&mut store, &tz, "big", pid, &more[16..]);
    let r = run(&mut store, &engine, &tz, &options);
    assert_eq!(
        (
            r.stage1.slices_split,
            r.stage1.parts_generated,
            r.stage1.parts_cached
        ),
        (1, 1, 4)
    );
    assert_eq!(engine.call_count(), 10);
}

#[test]
fn oversized_day_condenses_the_largest_project_before_the_report() {
    let (mut store, tz) = fixture();
    let other = store.create_project("tiny", None).unwrap();
    add_session(
        &mut store,
        &tz,
        "t",
        other,
        &[turn("u1", "2026-11-16T04:00:00Z", "a quick question")],
    );
    // Give the long session's digest a lot of text so that project "app" dominates.
    let engine = MockEngine::new(|prompt| {
        Ok(if prompt.contains("Session record:") {
            format!(
                r#"{{"goal":"big goal","key_changes":[{{"text":"{}","turns":[0]}}]}}"#,
                "long change ".repeat(60)
            )
        } else if prompt.contains(
            crate::prompts::ZH_HANS
                .project_brief
                .lines()
                .next()
                .unwrap(),
        ) {
            r#"{"clusters":[{"title":"condensed point","points":[{"text":"did it","anchors":["S1.T0"]}],"slice_ids":[1]}]}"#.into()
        } else {
            "# Daily report\n\n## app\n- done [#S1.T0]\n\n## tiny\n- quick question".into()
        })
    });
    let options = DailyOptions {
        budgets: Budgets {
            stage2_chars: 600,
            ..Budgets::default()
        },
        ..opts()
    };
    let r = run(&mut store, &engine, &tz, &options);
    assert_eq!((r.projects_condensed, r.briefs_cached), (1, 0));
    let briefs = store.project_briefs("2026-11-16").unwrap();
    assert!(!briefs.is_empty());
    assert!(briefs.iter().all(|b| b.engine.as_deref() == Some("mock")));

    let prompts = engine.prompts.lock().unwrap();
    let final_prompt = prompts.last().unwrap();
    assert!(final_prompt
        .contains("Task clusters condensed from this project's material:\n- condensed point (slices: S1)\n  - did it [#S1.T0]"));
    assert!(!final_prompt.contains("long change long change"));
    // The small project keeps its full material.
    assert!(final_prompt.contains("## Project: tiny"));
    assert!(final_prompt.contains("a quick question"));
    // Anchors from the brief still verify against the original slices.
    assert_eq!(
        r.anchors,
        AnchorCheck {
            total: 1,
            invalid: 0
        }
    );
    drop(prompts);

    // Second run reuses both slice digests and the cached project brief (only stage 2 runs).
    let calls_before = engine.call_count();
    let r2 = run(&mut store, &engine, &tz, &options);
    assert_eq!((r2.projects_condensed, r2.briefs_cached), (1, 1));
    assert_eq!(engine.call_count() - calls_before, 1);
}

#[test]
fn llm_suggestion_applies_to_unassigned_session_and_respects_existing_state() {
    let tz = FixedOffset::east_opt(8 * 3600).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    prompt_library::sync_builtins(&mut store).unwrap();
    let atlas = store
        .create_project("Atlas", Some("research paper and evaluation"))
        .unwrap();
    let other = store.create_project("Other", Some("unrelated")).unwrap();

    let events = |sid: &str, prompt: &str| {
        vec![
            turn(&format!("{sid}-u1"), "2026-11-16T02:00:00Z", prompt),
            message(
                &format!("{sid}-m1"),
                "2026-11-16T02:05:00Z",
                "Detailed analysis. ".repeat(100),
            ),
        ]
    };
    let s_free = store
        .ingest(
            &AgentId::CLAUDE,
            "free",
            &events("f", "revise the Atlas paper"),
        )
        .unwrap()
        .session_id;
    reslice_session(&mut store, s_free, &Config::default(), &tz).unwrap();

    let s_none = store
        .ingest(
            &AgentId::CLAUDE,
            "explicit-none",
            &events("n", "random chat"),
        )
        .unwrap()
        .session_id;
    store.assign_manual(s_none, None).unwrap();
    reslice_session(&mut store, s_none, &Config::default(), &tz).unwrap();

    let s_ruled = store
        .ingest(
            &AgentId::CLAUDE,
            "rule-suggested",
            &events("r", "something else"),
        )
        .unwrap()
        .session_id;
    store
        .set_suggestion(s_ruled, Some((other, "rule", "directory /work/other")))
        .unwrap();
    reslice_session(&mut store, s_ruled, &Config::default(), &tz).unwrap();

    let engine = MockEngine::new(|prompt| {
        if prompt.contains("Session record:") {
            if prompt.contains("revise the Atlas paper") {
                assert!(prompt.contains("Candidate projects:\n"));
                assert!(prompt.contains("- \"Atlas\" — research paper and evaluation"));
                return Ok(r#"{"goal":"revise paper","key_changes":[{"text":"edited","turns":[0]}],"suggested_project":"\"Atlas\": research paper and evaluation"}"#.into());
            }
            assert!(!prompt.contains("Candidate projects:"));
            return Ok(r#"{"goal":"other","key_changes":[{"text":"done","turns":[0]}],"suggested_project":"Atlas"}"#.into());
        }
        Ok("# Daily report\n\n## Unassigned\n- reviewed pending work".into())
    });

    let r = run(&mut store, &engine, &tz, &opts());
    assert_eq!(r.stage1.suggestions_made, 1);

    let free_row = store.session(s_free).unwrap().unwrap();
    assert_eq!(free_row.project_id, None);
    assert_eq!(free_row.suggested_project_id, Some(atlas));
    assert_eq!(free_row.suggestion_source.as_deref(), Some("llm"));
    assert_eq!(
        free_row.suggestion_reason.as_deref(),
        Some("judged by model")
    );

    // Explicitly unassigned ("none") and rule-suggested sessions are left untouched.
    let none_row = store.session(s_none).unwrap().unwrap();
    assert_eq!(none_row.suggested_project_id, None);
    let ruled_row = store.session(s_ruled).unwrap().unwrap();
    assert_eq!(ruled_row.suggested_project_id, Some(other));
    assert_eq!(ruled_row.suggestion_source.as_deref(), Some("rule"));
}

#[test]
fn estimate_single_digest_and_cancellation_work() {
    let (mut store, tz) = fixture();
    let engine = mock();
    let est = estimate_daily(
        &store,
        &tz,
        "2026-11-16",
        None,
        &ctx(&engine.name(), &opts().budgets),
    )
    .unwrap();
    assert_eq!(
        (
            est.slices_used,
            est.slices_ignored,
            est.digests_cached,
            est.digests_pending
        ),
        (2, 0, 0, 2)
    );
    assert!(est.estimated_chars > 0);

    let long_slice_id = store.slices_for_date("2026-11-16").unwrap()[0].id;
    let stored = crate::stage1::digest_single_slice(
        &mut store,
        &engine,
        &tz,
        long_slice_id,
        &opts().budgets,
        opts().now,
        &crate::lang::ZH_HANS,
    )
    .unwrap();
    assert!(matches!(stored, StoredDigest::Summary(_)));
    assert_eq!(engine.call_count(), 1);

    // Now one slice is cached and one remains pending.
    let est2 = estimate_daily(
        &store,
        &tz,
        "2026-11-16",
        None,
        &ctx(&engine.name(), &opts().budgets),
    )
    .unwrap();
    assert_eq!((est2.digests_cached, est2.digests_pending), (1, 1));
    assert_eq!(est2.digests_other_model, 0);

    // Another model would digest the cached slice again, and says so.
    let est3 = estimate_daily(
        &store,
        &tz,
        "2026-11-16",
        None,
        &ctx("other:model", &opts().budgets),
    )
    .unwrap();
    assert_eq!(
        (
            est3.digests_cached,
            est3.digests_pending,
            est3.digests_other_model
        ),
        (0, 2, 1)
    );

    // Cancelling before completion leaves already-cached digests intact and errors cleanly.
    let cancel = Arc::new(AtomicBool::new(true));
    let err = generate_daily(
        &mut store,
        &engine,
        &tz,
        &DailyOptions {
            cancel: Some(cancel),
            ..opts()
        },
        &mut |_| {},
    )
    .unwrap_err();
    assert!(err.to_string().contains("cancelled"));
    assert_eq!(engine.call_count(), 1);
}

#[test]
fn unstructured_stage2_triggers_repair_retry_and_fails_if_still_invalid() {
    let (mut store, tz) = fixture();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let calls_ref = calls.clone();
    let engine = MockEngine::new(move |prompt| {
        if prompt.contains("Session record:") {
            return Ok(DIGEST_REPLY.into());
        }
        let n = calls_ref.fetch_add(1, Ordering::Relaxed);
        if n == 0 {
            // First Stage 2 output misses the required `## app` heading and uses prose.
            Ok("# Daily report\nWorked on fixing the OAuth bug today.".into())
        } else {
            assert!(prompt.contains("violated the required Markdown structure"));
            Ok("# Daily report\n\n## app\n- added a mutex [#S1.T0]".into())
        }
    });

    let r = run(&mut store, &engine, &tz, &opts());
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(r
        .warnings
        .iter()
        .any(|w| w.contains("required structural repair")));
    assert!(r.markdown.contains("## app\n- added a mutex [#S1.T0]"));

    // If both initial and repair replies fail structure validation, no version is saved.
    let bad_engine = MockEngine::new(|prompt| {
        if prompt.contains("Session record:") {
            Ok(DIGEST_REPLY.into())
        } else {
            Ok("# Daily report\nStill plain prose without sections.".into())
        }
    });
    let err = generate_daily(&mut store, &bad_engine, &tz, &opts(), &mut |_| {}).unwrap_err();
    assert!(err.to_string().contains("failed structural validation"));
    assert_eq!(
        store
            .report_versions("daily", "2026-11-16", None)
            .unwrap()
            .len(),
        1
    );
}

// ---- Incremental updates ---------------------------------------------------

/// Answers digest requests with a digest and report requests with one bullet per
/// requested project, tagged with a running stage 2 call number.
fn sectioned_mock() -> MockEngine {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    MockEngine::new(move |prompt| {
        if prompt.contains("Session record:") || prompt.contains("Current digest:") {
            return Ok(DIGEST_REPLY.into());
        }
        let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
        let mut out = String::from("# report\n");
        for line in prompt.lines() {
            if let Some(project) = line.strip_prefix("## Project: ") {
                out.push_str(&format!("\n## {project}\n- {project} v{n}\n"));
            }
        }
        Ok(out)
    })
}

fn add_web(store: &mut Store, tz: &FixedOffset, sid: &str, at: &str) {
    let web = match store.find_project("web").unwrap() {
        Some(p) => p.id,
        None => store.create_project("web", None).unwrap(),
    };
    add_session(
        store,
        tz,
        sid,
        web,
        &[turn("u1", at, "tweak the landing page")],
    );
}

#[test]
fn removes_cross_project_anchors_even_when_both_slices_have_the_turn() {
    let (mut store, tz) = fixture();
    add_web(&mut store, &tz, "web1", "2026-11-16T04:00:00Z");
    let engine = MockEngine::new(|prompt| {
        if prompt.contains("Session record:") {
            return Ok(DIGEST_REPLY.into());
        }
        Ok("# Report\n\n## app\n- Added a mutex [#S1.T0]\n\n## web\n- Tweaked the landing page [#S1.T0] [#S3.T0]".into())
    });

    let result = run(&mut store, &engine, &tz, &opts());
    assert_eq!(
        result.anchors,
        AnchorCheck {
            total: 3,
            invalid: 1
        }
    );
    let saved = store
        .current_report_version(KIND_DAILY, "2026-11-16", None)
        .unwrap()
        .unwrap();
    assert!(saved.markdown.contains("## app\n- Added a mutex [#S1.T0]"));
    assert_eq!(
        structure::extract_project_section(&saved.markdown, "web").as_deref(),
        Some("- Tweaked the landing page [#S3.T0]")
    );
}

fn update_opts() -> DailyOptions {
    DailyOptions {
        mode: ReportMode::Update,
        ..opts()
    }
}

fn staleness_now(store: &Store, tz: &FixedOffset) -> Option<Staleness> {
    let budgets = Budgets::default();
    estimate_daily(store, tz, "2026-11-16", None, &ctx("mock", &budgets))
        .unwrap()
        .staleness
}

/// Save a user edit of the current version, carrying over its manifest as the app does.
fn edit_current(store: &mut Store, from: &str, to: &str) {
    let v = store
        .current_report_version(KIND_DAILY, "2026-11-16", None)
        .unwrap()
        .unwrap();
    store
        .save_report_version(&NewReportVersion {
            report_type: KIND_DAILY,
            period_key: "2026-11-16",
            project_id: None,
            origin: "user_edit",
            markdown: &v.markdown.replace(from, to),
            engine: None,
            input_hash: None,
            prompt_id: None,
            prompt_name: None,
            prompt_snapshot: None,
            input_manifest: v.input_manifest.as_deref(),
            language: &v.language,
        })
        .unwrap();
}

#[test]
fn update_with_another_model_records_each_sections_writer() {
    let (mut store, tz) = fixture();
    add_web(&mut store, &tz, "web1", "2026-11-16T04:00:00Z");
    run(&mut store, &sectioned_mock(), &tz, &opts());
    let app = store.find_project("app").unwrap().unwrap().id;
    add_session(
        &mut store,
        &tz,
        "late",
        app,
        &[turn("u1", "2026-11-16T05:00:00Z", "one more fix")],
    );
    let r = run(
        &mut store,
        &sectioned_mock().named("other"),
        &tz,
        &update_opts(),
    );
    assert_eq!(r.projects_written, vec!["app"]);

    let current = store
        .report_versions(KIND_DAILY, "2026-11-16", None)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(current.engine.as_deref(), Some("other"));
    let m = Manifest::parse(current.input_manifest.as_deref().unwrap()).unwrap();
    let engine_of = |name: &str| {
        m.projects
            .values()
            .find(|e| e.name == name)
            .and_then(|e| e.engine.clone())
    };
    assert_eq!(engine_of("app").as_deref(), Some("other"));
    assert_eq!(engine_of("web").as_deref(), Some("mock"));
}

#[test]
fn update_rewrites_only_changed_projects_and_keeps_user_edits() {
    let (mut store, tz) = fixture();
    add_web(&mut store, &tz, "web1", "2026-11-16T04:00:00Z");
    let engine = sectioned_mock();
    let r1 = run(&mut store, &engine, &tz, &opts());
    assert!(r1.markdown.contains("## app\n- app v1") && r1.markdown.contains("## web\n- web v1"));
    assert_eq!(staleness_now(&store, &tz), None);

    edit_current(&mut store, "- web v1", "- web v1, edited by me [#S1.T0]");
    // An edit alone does not make the report stale.
    assert_eq!(staleness_now(&store, &tz), None);
    store
        .set_report_status(KIND_DAILY, "2026-11-16", None, "final")
        .unwrap();

    // A new app session arrives.
    let app = store.find_project("app").unwrap().unwrap().id;
    add_session(
        &mut store,
        &tz,
        "late",
        app,
        &[turn("u1", "2026-11-16T05:00:00Z", "one more fix")],
    );
    let stale = staleness_now(&store, &tz).unwrap();
    assert_eq!(stale.projects, vec!["app"]);
    assert_eq!((stale.new_slices, stale.changed_slices), (1, 0));

    let r2 = run(&mut store, &engine, &tz, &update_opts());
    assert!(r2.updated);
    assert_eq!(r2.projects_written, vec!["app"]);
    assert_eq!(r2.sections_kept, 1);
    assert!(r2.markdown.contains("## app\n- app v2"));
    assert!(r2
        .markdown
        .contains("## web\n- web v1, edited by me [#S1.T0]"));
    let stage2 = engine.prompts.lock().unwrap().last().unwrap().clone();
    assert!(stage2.contains("Write only the sections for these projects: app."));
    assert!(!stage2.contains("## Project: web"));
    assert!(!stage2.contains("Current section:"));
    // The finalized report needs a fresh review.
    assert_eq!(
        store
            .report_meta(KIND_DAILY, "2026-11-16", None)
            .unwrap()
            .unwrap()
            .status,
        "draft"
    );
    assert_eq!(staleness_now(&store, &tz), None);

    let err = generate_daily(&mut store, &engine, &tz, &update_opts(), &mut |_| {}).unwrap_err();
    assert!(err.to_string().contains("already up to date"));

    // The kept section is still recognized as edited by a later update.
    add_web(&mut store, &tz, "web2", "2026-11-16T06:00:00Z");
    run(&mut store, &engine, &tz, &update_opts());
    let stage2 = engine.prompts.lock().unwrap().last().unwrap().clone();
    assert!(stage2.contains("## Current section: web\n- web v1, edited by me"));
}

#[test]
fn update_drops_a_project_whose_slices_were_all_ignored() {
    let (mut store, tz) = fixture();
    add_web(&mut store, &tz, "web1", "2026-11-16T04:00:00Z");
    let engine = sectioned_mock();
    run(&mut store, &engine, &tz, &opts());
    let web_slice = store
        .slices_for_date("2026-11-16")
        .unwrap()
        .into_iter()
        .find(|s| s.slice_start_at == t("2026-11-16T04:00:00Z"))
        .unwrap();
    store
        .set_slice_ignore_state(web_slice.id, "ignored")
        .unwrap();
    assert_eq!(staleness_now(&store, &tz).unwrap().projects, vec!["web"]);

    let calls = engine.call_count();
    let r = run(&mut store, &engine, &tz, &update_opts());
    assert_eq!(
        engine.call_count(),
        calls,
        "nothing to write, so no model call"
    );
    assert!(r.projects_written.is_empty());
    assert!(r.markdown.contains("## app\n- app v1"));
    assert!(!r.markdown.contains("## web"));
}

#[test]
fn changed_prompt_rewrites_every_project_and_missing_base_regenerates() {
    let (mut store, tz) = fixture();
    add_web(&mut store, &tz, "web1", "2026-11-16T04:00:00Z");
    let engine = sectioned_mock();

    // No version yet: an update falls back to a full report.
    let r = run(&mut store, &engine, &tz, &update_opts());
    assert!(!r.updated);
    assert!(r.warnings.iter().any(|w| w.contains("no previous version")));

    let concise = DailyOptions {
        prompt: Some("concise".into()),
        ..update_opts()
    };
    let r2 = run(&mut store, &engine, &tz, &concise);
    assert!(r2.updated);
    assert_eq!(r2.projects_written, vec!["app", "web"]);
}

#[test]
fn switching_language_rewrites_everything_and_reuses_no_digest() {
    let (mut store, tz) = fixture();
    let engine = sectioned_mock();
    let zh = run(&mut store, &engine, &tz, &update_opts());
    assert_eq!(zh.stage1.digests_generated, 1);
    assert!(zh.markdown.starts_with(&ZH_HANS.daily_title("2026-11-16")));
    let zh_calls = engine.call_count();

    // Nothing changed, but the language did: every section is written again in
    // English, from English digests, under an English title.
    let en = DailyOptions {
        language: &EN,
        ..update_opts()
    };
    let r = run(&mut store, &engine, &tz, &en);
    assert!(r.updated);
    assert_eq!(r.projects_written, vec!["app"]);
    assert_eq!(r.stage1.digests_generated, 1);
    assert!(r.markdown.starts_with("# 2026-11-16 \u{00b7} Mon"));
    let prompts = engine.prompts.lock().unwrap()[zh_calls..].to_vec();
    assert!(prompts.iter().any(|p| p.contains(&EN.json_instruction())));
    assert!(prompts.last().unwrap().contains(&EN.report_instruction()));
    let v = store
        .current_report_version(KIND_DAILY, "2026-11-16", None)
        .unwrap()
        .unwrap();
    assert_eq!(v.language, "en");

    // Asked again in English with nothing changed, the report is up to date.
    assert!(generate_daily(&mut store, &engine, &tz, &en, &mut |_| {}).is_err());
}

#[test]
fn live_slice_that_grew_is_digested_incrementally() {
    let (mut store, tz) = fixture();
    let engine = sectioned_mock();
    // The long session ended at 02:05; at 02:10 it is still live.
    let live_opts = DailyOptions {
        now: t("2026-11-16T02:10:00Z"),
        ..opts()
    };
    let r = run(&mut store, &engine, &tz, &live_opts);
    assert_eq!(r.stage1.digests_generated, 1);

    let long_id = store.find_sessions_by_prefix("long").unwrap()[0].id;
    store
        .ingest(
            &AgentId::CLAUDE,
            "long",
            &[turn(
                "u2",
                "2026-11-16T02:12:00Z",
                "also handle the retry case",
            )],
        )
        .unwrap();
    reslice_session(&mut store, long_id, &Config::default(), &tz).unwrap();

    let still_live = DailyOptions {
        now: t("2026-11-16T02:15:00Z"),
        ..opts()
    };
    let r2 = run(&mut store, &engine, &tz, &still_live);
    assert_eq!(
        (r2.stage1.digests_incremental, r2.stage1.digests_generated),
        (1, 0)
    );
    let update_prompt = engine
        .prompts
        .lock()
        .unwrap()
        .iter()
        .find(|p| p.contains("Current digest:"))
        .cloned()
        .unwrap();
    assert!(update_prompt.contains("also handle the retry case"));
    assert!(!update_prompt
        .split("New turns:")
        .nth(1)
        .unwrap()
        .contains("Detailed explanation"));

    // Once the session is quiet, the incrementally updated digest is redone in full.
    let r3 = run(&mut store, &engine, &tz, &opts());
    assert_eq!(
        (r3.stage1.digests_incremental, r3.stage1.digests_generated),
        (0, 1)
    );
    let r4 = run(&mut store, &engine, &tz, &opts());
    assert_eq!(r4.stage1.digests_generated, 0);
}

#[test]
fn cancel_during_the_last_model_call_saves_no_version() {
    let (mut store, tz) = fixture();
    let flag = Arc::new(AtomicBool::new(false));
    let set = flag.clone();
    let engine = MockEngine::new(move |prompt| {
        if prompt.contains("Session record:") {
            return Ok(DIGEST_REPLY.into());
        }
        // The user cancels while stage 2 is in flight.
        set.store(true, Ordering::Relaxed);
        Ok("# r\n\n## app\n- done".into())
    });
    let err = generate_daily(
        &mut store,
        &engine,
        &tz,
        &DailyOptions {
            cancel: Some(flag),
            ..opts()
        },
        &mut |_| {},
    )
    .unwrap_err();
    assert!(err.to_string().contains("cancelled"));
    assert!(store
        .report_versions(KIND_DAILY, "2026-11-16", None)
        .unwrap()
        .is_empty());
    // Digests finished before the cancel stay cached.
    let r = run(&mut store, &mock(), &tz, &opts());
    assert_eq!(r.stage1.digests_generated, 0);
    assert!(r.stage1.digests_cached > 0);
}

#[test]
fn material_names_agents_and_links_slices_of_the_same_task() {
    let tz = FixedOffset::east_opt(8 * 3600).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    prompt_library::sync_builtins(&mut store).unwrap();
    let pid = store.create_project("app", None).unwrap();
    let session = |cwd: &str, id: &str, at: &str, file: &str| {
        vec![
            IrEvent::SessionMeta {
                cwd: Some(cwd.into()),
                git_branch: Some("main".into()),
                agent_version: None,
                parent_session_id: None,
            },
            turn(id, at, "fix the login layout"),
            IrEvent::ToolAction {
                source_event_id: format!("{id}-edit"),
                timestamp: t(at),
                kind: aj_ir::ToolKind::FileEdit,
                tool_name: "edit".into(),
                target: Some(file.into()),
                summary: None,
                diff_stat: None,
            },
        ]
    };
    // Codex reviews from a worktree, Claude Code edits in the main checkout hours later.
    for (agent, sid, events) in [
        (
            AgentId::CODEX,
            "review",
            session(
                "/wt/app",
                "c1",
                "2026-11-16T01:00:00Z",
                "/wt/app/src/login.css",
            ),
        ),
        (
            AgentId::CLAUDE,
            "fix",
            session("/repo", "u1", "2026-11-16T06:00:00Z", "/repo/src/login.css"),
        ),
    ] {
        let id = store.ingest(&agent, sid, &events).unwrap().session_id;
        store.set_session_git(id, Some("/repo"), None).unwrap();
        store.assign_manual(id, Some(pid)).unwrap();
        reslice_session(&mut store, id, &Config::default(), &tz).unwrap();
    }

    let engine = mock();
    run(&mut store, &engine, &tz, &opts());
    let stage2 = engine.prompts.lock().unwrap().last().unwrap().clone();
    assert!(stage2.contains("### Slice S1 (Codex CLI session"));
    assert!(stage2.contains("### Slice S2 (Claude Code session"));
    assert!(stage2.contains(
        "- S1 (Codex CLI) and S2 (Claude Code): same branch main, 1 changed file(s) in common\n"
    ));
}
