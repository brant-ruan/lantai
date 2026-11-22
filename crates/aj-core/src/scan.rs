//! Incremental scan: discover source files, parse from the cursor, ingest, reslice, assign projects.

use aj_adapter::{head_hash, ParseOutput, SourceAdapter, SourceFile};
use aj_ir::{AgentId, IrEvent};
use aj_store::{CursorRow, SessionRow, Store};
use anyhow::Result;
use chrono::{TimeZone, Utc};

use crate::project::{evaluate_all, EvaluateStats};
use crate::redact::redact_event;
use crate::{slice, Config, INTERNAL_TASK_FINGERPRINT};

#[derive(Debug, Default, Clone)]
pub struct ScanOptions {
    /// Skip files seen for the first time whose modification time is older than this many days (the backfill range).
    pub since_days: Option<i64>,
}

#[derive(Debug, Default, Clone)]
pub struct ScanReport {
    pub files_seen: u64,
    pub files_parsed: u64,
    pub files_unchanged: u64,
    pub files_too_old: u64,
    pub files_failed: u64,
    pub files_reset: u64,
    pub bytes_read: u64,
    pub new_turns: u64,
    pub new_items: u64,
    pub parse_errors: u64,
    pub unknown_records: u64,
    pub failures: Vec<String>,
    /// Result of re-evaluating assignments by rule after the scan.
    pub assignment: EvaluateStats,
}

