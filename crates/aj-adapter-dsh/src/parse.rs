//! Translation of DeepSeek Harness session records into IR events.
//!
//! The first line is a `session` header; every other line is `{"type","seq","time","data"}`.
//! Only human input, assistant messages (text and usage), tool calls and results, and titles
//! carry anything for a journal; everything else is skipped by type without deserializing it.

use aj_adapter::json::{line_count, str_of};
use aj_adapter::limits::{MAX_OUTPUT_SUMMARY_CHARS, MAX_PROMPT_CHARS, MAX_TARGET_CHARS};
use aj_adapter::text::{truncate_middle, truncate_tail};
use aj_adapter::{peek_type, ParseOutput};
use aj_ir::{DiffStat, IrEvent, ToolKind, Usage};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

/// The only session format this adapter understands.
const FORMAT_VERSION: u64 = 4;

const HEADER: &str = "session";
const END_SEED: &str = "session/end-seed";

/// Record types that carry nothing for a journal (2026-11, format v4).
const KNOWN_SKIPPED_RECORDS: [&str; 22] = [
    "step/start",
    "step/end",
    "turn/start",
    "turn/end",
    "job/registered",
    "job/settled",
    "request/header",
    "request/context",
    "system/message",
    "web/deepseek-search-llm-request",
    "session-log-deepseek/delivery-accepted",
    "session/title-llm-request",
    "sandbox/mode",
    "approval/policy",
    "permission/preset",
    "model/selection",
    "workspace/changes",
    "deliverables/presented",
    "subagent/catalog",
    "subagent/descriptor",
    "team/mode",
    // Queues a human message that is then recorded again as `user/message` with the same ID.
    "agent/inbox/spliced",
];

/// `user/message` source kind of human input; the others are context the harness injects.
const HUMAN_SOURCE: &str = "user";

/// `session/title` sources, lowest priority first.
const TITLE_SOURCES: [&str; 3] = ["fallback", "provider", "user"];

#[derive(Default)]
pub(crate) struct Parser {
    header_seen: bool,
    /// Subagent session, or a format this adapter does not understand: nothing is emitted.
    excluded: bool,
    /// While set, records are history copied from the parent session of a fork.
    inheriting: bool,
    created_at: i64,
    /// Priority of the title emitted so far.
    title_rank: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Header {
    version: Option<u64>,
    created_at: Option<i64>,
    cwd: Option<String>,
    #[serde(default)]
    is_seeded: bool,
    #[serde(default)]
    delegation_depth: u64,
    parent_session: Option<String>,
    origin: Option<String>,
}

#[derive(Deserialize)]
struct Record {
    #[serde(rename = "type")]
    ty: String,
    seq: Option<u64>,
    time: Option<i64>,
    #[serde(default)]
    data: Value,
}

impl Parser {
    pub(crate) fn line(&mut self, line: &[u8], out: &mut ParseOutput) {
        if self.excluded {
            out.skipped_lines += 1;
            return;
        }
        let ty = peek_type(line);
        if !self.header_seen {
            self.header(line, ty, out);
            return;
        }
        match ty {
            Some(t) if KNOWN_SKIPPED_RECORDS.contains(&t) => {
                out.skipped_lines += 1;
                return;
            }
            Some(t) if !is_handled(t) => {
                out.events.push(IrEvent::Unknown {
                    raw_type: t.to_string(),
                });
                return;
            }
            // A handled type, or an unexpected layout: parse the whole line.
            _ => {}
        }

        let rec: Record = match serde_json::from_slice(line) {
            Ok(r) => r,
            Err(e) => {
                out.error_count += 1;
                out.last_error = Some(e.to_string());
                return;
            }
        };
        if self.inheriting {
            if rec.ty == END_SEED
                && rec.data.get("inherited").and_then(Value::as_bool) == Some(true)
            {
                self.inheriting = false;
                out.skipped_lines += 1;
                return;
            }
            // Copied records predate the fork; the marker may be missing from older files.
            if rec.time.is_some_and(|t| t < self.created_at) {
                out.skipped_lines += 1;
                return;
            }
            self.inheriting = false;
        }
        let Some(timestamp) = rec.time.and_then(DateTime::<Utc>::from_timestamp_millis) else {
            out.error_count += 1;
            out.last_error = Some(format!("{} record without a valid time", rec.ty));
            return;
        };
        let d = &rec.data;
        match rec.ty.as_str() {
            "user/message" => user_message(d, timestamp, out),
            "assistant/message" => assistant_message(d, rec.seq, timestamp, out),
            "tool/call" => tool_call(d, timestamp, out),
            "tool/result" => tool_result(d, out),
            "session/title" => self.title(d, out),
            END_SEED => out.skipped_lines += 1,
            other if KNOWN_SKIPPED_RECORDS.contains(&other) => out.skipped_lines += 1,
            other => out.events.push(IrEvent::Unknown {
                raw_type: other.to_string(),
            }),
        }
    }

