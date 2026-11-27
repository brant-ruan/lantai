//! Translation of WorkBuddy session records into IR events.
//!
//! Each line is one record in the shape of the OpenAI Agents SDK items: `message` (user or
//! assistant), `reasoning`, `function_call`, `function_call_result`, plus a few side records.
//! The `type` field is usually not first, so every line is deserialized; serde skips the fields
//! that are not declared below.

use aj_adapter::json::{line_count, str_of};
use aj_adapter::limits::{MAX_OUTPUT_SUMMARY_CHARS, MAX_PROMPT_CHARS, MAX_TARGET_CHARS};
use aj_adapter::text::{strip_tag_blocks, tag_content, truncate_middle, truncate_tail};
use aj_adapter::ParseOutput;
use aj_ir::{DiffStat, IrEvent, ToolKind, Usage};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

/// Record types that carry nothing for a journal (2026-11, WorkBuddy 5.11).
const KNOWN_SKIPPED_RECORDS: [&str; 4] = [
    // Written twice at the start of every turn: host kind and a copy of the title.
    "session-meta",
    // Model reasoning; a copy also rides on the step's `function_call`.
    "reasoning",
    "file-history-snapshot",
    // Marks a regenerated or edited input; the new input follows as an ordinary message.
    "resend-fork-notice",
];

/// The tag that wraps the human's own words inside the injected context.
const QUERY_TAG: &str = "user_query";

#[derive(Default)]
pub(crate) struct Parser {
    /// Working directory emitted so far.
    cwd: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    #[serde(rename = "type")]
    ty: Option<String>,
    id: Option<String>,
    timestamp: Option<i64>,
    role: Option<String>,
    cwd: Option<String>,
    #[serde(default)]
    content: Value,
    call_id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
    #[serde(default)]
    output: Value,
    #[serde(default)]
    provider_data: Value,
    ai_title: Option<String>,
}

impl Parser {
    pub(crate) fn line(&mut self, line: &[u8], out: &mut ParseOutput) {
        let rec: Record = match serde_json::from_slice(line) {
            Ok(r) => r,
            Err(e) => {
                out.error_count += 1;
                out.last_error = Some(e.to_string());
                return;
            }
        };
        let Some(ty) = rec.ty.as_deref() else {
            out.events.push(IrEvent::Unknown {
                raw_type: "<missing type field>".into(),
            });
            return;
        };
        if KNOWN_SKIPPED_RECORDS.contains(&ty) {
            out.skipped_lines += 1;
            return;
        }
        if !matches!(
            ty,
            "message" | "function_call" | "function_call_result" | "ai-title"
        ) {
            out.events.push(IrEvent::Unknown {
                raw_type: ty.to_string(),
            });
            return;
        }
        if ty == "ai-title" {
            match rec.ai_title.as_deref().map(str::trim) {
                Some(t) if !t.is_empty() => out.events.push(IrEvent::SessionTitle {
                    title: t.to_string(),
                }),
                _ => out.skipped_lines += 1,
            }
            return;
        }

        let Some(timestamp) = rec
            .timestamp
            .and_then(DateTime::<Utc>::from_timestamp_millis)
        else {
            out.error_count += 1;
            out.last_error = Some(format!("{ty} record without a valid timestamp"));
            return;
        };
        self.track_cwd(&rec, out);
        match (ty, rec.role.as_deref()) {
            ("message", Some("user")) => user_message(&rec, timestamp, out),
            ("message", Some("assistant")) => {
                assistant_message(&rec, timestamp, out);
                usage(&rec, timestamp, out);
            }
            ("message", _) => out.skipped_lines += 1,
            ("function_call", _) => {
                tool_call(&rec, timestamp, out);
                usage(&rec, timestamp, out);
            }
            _ => tool_result(&rec, out),
        }
    }

    fn track_cwd(&mut self, rec: &Record, out: &mut ParseOutput) {
        let Some(cwd) = rec.cwd.as_deref().filter(|c| !c.is_empty()) else {
            return;
        };
        if self.cwd.as_deref() == Some(cwd) {
            return;
        }
        self.cwd = Some(cwd.to_string());
        out.events.push(IrEvent::SessionMeta {
            cwd: self.cwd.clone(),
            git_branch: None,
            agent_version: None,
            parent_session_id: None,
        });
    }
}

fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool) == Some(true)
}

/// Text blocks of the given type joined by `sep`, and the number of image blocks.
fn content_text(content: &Value, text_type: &str, sep: &str) -> (String, usize) {
    let mut texts = Vec::new();
    let mut images = 0;
    for block in content.as_array().into_iter().flatten() {
        match str_of(block, "type") {
            Some(t) if t == text_type => {
                if let Some(text) = str_of(block, "text") {
                    texts.push(text);
                }
            }
            Some("image_blob_ref") => images += 1,
            _ => {}
        }
    }
    (texts.join(sep), images)
}

fn user_message(rec: &Record, timestamp: DateTime<Utc>, out: &mut ParseOutput) {
    let pd = &rec.provider_data;
    // A compaction summary quotes every earlier query; only flagged requests are human input.
    if flag(pd, "isCompacted") || !flag(pd, "startsNewUserRequest") {
        out.skipped_lines += 1;
        return;
    }
    let (text, images) = content_text(&rec.content, "input_text", "\n");
    let words =
        tag_content(&text, QUERY_TAG).unwrap_or_else(|| strip_tag_blocks(&text, "system-reminder"));
    let mut prompt = truncate_middle(words.trim(), MAX_PROMPT_CHARS);
    if images > 0 {
        if !prompt.is_empty() {
            prompt.push('\n');
        }
        prompt.push_str(&format!("[image x{images}]"));
    }
    let Some(id) = rec.id.as_deref().filter(|_| !prompt.is_empty()) else {
        out.skipped_lines += 1;
        return;
    };
    out.events.push(IrEvent::TurnStart {
        source_event_id: id.to_string(),
        timestamp,
        user_prompt: prompt,
    });
}

fn assistant_message(rec: &Record, timestamp: DateTime<Utc>, out: &mut ParseOutput) {
    // The notice left behind when a reply is stopped or fails is not model output.
    if flag(&rec.provider_data, "skipRun") {
        return;
    }
    let (text, _) = content_text(&rec.content, "output_text", "\n\n");
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    match rec.id.as_deref() {
        Some(id) => out.events.push(IrEvent::AgentMessage {
            source_event_id: id.to_string(),
            timestamp,
            text: text.to_string(),
        }),
        None => {
            out.error_count += 1;
            out.last_error = Some("assistant message without an id".into());
        }
    }
}

/// Token usage of the reasoning step that ends with this record, if it carries any.
fn usage(rec: &Record, timestamp: DateTime<Utc>, out: &mut ParseOutput) {
    let pd = &rec.provider_data;
    let Some(u) = pd.get("usage") else { return };
    let num = |v: Option<&Value>| v.and_then(Value::as_u64).unwrap_or(0);
    // OpenAI semantics: `inputTokens` includes the cache hits listed in its details.
    let cached: u64 = u
        .get("inputTokensDetails")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|d| num(d.get("cached_tokens")))
        .sum();
    let usage = Usage {
        input_tokens: num(u.get("inputTokens")).saturating_sub(cached),
        output_tokens: num(u.get("outputTokens")),
        cache_read_tokens: cached,
        cache_write_tokens: num(pd.pointer("/rawUsage/cache_creation_input_tokens")),
    };
    let Some(id) = rec.id.as_deref().filter(|_| !usage.is_zero()) else {
        return;
    };
    out.events.push(IrEvent::TokenUsage {
        source_event_id: format!("usage:{id}"),
        timestamp,
        model: str_of(pd, "model").map(str::to_string),
        usage,
    });
}

