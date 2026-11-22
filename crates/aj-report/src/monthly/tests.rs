use super::*;
use crate::engine::testing::MockEngine;
use aj_core::{slice::reslice_session, Config};
use aj_ir::{AgentId, IrEvent};
use chrono::FixedOffset;

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

fn add_session(store: &mut Store, tz: &FixedOffset, sid: &str, project: i64, events: &[IrEvent]) {
    let id = store
        .ingest(&AgentId::CLAUDE, sid, events)
        .unwrap()
        .session_id;
    store.assign_manual(id, Some(project)).unwrap();
    reslice_session(store, id, &Config::default(), tz).unwrap();
}

fn save_weekly(store: &mut Store, week: &str, markdown: &str) {
    store
        .save_report_version(&NewReportVersion {
            report_type: KIND_WEEKLY,
            period_key: week,
            project_id: None,
            origin: "generated",
            markdown,
            engine: Some("mock"),
            input_hash: None,
            prompt_id: None,
            prompt_name: None,
            prompt_snapshot: None,
            input_manifest: None,
            language: "zh-Hans",
        })
        .unwrap();
}

/// November 2026 owns W45, W46, W47, W48.
/// `app` has slices in W45, W46, W47; `lib` in W46 and W47.
/// Weekly reports exist for W45 and W46; W47 is left unreported.
fn fixture() -> (Store, FixedOffset) {
    let tz = FixedOffset::east_opt(8 * 3600).unwrap();
    let mut store = Store::open_in_memory().unwrap();
    prompt_library::sync_builtins(&mut store).unwrap();
    let app = store.create_project("app", None).unwrap();
    let lib = store.create_project("lib", None).unwrap();

    // W45 (2026-11-02 .. 2026-11-08): slice S1
    add_session(
        &mut store,
        &tz,
        "a",
        app,
        &[turn("u1", "2026-11-03T02:00:00Z", "start OAuth refactor")],
    );
    // W46 (2026-11-09 .. 2026-11-15): slice S2 (app), S3 (lib)
    add_session(
        &mut store,
        &tz,
        "b",
        app,
        &[turn("u1", "2026-11-10T02:00:00Z", "finish OAuth lock")],
    );
    add_session(
        &mut store,
        &tz,
        "c",
        lib,
        &[turn("u1", "2026-11-11T03:00:00Z", "bump crates")],
    );
    // W47 (2026-11-16 .. 2026-11-22): slice S4 (app), no weekly report yet
    add_session(
        &mut store,
        &tz,
        "d",
        app,
        &[turn("u1", "2026-11-17T02:00:00Z", "add integration test")],
    );

    save_weekly(
        &mut store,
        "2026-W45",
        "# 2026-W45 \u{00b7} 11-02 ~ 11-08\n\n## app\n> \u{6d3b}\u{8dc3} 1 \u{5929} \u{00b7} \u{7ea6} 0.1 \u{5c0f}\u{65f6} \u{00b7} 1 \u{4e2a}\u{4f1a}\u{8bdd}\n- Started OAuth refactor [#S1.T0]",
    );
    save_weekly(
        &mut store,
        "2026-W46",
        "# 2026-W46 \u{00b7} 11-09 ~ 11-15\n\n## app\n> \u{6d3b}\u{8dc3} 1 \u{5929} \u{00b7} \u{7ea6} 0.1 \u{5c0f}\u{65f6} \u{00b7} 1 \u{4e2a}\u{4f1a}\u{8bdd}\n- Added shared refresh lock [#S2.T0]\n\n## lib\n> \u{6d3b}\u{8dc3} 1 \u{5929} \u{00b7} \u{7ea6} 0.1 \u{5c0f}\u{65f6} \u{00b7} 1 \u{4e2a}\u{4f1a}\u{8bdd}\n- Updated dependencies [#S3.T0]",
    );
    (store, tz)
}

fn opts() -> MonthlyOptions {
    MonthlyOptions {
        month: Month::parse("2026-11").unwrap(),
        project: None,
        prompt: None,
        concurrency: 2,
        dry_run: false,
        budgets: Budgets::default(),
        cancel: None,
        mode: ReportMode::Regenerate,
        now: t("2026-11-30T00:00:00Z"),
        language: &crate::lang::ZH_HANS,
    }
}

fn mock() -> MockEngine {
    MockEngine::new(|prompt| {
        Ok(if prompt.contains("## Project: app") {
            "# whatever\n\n## app\n> invented stats\n- OAuth refresh rework [#S1]\n  - W45: started [#S1.T0]\n  - W46: added lock [#S2.T0]\n- invented [#S99]".into()
        } else {
            "## lib\n- dependency maintenance [#S3.T0]".into()
        })
    })
}

