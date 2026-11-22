//! Codex adapter.
//!
//! All knowledge about the Codex rollout file format lives in this crate only.
//! Format findings come from inspecting Codex Desktop rollouts (cli_version up to 0.169) in 2026-11;
//! see `docs/ADAPTERS.md`.
//!
//! Content is read only from `event_msg` / `item_completed` records, whose `item` is an already
//! structured thread item (user message, agent message, command, file change, ...). The raw model
//! traffic in `response_item` records also carries injected context, so it is skipped.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use aj_adapter::text::{strip_tag_blocks, truncate_middle, truncate_tail, unwrap_tag};
use aj_adapter::{read_complete_lines, read_zstd_lines, ParseOutput, SourceAdapter, SourceFile};
use aj_ir::{AgentKind, DiffStat, IrEvent, ToolKind, Usage};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

const MAX_PROMPT_CHARS: usize = 20_000;
const MAX_TARGET_CHARS: usize = 2_000;
const MAX_OUTPUT_SUMMARY_CHARS: usize = 300;

/// Top-level record types we do not need (2026-11).
const KNOWN_SKIPPED_RECORDS: [&str; 7] = [
    "response_item",
    "token_usage_record",
    "world_state",
    "compacted",
    "inter_agent_communication_metadata",
    "event_msg",
    "session_meta",
];

/// Thread item types that carry nothing for a journal.
const SKIPPED_ITEMS: [&str; 4] = [
    "Reasoning",
    "ContextCompaction",
    "CollabAgentToolCall",
    "Plan",
];

/// `thread_source` values of sessions that are not user work: automatic approval reviews,
/// and subagents (represented by the spawning call in the parent session).
const EXCLUDED_THREAD_SOURCES: [&str; 2] = ["guardian_review", "subagent"];

const ROLLOUT_PREFIX: &str = "rollout-";
/// Length of the `YYYY-MM-DDTHH-MM-SS-` timestamp after the prefix.
const ROLLOUT_TIME_LEN: usize = 20;

pub struct CodexAdapter {
    /// Session roots, normally `~/.codex/sessions` and `~/.codex/archived_sessions`.
    roots: Vec<PathBuf>,
}

impl CodexAdapter {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }
}

impl SourceAdapter for CodexAdapter {
    fn agent(&self) -> AgentKind {
        AgentKind::Codex
    }

    fn discover(&self) -> Result<Vec<SourceFile>> {
        // Keyed by session ID: while a rollout is being archived or compressed, two files of
        // the same session can exist; keep the newest so the cursor does not flip between them.
        let mut by_id: HashMap<String, SourceFile> = HashMap::new();
        for root in &self.roots {
            let mut files = Vec::new();
            walk(root, &mut files);
            for file in files {
                match by_id.get(&file.source_session_id) {
                    Some(existing) if existing.mtime >= file.mtime => {}
                    _ => {
                        by_id.insert(file.source_session_id.clone(), file);
                    }
                }
            }
        }
        let mut out: Vec<SourceFile> = by_id.into_values().collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn parse(&self, file: &SourceFile, from_offset: u64) -> Result<ParseOutput> {
        let mut out = ParseOutput::default();
        let mut parser = Parser::new(&file.source_session_id);

        if is_compressed(&file.path) {
            // A compressed stream cannot be resumed at a byte offset: always parse it whole.
            // Ingestion deduplicates by source event ID, so this is idempotent.
            read_zstd_lines(&file.path, |line| parser.line(line, &mut out))?;
            out.new_offset = file.size;
            return Ok(out);
        }

        if from_offset > 0 {
            // Resuming mid-file: the session metadata (and whether the session is excluded) is on the first line.
            if let Some(first) = first_line(&file.path)? {
                parser.line(&first, &mut ParseOutput::default());
            }
        }
        out.new_offset = read_complete_lines(&file.path, from_offset, |line| {
            parser.line(line, &mut out);
        })?;
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

fn is_compressed(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some("zst")
}

/// Session ID from a rollout file name: `rollout-<time>-<id>.jsonl[.zst]`.
/// A continuation segment is named `<threadId>_<segmentId>` and keeps that whole name as its ID.
fn session_id_from_name(name: &str) -> Option<&str> {
    let stem = name
        .strip_suffix(".jsonl.zst")
        .or_else(|| name.strip_suffix(".jsonl"))?;
    let rest = stem.strip_prefix(ROLLOUT_PREFIX)?;
    let id = rest.get(ROLLOUT_TIME_LEN..)?;
    (!id.is_empty()).then_some(id)
}

/// The thread a continuation segment belongs to.
fn segment_parent(session_id: &str) -> Option<&str> {
    session_id.split_once('_').map(|(thread, _)| thread)
}

fn walk(dir: &Path, out: &mut Vec<SourceFile>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            walk(&path, out);
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        let Some(id) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(session_id_from_name)
        else {
            continue;
        };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        out.push(SourceFile {
            source_session_id: id.to_string(),
            size: meta.len(),
            mtime,
            revision: None,
            path,
        });
    }
}

fn first_line(path: &Path) -> Result<Option<Vec<u8>>> {
    let f = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = BufReader::new(f);
    let mut line = Vec::new();
    reader.read_until(b'\n', &mut line)?;
    if line.last() != Some(&b'\n') {
        return Ok(None);
    }
    Ok(Some(line.trim_ascii_end().to_vec()))
}

// ---------------------------------------------------------------------------
// Raw record shapes
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "type")]
    ty: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    payload: Value,
}