fn tool_call(rec: &Record, timestamp: DateTime<Utc>, out: &mut ParseOutput) {
    let (Some(id), Some(name)) = (rec.call_id.as_deref(), rec.name.as_deref()) else {
        out.error_count += 1;
        out.last_error = Some("function call without a call id or name".into());
        return;
    };
    // Arguments are a JSON object encoded as a string.
    let args: Value = rec
        .arguments
        .as_deref()
        .and_then(|a| serde_json::from_str(a).ok())
        .unwrap_or(Value::Null);
    let s = |key: &str| str_of(&args, key).map(str::to_string);
    let lines = |key: &str| str_of(&args, key).map(line_count);

    let (kind, target, summary, diff_stat) = match name {
        "Bash" => (ToolKind::BashCmd, s("command"), s("description"), None),
        "Read" => (ToolKind::FileRead, s("file_path"), None, None),
        "ViewImage" => (ToolKind::FileRead, s("image_path"), None, None),
        "Write" => (
            ToolKind::FileWrite,
            s("file_path"),
            None,
            lines("content").map(|added| DiffStat { added, removed: 0 }),
        ),
        "Edit" | "MultiEdit" => {
            let diff = match (lines("old_string"), lines("new_string")) {
                (Some(removed), Some(added)) => Some(DiffStat { added, removed }),
                _ => None,
            };
            (ToolKind::FileEdit, s("file_path"), None, diff)
        }
        "WebSearch" => (ToolKind::WebSearch, s("query"), None, None),
        "WebFetch" => (ToolKind::WebFetch, s("url"), None, None),
        "Agent" => (
            ToolKind::Subagent,
            s("subagent_type"),
            s("description"),
            None,
        ),
        "Grep" | "Glob" => (ToolKind::Other, s("pattern"), None, None),
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

fn tool_result(rec: &Record, out: &mut ParseOutput) {
    let Some(id) = rec.call_id.as_deref() else {
        out.error_count += 1;
        out.last_error = Some("function call result without a call id".into());
        return;
    };
    // The record status stays `completed` for a failed tool; only the raw response tells.
    let raw = rec.provider_data.pointer("/toolResult/rawResponse");
    let is_error = raw.is_some_and(|r| {
        flag(r, "is_error")
            || r.get("exitCode")
                .and_then(Value::as_i64)
                .is_some_and(|c| c != 0)
    });
    // Keep an output summary only for commands and failures; file contents etc. are dropped.
    let output_summary = (is_error || rec.name.as_deref() == Some("Bash"))
        .then(|| output_text(&rec.output))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .map(|t| truncate_tail(&t, MAX_OUTPUT_SUMMARY_CHARS));
    out.events.push(IrEvent::ToolOutcome {
        tool_source_id: id.to_string(),
        is_error,
        diff_stat: None,
        output_summary,
    });
}

/// `output` is a single `{type, text}` block or an array of them.
fn output_text(output: &Value) -> String {
    match output {
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| str_of(b, "text"))
            .collect::<Vec<_>>()
            .join("\n"),
        other => str_of(other, "text").unwrap_or_default().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(lines: &[&str]) -> ParseOutput {
        let mut out = ParseOutput::default();
        let mut parser = Parser::default();
        for l in lines {
            parser.line(l.as_bytes(), &mut out);
        }
        out
    }

    #[test]
    fn the_query_tag_is_the_prompt_and_context_is_dropped() {
        let out = parse(&[
            r#"{"id":"u","timestamp":1,"type":"message","role":"user","content":[{"type":"input_text","text":"<system-reminder>x</system-reminder><current_time>t</current_time><user_query>\n fix it \n</user_query>"}],"providerData":{"startsNewUserRequest":true}}"#,
        ]);
        let IrEvent::TurnStart { user_prompt, .. } = &out.events[0] else {
            panic!("{:?}", out.events)
        };
        assert_eq!(user_prompt, "fix it");
    }

    #[test]
    fn usage_excludes_cache_hits_from_input() {
        let out = parse(&[
            r#"{"id":"a","timestamp":1,"type":"message","role":"assistant","content":[],"providerData":{"model":"m","usage":{"inputTokens":100,"outputTokens":7,"inputTokensDetails":[{"cached_tokens":60}]},"rawUsage":{}}}"#,
        ]);
        let IrEvent::TokenUsage { usage, model, .. } = &out.events[0] else {
            panic!("{:?}", out.events)
        };
        assert_eq!(model.as_deref(), Some("m"));
        assert_eq!(
            *usage,
            Usage {
                input_tokens: 40,
                output_tokens: 7,
                cache_read_tokens: 60,
                cache_write_tokens: 0
            }
        );
    }
}
