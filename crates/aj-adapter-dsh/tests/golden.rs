//! Golden test: parses the session samples under fixtures/dsh into IR events and compares them verbatim with the expected output.
//!
//! The samples are hand-written synthetic data in plain JSONL covering every known record shape; the tests
//! compress them the way DeepSeek Harness stores sessions. When a DeepSeek Harness upgrade changes the format,
//! extend the samples first, then fix the adapter. To update the expected output:
//! `UPDATE_GOLDEN=1 cargo test -p aj-adapter-dsh --test golden`

use std::path::{Path, PathBuf};

use aj_adapter::{ParseOutput, SourceAdapter, SourceFile};
use aj_adapter_dsh::DshAdapter;
use aj_ir::IrEvent;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dsh")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aj-dsh-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Write `frames` as concatenated zstd frames, the way the app appends to a running session.
fn write_session(dir: &Path, frames: &[&[u8]]) -> SourceFile {
    let path = dir.join("session.v4.jsonl.zstd");
    let mut bytes = Vec::new();
    for frame in frames {
        bytes.extend(zstd::encode_all(*frame, 3).unwrap());
    }
    std::fs::write(&path, &bytes).unwrap();
    SourceFile {
        path,
        source_session_id: "session-test".into(),
        size: bytes.len() as u64,
        mtime: 0,
        revision: None,
    }
}

fn parse(file: &SourceFile) -> ParseOutput {
    DshAdapter::new(fixtures()).parse(file, 0).unwrap()
}

fn parse_fixture(name: &str, tmp: &str) -> ParseOutput {
    let dir = temp_dir(tmp);
    let raw = std::fs::read(fixtures().join(name)).unwrap();
    let out = parse(&write_session(&dir, &[&raw]));
    std::fs::remove_dir_all(&dir).ok();
    out
}

fn events_text(out: &ParseOutput) -> String {
    out.events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap() + "\n")
        .collect()
}

#[test]
fn basic_session_matches_golden() {
    let out = parse_fixture("basic.jsonl", "basic");

    // The cut-off tool call line is counted without aborting.
    assert_eq!(out.error_count, 1);
    // Preamble x3, inbox splice, turn/start, step/start, system/message, injected context x4,
    // request x2, title request, job x2, step/end, a lower-priority title, turn/end, end-seed,
    // model/selection.
    assert_eq!(out.skipped_lines, 21);

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
fn running_session_keeps_the_lines_before_a_partial_frame() {
    let dir = temp_dir("partial");
    let raw = std::fs::read(fixtures().join("basic.jsonl")).unwrap();
    let complete = parse_fixture("basic.jsonl", "partial-full");

    // Split into two frames and cut the second one short, as a session still being written.
    let mid = raw.len() / 2;
    let split = mid + raw[mid..].iter().position(|b| *b == b'\n').unwrap() + 1;
    let first = zstd::encode_all(&raw[..split], 3).unwrap();
    let second = zstd::encode_all(&raw[split..], 3).unwrap();
    let path = dir.join("session.v4.jsonl.zstd");
    let mut bytes = first.clone();
    bytes.extend(&second[..second.len() / 2]);
    std::fs::write(&path, &bytes).unwrap();
    let file = SourceFile {
        path,
        source_session_id: "session-test".into(),
        size: bytes.len() as u64,
        mtime: 0,
        revision: None,
    };

    let out = parse(&file);
    assert_eq!(out.new_offset, file.size);
    assert!(!out.events.is_empty());
    assert!(out.events.len() < complete.events.len());
    // Whatever was read is a prefix of the complete session's events.
    assert_eq!(out.events[..], complete.events[..out.events.len()]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn forks_skip_the_history_copied_from_the_parent() {
    let out = parse_fixture("fork.jsonl", "fork");
    assert_eq!(out.error_count, 0);
    let ids: Vec<String> = out
        .events
        .iter()
        .map(|e| match e {
            IrEvent::SessionMeta {
                parent_session_id, ..
            } => format!("meta:{}", parent_session_id.as_deref().unwrap_or("-")),
            IrEvent::TurnStart {
                source_event_id, ..
            }
            | IrEvent::AgentMessage {
                source_event_id, ..
            }
            | IrEvent::TokenUsage {
                source_event_id, ..
            } => source_event_id.clone(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(ids, ["meta:session-basic", "fu1", "fa1", "usage:6"]);
}

#[test]
fn subagent_sessions_emit_nothing() {
    let out = parse_fixture("subagent.jsonl", "subagent");
    assert!(out.events.is_empty());
    assert_eq!(out.error_count, 0);
    assert_eq!(out.skipped_lines, 3);
}

#[test]
fn unknown_format_versions_are_an_error_not_a_guess() {
    let dir = temp_dir("version");
    let file = write_session(
        &dir,
        &[br#"{"type":"session","version":5,"id":"s","createdAt":0,"cwd":"/w"}
{"type":"user/message","seq":0,"time":1,"data":{"id":"u","source":{"kind":"user"},"content":[{"type":"text","text":"hi"}]}}
"#],
    );
    let out = parse(&file);
    assert!(out.events.is_empty());
    assert_eq!(out.error_count, 1);
    assert!(out.last_error.unwrap().contains("version"));
    std::fs::remove_dir_all(&dir).ok();
}