#[derive(Deserialize)]
struct SessionMeta {
    id: Option<String>,
    cwd: Option<String>,
    cli_version: Option<String>,
    forked_from_id: Option<String>,
    thread_source: Option<String>,
    #[serde(default)]
    source: Value,
    git: Option<GitInfo>,
}

#[derive(Deserialize)]
struct GitInfo {
    branch: Option<String>,
}

/// Record type and payload type from the head of a line, without deserializing it.
///
/// Lines start with `{"timestamp":"…","ordinal":N,"type":"…","payload":{"type":"…"`; only that
/// head is looked at, so multi-megabyte lines (generated images, tool output) are never scanned.
fn peek_types(line: &[u8]) -> Option<(&str, Option<&str>)> {
    const WINDOW: usize = 256;
    if !line.starts_with(b"{\"timestamp\":\"") {
        return None;
    }
    let head = &line[..line.len().min(WINDOW)];
    let needle = b"\"type\":\"";
    let value_at = |from: usize| -> Option<(usize, &str)> {
        let pos = head[from..]
            .windows(needle.len())
            .position(|w| w == needle)?
            + from;
        let start = pos + needle.len();
        let len = head[start..].iter().position(|b| *b == b'"')?;
        let value = std::str::from_utf8(&head[start..start + len]).ok()?;
        Some((start + len, value))
    };
    let (end, ty) = value_at(0)?;
    let payload_ty = head[end..]
        .starts_with(b"\",\"payload\":{\"type\":\"")
        .then(|| value_at(end).map(|(_, v)| v))
        .flatten();
    Some((ty, payload_ty))
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

struct Parser<'a> {
    own_session_id: &'a str,
    meta_seen: bool,
    /// The session is not user work; every record is skipped.
    excluded: bool,
    /// Model of the latest `turn_context`, attached to token usage.
    model: Option<String>,
}

impl<'a> Parser<'a> {
    fn new(own_session_id: &'a str) -> Self {
        Self {
            own_session_id,
            meta_seen: false,
            excluded: false,
            model: None,
        }
    }

