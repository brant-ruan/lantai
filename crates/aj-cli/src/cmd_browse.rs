//! Scanning and browsing: scan, days, sessions, show, digest, ignore/include, export, doctor.

use std::collections::BTreeMap;

use aj_adapter::{CompatReason, Compatibility, SourceAdapter};
use aj_core::dehydrate::dehydrate_slice;
use aj_core::scan::{scan, ScanOptions};
use aj_report::material::UNASSIGNED;
use aj_report::StoredDigest;
use aj_store::{SessionRow, SliceRow, TurnItem};
use anyhow::{Context, Result};
use chrono::Local;

use crate::app::{
    adapters, clip, enabled_adapters, find_session, find_slice, short_id, title, App,
};

pub(crate) fn cmd_scan(app: &mut App, since_days: Option<i64>) -> Result<()> {
    let adapters = enabled_adapters(app)?;
    let refs: Vec<&dyn SourceAdapter> = adapters.iter().map(|a| a.as_ref()).collect();
    let started = std::time::Instant::now();
    let r = scan(
        &mut app.store,
        &refs,
        &app.cfg,
        &Local,
        &ScanOptions { since_days },
    )?;

    println!(
        "Scan finished in {:.1}s: {} files found, {} parsed, {} unchanged, {} outside the backfill range, {} failed",
        started.elapsed().as_secs_f64(),
        r.files_seen,
        r.files_parsed,
        r.files_unchanged,
        r.files_too_old,
        r.files_failed
    );
    println!(
        "Read {:.1} MB; {} new turns, {} new items; {} lines failed to parse, {} unknown records{}",
        r.bytes_read as f64 / 1_048_576.0,
        r.new_turns,
        r.new_items,
        r.parse_errors,
        r.unknown_records,
        if r.files_reset > 0 {
            format!("; {} files were rewritten and reparsed", r.files_reset)
        } else {
            String::new()
        }
    );
    for f in r.failures.iter().take(5) {
        eprintln!("  failed: {f}");
    }
    let (sessions, turns) = app.store.session_count()?;
    println!("The database holds {sessions} sessions and {turns} turns");
    if r.assignment.assigned + r.assignment.suggested > 0 {
        println!(
            "Assignment: {} sessions assigned by rule, {} suggestions awaiting confirmation",
            r.assignment.assigned, r.assignment.suggested
        );
    }
    Ok(())
}

pub(crate) fn cmd_reslice(app: &mut App) -> Result<()> {
    let n = aj_core::slice::reslice_all(&mut app.store, &app.cfg, &Local)?;
    println!(
        "Resliced {n} sessions (day boundary {:02}:00)",
        app.cfg.day_boundary_hour
    );
    Ok(())
}

pub(crate) fn cmd_days(app: &App, limit: u32) -> Result<()> {
    for (date, slices, minutes) in app.store.dates_with_activity(limit)? {
        println!(
            "{date}  {slices:>3} slices  {:>4} active min (summed over sessions)",
            minutes
        );
    }
    Ok(())
}

