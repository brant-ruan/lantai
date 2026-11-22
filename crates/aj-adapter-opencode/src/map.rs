//! Translation of one OpenCode 1.x session (its messages and their parts) into IR events.
//!
//! Pure logic: the rows come from `db`. A message's `data` holds the role, times, model and token
//! usage; its content is split into `part` rows (`text`, `tool`, `reasoning`, `step-*`, ...).
//! Rows are updated in place while a reply streams, and storage keeps the first version of an
//! event, so an assistant message is only read once it has `time.completed`.

use std::collections::HashMap;

use aj_adapter::text::{truncate_middle, truncate_tail};
use aj_adapter::ParseOutput;
use aj_ir::{DiffStat, IrEvent, ToolKind, Usage};
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::db::{DataRow, SessionRow};

const MAX_PROMPT_CHARS: usize = 20_000;
const MAX_TARGET_CHARS: usize = 2_000;
const MAX_OUTPUT_SUMMARY_CHARS: usize = 300;

/// Placeholder title OpenCode gives a session before it has generated one.
const DEFAULT_TITLE_PREFIX: &str = "New session";

/// Part types that carry nothing for a journal (2026-11, OpenCode 1.18).
const KNOWN_SKIPPED_PARTS: [&str; 9] = [
    "reasoning",
    // Step boundaries; `step-finish` repeats the message's token usage.
    "step-start",
    "step-finish",
    // Working-tree snapshots taken for undo, not the patch tool.
    "snapshot",
    "patch",
    // Context compaction marker and the agent / subtask mentions of a prompt.
    "compaction",
    "agent",
    "subtask",
    "retry",
];

fn str_of<'v>(v: &'v Value, key: &str) -> Option<&'v str> {
    v.get(key).and_then(Value::as_str)
}

fn millis(v: &Value, pointer: &str) -> Option<DateTime<Utc>> {
    v.pointer(pointer)
        .and_then(Value::as_i64)
        .and_then(DateTime::<Utc>::from_timestamp_millis)
}

fn bad(out: &mut ParseOutput, msg: String) {
    out.error_count += 1;
    out.last_error = Some(msg);
}