pub fn scan<Tz: TimeZone>(
    store: &mut Store,
    adapters: &[&dyn SourceAdapter],
    cfg: &Config,
    tz: &Tz,
    opts: &ScanOptions,
) -> Result<ScanReport> {
    let mut report = ScanReport::default();
    let cutoff = opts.since_days.map(|d| Utc::now().timestamp() - d * 86_400);

    for adapter in adapters {
        // One agent whose sources cannot be listed (e.g. its database is mid-write) does not stop the others.
        let files = match adapter.discover() {
            Ok(files) => files,
            Err(e) => {
                report
                    .failures
                    .push(format!("{}: {e:#}", adapter.agent().product_name()));
                continue;
            }
        };
        for file in files {
            report.files_seen += 1;
            // One failing file does not affect the others.
            if let Err(e) = scan_file(store, *adapter, &file, cfg, tz, cutoff, &mut report) {
                report.files_failed += 1;
                report
                    .failures
                    .push(format!("{}: {e:#}", file.path.display()));
            }
        }
    }
    // Rules are evaluated once at the end: new sessions need it, and directories or rules of old sessions may have changed too.
    report.assignment = evaluate_all(store, &|session| normalized_cwd(adapters, session))?;
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn scan_file<Tz: TimeZone>(
    store: &mut Store,
    adapter: &dyn SourceAdapter,
    file: &SourceFile,
    cfg: &Config,
    tz: &Tz,
    cutoff: Option<i64>,
    report: &mut ScanReport,
) -> Result<()> {
    let agent = adapter.agent();
    let path_str = file.path.to_string_lossy().into_owned();
    let cursor = store.cursor(&agent, &file.source_session_id)?;

    let from_offset = match &cursor {
        None => {
            if cutoff.is_some_and(|c| file.mtime < c) {
                report.files_too_old += 1;
                return Ok(());
            }
            0
        }
        // Database-backed sources version each session themselves; a change means a full reparse.
        Some(c) if file.revision.is_some() => {
            if file.revision.as_deref() == Some(c.head_hash.as_str()) && c.current_path == path_str
            {
                report.files_unchanged += 1;
                return Ok(());
            }
            0
        }
        Some(c) if c.size == file.size && c.mtime == file.mtime && c.current_path == path_str => {
            report.files_unchanged += 1;
            return Ok(());
        }
        Some(c) => {
            // A smaller file or a changed head means it was rewritten: reparse from the start. Ingestion is idempotent, so nothing is duplicated.
            if file.size < c.byte_offset || head_hash(&file.path)? != c.head_hash {
                report.files_reset += 1;
                0
            } else {
                c.byte_offset
            }
        }
    };
    let first_time = cursor.is_none();
    // A full reparse replaces the counters; a resumed parse adds to them.
    let reset = !first_time && from_offset == 0;

    let mut parsed = adapter.parse(file, from_offset)?;
    // Recognizable secrets are masked before storage; text is redacted once more before it is sent to a model.
    parsed.events.iter_mut().for_each(redact_event);
    report.files_parsed += 1;
    report.bytes_read += parsed.new_offset.saturating_sub(from_offset);
    report.parse_errors += parsed.error_count;

    // Nothing to ingest (e.g. a session the adapter excludes): remember the cursor without creating an empty session.
    if parsed.events.is_empty() {
        return save_cursor(store, &agent, file, path_str, &parsed, reset, 0);
    }

    let stats = store.ingest(&agent, &file.source_session_id, &parsed.events)?;
    report.new_turns += stats.new_turns;
    report.new_items += stats.new_items;
    report.unknown_records += stats.unknown;

    if stats.new_turns > 0 || stats.new_items > 0 {
        slice::reslice_session(store, stats.session_id, cfg, tz)?;
    }

    // Sessions started by this tool itself: normally never persisted, this is the fallback.
    let is_internal = parsed.events.iter().any(|e| {
        matches!(e, IrEvent::TurnStart { user_prompt, .. } if user_prompt.contains(INTERNAL_TASK_FINGERPRINT))
    });
    if is_internal {
        store.set_session_ignored(stats.session_id, true)?;
    }

    save_cursor(store, &agent, file, path_str, &parsed, reset, stats.unknown)
}

fn save_cursor(
    store: &Store,
    agent: &AgentId,
    file: &SourceFile,
    current_path: String,
    parsed: &ParseOutput,
    reset: bool,
    unknown: u64,
) -> Result<()> {
    store.save_cursor(
        agent,
        &file.source_session_id,
        &CursorRow {
            current_path,
            size: file.size,
            mtime: file.mtime,
            byte_offset: parsed.new_offset,
            head_hash: match &file.revision {
                Some(rev) => rev.clone(),
                None => head_hash(&file.path)?,
            },
        },
        reset,
        parsed.error_count,
        unknown,
        parsed.last_error.as_deref(),
    )
}

/// Using the matching adapter, normalize a session's working directory.
pub fn normalized_cwd(adapters: &[&dyn SourceAdapter], session: &SessionRow) -> Option<String> {
    let cwd = session.detected_cwd.as_deref()?;
    let adapter = adapters.iter().find(|a| a.agent() == session.agent)?;
    Some(adapter.normalize_cwd(cwd))
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;

    /// A database-backed source: one session whose content version the test controls.
    struct Versioned {
        revision: RefCell<String>,
        parses: Cell<u32>,
        fail_discover: Cell<bool>,
    }

    impl SourceAdapter for Versioned {
        fn agent(&self) -> AgentId {
            AgentId::OPENCODE
        }

        fn discover(&self) -> Result<Vec<SourceFile>> {
            if self.fail_discover.get() {
                anyhow::bail!("database changed while copying");
            }
            Ok(vec![SourceFile {
                // Does not exist: a revision-based source must never need the file head.
                path: "/nonexistent/opencode.db".into(),
                source_session_id: "ses_1".into(),
                size: 0,
                mtime: Utc::now().timestamp(),
                revision: Some(self.revision.borrow().clone()),
            }])
        }

        fn parse(&self, _file: &SourceFile, from_offset: u64) -> Result<ParseOutput> {
            assert_eq!(
                from_offset, 0,
                "revision sources always reparse whole sessions"
            );
            self.parses.set(self.parses.get() + 1);
            Ok(ParseOutput {
                events: vec![IrEvent::TurnStart {
                    source_event_id: "msg_1".into(),
                    timestamp: Utc::now(),
                    user_prompt: "hello".into(),
                }],
                ..Default::default()
            })
        }
    }

    #[test]
    fn revision_decides_whether_a_session_is_reparsed() {
        let mut store = Store::open_in_memory().unwrap();
        let adapter = Versioned {
            revision: RefCell::new("r1".into()),
            parses: Cell::new(0),
            fail_discover: Cell::new(false),
        };
        let run = |store: &mut Store| {
            scan(
                store,
                &[&adapter],
                &Config::default(),
                &Utc,
                &ScanOptions::default(),
            )
            .unwrap()
        };

        let first = run(&mut store);
        assert_eq!((first.files_parsed, first.new_turns), (1, 1));
        let again = run(&mut store);
        assert_eq!((again.files_parsed, again.files_unchanged), (0, 1));

        *adapter.revision.borrow_mut() = "r2".into();
        let changed = run(&mut store);
        // Reparsed, deduplicated, and not counted as a rewrite.
        assert_eq!(
            (changed.files_parsed, changed.new_turns, changed.files_reset),
            (1, 0, 0)
        );
        assert_eq!(adapter.parses.get(), 2);

        // An agent whose sources cannot be listed is reported, not fatal.
        adapter.fail_discover.set(true);
        let failed = run(&mut store);
        assert_eq!(failed.failures.len(), 1);
    }
}