#[test]
fn consumes_weekly_reports_computes_stats_and_warns_unreported_weeks() {
    let (mut store, tz) = fixture();
    let engine = mock();
    let r = generate_monthly(&mut store, &engine, &tz, &opts(), &mut |_| {}).unwrap();

    assert_eq!(engine.call_count(), 2, "one request per project");
    assert_eq!(r.weeks_used, 2);
    assert_eq!(r.unreported_weeks, vec!["2026-W47".to_string()]);
    assert!(r.warnings.iter().any(|w| w.contains("2026-W47")));
    assert_eq!(
        r.anchors,
        AnchorCheck {
            total: 5,
            invalid: 1
        }
    );
    assert!(
        r.markdown
            .starts_with("# 2026-11 \u{00b7} W45 ~ W48\n\n## app\n> \u{6d3b}\u{8dc3} 3 \u{5929}"),
        "{}",
        r.markdown
    );
    assert!(r
        .markdown
        .contains("\u{00b7} 3 \u{4e2a}\u{4f1a}\u{8bdd}\n- OAuth refresh rework [#S1]"));
    assert!(!r.markdown.contains("invented stats"));
    assert!(r.markdown.contains("## lib\n> \u{6d3b}\u{8dc3} 1 \u{5929}"));

    let prompts = engine.prompts.lock().unwrap().clone();
    let app_prompt = prompts
        .iter()
        .find(|p| p.contains("## Project: app"))
        .unwrap();
    assert!(app_prompt.contains("=== 2026-W45 (11-02 ~ 11-08) ==="));
    assert!(app_prompt.contains("=== 2026-W46 (11-09 ~ 11-15) ==="));
    assert!(!app_prompt.contains("\u{6d3b}\u{8dc3} 1 \u{5929}"));
    assert!(!app_prompt.contains("## Project: lib"));

    let saved = store
        .current_report_version(KIND_MONTHLY, "2026-11", None)
        .unwrap()
        .unwrap();
    assert_eq!(saved.markdown, r.markdown);
}

#[test]
fn update_rewrites_only_projects_whose_weekly_section_changed() {
    let (mut store, tz) = fixture();
    let engine = mock();
    generate_monthly(&mut store, &engine, &tz, &opts(), &mut |_| {}).unwrap();

    // Update W46 for `lib` only; `app` stays identical.
    save_weekly(
        &mut store,
        "2026-W46",
        "# 2026-W46 \u{00b7} 11-09 ~ 11-15\n\n## app\n> \u{6d3b}\u{8dc3} 1 \u{5929} \u{00b7} \u{7ea6} 0.1 \u{5c0f}\u{65f6} \u{00b7} 1 \u{4e2a}\u{4f1a}\u{8bdd}\n- Added shared refresh lock [#S2.T0]\n\n## lib\n> \u{6d3b}\u{8dc3} 1 \u{5929} \u{00b7} \u{7ea6} 0.1 \u{5c0f}\u{65f6} \u{00b7} 1 \u{4e2a}\u{4f1a}\u{8bdd}\n- Updated dependencies and cut release [#S3.T0]",
    );

    let est = estimate_monthly(
        &store,
        &tz,
        opts().month,
        None,
        &MaterialContext {
            engine_name: "mock",
            budgets: &Budgets::default(),
            now: t("2026-11-30T00:00:00Z"),
            language: &crate::lang::ZH_HANS,
        },
    )
    .unwrap();
    assert_eq!(est.weeks_reported, 2);
    assert_eq!(est.unreported_weeks, vec!["2026-W47".to_string()]);
    assert_eq!(est.staleness.unwrap().projects, vec!["lib".to_string()]);

    let mut o = opts();
    o.mode = ReportMode::Update;
    let r = generate_monthly(&mut store, &engine, &tz, &o, &mut |_| {}).unwrap();
    assert!(r.updated);
    assert_eq!(r.projects_written, vec!["lib".to_string()]);
    assert_eq!(r.sections_kept, 1);
    assert_eq!(engine.call_count(), 3);
    assert!(engine.prompts.lock().unwrap()[2].contains("## Project: lib"));
    assert!(r.markdown.contains("- OAuth refresh rework [#S1]"));

    assert!(generate_monthly(&mut store, &engine, &tz, &o, &mut |_| {}).is_err());
}

#[test]
fn dry_run_shows_each_project_request_and_saves_nothing() {
    let (mut store, tz) = fixture();
    let engine = mock();
    let mut o = opts();
    o.dry_run = true;
    let r = generate_monthly(&mut store, &engine, &tz, &o, &mut |_| {}).unwrap();
    assert_eq!(engine.call_count(), 0);
    assert_eq!(r.markdown.matches("Material:").count(), 2);
    assert!(r
        .markdown
        .contains("[statistics written by the program] > "));
    assert!(store
        .current_report_version(KIND_MONTHLY, "2026-11", None)
        .unwrap()
        .is_none());
}
