use super::*;
use crate::engine::testing::MockEngine;
use crate::summarize::Budgets;
use aj_core::{slice::reslice_session, Config};
use aj_ir::{AgentId, IrEvent};
use chrono::{DateTime, FixedOffset, Utc};

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

/// Short sessions: their digests pass through without a model call.
fn add_session(store: &mut Store, tz: &FixedOffset, sid: &str, project: i64, events: &[IrEvent]) {
    let id = store
        .ingest(&AgentId::CLAUDE, sid, events)
        .unwrap()
        .session_id;
    store.assign_manual(id, Some(project)).unwrap();
    reslice_session(store, id, &Config::default(), tz).unwrap();
}

/// `app` has a session over 11-16 and 11-17 and another on 11-18; `lib` one on 11-17.
fn fixture() -> (Store, FixedOffset) {
    let tz = FixedOffset::east_opt(8 * 3600).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    prompt_library::sync_builtins(&mut store).unwrap();
    let app = store.create_project("app", None).unwrap();
    let lib = store.create_project("lib", None).unwrap();
    add_session(
        &mut store,
        &tz,
        "a",
        app,
        &[
            turn("u1", "2026-11-16T02:00:00Z", "refactor the OAuth refresh"),
            turn("u2", "2026-11-17T02:00:00Z", "continue the OAuth refresh"),
        ],
    );
    add_session(
        &mut store,
        &tz,
        "b",
        app,
        &[turn("u1", "2026-11-18T02:00:00Z", "test the refresh")],
    );
    add_session(
        &mut store,
        &tz,
        "c",
        lib,
        &[turn("u1", "2026-11-17T03:00:00Z", "bump deps")],
    );
    (store, tz)
}

fn opts() -> WeeklyOptions {
    WeeklyOptions {
        period: Week::parse("2026-W47").unwrap(),
        project: None,
        prompt: None,
        concurrency: 2,
        dry_run: false,
        budgets: Budgets::default(),
        cancel: None,
        mode: ReportMode::Regenerate,
        now: t("2026-11-25T00:00:00Z"),
        language: &crate::lang::ZH_HANS,
    }
}

fn mock() -> MockEngine {
    MockEngine::new(|prompt| {
        Ok(if prompt.contains("## Project: app") {
            "# whatever\n\n## app\n> made-up stats\n- OAuth refresh rework [#S1]\n  - 11-16: started [#S1.T0]\n- invented [#S99]".into()
        } else {
            "## lib\n- dependency bump [#S3]".into()
        })
    })
}

#[test]
fn writes_one_section_per_project_with_program_statistics() {
    let (mut store, tz) = fixture();
    let engine = mock();
    let r = generate_weekly(&mut store, &engine, &tz, &opts(), &mut |_| {}).unwrap();

    assert_eq!(r.slices_used, 4);
    assert_eq!(engine.call_count(), 2, "one request per project");
    assert_eq!(
        r.anchors,
        AnchorCheck {
            total: 4,
            invalid: 1
        }
    );
    assert!(
        r.markdown.starts_with(
            "# 2026-W47 \u{00b7} 11-16 ~ 11-22\n\n## app\n> \u{6d3b}\u{8dc3} 3 \u{5929}"
        ),
        "{}",
        r.markdown
    );
    assert!(r
        .markdown
        .contains("\u{00b7} 2 \u{4e2a}\u{4f1a}\u{8bdd}\n- OAuth refresh rework [#S1]"));
    assert!(!r.markdown.contains("made-up stats"));
    assert!(r.markdown.contains("## lib\n> \u{6d3b}\u{8dc3} 1 \u{5929}"));

    let prompts = engine.prompts.lock().unwrap().clone();
    let app_prompt = prompts
        .iter()
        .find(|p| p.contains("## Project: app"))
        .unwrap();
    assert!(app_prompt.contains("=== 2026-11-16 (Mon) ==="));
    assert!(app_prompt.contains("=== 2026-11-18 (Wed) ==="));
    assert!(app_prompt.contains("Same-session hints"));
    assert!(app_prompt.contains("S1 (11-16), S2 (11-17): one session continued across days"));
    assert!(!app_prompt.contains("## Project: lib"));

    let saved = store
        .current_report_version(KIND_WEEKLY, "2026-W47", None)
        .unwrap()
        .unwrap();
    assert_eq!(saved.markdown, r.markdown);
}

#[test]
fn update_rewrites_only_projects_with_new_material() {
    let (mut store, tz) = fixture();
    let engine = mock();
    generate_weekly(&mut store, &engine, &tz, &opts(), &mut |_| {}).unwrap();

    let lib = store.find_project("lib").unwrap().unwrap().id;
    add_session(
        &mut store,
        &tz,
        "d",
        lib,
        &[turn("u1", "2026-11-19T03:00:00Z", "release")],
    );
    let est = estimate_weekly(
        &store,
        &tz,
        opts().period,
        None,
        &MaterialContext {
            engine_name: "mock",
            budgets: &Budgets::default(),
            now: t("2026-11-25T00:00:00Z"),
            language: &crate::lang::ZH_HANS,
        },
    )
    .unwrap();
    assert_eq!(est.staleness.unwrap().projects, vec!["lib".to_string()]);

    let mut o = opts();
    o.mode = ReportMode::Update;
    let r = generate_weekly(&mut store, &engine, &tz, &o, &mut |_| {}).unwrap();
    assert!(r.updated);
    assert_eq!(r.projects_written, vec!["lib".to_string()]);
    assert_eq!(r.sections_kept, 1);
    assert_eq!(engine.call_count(), 3);
    assert!(engine.prompts.lock().unwrap()[2].contains("## Project: lib"));
    assert!(r.markdown.contains("## lib\n> \u{6d3b}\u{8dc3} 2 \u{5929}"));
    assert!(r.markdown.contains("- OAuth refresh rework [#S1]"));

    // Nothing changed since: an update has nothing to do.
    assert!(generate_weekly(&mut store, &engine, &tz, &o, &mut |_| {}).is_err());
}

#[test]
fn dry_run_shows_each_project_request_and_saves_nothing() {
    let (mut store, tz) = fixture();
    let engine = mock();
    let mut o = opts();
    o.dry_run = true;
    let r = generate_weekly(&mut store, &engine, &tz, &o, &mut |_| {}).unwrap();
    assert_eq!(engine.call_count(), 0);
    assert_eq!(r.markdown.matches("Material:").count(), 2);
    assert!(r
        .markdown
        .contains("[statistics written by the program] > "));
    assert!(store
        .current_report_version(KIND_WEEKLY, "2026-W47", None)
        .unwrap()
        .is_none());
}