pub(crate) fn cmd_sessions(app: &App, date: &str) -> Result<()> {
    let slices = app.store.slices_for_date(date)?;
    if slices.is_empty() {
        println!("No sessions on {date}. Run `aj scan` first, or use `aj days` to see which days have activity.");
        return Ok(());
    }
    let mut groups: BTreeMap<String, Vec<(SliceRow, SessionRow)>> = BTreeMap::new();
    for slice in slices {
        let Some(session) = app.store.session(slice.session_id)? else {
            continue;
        };
        let project = match slice.effective_project(&session) {
            Some(id) => app.store.project(id)?.map(|p| p.name),
            None => None,
        };
        groups
            .entry(project.unwrap_or_else(|| UNASSIGNED.into()))
            .or_default()
            .push((slice, session));
    }
    println!("{date}");
    for (project, items) in groups {
        println!("\n■ {project}");
        for (slice, session) in items {
            let ignored = slice.is_ignored(&session);
            println!(
                "  S{:<5} {} {}–{}  {:>3} min  {:>2} turns  {:>2} files changed  {}{}",
                slice.id,
                short_id(&session),
                slice.slice_start_at.with_timezone(&Local).format("%H:%M"),
                slice.slice_end_at.with_timezone(&Local).format("%H:%M"),
                slice.active_minutes,
                slice.end_turn_index - slice.start_turn_index + 1,
                slice.files_modified.len(),
                title(&session),
                if ignored { "  [ignored]" } else { "" },
            );
            if let (None, Some(suggested)) = (
                slice.effective_project(&session),
                session.suggested_project_id,
            ) {
                let name = app
                    .store
                    .project(suggested)?
                    .map(|p| p.name)
                    .unwrap_or_default();
                println!(
                    "         suggested → {name} ({})",
                    session
                        .suggestion_reason
                        .as_deref()
                        .unwrap_or("no reason given")
                );
            }
            if let Some(note) = &session.user_note {
                println!("         note: {note}");
            }
        }
    }
    Ok(())
}

