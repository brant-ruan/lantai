//! Claude Code adapter.
//!
//! All knowledge about the Claude Code session file format lives in this crate only.
//! Format findings come from inspecting Claude Code 2.1.x in 2026-11; see `docs/ADAPTERS.md`.

use aj_adapter::json::{line_count, str_of};
use aj_adapter::limits::{MAX_OUTPUT_SUMMARY_CHARS, MAX_PROMPT_CHARS, MAX_TARGET_CHARS};
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use aj_adapter::text::{strip_tag_blocks, tag_content, truncate_middle, truncate_tail};
use aj_adapter::{peek_type, read_complete_lines, ParseOutput, SourceAdapter, SourceFile};
use aj_ir::{AgentId, DiffStat, IrEvent, ToolKind, Usage};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

/// Known side-record types that we do not need (2026-11, Claude Code 2.1.x).
const KNOWN_SIDE_RECORDS: [&str; 18] = [
    "api-request",
    "api-request-blob",
    "api-request-shape",
    "agent-name",
    "last-prompt",
    "atis-latch",
    "relocated",
    "queue-operation",
    "mode",
    "permission-mode",
    "file-history-snapshot",
    "file-history-delta",
    "tools-used",
    "cost-state",
    "frame-link",
    "session-owner",
    "artifact-comment-monitor",
    "artifact-autoreact-ledger",
];

pub struct ClaudeAdapter {
    /// Session root, `~/.claude/projects` by default.
    root: PathBuf,
}

impl ClaudeAdapter {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_root() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".claude").join("projects"))
    }
}

impl SourceAdapter for ClaudeAdapter {
    fn agent(&self) -> AgentId {
        AgentId::CLAUDE
    }