    fn line(&mut self, line: &[u8], out: &mut ParseOutput) {
        if self.excluded {
            out.skipped_lines += 1;
            return;
        }
        let wanted = match peek_types(line) {
            Some(("session_meta", _)) => !self.meta_seen,
            Some(("event_msg", Some("item_completed" | "token_count"))) => true,
            Some(("turn_context", _)) => true,
            Some((ty, _)) => {
                self.skip_type(ty, out);
                return;
            }
            // Unexpected layout: fall back to a full parse.
            None => true,
        };
        if !wanted {
            out.skipped_lines += 1;
            return;
        }

        let env: Envelope = match serde_json::from_slice(line) {
            Ok(e) => e,
            Err(e) => {
                out.error_count += 1;
                out.last_error = Some(e.to_string());
                return;
            }
        };
        let payload_ty = env.payload.get("type").and_then(Value::as_str);
        match (env.ty.as_deref(), payload_ty) {
            (Some("session_meta"), _) if !self.meta_seen => self.session_meta(env.payload, out),
            (Some("event_msg"), Some("item_completed")) => {
                let Some(timestamp) = env.timestamp else {
                    out.error_count += 1;
                    out.last_error = Some("item without a timestamp".into());
                    return;
                };
                match env.payload.get("item") {
                    Some(item) => item_events(item, timestamp, out),
                    None => out.skipped_lines += 1,
                }
            }
            (Some("event_msg"), Some("token_count")) => match env.timestamp {
                Some(timestamp) => token_count(&env.payload, timestamp, self.model.clone(), out),
                None => out.skipped_lines += 1,
            },
            (Some("turn_context"), _) => {
                out.skipped_lines += 1;
                if let Some(model) = str_of(&env.payload, "model").filter(|m| !m.is_empty()) {
                    self.model = Some(model.to_string());
                }
            }
            (Some(ty), _) => self.skip_type(ty, out),
            (None, _) => out.events.push(IrEvent::Unknown {
                raw_type: "<missing type field>".into(),
            }),
        }
    }

    fn skip_type(&self, ty: &str, out: &mut ParseOutput) {
        if KNOWN_SKIPPED_RECORDS.contains(&ty) {
            out.skipped_lines += 1;
        } else {
            out.events.push(IrEvent::Unknown {
                raw_type: ty.to_string(),
            });
        }
    }

    fn session_meta(&mut self, payload: Value, out: &mut ParseOutput) {
        self.meta_seen = true;
        let meta: SessionMeta = match serde_json::from_value(payload) {
            Ok(m) => m,
            Err(e) => {
                out.error_count += 1;
                out.last_error = Some(e.to_string());
                return;
            }
        };
        let is_subagent = meta.source.get("subagent").is_some();
        let excluded_source = meta
            .thread_source
            .as_deref()
            .is_some_and(|s| EXCLUDED_THREAD_SOURCES.contains(&s));
        if is_subagent || excluded_source {
            self.excluded = true;
            out.skipped_lines += 1;
            return;
        }
        // A fork names its origin; a continuation segment belongs to the thread in its file name.
        let parent = meta
            .forked_from_id
            .or_else(|| segment_parent(self.own_session_id).map(str::to_string))
            .or_else(|| meta.id.filter(|id| id != self.own_session_id));
        out.events.push(IrEvent::SessionMeta {
            cwd: meta.cwd,
            git_branch: meta.git.and_then(|g| g.branch),
            agent_version: meta.cli_version,
            parent_session_id: parent,
        });
    }
}

/// `token_count` carries the usage of the latest model call (`last_token_usage`) and a running
/// total. The same snapshot can repeat, so the running total is the dedup key.
fn token_count(
    payload: &Value,
    timestamp: DateTime<Utc>,
    model: Option<String>,
    out: &mut ParseOutput,
) {
    let Some(info) = payload.get("info").filter(|i| !i.is_null()) else {
        out.skipped_lines += 1;
        return;
    };
    let num = |v: Option<&Value>, key: &str| {
        v.and_then(|u| u.get(key))
            .and_then(Value::as_u64)
            .unwrap_or(0)
    };
    let last = info.get("last_token_usage");
    let Some(total) = info
        .get("total_token_usage")
        .and_then(|t| t.get("total_tokens"))
        .and_then(Value::as_u64)
    else {
        out.skipped_lines += 1;
        return;
    };
    // OpenAI semantics: input includes cache hits (and writes); split them out.
    let cache_read = num(last, "cached_input_tokens");
    let cache_write = num(last, "cache_write_input_tokens");
    let usage = Usage {
        input_tokens: num(last, "input_tokens").saturating_sub(cache_read + cache_write),
        output_tokens: num(last, "output_tokens"),
        cache_read_tokens: cache_read,
        cache_write_tokens: cache_write,
    };
    if usage.is_zero() {
        out.skipped_lines += 1;
        return;
    }
    out.events.push(IrEvent::TokenUsage {
        source_event_id: format!("usage:{total}"),
        timestamp,
        model,
        usage,
    });
}

