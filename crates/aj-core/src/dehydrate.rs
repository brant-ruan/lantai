//! Dehydration: compress one slice into high signal-to-noise text for the model.
//!
//! Keeps user input, metadata summaries of tool calls and the conclusion of each turn; drops file contents and long output.
//! The output is redacted before it is returned, so callers need no further handling.

use std::fmt::Write;

use aj_ir::ToolKind;
use aj_store::{SessionRow, SliceRow, Store, TurnItem};
use anyhow::Result;
use chrono::TimeZone;

use crate::redact::redact;

const MAX_PROMPT_CHARS: usize = 3_000;
const MAX_CONCLUSION_CHARS: usize = 1_500;
const MAX_COMMANDS_PER_TURN: usize = 12;
const MAX_COMMAND_CHARS: usize = 160;
const MAX_ERROR_OUTPUT_CHARS: usize = 160;

/// One turn of a slice, dehydrated and redacted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DehydratedTurn {
    /// Turn index within the session; source anchors refer to it.
    pub index: i64,
    pub text: String,
}

/// A dehydrated slice, kept per turn so that long slices can be split at turn boundaries.
#[derive(Debug, Clone, Default)]
pub struct Dehydrated {
    pub turns: Vec<DehydratedTurn>,
}

impl Dehydrated {
    /// The whole slice as one text.
    pub fn text(&self) -> String {
        join_turns(&self.turns)
    }

    /// Turn indexes contained in the slice, used to verify source anchors.
    pub fn turn_indexes(&self) -> Vec<i64> {
        self.turns.iter().map(|t| t.index).collect()
    }
}

