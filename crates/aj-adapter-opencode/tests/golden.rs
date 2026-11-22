//! Golden test: builds a database from fixtures/opencode/basic.sql, scans it like a real data
//! directory and compares every session's IR events verbatim with the expected output.
//!
//! The sample is hand-written synthetic data in the shape of OpenCode 1.18.35. When an OpenCode
//! upgrade changes the format, extend the sample first, then fix the adapter. To update the expected output:
//! `UPDATE_GOLDEN=1 cargo test -p aj-adapter-opencode --test golden`

use std::path::{Path, PathBuf};

use aj_adapter::{ParseOutput, SourceAdapter};
use aj_adapter_opencode::OpenCodeAdapter;
use aj_ir::IrEvent;
use rusqlite::Connection;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/opencode")
}

/// A fresh data directory holding `opencode.db` built from the SQL sample, in WAL mode like OpenCode's.
fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aj-oc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let conn = Connection::open(dir.join("src/opencode.db")).unwrap();
    conn.pragma_update(None, "journal_mode", "wal").unwrap();
    let sql = std::fs::read_to_string(fixtures().join("basic.sql")).unwrap();
    conn.execute_batch(&sql).unwrap();
    drop(conn);
    dir
}

fn adapter(dir: &Path) -> OpenCodeAdapter {
    OpenCodeAdapter::new(dir.join("src"), dir.join("cache"))
}

fn parse_all(dir: &Path) -> Vec<(String, ParseOutput)> {
    let a = adapter(dir);
    a.discover()
        .unwrap()
        .into_iter()
        .map(|f| (f.source_session_id.clone(), a.parse(&f, 0).unwrap()))
        .collect()
}

#[test]
fn basic_database_matches_golden() {
    let dir = data_dir("golden");
    let sessions = parse_all(&dir);
    let ids: Vec<&str> = sessions.iter().map(|(id, _)| id.as_str()).collect();
    assert_eq!(ids, ["ses_0001", "ses_0002", "ses_0003"]);

    let (_, main) = &sessions[0];
    // The broken part row.
    assert_eq!(main.error_count, 1);
    // Synthetic text, step boundaries, reasoning, the streaming reply and the compaction records.
    assert!(main.skipped_lines > 0);

    let mut actual = String::new();
    for (id, out) in &sessions {
        actual.push_str(&format!("# {id}\n"));
        for e in &out.events {
            actual.push_str(&serde_json::to_string(e).unwrap());
            actual.push('\n');
        }
    }
    let golden_path = fixtures().join("basic.events.jsonl");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&golden_path, &actual).unwrap();
    }
    let expected = std::fs::read_to_string(&golden_path)
        .expect("missing expected output; generate it with UPDATE_GOLDEN=1");
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(actual, expected);
}

#[test]
fn streaming_and_compaction_records_are_not_ingested() {
    let dir = data_dir("partial");
    let sessions = parse_all(&dir);
    std::fs::remove_dir_all(&dir).ok();
    let events = &sessions[0].1.events;
    let turns: Vec<&str> = events
        .iter()
        .filter_map(|e| match e {
            IrEvent::TurnStart {
                source_event_id, ..
            } => Some(source_event_id.as_str()),
            _ => None,
        })
        .collect();
    // The compaction request is not a turn.
    assert_eq!(turns, ["msg_0001", "msg_0003"]);
    let has = |id: &str| {
        events.iter().any(|e| match e {
            IrEvent::AgentMessage {
                source_event_id, ..
            } => source_event_id == id,
            IrEvent::ToolAction {
                source_event_id, ..
            } => source_event_id == id,
            _ => false,
        })
    };
    // The aborted reply's partial text is kept; the streaming reply and the compaction summary are not.
    assert!(has("prt_0040"));
    assert!(!has("prt_0050"));
    assert!(!has("call_running"));
    assert!(!has("prt_0070"));
    // The subagent session produces nothing.
    assert!(sessions[2].1.events.is_empty());
}

#[test]
fn revision_changes_when_a_part_is_updated_in_place() {
    let dir = data_dir("revision");
    let a = adapter(&dir);
    let before = a.discover().unwrap();
    let conn = Connection::open(dir.join("src/opencode.db")).unwrap();
    // A streamed text part grows without adding rows.
    conn.execute(
        "UPDATE part SET data = json_set(data, '$.text', 'Half a sentence'), time_updated = 1795000700000 WHERE id = 'prt_0050'",
        [],
    )
    .unwrap();
    drop(conn);
    let after = a.discover().unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert_ne!(before[0].revision, after[0].revision);
    assert_eq!(before[1].revision, after[1].revision);
}

#[test]
fn probe_reports_versions_and_never_claims_a_2x_store() {
    use aj_adapter::{CompatReason, Compatibility};

    let dir = data_dir("probe");
    let p = adapter(&dir).probe().unwrap();
    assert_eq!(p.compatibility, Compatibility::Supported);
    assert_eq!(p.versions, ["1.18.33", "1.18.35"]);

    // After an upgrade to 2.x the store also holds `session_v2`, with sessions the 1.x tables lack.
    let conn = Connection::open(dir.join("src/opencode.db")).unwrap();
    conn.execute_batch(
        "CREATE TABLE session_v2 (id text PRIMARY KEY, version text);
         INSERT INTO session_v2 VALUES ('ses_0001', '2.0.12'), ('ses_9001', '2.0.12');",
    )
    .unwrap();
    drop(conn);
    let p = adapter(&dir).probe().unwrap();
    assert_eq!(p.compatibility, Compatibility::Partial);
    assert_eq!(p.reason, Some(CompatReason::SessionsInNewerFormat(1)));

    // A store created by 2.x has no 1.x tables at all: nothing is listed, and it says so.
    let conn = Connection::open(dir.join("src/opencode.db")).unwrap();
    conn.execute_batch("DROP TABLE part; DROP TABLE message; DROP TABLE session;")
        .unwrap();
    drop(conn);
    let a = adapter(&dir);
    assert_eq!(a.probe().unwrap().compatibility, Compatibility::Unsupported);
    assert!(a.discover().unwrap().is_empty());
    std::fs::remove_dir_all(&dir).ok();
}