fn str_of<'v>(v: &'v Value, key: &str) -> Option<&'v str> {
    v.get(key).and_then(Value::as_str)
}

fn item_events(item: &Value, timestamp: DateTime<Utc>, out: &mut ParseOutput) {
    let Some(ty) = str_of(item, "type") else {
        out.skipped_lines += 1;
        return;
    };
    if SKIPPED_ITEMS.contains(&ty) {
        out.skipped_lines += 1;
        return;
    }
    let Some(id) = str_of(item, "id").map(str::to_string) else {
        out.skipped_lines += 1;
        return;
    };

    match ty {
        "UserMessage" => match user_prompt(item) {
            Some(user_prompt) => out.events.push(IrEvent::TurnStart {
                source_event_id: id,
                timestamp,
                user_prompt,
            }),
            None => out.skipped_lines += 1,
        },
        "AgentMessage" => {
            let text = block_texts(item.get("content"));
            if text.trim().is_empty() {
                out.skipped_lines += 1;
                return;
            }
            out.events.push(IrEvent::AgentMessage {
                source_event_id: id,
                timestamp,
                text,
            });
        }
        "CommandExecution" => command(item, id, timestamp, out),
        "FileChange" => file_change(item, &id, timestamp, out),
        "McpToolCall" => {
            let server = str_of(item, "server").unwrap_or("unknown");
            let tool = str_of(item, "tool").unwrap_or("unknown");
            let is_error = str_of(item, "status") == Some("failed")
                || item
                    .pointer("/result/isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            let error_text = is_error.then(|| block_texts(item.pointer("/result/content")));
            push_tool(
                out,
                Tool {
                    id,
                    timestamp,
                    kind: ToolKind::Mcp,
                    name: format!("mcp__{server}__{tool}"),
                    target: None,
                    summary: None,
                    diff_stat: None,
                },
                Some((is_error, error_text)),
            );
        }
        "DynamicToolCall" => {
            let namespace = str_of(item, "namespace").unwrap_or("dynamic");
            let tool = str_of(item, "tool").unwrap_or("unknown");
            let ok = item.get("success").and_then(Value::as_bool).unwrap_or(true);
            push_tool(
                out,
                Tool::new(
                    id,
                    timestamp,
                    ToolKind::Other,
                    format!("{namespace}.{tool}"),
                ),
                Some((!ok, None)),
            );
        }
        "WebSearch" => {
            let mut tool = Tool::new(id, timestamp, ToolKind::WebSearch, "web_search".into());
            tool.target = str_of(item, "query").map(str::to_string);
            push_tool(out, tool, None);
        }
        "ImageView" => {
            let mut tool = Tool::new(id, timestamp, ToolKind::FileRead, "view_image".into());
            tool.target = str_of(item, "path").map(str::to_string);
            push_tool(out, tool, None);
        }
        "SubAgentActivity" => {
            // Only the start of a subagent is a call; interactions and completions follow from it.
            if str_of(item, "kind") != Some("started") {
                out.skipped_lines += 1;
                return;
            }
            let mut tool = Tool::new(id, timestamp, ToolKind::Subagent, "spawn_agent".into());
            tool.target = str_of(item, "agent_path").map(str::to_string);
            push_tool(out, tool, None);
        }
        "Extension" => extension(item, id, timestamp, out),
        other => out.events.push(IrEvent::Unknown {
            raw_type: format!("item:{other}"),
        }),
    }
}

/// The human input of a `UserMessage` item, or `None` when it is synthetic.
fn user_prompt(item: &Value) -> Option<String> {
    let mut text = String::new();
    let mut images = 0;
    for block in item
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match str_of(block, "type") {
            Some("text") => {
                if let Some(t) = str_of(block, "text") {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(t);
                }
            }
            Some("image" | "local_image") => images += 1,
            _ => {}
        }
    }
    let prompt = clean_user_prompt(&text);
    match (prompt, images) {
        (None, 0) => None,
        (None, n) => Some(format!("[image x{n}]")),
        (Some(p), 0) => Some(truncate_middle(&p, MAX_PROMPT_CHARS)),
        (Some(p), n) => Some(format!(
            "{}\n[image x{n}]",
            truncate_middle(&p, MAX_PROMPT_CHARS)
        )),
    }
}