/// Join dehydrated turns into the text sent to a model.
pub fn join_turns(turns: &[DehydratedTurn]) -> String {
    turns
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn dehydrate_slice<Tz: TimeZone>(
    store: &Store,
    session: &SessionRow,
    slice: &SliceRow,
    tz: &Tz,
) -> Result<Dehydrated>
where
    Tz::Offset: std::fmt::Display,
{
    let mut turns = Vec::new();
    let cwd = session.detected_cwd.as_deref();

    for turn in store.turns(session.id, slice.start_turn_index, slice.end_turn_index)? {
        let mut text = String::new();
        let time = turn.started_at.with_timezone(tz).format("%H:%M");
        let _ = writeln!(text, "### T{} {time}", turn.turn_index);
        if turn.user_prompt.trim().is_empty() {
            text.push_str("User: (no new input; continues the previous context)\n");
        } else {
            let _ = writeln!(
                text,
                "User: {}",
                truncate_middle(turn.user_prompt.trim(), MAX_PROMPT_CHARS)
            );
        }

        let items = store.turn_items(turn.id)?;
        let actions = summarize_actions(&items, cwd);
        if !actions.is_empty() {
            text.push_str("Actions:\n");
            for a in actions {
                let _ = writeln!(text, "- {a}");
            }
        }
        // Keep only the last agent text of each turn: intermediate narration has no value for a journal.
        let conclusion = items.iter().rev().find_map(|i| match i {
            TurnItem::Message { text, .. } => Some(text.trim()),
            _ => None,
        });
        if let Some(c) = conclusion.filter(|c| !c.is_empty()) {
            let _ = writeln!(text, "Result: {}", truncate_middle(c, MAX_CONCLUSION_CHARS));
        }
        turns.push(DehydratedTurn {
            index: turn.turn_index,
            text: redact(text.trim_end()),
        });
    }

    Ok(Dehydrated { turns })
}

/// Accumulated changes to one file within a turn.
struct FileChange {
    path: String,
    /// The first operation was a whole-file write.
    created: bool,
    count: u32,
    added: u32,
    removed: u32,
}

/// Compress the tool calls of one turn into a few summary lines.
fn summarize_actions(items: &[TurnItem], cwd: Option<&str>) -> Vec<String> {
    // File changes are aggregated by path, in order of first appearance.
    let mut edits: Vec<FileChange> = Vec::new();
    let mut commands: Vec<String> = Vec::new();
    let mut extra_commands = 0usize;
    let mut reads = 0usize;
    let mut searches: Vec<String> = Vec::new();
    let mut fetches = 0usize;
    let mut subagents: Vec<String> = Vec::new();
    let mut mcp = 0usize;
    let mut other = 0usize;

    for item in items {
        let TurnItem::Tool {
            kind,
            target,
            summary,
            is_error,
            added,
            removed,
            output_summary,
            ..
        } = item
        else {
            continue;
        };
        match kind {
            ToolKind::FileEdit | ToolKind::FileWrite => {
                let path = relative_path(target.as_deref().unwrap_or("?"), cwd);
                let created = *kind == ToolKind::FileWrite;
                match edits.iter_mut().find(|e| e.path == path) {
                    Some(e) => {
                        e.count += 1;
                        e.added += added.unwrap_or(0);
                        e.removed += removed.unwrap_or(0);
                    }
                    None => edits.push(FileChange {
                        path,
                        created,
                        count: 1,
                        added: added.unwrap_or(0),
                        removed: removed.unwrap_or(0),
                    }),
                }
            }
            ToolKind::BashCmd => {
                if commands.len() >= MAX_COMMANDS_PER_TURN {
                    extra_commands += 1;
                    continue;
                }
                let cmd = one_line(target.as_deref().unwrap_or("?"), MAX_COMMAND_CHARS);
                let mut line = format!("command `{cmd}`");
                if let Some(desc) = summary.as_deref().filter(|s| !s.trim().is_empty()) {
                    let _ = write!(line, " ({})", one_line(desc, 80));
                }
                if *is_error == Some(true) {
                    line.push_str(" → failed");
                    if let Some(out) = output_summary.as_deref() {
                        let _ = write!(line, ": {}", one_line_tail(out, MAX_ERROR_OUTPUT_CHARS));
                    }
                }
                commands.push(line);
            }
            ToolKind::FileRead => reads += 1,
            ToolKind::WebSearch => searches.push(one_line(target.as_deref().unwrap_or("?"), 80)),
            ToolKind::WebFetch => fetches += 1,
            ToolKind::Subagent => subagents.push(one_line(
                summary.as_deref().or(target.as_deref()).unwrap_or("?"),
                100,
            )),
            ToolKind::Mcp => mcp += 1,
            ToolKind::Other => other += 1,
        }
    }

    let mut out = Vec::new();
    for FileChange {
        path,
        created,
        count,
        added,
        removed,
    } in edits
    {
        let verb = if created { "wrote" } else { "edited" };
        let times = if count > 1 {
            format!(" ×{count}")
        } else {
            String::new()
        };
        out.push(format!("{verb} {path}{times} (+{added} -{removed})"));
    }
    out.extend(commands);
    if extra_commands > 0 {
        out.push(format!("…and {extra_commands} more commands"));
    }
    for s in subagents {
        out.push(format!("subagent: {s}"));
    }
    if !searches.is_empty() {
        out.push(format!("web search: {}", searches.join("; ")));
    }
    let mut misc = Vec::new();
    if reads > 0 {
        misc.push(format!("{reads} file reads"));
    }
    if fetches > 0 {
        misc.push(format!("{fetches} web fetches"));
    }
    if mcp > 0 {
        misc.push(format!("{mcp} external tool calls"));
    }
    if other > 0 {
        misc.push(format!("{other} other tool calls"));
    }
    if !misc.is_empty() {
        out.push(misc.join(", "));
    }
    out
}

fn relative_path(path: &str, cwd: Option<&str>) -> String {
    cwd.and_then(|c| path.strip_prefix(c))
        .map(|p| p.trim_start_matches('/').to_string())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| path.to_string())
}

