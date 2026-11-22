//! Golden test: parses the session samples under fixtures/ into IR events and compares them verbatim with the expected output.
//!
//! The samples are hand-written synthetic data covering every known record shape. When a Claude Code
//! upgrade changes the format, extend the samples first, then fix the adapter. To update the expected output:
//! `UPDATE_GOLDEN=1 cargo test -p aj-adapter-claude --test golden`

use std::path::PathBuf;

use aj_adapter::{SourceAdapter, SourceFile};
use aj_adapter_claude::ClaudeAdapter;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/claude")
}

#[test]
fn basic_session_matches_golden() {
    let path = fixtures().join("basic.jsonl");
    let size = std::fs::metadata(&path).unwrap().len();
    let file = SourceFile {
        path,
        source_session_id: "basic".into(),
        size,
        mtime: 0,
        revision: None,
    };
    let out = ClaudeAdapter::new(fixtures()).parse(&file, 0).unwrap();

    // A line of bad JSON is counted without aborting; the trailing partial line is left for next time.
    assert_eq!(out.error_count, 1);
    assert!(out.new_offset < size);
    // api-request is skipped without deserializing; attachment and system are known useless types.
    // Two records inherited from the parent session are skipped as well.
    assert_eq!(out.skipped_lines, 5);

    let actual: String = out
        .events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap() + "\n")
        .collect();
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
    let path = fixtures().join("basic.jsonl");
    let size = std::fs::metadata(&path).unwrap().len();
    let file = SourceFile {
        path,
        source_session_id: "basic".into(),
        size,
        mtime: 0,
        revision: None,
    };
    let adapter = ClaudeAdapter::new(fixtures());
    let full = adapter.parse(&file, 0).unwrap();
    let again = adapter.parse(&file, full.new_offset).unwrap();
    assert!(again.events.is_empty());
    assert_eq!(again.new_offset, full.new_offset);
}