fn clean_user_prompt(raw: &str) -> Option<String> {
    // Automation heartbeats are sent by Codex itself, not typed by the user.
    if raw.trim_start().starts_with("<heartbeat>") {
        return None;
    }
    let text = strip_tag_blocks(raw, "in-app-browser-context");
    let text = unwrap_tag(&text, "send_user_message_question_reply");
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Join the `text` of every block in a content array.
fn block_texts(content: Option<&Value>) -> String {
    content
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|b| str_of(b, "text"))
        .collect::<Vec<_>>()
        .join("\n")
}

struct Tool {
    id: String,
    timestamp: DateTime<Utc>,
    kind: ToolKind,
    name: String,
    target: Option<String>,
    summary: Option<String>,
    diff_stat: Option<DiffStat>,
}

impl Tool {
    fn new(id: String, timestamp: DateTime<Utc>, kind: ToolKind, name: String) -> Self {
        Self {
            id,
            timestamp,
            kind,
            name,
            target: None,
            summary: None,
            diff_stat: None,
        }
    }
}

/// Emit a tool call and, when given, its outcome `(is_error, output text)`.
fn push_tool(out: &mut ParseOutput, tool: Tool, outcome: Option<(bool, Option<String>)>) {
    let id = tool.id.clone();
    out.events.push(IrEvent::ToolAction {
        source_event_id: tool.id,
        timestamp: tool.timestamp,
        kind: tool.kind,
        tool_name: tool.name,
        target: tool.target.map(|t| truncate_middle(&t, MAX_TARGET_CHARS)),
        summary: tool.summary,
        diff_stat: tool.diff_stat,
    });
    if let Some((is_error, text)) = outcome {
        let output_summary = text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| truncate_tail(t, MAX_OUTPUT_SUMMARY_CHARS));
        out.events.push(IrEvent::ToolOutcome {
            tool_source_id: id,
            is_error,
            diff_stat: None,
            output_summary,
        });
    }
}

fn command(item: &Value, id: String, timestamp: DateTime<Utc>, out: &mut ParseOutput) {
    // `command` is the argv, normally `[shell, "-lc", script]`; the script is what was run.
    let cmd = match item.get("command") {
        Some(Value::Array(argv)) => argv.last().and_then(Value::as_str).map(str::to_string),
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    };
    let exit_code = item.get("exit_code").and_then(Value::as_i64);
    let is_error = str_of(item, "status") == Some("failed") || exit_code.is_some_and(|c| c != 0);
    let output = str_of(item, "aggregated_output").map(str::to_string);
    let mut tool = Tool::new(id, timestamp, ToolKind::BashCmd, "exec_command".into());
    tool.target = cmd;
    push_tool(out, tool, Some((is_error, output)));
}