/// Flatten to one line and keep the beginning.
fn one_line(text: &str, max_chars: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max_chars {
        return flat;
    }
    let head: String = flat.chars().take(max_chars).collect();
    format!("{head}…")
}

/// Flatten to one line and keep the end.
fn one_line_tail(text: &str, max_chars: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = flat.chars().count();
    if count <= max_chars {
        return flat;
    }
    let tail: String = flat.chars().skip(count - max_chars).collect();
    format!("…{tail}")
}

pub fn truncate_middle(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars * 2 / 3).collect();
    let tail: String = text.chars().skip(count - max_chars / 3).collect();
    format!("{head}\n…[{} chars omitted]…\n{tail}", count - max_chars)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{slice::reslice_session, Config};
    use aj_ir::{AgentId, DiffStat, IrEvent};
    use chrono::{DateTime, FixedOffset, Utc};

    fn t(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn tool(id: &str, kind: ToolKind, target: &str, diff: Option<(u32, u32)>) -> IrEvent {
        IrEvent::ToolAction {
            source_event_id: id.into(),
            timestamp: t("2026-11-16T02:00:10Z"),
            kind,
            tool_name: "x".into(),
            target: Some(target.into()),
            summary: None,
            diff_stat: diff.map(|(added, removed)| DiffStat { added, removed }),
        }
    }

    #[test]
    fn dehydrates_turn_into_compact_redacted_text() {
        let tz = FixedOffset::east_opt(8 * 3600).unwrap();
        let mut store = Store::open_in_memory().unwrap();
        let events = vec![
            IrEvent::SessionMeta {
                cwd: Some("/work/app".into()),
                git_branch: None,
                agent_version: None,
                parent_session_id: None,
            },
            IrEvent::TurnStart {
                source_event_id: "u1".into(),
                timestamp: t("2026-11-16T02:00:00Z"),
                user_prompt: "fix the login, the key is API_KEY=abcdef123456789".into(),
            },
            tool("r1", ToolKind::FileRead, "/work/app/src/a.rs", None),
            tool("r2", ToolKind::FileRead, "/work/app/src/b.rs", None),
            tool("e1", ToolKind::FileEdit, "/work/app/src/a.rs", Some((3, 1))),
            tool("e2", ToolKind::FileEdit, "/work/app/src/a.rs", Some((2, 2))),
            tool(
                "w1",
                ToolKind::FileWrite,
                "/work/app/src/new.rs",
                Some((40, 0)),
            ),
            tool("b1", ToolKind::BashCmd, "cargo test", None),
            IrEvent::ToolOutcome {
                tool_source_id: "b1".into(),
                is_error: true,
                diff_stat: None,
                output_summary: Some("test auth::login ... FAILED".into()),
            },
            IrEvent::AgentMessage {
                source_event_id: "m1".into(),
                timestamp: t("2026-11-16T02:00:20Z"),
                text: "let me look at the code first".into(),
            },
            IrEvent::AgentMessage {
                source_event_id: "m2".into(),
                timestamp: t("2026-11-16T02:01:00Z"),
                text: "fixed, one test still fails".into(),
            },
        ];
        let sid = store
            .ingest(&AgentId::CLAUDE, "s", &events)
            .unwrap()
            .session_id;
        reslice_session(&mut store, sid, &Config::default(), &tz).unwrap();
        let session = store.session(sid).unwrap().unwrap();
        let slice = &store.slices_for_session(sid).unwrap()[0];

        let d = dehydrate_slice(&store, &session, slice, &tz).unwrap();
        assert_eq!(d.turn_indexes(), vec![0]);
        let expected = "\
### T0 10:00
User: fix the login, the key is API_KEY=[REDACTED]
Actions:
- edited src/a.rs ×2 (+5 -3)
- wrote src/new.rs (+40 -0)
- command `cargo test` → failed: test auth::login ... FAILED
- 2 file reads
Result: fixed, one test still fails";
        assert_eq!(d.text(), expected);
    }
}
