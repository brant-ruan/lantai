//! Golden test: parses Antigravity transcript samples under fixtures/antigravity into IR events
//! and compares them verbatim with the expected output.
//!
//! To update the expected output: `UPDATE_GOLDEN=1 cargo test -p aj-adapter-antigravity --test golden`

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use aj_adapter::{ParseOutput, SourceAdapter, SourceFile};
use aj_adapter_antigravity::AntigravityAdapter;
use aj_ir::{DiffStat, IrEvent, ToolKind};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/antigravity")
}

fn source(path: PathBuf, id: &str) -> SourceFile {
    let size = std::fs::metadata(&path).unwrap().len();
    SourceFile {
        path,
        source_session_id: id.into(),
        size,
        mtime: 0,
        revision: None,
    }
}

fn parse(file: &SourceFile, from: u64) -> ParseOutput {
    AntigravityAdapter::new(vec![fixtures()])
        .parse(file, from)
        .unwrap()
}

fn events_text(out: &ParseOutput) -> String {
    out.events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap() + "\n")
        .collect()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aj-antigravity-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn basic_session_matches_golden() {
    let file = source(fixtures().join("basic.jsonl"), "conv-1");
    let out = parse(&file, 0);

    // One bad JSON line is counted; the trailing partial line is left for the next scan.
    assert_eq!(out.error_count, 1);
    assert!(out.new_offset < file.size);
    // Skipped: manage_task GENERIC, RUNNING run_command GENERIC, SYSTEM_MESSAGE, CHECKPOINT, SYSTEM USER_INPUT.
    assert_eq!(out.skipped_lines, 5);

    let actual = events_text(&out);
    let golden_path = fixtures().join("basic.events.jsonl");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&golden_path, &actual).unwrap();
    }
    let expected = std::fs::read_to_string(&golden_path)
        .expect("missing expected output; generate it with UPDATE_GOLDEN=1");
    assert_eq!(actual, expected);
}

#[test]
fn ide_session_matches_golden() {
    let file = source(fixtures().join("ide.jsonl"), "conv-2");
    let out = parse(&file, 0);

    assert_eq!(out.error_count, 0);
    assert_eq!(out.new_offset, file.size);
    // Skipped: CONVERSATION_HISTORY, KNOWLEDGE_ARTIFACTS.
    assert_eq!(out.skipped_lines, 2);

    let actual = events_text(&out);
    let golden_path = fixtures().join("ide.events.jsonl");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&golden_path, &actual).unwrap();
    }
    let expected = std::fs::read_to_string(&golden_path)
        .expect("missing expected output; generate it with UPDATE_GOLDEN=1");
    assert_eq!(actual, expected);
}