    fn discover(&self) -> Result<Vec<SourceFile>> {
        let mut out = Vec::new();
        let Ok(projects) = std::fs::read_dir(&self.root) else {
            return Ok(out);
        };
        for project in projects.flatten() {
            let Ok(files) = std::fs::read_dir(project.path()) else {
                continue;
            };
            // Only take <sessionId>.jsonl directly under a project directory. Subagent logs live deeper,
            // under subagents/; for now they are represented by the Subagent tool call in the parent session.
            for f in files.flatten() {
                let path = f.path();
                if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                    continue;
                }
                let Ok(meta) = f.metadata() else { continue };
                if !meta.is_file() {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                out.push(SourceFile {
                    source_session_id: stem.to_string(),
                    size: meta.len(),
                    mtime,
                    revision: None,
                    path,
                });
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn parse(&self, file: &SourceFile, from_offset: u64) -> Result<ParseOutput> {
        let mut out = ParseOutput::default();
        let mut meta = MetaTracker {
            own_session_id: file.source_session_id.clone(),
            ..Default::default()
        };
        let new_offset = read_complete_lines(&file.path, from_offset, |line| {
            parse_line(line, &mut out, &mut meta);
        })?;
        out.new_offset = new_offset;
        Ok(out)
    }

    fn normalize_cwd(&self, cwd: &str) -> String {
        // Claude Code worktrees live at <repo>/.claude/worktrees/<name>; map them back to the main repository.
        match cwd.find("/.claude/worktrees/") {
            Some(pos) => cwd[..pos].to_string(),
            None => cwd.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Raw record shapes (only the fields we use; serde skips the rest)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    #[serde(rename = "type")]
    ty: Option<String>,
    uuid: Option<String>,
    timestamp: Option<DateTime<Utc>>,
    cwd: Option<String>,
    git_branch: Option<String>,
    version: Option<String>,
    session_id: Option<String>,
    #[serde(default)]
    is_sidechain: bool,
    #[serde(default)]
    is_meta: bool,
    #[serde(default)]
    is_compact_summary: bool,
    origin: Option<Origin>,
    message: Option<Message>,
    custom_title: Option<String>,
    tool_use_result: Option<Value>,
}

#[derive(Deserialize)]
struct Origin {
    kind: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<MessageUsage>,
    content: Option<Content>,
}

/// `message.usage` of an assistant record; Anthropic semantics, so `input_tokens` already excludes the cache.
#[derive(Deserialize)]
struct MessageUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Content {
    Text(String),
    Blocks(Vec<Block>),
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Block {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        is_error: Option<bool>,
        #[serde(default)]
        content: Option<Value>,
    },
    Image {},
    #[serde(other)]
    Other,
}

/// Emits SessionMeta only when the metadata changes, instead of once per record.
#[derive(Default)]
struct MetaTracker {
    own_session_id: String,
    parent_session_id: Option<String>,
    cwd: Option<String>,
    git_branch: Option<String>,
    version: Option<String>,
    /// `message.id` whose usage was last emitted. Records of one message are consecutive and repeat the same usage.
    last_usage_message: Option<String>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

fn parse_line(line: &[u8], out: &mut ParseOutput, meta: &mut MetaTracker) {
    // Lines starting with {"type":"..." are side records (API request bodies, file snapshots, ...) and make up most of the file size.
    // Only custom-title is useful. Other known types are skipped without deserializing; unseen types are counted as unknown.
    if let Some(ty) = peek_type(line) {
        if ty != "custom-title" {
            if KNOWN_SIDE_RECORDS.contains(&ty) {
                out.skipped_lines += 1;
            } else {
                out.events.push(IrEvent::Unknown {
                    raw_type: ty.to_string(),
                });
            }
            return;
        }
    }

    let rec: Record = match serde_json::from_slice(line) {
        Ok(r) => r,
        Err(e) => {
            out.error_count += 1;
            out.last_error = Some(e.to_string());
            return;
        }
    };

    match rec.ty.as_deref() {
        Some("custom-title") => {
            if let Some(title) = rec.custom_title.filter(|t| !t.trim().is_empty()) {
                out.events.push(IrEvent::SessionTitle { title });
            }
        }
        Some("user") => {
            if rec.is_sidechain || is_inherited(&rec, out, meta) {
                return;
            }
            track_meta(&rec, out, meta);
            parse_user(rec, out);
        }
        Some("assistant") => {
            if rec.is_sidechain || is_inherited(&rec, out, meta) {
                return;
            }
            track_meta(&rec, out, meta);
            parse_assistant(rec, out, meta);
        }
        // Known types we do not need.
        Some("system" | "attachment" | "summary") => out.skipped_lines += 1,
        Some(other) => out.events.push(IrEvent::Unknown {
            raw_type: other.to_string(),
        }),
        None => out.events.push(IrEvent::Unknown {
            raw_type: "<missing type field>".to_string(),
        }),
    }
}

/// A forked session file starts with history copied from the original session; those records keep the original session ID.
/// They belong to the original session, so we skip them and only record the parent, otherwise the same conversation is counted twice.
fn is_inherited(rec: &Record, out: &mut ParseOutput, meta: &mut MetaTracker) -> bool {
    let Some(sid) = rec.session_id.as_deref() else {
        return false;
    };
    if sid == meta.own_session_id {
        return false;
    }
    if meta.parent_session_id.as_deref() != Some(sid) {
        meta.parent_session_id = Some(sid.to_string());
        out.events.push(IrEvent::SessionMeta {
            cwd: None,
            git_branch: None,
            agent_version: None,
            parent_session_id: Some(sid.to_string()),
        });
    }
    out.skipped_lines += 1;
    true
}

fn track_meta(rec: &Record, out: &mut ParseOutput, meta: &mut MetaTracker) {
    let changed = (rec.cwd.is_some() && rec.cwd != meta.cwd)
        || (rec.git_branch.is_some() && rec.git_branch != meta.git_branch)
        || (rec.version.is_some() && rec.version != meta.version);
    if !changed {
        return;
    }
    if rec.cwd.is_some() {
        meta.cwd = rec.cwd.clone();
    }
    if rec.git_branch.is_some() {
        meta.git_branch = rec.git_branch.clone();
    }
    if rec.version.is_some() {
        meta.version = rec.version.clone();
    }
    out.events.push(IrEvent::SessionMeta {
        cwd: meta.cwd.clone(),
        git_branch: meta.git_branch.clone(),
        agent_version: meta.version.clone(),
        parent_session_id: None,
    });
}

fn parse_user(rec: Record, out: &mut ParseOutput) {
    let (Some(uuid), Some(timestamp)) = (rec.uuid, rec.timestamp) else {
        return;
    };
    let Some(content) = rec.message.and_then(|m| m.content) else {
        return;
    };

    let mut text = String::new();
    let mut image_count = 0;
    let mut has_tool_result = false;

    match content {
        Content::Text(t) => text = t,
        Content::Blocks(blocks) => {
            for block in blocks {
                match block {
                    Block::Text { text: t } => {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(&t);
                    }
                    Block::Image {} => image_count += 1,
                    Block::ToolResult {
                        tool_use_id,
                        is_error,
                        content,
                    } => {
                        has_tool_result = true;
                        out.events.push(tool_outcome(
                            tool_use_id,
                            is_error.unwrap_or(false),
                            content.as_ref(),
                            rec.tool_use_result.as_ref(),
                        ));
                    }
                    _ => {}
                }
            }
        }
    }

    // Tool results, meta records and compaction summaries are not human input.
    if has_tool_result || rec.is_meta || rec.is_compact_summary {
        return;
    }
    // Newer versions label the origin explicitly; task notifications, messages from other sessions etc. do not start a turn.
    if let Some(kind) = rec.origin.and_then(|o| o.kind) {
        if kind != "human" {
            return;
        }
    }

    let Some(mut prompt) = clean_user_prompt(&text) else {
        if image_count == 0 {
            return;
        }
        // An input with only images still counts as human input.
        out.events.push(IrEvent::TurnStart {
            source_event_id: uuid,
            timestamp,
            user_prompt: format!("[image x{image_count}]"),
        });
        return;
    };
    if image_count > 0 {
        prompt.push_str(&format!("\n[image x{image_count}]"));
    }
    out.events.push(IrEvent::TurnStart {
        source_event_id: uuid,
        timestamp,
        user_prompt: truncate_middle(&prompt, MAX_PROMPT_CHARS),
    });
}

/// Clean up user input. Returns `None` when this is not a human input.
fn clean_user_prompt(raw: &str) -> Option<String> {
    let text = strip_tag_blocks(raw, "system-reminder");
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    // Older versions have no origin field; synthetic messages are recognized by their prefix.
    const SYNTHETIC_PREFIXES: [&str; 5] = [
        "<local-command-stdout>",
        "<local-command-stderr>",
        "<local-command-caveat>",
        "<task-notification>",
        "[Request interrupted by user",
    ];
    if SYNTHETIC_PREFIXES.iter().any(|p| text.starts_with(p)) {
        return None;
    }
    // Slash command: <command-name>/foo</command-name> ... <command-args>bar</command-args>
    if let Some(name) = tag_content(text, "command-name") {
        let args = tag_content(text, "command-args").unwrap_or_default();
        let cmd = format!("{} {}", name.trim(), args.trim());
        return Some(cmd.trim().to_string());
    }
    Some(text.to_string())
}

fn parse_assistant(rec: Record, out: &mut ParseOutput, meta: &mut MetaTracker) {
    let (Some(uuid), Some(timestamp), Some(message)) = (rec.uuid, rec.timestamp, rec.message)
    else {
        return;
    };
    if let Some(blocks) = match message.content {
        Some(Content::Blocks(blocks)) => Some(blocks),
        _ => None,
    } {
        assistant_blocks(&uuid, timestamp, blocks, out);
    }
    if let (Some(id), Some(u)) = (message.id, message.usage) {
        if meta.last_usage_message.as_deref() == Some(id.as_str()) {
            return;
        }
        let usage = Usage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cache_read_tokens: u.cache_read_input_tokens,
            cache_write_tokens: u.cache_creation_input_tokens,
        };
        let synthetic = message.model.as_deref() == Some("<synthetic>");
        if !synthetic && !usage.is_zero() {
            out.events.push(IrEvent::TokenUsage {
                source_event_id: format!("usage:{id}"),
                timestamp,
                model: message.model,
                usage,
            });
        }
        meta.last_usage_message = Some(id);
    }
}

fn assistant_blocks(
    uuid: &str,
    timestamp: DateTime<Utc>,
    blocks: Vec<Block>,
    out: &mut ParseOutput,
) {
    for (i, block) in blocks.into_iter().enumerate() {
        match block {
            Block::Text { text } => {
                if text.trim().is_empty() {
                    continue;
                }
                // In practice each record holds one content block; the index guards against several text blocks in one record.
                let source_event_id = if i == 0 {
                    uuid.to_string()
                } else {
                    format!("{uuid}#{i}")
                };
                out.events.push(IrEvent::AgentMessage {
                    source_event_id,
                    timestamp,
                    text,
                });
            }
            Block::ToolUse { id, name, input } => {
                out.events.push(tool_action(id, timestamp, &name, &input));
            }
            _ => {}
        }
    }
}

fn tool_action(id: String, timestamp: DateTime<Utc>, name: &str, input: &Value) -> IrEvent {
    let s = |key: &str| str_of(input, key).map(str::to_string);
    let lines = |key: &str| str_of(input, key).map(line_count);

    let (kind, target, summary, diff_stat) = match name {
        "Read" => (ToolKind::FileRead, s("file_path"), None, None),
        "Edit" | "MultiEdit" => {
            // An estimate from the arguments; the patch in the tool result overrides it with exact counts.
            let diff = match (lines("old_string"), lines("new_string")) {
                (Some(removed), Some(added)) => Some(DiffStat { added, removed }),
                _ => None,
            };
            (ToolKind::FileEdit, s("file_path"), None, diff)
        }
        "NotebookEdit" => (ToolKind::FileEdit, s("notebook_path"), None, None),
        "Write" => {
            let diff = lines("content").map(|added| DiffStat { added, removed: 0 });
            (ToolKind::FileWrite, s("file_path"), None, diff)
        }
        "Bash" => (ToolKind::BashCmd, s("command"), s("description"), None),
        "WebSearch" => (ToolKind::WebSearch, s("query"), None, None),
        "WebFetch" => (ToolKind::WebFetch, s("url"), None, None),
        "Agent" | "Task" => (
            ToolKind::Subagent,
            s("subagent_type"),
            s("description"),
            None,
        ),
        "Grep" | "Glob" => (ToolKind::Other, s("pattern"), None, None),
        "Skill" => (ToolKind::Other, s("skill"), None, None),
        n if n.starts_with("mcp__") => (ToolKind::Mcp, None, None, None),
        _ => (ToolKind::Other, None, None, None),
    };

    IrEvent::ToolAction {
        source_event_id: id,
        timestamp,
        kind,
        tool_name: name.to_string(),
        target: target.map(|t| truncate_middle(&t, MAX_TARGET_CHARS)),
        summary,
        diff_stat,
    }
}

fn tool_outcome(
    tool_use_id: String,
    is_error: bool,
    block_content: Option<&Value>,
    tool_use_result: Option<&Value>,
) -> IrEvent {
    // Results of edit tools carry a structured patch, which gives exact added/removed line counts.
    let diff_stat = tool_use_result
        .and_then(|r| r.get("structuredPatch"))
        .and_then(Value::as_array)
        .filter(|hunks| !hunks.is_empty())
        .map(|hunks| {
            let mut stat = DiffStat::default();
            for line in hunks
                .iter()
                .filter_map(|h| h.get("lines").and_then(Value::as_array))
                .flatten()
                .filter_map(Value::as_str)
            {
                match line.as_bytes().first() {
                    Some(b'+') => stat.added += 1,
                    Some(b'-') => stat.removed += 1,
                    _ => {}
                }
            }
            stat
        });

    // Keep an output summary only for command results and errors; full file contents etc. are dropped.
    let is_command = tool_use_result.is_some_and(|r| r.get("stdout").is_some());
    let output_summary = if is_command || is_error {
        let text = match tool_use_result {
            Some(r) if is_command => {
                let stdout = r.get("stdout").and_then(Value::as_str).unwrap_or("");
                let stderr = r.get("stderr").and_then(Value::as_str).unwrap_or("");
                if stderr.trim().is_empty() {
                    stdout.to_string()
                } else {
                    format!("{stdout}\n{stderr}")
                }
            }
            _ => block_content.map(block_text).unwrap_or_default(),
        };
        let text = text.trim();
        (!text.is_empty()).then(|| truncate_tail(text, MAX_OUTPUT_SUMMARY_CHARS))
    } else {
        None
    };

    IrEvent::ToolOutcome {
        tool_source_id: tool_use_id,
        is_error,
        diff_stat,
        output_summary,
    }
}

/// The content of a tool_result is either a string or an array of content blocks.
fn block_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|i| i.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_system_reminders_and_detects_synthetic() {
        assert_eq!(
            clean_user_prompt("<system-reminder>x</system-reminder>\nfix the login").as_deref(),
            Some("fix the login")
        );
        assert_eq!(
            clean_user_prompt("<system-reminder>only</system-reminder>"),
            None
        );
        assert_eq!(
            clean_user_prompt("<local-command-stdout>ok</local-command-stdout>"),
            None
        );
        assert_eq!(clean_user_prompt("[Request interrupted by user]"), None);
        assert_eq!(
            clean_user_prompt(
                "<command-name>/review</command-name><command-args>pr 12</command-args>"
            )
            .as_deref(),
            Some("/review pr 12")
        );
    }
}