/// Events for a whole session. Subagent sessions (those with a parent) produce nothing: the
/// parent's `task` tool call stands for them.
pub(crate) fn session_events(
    session: &SessionRow,
    messages: &[DataRow],
    parts: &[DataRow],
    out: &mut ParseOutput,
) {
    if session.parent_id.is_some() {
        return;
    }
    out.events.push(IrEvent::SessionMeta {
        cwd: Some(session.directory.clone()).filter(|d| !d.is_empty()),
        git_branch: None,
        agent_version: Some(session.version.clone()).filter(|v| !v.is_empty()),
        parent_session_id: None,
    });
    let title = session.title.trim();
    if !title.is_empty() && !title.starts_with(DEFAULT_TITLE_PREFIX) {
        out.events.push(IrEvent::SessionTitle {
            title: title.to_string(),
        });
    }

    let mut by_message: HashMap<&str, Vec<Value>> = HashMap::new();
    for part in parts {
        match serde_json::from_str::<Value>(&part.data) {
            Ok(v) => by_message
                .entry(part.message_id.as_str())
                .or_default()
                .push(
                    // Keep the row ID with the payload; it is the part's stable event ID.
                    with_id(v, &part.id),
                ),
            Err(e) => bad(out, format!("part {}: {e}", part.id)),
        }
    }
    for message in messages {
        let data: Value = match serde_json::from_str(&message.data) {
            Ok(v) => v,
            Err(e) => {
                bad(out, format!("message {}: {e}", message.id));
                continue;
            }
        };
        let parts = by_message
            .get(message.id.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        match str_of(&data, "role") {
            Some("user") => user_message(&message.id, &data, parts, out),
            Some("assistant") => assistant_message(&message.id, &data, parts, out),
            other => out.events.push(IrEvent::Unknown {
                raw_type: format!("message:{}", other.unwrap_or("<missing role>")),
            }),
        }
    }
}

const ID_KEY: &str = "__row_id";

fn with_id(mut v: Value, id: &str) -> Value {
    if let Value::Object(map) = &mut v {
        map.insert(ID_KEY.into(), Value::String(id.to_string()));
    }
    v
}

fn part_id(part: &Value) -> &str {
    str_of(part, ID_KEY).unwrap_or_default()
}

fn is_synthetic(part: &Value) -> bool {
    part.get("synthetic").and_then(Value::as_bool) == Some(true)
}

fn user_message(id: &str, data: &Value, parts: &[Value], out: &mut ParseOutput) {
    let Some(timestamp) = millis(data, "/time/created") else {
        bad(out, format!("user message {id} without a valid time"));
        return;
    };
    let mut texts = Vec::new();
    let mut images = 0;
    let mut files = Vec::new();
    for part in parts {
        match str_of(part, "type") {
            Some("text") if !is_synthetic(part) => {
                if let Some(t) = str_of(part, "text")
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                {
                    texts.push(t);
                }
            }
            Some("text") => out.skipped_lines += 1,
            Some("file") => {
                if str_of(part, "mime").is_some_and(|m| m.starts_with("image/")) {
                    images += 1;
                } else {
                    files.push(str_of(part, "filename").unwrap_or("file").to_string());
                }
            }
            Some(t) if KNOWN_SKIPPED_PARTS.contains(&t) => out.skipped_lines += 1,
            other => out.events.push(IrEvent::Unknown {
                raw_type: format!("part:{}", other.unwrap_or("<missing type>")),
            }),
        }
    }
    let mut prompt = truncate_middle(&texts.join("\n"), MAX_PROMPT_CHARS);
    let mut extra: Vec<String> = files.iter().map(|f| format!("[file: {f}]")).collect();
    if images > 0 {
        extra.push(format!("[image x{images}]"));
    }
    for e in extra {
        if !prompt.is_empty() {
            prompt.push('\n');
        }
        prompt.push_str(&e);
    }
    // A user message with nothing typed (only synthetic text, e.g. a compaction request) is not human input.
    if texts.is_empty() {
        out.skipped_lines += 1;
        return;
    }
    out.events.push(IrEvent::TurnStart {
        source_event_id: id.to_string(),
        timestamp,
        user_prompt: prompt,
    });
}

fn assistant_message(id: &str, data: &Value, parts: &[Value], out: &mut ParseOutput) {
    // Still streaming: read it once it is complete (the session revision changes then).
    let Some(completed) = millis(data, "/time/completed") else {
        out.skipped_lines += 1;
        return;
    };
    // A compaction summary restates history; it is not new output.
    if data.get("summary").and_then(Value::as_bool) == Some(true) {
        out.skipped_lines += 1;
        return;
    }
    let started = millis(data, "/time/created").unwrap_or(completed);
    for part in parts {
        match str_of(part, "type") {
            Some("text") if !is_synthetic(part) => {
                let text = str_of(part, "text").map(str::trim).unwrap_or_default();
                if !text.is_empty() {
                    out.events.push(IrEvent::AgentMessage {
                        source_event_id: part_id(part).to_string(),
                        timestamp: millis(part, "/time/start").unwrap_or(started),
                        text: text.to_string(),
                    });
                }
            }
            Some("tool") => tool(part, started, out),
            Some(t) if t == "text" || KNOWN_SKIPPED_PARTS.contains(&t) => out.skipped_lines += 1,
            other => out.events.push(IrEvent::Unknown {
                raw_type: format!("part:{}", other.unwrap_or("<missing type>")),
            }),
        }
    }
    usage(id, data, completed, out);
}

/// One usage record per assistant message; `step-finish` parts carry the same numbers.
fn usage(id: &str, data: &Value, timestamp: DateTime<Utc>, out: &mut ParseOutput) {
    let num = |p: &str| data.pointer(p).and_then(Value::as_u64).unwrap_or(0);
    // `input` excludes the cache; IR output includes reasoning.
    let usage = Usage {
        input_tokens: num("/tokens/input"),
        output_tokens: num("/tokens/output") + num("/tokens/reasoning"),
        cache_read_tokens: num("/tokens/cache/read"),
        cache_write_tokens: num("/tokens/cache/write"),
    };
    if usage.is_zero() {
        return;
    }
    out.events.push(IrEvent::TokenUsage {
        source_event_id: format!("usage:{id}"),
        timestamp,
        model: str_of(data, "modelID").map(str::to_string),
        usage,
    });
}

/// One file touched by a patch: path, whether it is new, and line counts.
#[derive(Debug, PartialEq)]
struct PatchFile {
    path: String,
    added_file: bool,
    added: u32,
    removed: u32,
}

/// Files in an `apply_patch` envelope (`*** Add File:`, `*** Update File:`, `*** Delete File:`).
fn patch_files(text: &str) -> Vec<PatchFile> {
    let mut files: Vec<PatchFile> = Vec::new();
    for line in text.lines() {
        let header = [
            ("*** Add File: ", true),
            ("*** Update File: ", false),
            ("*** Delete File: ", false),
        ]
        .into_iter()
        .find_map(|(prefix, added_file)| line.strip_prefix(prefix).map(|p| (p, added_file)));
        if let Some((path, added_file)) = header {
            files.push(PatchFile {
                path: path.trim().to_string(),
                added_file,
                added: 0,
                removed: 0,
            });
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if line.starts_with("***") {
            continue;
        }
        if line.starts_with('+') {
            file.added += 1;
        } else if line.starts_with('-') {
            file.removed += 1;
        }
    }
    files
}

/// One IR tool action: kind, target, summary and line counts.
type Action = (ToolKind, Option<String>, Option<String>, Option<DiffStat>);

fn tool(part: &Value, fallback_time: DateTime<Utc>, out: &mut ParseOutput) {
    let Some(name) = str_of(part, "tool") else {
        bad(
            out,
            format!("tool part {} without a tool name", part_id(part)),
        );
        return;
    };
    let state = part.get("state").unwrap_or(&Value::Null);
    let status = str_of(state, "status").unwrap_or_default();
    // Pending or running calls are read once they finish.
    if !matches!(status, "completed" | "error") {
        out.skipped_lines += 1;
        return;
    }
    let call_id = str_of(part, "callID").unwrap_or(part_id(part)).to_string();
    let timestamp = millis(state, "/time/start").unwrap_or(fallback_time);
    let input = state.get("input").unwrap_or(&Value::Null);
    let s = |key: &str| str_of(input, key).map(str::to_string);
    let lines = |key: &str| str_of(input, key).map(|v| v.lines().count() as u32);

    let actions: Vec<Action> = match name {
        "bash" => vec![(ToolKind::BashCmd, s("command"), s("description"), None)],
        "read" => vec![(ToolKind::FileRead, s("filePath"), None, None)],
        "write" => vec![(
            ToolKind::FileWrite,
            s("filePath"),
            None,
            lines("content").map(|added| DiffStat { added, removed: 0 }),
        )],
        "edit" => {
            let diff = match (lines("oldString"), lines("newString")) {
                (Some(removed), Some(added)) => Some(DiffStat { added, removed }),
                _ => None,
            };
            vec![(ToolKind::FileEdit, s("filePath"), None, diff)]
        }
        "patch" | "apply_patch" => {
            let files = patch_files(str_of(input, "patchText").unwrap_or_default());
            if files.is_empty() {
                vec![(ToolKind::FileEdit, None, None, None)]
            } else {
                files
                    .into_iter()
                    .map(|f| {
                        let kind = if f.added_file {
                            ToolKind::FileWrite
                        } else {
                            ToolKind::FileEdit
                        };
                        let diff = DiffStat {
                            added: f.added,
                            removed: f.removed,
                        };
                        (kind, Some(f.path), None, Some(diff))
                    })
                    .collect()
            }
        }
        "glob" | "grep" => vec![(ToolKind::Other, s("pattern"), None, None)],
        "list" => vec![(ToolKind::Other, s("path"), None, None)],
        "webfetch" => vec![(ToolKind::WebFetch, s("url"), None, None)],
        "websearch" => vec![(ToolKind::WebSearch, s("query"), None, None)],
        "task" => vec![(
            ToolKind::Subagent,
            s("subagent_type"),
            s("description"),
            None,
        )],
        "skill" => vec![(ToolKind::Other, s("name"), None, None)],
        // question, todowrite, todoread, invalid and MCP tools (`<server>_<tool>`, not reliably recognizable).
        _ => vec![(ToolKind::Other, None, None, None)],
    };

    let is_error = status == "error"
        || state
            .pointer("/metadata/exit")
            .and_then(Value::as_i64)
            .is_some_and(|c| c != 0);
    // Keep an output summary only for commands and failures.
    let output = if status == "error" {
        str_of(state, "error")
    } else if name == "bash" || is_error {
        str_of(state, "output")
    } else {
        None
    };
    let output_summary = output
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| truncate_tail(t, MAX_OUTPUT_SUMMARY_CHARS));

    for (i, (kind, target, summary, diff_stat)) in actions.into_iter().enumerate() {
        // A patch touching several files gets one action per file.
        let source_event_id = if i == 0 {
            call_id.clone()
        } else {
            format!("{call_id}:{i}")
        };
        out.events.push(IrEvent::ToolAction {
            source_event_id: source_event_id.clone(),
            timestamp,
            kind,
            tool_name: name.to_string(),
            target: target
                .filter(|t| !t.is_empty())
                .map(|t| truncate_middle(&t, MAX_TARGET_CHARS)),
            summary: summary.filter(|t| !t.is_empty()),
            diff_stat,
        });
        out.events.push(IrEvent::ToolOutcome {
            tool_source_id: source_event_id,
            is_error,
            diff_stat: None,
            output_summary: output_summary.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_envelope_counts_lines_per_file() {
        let text = "*** Begin Patch\n*** Update File: src/a.rs\n@@\n-old\n+new\n+more\n*** Add File: b.txt\n+hello\n*** End Patch";
        assert_eq!(
            patch_files(text),
            [
                PatchFile {
                    path: "src/a.rs".into(),
                    added_file: false,
                    added: 2,
                    removed: 1
                },
                PatchFile {
                    path: "b.txt".into(),
                    added_file: true,
                    added: 1,
                    removed: 0
                },
            ]
        );
    }
}
