//! Golden test: parses the rollout samples under fixtures/codex into IR events and compares them verbatim with the expected output.
//!
//! The samples are hand-written synthetic data covering every known record and item shape. When a Codex
//! upgrade changes the format, extend the samples first, then fix the adapter. To update the expected output:
//! `UPDATE_GOLDEN=1 cargo test -p aj-adapter-codex --test golden`

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use aj_adapter::{ParseOutput, SourceAdapter, SourceFile};
use aj_adapter_codex::CodexAdapter;

/// Session ID of `basic.jsonl` (its file name would end in this ID).
const BASIC_ID: &str = "019a0000-0000-7000-8000-000000000001";

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/codex")
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
    CodexAdapter::new(vec![fixtures()])
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
    let dir = std::env::temp_dir().join(format!("aj-codex-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn basic_session_matches_golden() {
    let file = source(fixtures().join("basic.jsonl"), BASIC_ID);
    let out = parse(&file, 0);

    // A line of bad JSON is counted without aborting; the trailing partial line is left for next time.
    assert_eq!(out.error_count, 1);
    assert!(out.new_offset < file.size);
    // response_item x2, turn_context, task_started, task_complete, token_usage_record,
    // Reasoning, clock.sleep, SubAgentActivity(completed), CollabAgentToolCall, heartbeat,
    // token_count without info.
    assert_eq!(out.skipped_lines, 12);

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
fn parsing_from_offset_yields_only_new_events() {
    let file = source(fixtures().join("basic.jsonl"), BASIC_ID);
    let full = parse(&file, 0);
    let again = parse(&file, full.new_offset);
    assert!(again.events.is_empty());
    assert_eq!(again.new_offset, full.new_offset);
}

#[test]
fn subagent_sessions_are_excluded_even_when_resuming() {
    let path = fixtures().join("subagent.jsonl");
    let file = source(path.clone(), "019a-child");
    let out = parse(&file, 0);
    assert!(out.events.is_empty());
    assert_eq!(out.skipped_lines, 2);

    let first_line_len = std::fs::read(&path)
        .unwrap()
        .iter()
        .position(|b| *b == b'\n')
        .unwrap() as u64
        + 1;
    let resumed = parse(&file, first_line_len);
    assert!(resumed.events.is_empty());
}

#[test]
fn compressed_rollouts_parse_like_plain_ones() {
    let dir = temp_dir("zst");
    let plain = std::fs::read(fixtures().join("basic.jsonl")).unwrap();
    // Only the complete lines: an archive never ends with a partial line.
    let complete = &plain[..=plain.iter().rposition(|b| *b == b'\n').unwrap()];
    let zst = dir.join("rollout-2026-11-16T01-00-00-basic.jsonl.zst");
    std::fs::write(&zst, zstd::encode_all(complete, 3).unwrap()).unwrap();

    let expected = parse(&source(fixtures().join("basic.jsonl"), BASIC_ID), 0);
    let file = source(zst, BASIC_ID);
    let out = parse(&file, 0);
    assert_eq!(events_text(&out), events_text(&expected));
    assert_eq!(out.new_offset, file.size);
    // A compressed file is always parsed whole, whatever the cursor says.
    assert_eq!(events_text(&parse(&file, 10)), events_text(&expected));
    std::fs::remove_dir_all(&dir).ok();
}

fn touch(path: &Path, secs_ago: u64) {
    std::fs::write(path, b"").unwrap();
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(secs_ago))
        .unwrap();
}

#[test]
fn discovery_walks_both_roots_and_keeps_the_newest_copy() {
    let dir = temp_dir("discover");
    let sessions = dir.join("sessions/2026/11/16");
    let archived = dir.join("archived_sessions");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::create_dir_all(&archived).unwrap();
    touch(&sessions.join("rollout-2026-11-16T01-00-00-aaa.jsonl"), 100);
    touch(
        &archived.join("rollout-2026-11-16T01-00-00-aaa.jsonl.zst"),
        10,
    );
    touch(
        &sessions.join("rollout-2026-11-16T02-00-00-aaa_bbb.jsonl"),
        10,
    );
    touch(&sessions.join("notes.txt"), 10);

    let adapter = CodexAdapter::new(vec![dir.join("sessions"), archived.clone()]);
    let found = adapter.discover().unwrap();
    let ids: Vec<(&str, bool)> = found
        .iter()
        .map(|f| (f.source_session_id.as_str(), f.path.starts_with(&archived)))
        .collect();
    assert_eq!(ids, vec![("aaa", true), ("aaa_bbb", false)]);
    std::fs::remove_dir_all(&dir).ok();
}