fn file_change(item: &Value, id: &str, timestamp: DateTime<Utc>, out: &mut ParseOutput) {
    let is_error = str_of(item, "status").is_some_and(|s| s != "completed");
    let Some(changes) = item.get("changes").and_then(Value::as_object) else {
        out.skipped_lines += 1;
        return;
    };
    for (i, (path, change)) in changes.iter().enumerate() {
        let lines = |key: &str| str_of(change, key).map(|c| c.lines().count() as u32);
        let (kind, diff_stat) = match str_of(change, "type") {
            Some("add") => (
                ToolKind::FileWrite,
                lines("content").map(|added| DiffStat { added, removed: 0 }),
            ),
            Some("delete") => (
                ToolKind::FileEdit,
                lines("content").map(|removed| DiffStat { added: 0, removed }),
            ),
            _ => (
                ToolKind::FileEdit,
                str_of(change, "unified_diff").map(diff_stat_of),
            ),
        };
        let target = str_of(change, "move_path").unwrap_or(path).to_string();
        let source_event_id = if i == 0 {
            id.to_string()
        } else {
            format!("{id}#{i}")
        };
        let mut tool = Tool::new(source_event_id, timestamp, kind, "apply_patch".into());
        tool.target = Some(target);
        tool.diff_stat = diff_stat;
        let error_text = is_error
            .then(|| str_of(item, "stderr").map(str::to_string))
            .flatten();
        push_tool(out, tool, Some((is_error, error_text)));
    }
}

/// Added and removed lines of a unified diff, ignoring the `---` / `+++` file headers.
fn diff_stat_of(diff: &str) -> DiffStat {
    let mut stat = DiffStat::default();
    for line in diff.lines() {
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        match line.as_bytes().first() {
            Some(b'+') => stat.added += 1,
            Some(b'-') => stat.removed += 1,
            _ => {}
        }
    }
    stat
}

fn extension(item: &Value, id: String, timestamp: DateTime<Utc>, out: &mut ParseOutput) {
    let kind = str_of(item, "kind").unwrap_or("extension");
    match kind {
        "clock.sleep" => out.skipped_lines += 1,
        "web.search" => {
            let mut tool = Tool::new(id, timestamp, ToolKind::WebSearch, "web_search".into());
            tool.target = str_of(item, "query").map(str::to_string);
            push_tool(out, tool, None);
        }
        _ => {
            let failed = str_of(item, "status") == Some("failed");
            push_tool(
                out,
                Tool::new(id, timestamp, ToolKind::Other, kind.to_string()),
                item.get("status").map(|_| (failed, None)),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_come_from_file_names() {
        assert_eq!(
            session_id_from_name("rollout-2026-11-17T17-42-39-01a1e8e3-180c.jsonl"),
            Some("01a1e8e3-180c")
        );
        assert_eq!(
            session_id_from_name("rollout-2026-09-25T16-24-15-aaa_bbb.jsonl.zst"),
            Some("aaa_bbb")
        );
        assert_eq!(segment_parent("aaa_bbb"), Some("aaa"));
        assert_eq!(segment_parent("aaa"), None);
        assert_eq!(session_id_from_name("history.jsonl"), None);
        assert_eq!(
            session_id_from_name("rollout-2026-11-17T17-42-39-x.json"),
            None
        );
    }

    #[test]
    fn peeks_record_and_payload_types() {
        let line = br#"{"timestamp":"2026-11-17T01:00:00Z","ordinal":3,"type":"event_msg","payload":{"type":"item_completed","item":{}}}"#;
        assert_eq!(
            peek_types(line),
            Some(("event_msg", Some("item_completed")))
        );
        let line = br#"{"timestamp":"2026-11-17T01:00:00Z","ordinal":0,"type":"session_meta","payload":{"id":"x"}}"#;
        assert_eq!(peek_types(line), Some(("session_meta", None)));
        assert_eq!(peek_types(br#"{"type":"x"}"#), None);
    }

    #[test]
    fn cleans_user_prompts() {
        assert_eq!(clean_user_prompt("<heartbeat>run</heartbeat>"), None);
        assert_eq!(
            clean_user_prompt("<in-app-browser-context>url</in-app-browser-context>\nfix it")
                .as_deref(),
            Some("fix it")
        );
        assert_eq!(
            clean_user_prompt(
                "<send_user_message_question_reply>Yes</send_user_message_question_reply>"
            )
            .as_deref(),
            Some("Yes")
        );
    }

    #[test]
    fn counts_unified_diff_lines() {
        let diff = "--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n-old\n+new\n+more\n ctx\n";
        assert_eq!(
            diff_stat_of(diff),
            DiffStat {
                added: 2,
                removed: 1
            }
        );
    }
}