#[test]
fn parsing_from_offset_preserves_pending_tool_pairing() {
    let dir = temp_dir("resume");
    let logs = dir.join("conv-resume/.system_generated/logs");
    std::fs::create_dir_all(&logs).unwrap();
    let path = logs.join("transcript_full.jsonl");

    let initial = concat!(
        r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-11-18T03:00:00Z","content":"<USER_REQUEST>\ncheck build\n</USER_REQUEST>"}"#,
        "\n",
        r#"{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-11-18T03:00:01Z","tool_calls":[{"name":"read_file","args":{"Path":"/work/app/lib.rs"}},{"name":"run_command","args":{"CommandLine":"cargo check"}}]}"#,
        "\n",
        r#"{"step_index":2,"source":"MODEL","type":"GENERIC","status":"DONE","created_at":"2026-11-18T03:00:02Z","content":"ok"}"#,
        "\n"
    );
    std::fs::write(&path, initial).unwrap();

    let first = parse(&source(path.clone(), "conv-resume"), 0);
    assert_eq!(first.new_offset, initial.len() as u64);

    // Append the second tool result later: resuming must skip result #0 and pair with `step:1:tool:1`.
    let appended = concat!(
        r#"{"step_index":3,"source":"MODEL","type":"GENERIC","status":"DONE","created_at":"2026-11-18T03:00:03Z","content":"Command exited with code 1.\n\nCommand output:\ncompile failed\n"}"#,
        "\n"
    );
    std::fs::write(&path, format!("{initial}{appended}")).unwrap();

    let second = parse(&source(path, "conv-resume"), first.new_offset);
    let outcomes: Vec<&IrEvent> = second
        .events
        .iter()
        .filter(|e| matches!(e, IrEvent::ToolOutcome { .. }))
        .collect();
    assert_eq!(
        outcomes,
        vec![&IrEvent::ToolOutcome {
            tool_source_id: "step:1:tool:1".into(),
            is_error: true,
            diff_stat: None,
            output_summary: Some("compile failed".into()),
        }]
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn compact_transcript_decodes_json_encoded_args() {
    let dir = temp_dir("compact");
    let logs = dir.join("conv-compact/.system_generated/logs");
    std::fs::create_dir_all(&logs).unwrap();
    let path = logs.join("transcript.jsonl");

    let content = concat!(
        r#"{"step_index":0,"source":"USER_EXPLICIT","type":"USER_INPUT","status":"DONE","created_at":"2026-11-18T04:00:00Z","content":"<USER_REQUEST>\nedit main\n</USER_REQUEST>"}"#,
        "\n",
        r#"{"step_index":1,"source":"MODEL","type":"PLANNER_RESPONSE","status":"DONE","created_at":"2026-11-18T04:00:01Z","tool_calls":[{"name":"edit_file","args":{"Path":"\"/Users/alice/pjts/capp/src/main.rs\"","Target":"\"old_1\\nold_2\"","Replacement":"\"new_1\\nnew_2\\nnew_3\n<truncated 120 bytes>"}},{"name":"run_command","args":{"CommandLine":"\"cargo test\"","Cwd":"\"/Users/alice/pjts/capp\""}}],"truncated_fields":["tool_calls"]}"#,
        "\n"
    );
    std::fs::write(&path, content).unwrap();

    let out = parse(&source(path, "conv-compact"), 0);
    assert!(out.events.contains(&IrEvent::SessionMeta {
        cwd: Some("/Users/alice/pjts/capp".into()),
        git_branch: None,
        agent_version: None,
        parent_session_id: None,
    }));
    assert!(out.events.contains(&IrEvent::ToolAction {
        source_event_id: "step:1:tool:0".into(),
        timestamp: "2026-11-18T04:00:01Z".parse().unwrap(),
        kind: ToolKind::FileEdit,
        tool_name: "edit_file".into(),
        target: Some("/Users/alice/pjts/capp/src/main.rs".into()),
        summary: None,
        diff_stat: Some(DiffStat {
            added: 3,
            removed: 2
        }),
    }));
    assert!(out.events.contains(&IrEvent::ToolAction {
        source_event_id: "step:1:tool:1".into(),
        timestamp: "2026-11-18T04:00:01Z".parse().unwrap(),
        kind: ToolKind::BashCmd,
        tool_name: "run_command".into(),
        target: Some("cargo test".into()),
        summary: None,
        diff_stat: None,
    }));
    std::fs::remove_dir_all(&dir).ok();
}

fn touch(path: &Path, secs_ago: u64) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, b"").unwrap();
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(secs_ago))
        .unwrap();
}

#[test]
fn discovery_prefers_full_transcript_and_deduplicates_across_roots() {
    let dir = temp_dir("discover");
    let root_a = dir.join("root_a");
    let root_b = dir.join("root_b");

    // Overlapping conversation: root_a has older file, root_b has newer file (prefer newer mtime).
    touch(
        &root_a.join("conv-overlap/.system_generated/logs/transcript_full.jsonl"),
        100,
    );
    touch(
        &root_b.join("conv-overlap/.system_generated/logs/transcript.jsonl"),
        10,
    );
    // Same directory holds both full and compact: prefer transcript_full.jsonl.
    touch(
        &root_a.join("conv-both/.system_generated/logs/transcript.jsonl"),
        10,
    );
    touch(
        &root_a.join("conv-both/.system_generated/logs/transcript_full.jsonl"),
        10,
    );
    // Compact-only conversation.
    touch(
        &root_b.join("conv-compact/.system_generated/logs/transcript.jsonl"),
        10,
    );
    // Conversation directory without logs is skipped; hidden dirs are skipped.
    std::fs::create_dir_all(root_a.join("conv-empty")).unwrap();
    touch(
        &root_a.join(".hidden/.system_generated/logs/transcript_full.jsonl"),
        10,
    );

    let adapter = AntigravityAdapter::new(vec![root_a.clone(), root_b.clone()]);
    let found = adapter.discover().unwrap();
    let summary: Vec<(&str, &str, bool)> = found
        .iter()
        .map(|f| {
            (
                f.source_session_id.as_str(),
                f.path.file_name().unwrap().to_str().unwrap(),
                f.path.starts_with(&root_b),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            ("conv-both", "transcript_full.jsonl", false),
            ("conv-compact", "transcript.jsonl", true),
            ("conv-overlap", "transcript.jsonl", true),
        ]
    );
    std::fs::remove_dir_all(&dir).ok();
}
