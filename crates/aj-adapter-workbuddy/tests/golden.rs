//! Golden test: parses the session sample under fixtures/workbuddy into IR events and compares them verbatim with the expected output.
//!
//! The sample is hand-written synthetic data covering every known record shape. When a WorkBuddy
//! upgrade changes the format, extend the sample first, then fix the adapter. To update the expected output:
//! `UPDATE_GOLDEN=1 cargo test -p aj-adapter-workbuddy --test golden`

use std::path::PathBuf;

use aj_adapter::{ParseOutput, SourceAdapter, SourceFile};
use aj_adapter_workbuddy::WorkBuddyAdapter;
use aj_ir::IrEvent;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/workbuddy")
}

fn sample() -> SourceFile {
    let path = fixtures().join("basic.jsonl");
    let size = std::fs::metadata(&path).unwrap().len();
    SourceFile {
        path,
        source_session_id: "basic".into(),
        size,
        mtime: 0,
        revision: None,
    }
}

fn parse(from: u64) -> ParseOutput {
    WorkBuddyAdapter::new(fixtures())
        .parse(&sample(), from)
        .unwrap()
}

fn events_text(events: &[IrEvent]) -> String {
    events
        .iter()
        .map(|e| serde_json::to_string(e).unwrap() + "\n")
        .collect()
}

#[test]
fn basic_session_matches_golden() {
    let out = parse(0);

    // The line cut off mid-write ends with a newline in the sample, so it counts as one bad line.
    assert_eq!(out.error_count, 1);
    // session-meta x2, snapshot, reasoning x5, fork notice, compaction summary.
    assert_eq!(out.skipped_lines, 10);

    let actual = events_text(&out.events);
    let golden_path = fixtures().join("basic.events.jsonl");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&golden_path, &actual).unwrap();
    }
    let expected = std::fs::read_to_string(&golden_path)
        .expect("missing expected output; generate it with UPDATE_GOLDEN=1");
    assert_eq!(actual, expected);
}

#[test]
fn compaction_summaries_and_abort_notices_start_nothing() {
    let out = parse(0);
    let turns: Vec<&str> = out
        .events
        .iter()
        .filter_map(|e| match e {
            IrEvent::TurnStart {
                source_event_id, ..
            } => Some(source_event_id.as_str()),
            _ => None,
        })
        .collect();
    // Both branches of the regenerated turn are kept; the compaction summary is not a turn.
    assert_eq!(turns, ["u1", "u2", "u3", "u4", "u5"]);
    assert!(!out.events.iter().any(|e| matches!(
        e,
        IrEvent::AgentMessage { source_event_id, .. } if source_event_id == "a4"
    )));
}

#[test]
fn resuming_at_an_offset_continues_where_the_last_parse_stopped() {
    let whole = parse(0);
    let raw = std::fs::read(sample().path).unwrap();
    let mid = raw.len() / 2;
    let split = (mid + raw[mid..].iter().position(|b| *b == b'\n').unwrap() + 1) as u64;

    // The first scan sees the file as it was halfway through being written.
    let dir = std::env::temp_dir().join(format!("aj-wb-resume-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("half.jsonl");
    std::fs::write(&path, &raw[..split as usize]).unwrap();
    let half = SourceFile {
        path,
        source_session_id: "half".into(),
        size: split,
        mtime: 0,
        revision: None,
    };
    let head = WorkBuddyAdapter::new(fixtures()).parse(&half, 0).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(head.new_offset, split);

    let tail = parse(split);
    let not_meta = |e: &&IrEvent| !matches!(e, IrEvent::SessionMeta { .. });
    let resumed: Vec<&IrEvent> = head
        .events
        .iter()
        .chain(tail.events.iter())
        .filter(not_meta)
        .collect();
    let expected: Vec<&IrEvent> = whole.events.iter().filter(not_meta).collect();
    assert_eq!(resumed, expected);
    assert_eq!(whole.new_offset, raw.len() as u64);
}