    fn header(&mut self, line: &[u8], ty: Option<&str>, out: &mut ParseOutput) {
        self.header_seen = true;
        let header = match ty {
            Some(HEADER) => serde_json::from_slice::<Header>(line).map_err(|e| e.to_string()),
            _ => Err("first line is not a session header".to_string()),
        };
        let header = match header {
            Ok(h) => h,
            Err(e) => {
                self.excluded = true;
                out.error_count += 1;
                out.last_error = Some(e);
                return;
            }
        };
        if header.version != Some(FORMAT_VERSION) {
            self.excluded = true;
            out.error_count += 1;
            out.last_error = Some(format!(
                "unsupported session format version {:?}",
                header.version
            ));
            return;
        }
        // Subagents are represented by the `subagent` call in the parent session.
        if header.origin.as_deref() == Some("subagent") || header.delegation_depth > 0 {
            self.excluded = true;
            out.skipped_lines += 1;
            return;
        }
        self.inheriting = header.is_seeded;
        self.created_at = header.created_at.unwrap_or(i64::MIN);
        out.events.push(IrEvent::SessionMeta {
            cwd: header.cwd,
            git_branch: None,
            agent_version: None,
            parent_session_id: header.parent_session,
        });
    }

    fn title(&mut self, d: &Value, out: &mut ParseOutput) {
        let source = d.pointer("/source/kind").and_then(Value::as_str);
        let rank = TITLE_SOURCES
            .iter()
            .position(|s| Some(*s) == source)
            .unwrap_or(0);
        let title = str_of(d, "title").map(str::trim).unwrap_or_default();
        if title.is_empty() || self.title_rank.is_some_and(|r| r > rank) {
            out.skipped_lines += 1;
            return;
        }
        self.title_rank = Some(rank);
        out.events.push(IrEvent::SessionTitle {
            title: title.to_string(),
        });
    }
}

fn is_handled(ty: &str) -> bool {
    matches!(
        ty,
        "user/message"
            | "assistant/message"
            | "tool/call"
            | "tool/result"
            | "session/title"
            | END_SEED
    )
}

/// Text blocks of a content array joined by blank lines, and the number of image blocks.
fn content_text(content: Option<&Value>) -> (String, usize) {
    let mut texts = Vec::new();
    let mut images = 0;
    for block in content.and_then(Value::as_array).into_iter().flatten() {
        match str_of(block, "type") {
            Some("text") => {
                if let Some(t) = str_of(block, "text")
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                {
                    texts.push(t);
                }
            }
            Some("image") => images += 1,
            _ => {}
        }
    }
    (texts.join("\n\n"), images)
}

fn user_message(d: &Value, timestamp: DateTime<Utc>, out: &mut ParseOutput) {
    if d.pointer("/source/kind").and_then(Value::as_str) != Some(HUMAN_SOURCE) {
        out.skipped_lines += 1;
        return;
    }
    let (text, images) = content_text(d.get("content"));
    let mut prompt = truncate_middle(&text, MAX_PROMPT_CHARS);
    if images > 0 {
        if !prompt.is_empty() {
            prompt.push('\n');
        }
        prompt.push_str(&format!("[image x{images}]"));
    }
    let Some(id) = str_of(d, "id").filter(|_| !prompt.is_empty()) else {
        out.skipped_lines += 1;
        return;
    };
    out.events.push(IrEvent::TurnStart {
        source_event_id: id.to_string(),
        timestamp,
        user_prompt: prompt,
    });
}

fn assistant_message(d: &Value, seq: Option<u64>, timestamp: DateTime<Utc>, out: &mut ParseOutput) {
    let message = d.get("message");
    let model = message
        .and_then(|m| m.pointer("/source/model"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let (text, _) = content_text(message.and_then(|m| m.get("content")));
    let id = message.and_then(|m| str_of(m, "id"));
    match (id, text.is_empty()) {
        (Some(id), false) => out.events.push(IrEvent::AgentMessage {
            source_event_id: id.to_string(),
            timestamp,
            text,
        }),
        (None, false) => {
            out.error_count += 1;
            out.last_error = Some("assistant message without an id".into());
        }
        _ => {}
    }

    // Four non-overlapping counts: `inputTokens` excludes cache reads and writes.
    let Some(u) = d.get("usage") else { return };
    let num = |key: &str| u.get(key).and_then(Value::as_u64).unwrap_or(0);
    let usage = Usage {
        input_tokens: num("inputTokens"),
        output_tokens: num("outputTokens"),
        cache_read_tokens: num("cacheReadTokens"),
        cache_write_tokens: num("cacheWriteTokens"),
    };
    let Some(seq) = seq.filter(|_| !usage.is_zero()) else {
        return;
    };
    out.events.push(IrEvent::TokenUsage {
        source_event_id: format!("usage:{seq}"),
        timestamp,
        model,
        usage,
    });
}

fn tool_call(d: &Value, timestamp: DateTime<Utc>, out: &mut ParseOutput) {
    let (Some(id), Some(name)) = (str_of(d, "callId"), str_of(d, "name")) else {
        out.error_count += 1;
        out.last_error = Some("tool call without an id or name".into());
        return;
    };
    // Arguments are a JSON object encoded as a string.
    let args: Value = str_of(d, "arguments")
        .and_then(|a| serde_json::from_str(a).ok())
        .unwrap_or(Value::Null);
    let s = |key: &str| str_of(&args, key).map(str::to_string);
    let lines = |key: &str| str_of(&args, key).map(line_count);

    let (kind, target, summary, diff_stat) = match name {
        "bash" => (ToolKind::BashCmd, s("command"), s("description"), None),
        "read" | "read_image" => (ToolKind::FileRead, s("file_path"), None, None),
        "write" => (
            ToolKind::FileWrite,
            s("file_path"),
            None,
            lines("content").map(|added| DiffStat { added, removed: 0 }),
        ),
        "edit" => {
            let diff = match (lines("old_string"), lines("new_string")) {
                (Some(removed), Some(added)) => Some(DiffStat { added, removed }),
                _ => None,
            };
            (ToolKind::FileEdit, s("file_path"), None, diff)
        }
        "web_search" => {
            let queries = args.get("queries").and_then(Value::as_array).map(|q| {
                q.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("; ")
            });
            (ToolKind::WebSearch, queries, None, None)
        }
        "web_fetch" => (ToolKind::WebFetch, s("url"), None, None),
        "subagent" => (ToolKind::Subagent, s("name"), s("description"), None),
        "grep" => (ToolKind::Other, s("pattern"), None, None),
        "skill" => (ToolKind::Other, s("name"), None, None),
        _ => (ToolKind::Other, None, None, None),
    };
    out.events.push(IrEvent::ToolAction {
        source_event_id: id.to_string(),
        timestamp,
        kind,
        tool_name: name.to_string(),
        target: target
            .filter(|t| !t.is_empty())
            .map(|t| truncate_middle(&t, MAX_TARGET_CHARS)),
        summary,
        diff_stat,
    });
}

fn tool_result(d: &Value, out: &mut ParseOutput) {
    let Some(message) = d.get("message") else {
        out.skipped_lines += 1;
        return;
    };
    let Some(id) = str_of(message, "toolCallId") else {
        out.error_count += 1;
        out.last_error = Some("tool result without a call id".into());
        return;
    };
    let is_error =
        message.get("isError").and_then(Value::as_bool) == Some(true) || d.get("error").is_some();
    let (text, _) = content_text(message.get("content"));
    let output_summary = (!text.is_empty()).then(|| truncate_tail(&text, MAX_OUTPUT_SUMMARY_CHARS));
    out.events.push(IrEvent::ToolOutcome {
        tool_source_id: id.to_string(),
        is_error,
        diff_stat: None,
        output_summary,
    });
}