pub(crate) fn cmd_show(app: &App, target: &str, full: bool) -> Result<()> {
    // S<digits> is a slice; anything else is a session ID prefix.
    let is_slice = target.len() > 1
        && target.starts_with(['S', 's'])
        && target[1..].chars().all(|c| c.is_ascii_digit());
    let (session, range) = if is_slice {
        let slice = find_slice(app, target)?;
        let session = app
            .store
            .session(slice.session_id)?
            .context("the session this slice belongs to does not exist")?;
        (session, (slice.start_turn_index, slice.end_turn_index))
    } else {
        (find_session(app, target)?, (0, i64::MAX))
    };

    println!(
        "{}  [{} {}]",
        title(&session),
        session.agent.as_str(),
        session.source_session_id
    );
    if let Some(cwd) = &session.detected_cwd {
        println!("Directory: {cwd}");
    }
    for turn in app.store.turns(session.id, range.0, range.1)? {
        println!(
            "\n── T{} · {} ──",
            turn.turn_index,
            turn.started_at.with_timezone(&Local).format("%m-%d %H:%M")
        );
        println!(
            "User: {}",
            clip(&turn.user_prompt, if full { usize::MAX } else { 600 })
        );
        let items = app.store.turn_items(turn.id)?;
        let last_message = items
            .iter()
            .rposition(|i| matches!(i, TurnItem::Message { .. }));
        for (i, item) in items.iter().enumerate() {
            match item {
                TurnItem::Message { text, .. } => {
                    if full || Some(i) == last_message {
                        println!("Agent: {}", clip(text, if full { usize::MAX } else { 800 }));
                    }
                }
                TurnItem::Tool {
                    tool_name,
                    target,
                    is_error,
                    added,
                    removed,
                    ..
                } => {
                    let mut line = format!("  · {tool_name}");
                    if let Some(t) = target {
                        line.push_str(&format!(" {}", clip(&t.replace('\n', " "), 100)));
                    }
                    if added.is_some() || removed.is_some() {
                        line.push_str(&format!(
                            " (+{} -{})",
                            added.unwrap_or(0),
                            removed.unwrap_or(0)
                        ));
                    }
                    if *is_error == Some(true) {
                        line.push_str("  ✗");
                    }
                    println!("{line}");
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn cmd_digest(app: &App, reference: &str) -> Result<()> {
    let slice = find_slice(app, reference)?;
    let session = app
        .store
        .session(slice.session_id)?
        .context("the session this slice belongs to does not exist")?;
    let text = dehydrate_slice(&app.store, &session, &slice, &Local)?.text();
    println!("S{} · {} · {}", slice.id, slice.local_date, title(&session));
    println!("Dehydrated text, {} chars:\n", text.chars().count());
    println!("{text}");
    println!("\n──────── Cached digest ────────");
    match slice
        .digest_json
        .as_deref()
        .map(serde_json::from_str::<StoredDigest>)
    {
        Some(Ok(StoredDigest::Summary(s))) => println!("{}", serde_json::to_string_pretty(&s)?),
        Some(Ok(StoredDigest::Raw { .. })) => {
            println!("(short slice; the text goes into the report as is)")
        }
        Some(Err(e)) => println!("(the cache cannot be parsed: {e})"),
        None => println!(
            "(not generated yet; `aj report daily --date {}` will generate it)",
            slice.local_date
        ),
    }
    Ok(())
}

pub(crate) fn set_ignored(app: &App, session: &str, ignored: bool) -> Result<()> {
    let s = find_session(app, session)?;
    app.store.set_session_ignored(s.id, ignored)?;
    println!(
        "{}: {}",
        if ignored { "Ignored" } else { "Included" },
        title(&s)
    );
    Ok(())
}

pub(crate) fn cmd_export(app: &App, session_prefix: Option<&str>) -> Result<()> {
    use std::io::Write;
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    for adapter in adapters(app)? {
        for file in adapter.discover()? {
            if session_prefix.is_some_and(|p| !file.source_session_id.starts_with(p)) {
                continue;
            }
            let parsed = adapter.parse(&file, 0)?;
            for event in parsed.events {
                let line = serde_json::json!({
                    "ir_version": aj_ir::IR_VERSION,
                    "agent": adapter.agent(),
                    "session": file.source_session_id,
                    "data": event,
                });
                writeln!(out, "{line}")?;
            }
        }
    }
    Ok(())
}

pub(crate) fn cmd_doctor(app: &App) -> Result<()> {
    println!("Data directory: {}", app.data_dir.display());
    println!("IR version: {}", aj_ir::IR_VERSION);
    let (sessions, turns) = app.store.session_count()?;
    println!("The database holds {sessions} sessions and {turns} turns\n");
    let health = app.store.source_health()?;
    if health.is_empty() {
        println!("No source has been scanned yet; run `aj scan` to start.");
    }
    for h in health {
        println!(
            "{}: {} files, {} lines failed to parse, {} unknown records",
            h.agent, h.files, h.error_count, h.unknown_count
        );
        if !h.agent_versions.is_empty() {
            let (first, last) = (
                h.agent_versions.first().unwrap(),
                h.agent_versions.last().unwrap(),
            );
            println!(
                "  versions seen: {first} … {last} ({} in total)",
                h.agent_versions.len()
            );
        }
        if let Some(e) = h.last_error {
            println!("  latest error: {e}");
        }
    }
    // Whether the local data of each agent is in a format its adapter reads (only adapters that check).
    for (kind, probe) in aj_sources::probe_all(&app.data_dir)? {
        let verdict = match probe.compatibility {
            Compatibility::Unchecked => continue,
            Compatibility::Supported => "supported",
            Compatibility::Partial => "partly supported",
            Compatibility::Unsupported => "not supported",
        };
        println!("\n{} local data: {verdict}", kind.display_name());
        if !probe.versions.is_empty() {
            println!("  versions found: {}", probe.versions.join(", "));
        }
        if let Some(reason) = probe.reason {
            println!("  {}", reason_text(&reason));
        }
    }
    Ok(())
}

fn reason_text(r: &CompatReason) -> String {
    match r {
        CompatReason::NewerFormat => "stored in a newer format that is not read yet".into(),
        CompatReason::SessionsInNewerFormat(n) => {
            format!("{n} session(s) stored in a newer format are not read yet")
        }
        CompatReason::UntestedVersions(v) => {
            format!("sessions from untested versions: {}", v.join(", "))
        }
        CompatReason::UnrecognizedLayout => "unexpected data layout; nothing is read".into(),
        CompatReason::ProbeFailed(e) => format!("could not check: {e}"),
    }
}
