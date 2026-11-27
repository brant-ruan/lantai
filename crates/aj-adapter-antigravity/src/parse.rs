//! Line-level Antigravity transcript parser, tool mapper, and positional tool-outcome matcher.
//!
//! Handles both the full (`transcript_full.jsonl`) and compact (`transcript.jsonl`) formats from
//! AGY 2.0 and Antigravity IDE without touching the filesystem.

use aj_adapter::json::{line_count, str_of};
use aj_adapter::limits::{MAX_OUTPUT_SUMMARY_CHARS, MAX_PROMPT_CHARS, MAX_TARGET_CHARS};
use std::collections::VecDeque;

use aj_adapter::text::{strip_tag_blocks, tag_content, truncate_middle, truncate_tail};
use aj_adapter::ParseOutput;
use aj_ir::{DiffStat, IrEvent, ToolKind, Usage};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};

use crate::cwd::{is_internal_brain_path, CwdCandidates};

const MAX_ARTIFACT_SUMMARY_CHARS: usize = 300;

/// Step types that carry system/lifecycle noise rather than conversation turns or actions.
pub(crate) const SKIPPED_STEP_TYPES: [&str; 5] = [
    "SYSTEM_MESSAGE",
    "CHECKPOINT",
    "ERROR_MESSAGE",
    "CONVERSATION_HISTORY",
    "KNOWLEDGE_ARTIFACTS",
];

/// Step types that deliver the outcome of a pending tool call in FIFO order.
pub(crate) const TOOL_RESULT_STEP_TYPES: [&str; 9] = [
    "GENERIC",
    "VIEW_FILE",
    "CODE_ACTION",
    "GREP_SEARCH",
    "RUN_COMMAND",
    "LIST_DIRECTORY",
    "FIND_BY_NAME",
    "SEARCH_WEB",
    "READ_URL_CONTENT",
];

/// Internal orchestration/UI tools omitted from the journal.
const SKIPPED_TOOLS: [&str; 11] = [
    "manage_task",
    "command_status",
    "send_command_input",
    "schedule",
    "enter_plan_mode",
    "exit_plan_mode",
    "define_subagent",
    "manage_subagents",
    "send_message",
    "list_plugin_accounts",
    "ask_question",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StepHead<'a> {
    pub ty: &'a str,
    pub status: Option<&'a str>,
}

/// Extract `"type"` and optional `"status"` from the header prefix of a transcript line without deserializing it.
pub(crate) fn peek_step_head(line: &[u8]) -> Option<StepHead<'_>> {
    const WINDOW: usize = 512;
    if !line.starts_with(b"{\"step_index\":") {
        return None;
    }
    let window = &line[..line.len().min(WINDOW)];
    // Never scan past large payload fields if a line has an unexpected field order.
    let limit = [
        b"\"content\":".as_slice(),
        b"\"thinking\":",
        b"\"tool_calls\":",
    ]
    .into_iter()
    .filter_map(|needle| window.windows(needle.len()).position(|w| w == needle))
    .min()
    .unwrap_or(window.len());
    let head = &window[..limit];
    let ty = peek_str_field(head, b"\"type\":\"")?;
    let status = peek_str_field(head, b"\"status\":\"");
    Some(StepHead { ty, status })
}

fn peek_str_field<'a>(hay: &'a [u8], needle: &[u8]) -> Option<&'a str> {
    let pos = hay.windows(needle.len()).position(|w| w == needle)?;
    let start = pos + needle.len();
    let len = hay[start..].iter().position(|b| *b == b'"')?;
    std::str::from_utf8(&hay[start..start + len]).ok()
}

/// Read just the `"type"` field of a line (fast-path header first, serde fallback otherwise).
pub(crate) fn step_type(line: &[u8]) -> Option<String> {
    if let Some(head) = peek_step_head(line) {
        return Some(head.ty.to_string());
    }
    #[derive(Deserialize)]
    struct TypeOnly {
        #[serde(rename = "type")]
        ty: Option<String>,
    }
    serde_json::from_slice::<TypeOnly>(line).ok()?.ty
}

#[derive(Clone)]
struct PendingTool {
    id: String,
    kind: ToolKind,
}

pub(crate) struct Parser {
    compact_args: bool,
    pending_tools: VecDeque<Option<PendingTool>>,
    cwd: CwdCandidates,
    /// Model named by the latest planner step that carried one; most steps omit it.
    model: Option<String>,
}

impl Parser {
    pub fn new(compact_args: bool) -> Self {
        Self {
            compact_args,
            pending_tools: VecDeque::new(),
            cwd: CwdCandidates::default(),
            model: None,
        }
    }

    /// Rebuild pending-tool queue state when resuming after `from_offset`.
    pub fn seed_pending_from_planner(&mut self, planner_line: &[u8], results_already_seen: usize) {
        self.pending_tools.clear();
        let Ok(step) = serde_json::from_slice::<Step>(planner_line) else {
            return;
        };
        let Some(idx) = step.step_index else {
            return;
        };
        for (i, tc) in step.tool_calls.unwrap_or_default().into_iter().enumerate() {
            let name = tc.name.unwrap_or_default();
            if SKIPPED_TOOLS.contains(&name.as_str()) {
                self.pending_tools.push_back(None);
                continue;
            }
            let args = self.decode_args(tc.args);
            let kind = map_tool(&name, &args).0;
            self.pending_tools.push_back(Some(PendingTool {
                id: format!("step:{idx}:tool:{i}"),
                kind,
            }));
        }
        for _ in 0..results_already_seen {
            self.pending_tools.pop_front();
        }
    }

    pub fn inferred_cwd(&self) -> Option<String> {
        self.cwd.infer()
    }

    pub fn line(&mut self, line: &[u8], out: &mut ParseOutput) {
        if let Some(head) = peek_step_head(line) {
            if SKIPPED_STEP_TYPES.contains(&head.ty) {
                out.skipped_lines += 1;
                return;
            }
            if TOOL_RESULT_STEP_TYPES.contains(&head.ty) {
                match head.status {
                    Some("RUNNING") => {
                        let _ = self.pending_tools.pop_front();
                        out.skipped_lines += 1;
                        return;
                    }
                    Some("DONE") => match self.pending_tools.front() {
                        None => {
                            out.skipped_lines += 1;
                            return;
                        }
                        Some(None) => {
                            self.pending_tools.pop_front();
                            out.skipped_lines += 1;
                            return;
                        }
                        Some(Some(tool)) if tool.kind != ToolKind::BashCmd => {
                            let id = tool.id.clone();
                            self.pending_tools.pop_front();
                            out.events.push(IrEvent::ToolOutcome {
                                tool_source_id: id,
                                is_error: false,
                                diff_stat: None,
                                output_summary: None,
                            });
                            return;
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }

        let step: Step = match serde_json::from_slice(line) {
            Ok(s) => s,
            Err(e) => {
                out.error_count += 1;
                out.last_error = Some(e.to_string());
                return;
            }
        };

        match step.ty.as_deref() {
            Some("USER_INPUT") => self.handle_user_input(step, out),
            Some("PLANNER_RESPONSE") => self.handle_planner_response(step, out),
            Some(ty) if TOOL_RESULT_STEP_TYPES.contains(&ty) => self.handle_tool_result(step, out),
            Some(ty) if SKIPPED_STEP_TYPES.contains(&ty) => out.skipped_lines += 1,
            Some(other) => out.events.push(IrEvent::Unknown {
                raw_type: other.to_string(),
            }),
            None => out.events.push(IrEvent::Unknown {
                raw_type: "<missing type field>".into(),
            }),
        }
    }

    fn handle_user_input(&mut self, step: Step, out: &mut ParseOutput) {
        self.pending_tools.clear();
        if step.source.as_deref() != Some("USER_EXPLICIT") {
            out.skipped_lines += 1;
            return;
        }
        let (Some(idx), Some(timestamp)) = (step.step_index, step.created_at) else {
            out.error_count += 1;
            out.last_error = Some("USER_INPUT missing step_index or created_at".into());
            return;
        };
        let raw = step.content.as_deref().unwrap_or("");
        self.collect_active_docs(raw);
        let media_count = step.media.as_ref().map_or(0, Vec::len);
        let Some(user_prompt) = clean_user_prompt(raw, media_count) else {
            out.skipped_lines += 1;
            return;
        };
        out.events.push(IrEvent::TurnStart {
            source_event_id: format!("step:{idx}"),
            timestamp,
            user_prompt,
        });
    }

    fn collect_active_docs(&mut self, raw: &str) {
        let Some(meta) = tag_content(raw, "ADDITIONAL_METADATA") else {
            return;
        };
        for line in meta.lines() {
            if let Some(rest) = line.trim().strip_prefix("Active Document:") {
                let path = rest.trim();
                if !path.is_empty() {
                    self.cwd.add_active_doc(path);
                }
            }
        }
    }

    fn handle_planner_response(&mut self, step: Step, out: &mut ParseOutput) {
        self.pending_tools.clear();
        let (Some(idx), Some(timestamp)) = (step.step_index, step.created_at) else {
            out.error_count += 1;
            out.last_error = Some("PLANNER_RESPONSE missing step_index or created_at".into());
            return;
        };

        if let Some(model) = step.model.as_deref().filter(|m| !m.is_empty()) {
            self.model = Some(model.to_string());
        }
        // Every planner step is a model call, whether or not it produced visible text.
        if step.input_tokens.is_some() || step.output_tokens.is_some() {
            let usage = Usage {
                input_tokens: step.input_tokens.unwrap_or(0),
                output_tokens: step.output_tokens.unwrap_or(0),
                cache_read_tokens: step.cache_read_tokens.unwrap_or(0),
                cache_write_tokens: 0,
            };
            if !usage.is_zero() {
                out.events.push(IrEvent::TokenUsage {
                    source_event_id: format!("step:{idx}:usage"),
                    timestamp,
                    model: self.model.clone(),
                    usage,
                });
            }
        }

        if let Some(text) = step
            .content
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            out.events.push(IrEvent::AgentMessage {
                source_event_id: format!("step:{idx}:msg"),
                timestamp,
                text: text.to_string(),
            });
        }

        for (i, tc) in step.tool_calls.unwrap_or_default().into_iter().enumerate() {
            let name = tc.name.unwrap_or_default();
            let args = self.decode_args(tc.args);
            self.collect_tool_paths(&name, &args);

            if SKIPPED_TOOLS.contains(&name.as_str()) {
                self.pending_tools.push_back(None);
                continue;
            }

            let id = format!("step:{idx}:tool:{i}");
            let (kind, target, summary, diff_stat) = map_tool(&name, &args);
            self.pending_tools.push_back(Some(PendingTool {
                id: id.clone(),
                kind,
            }));

            out.events.push(IrEvent::ToolAction {
                source_event_id: id,
                timestamp,
                kind,
                tool_name: name,
                target: target.map(|t| truncate_middle(&t, MAX_TARGET_CHARS)),
                summary: summary.map(|s| truncate_middle(&s, MAX_ARTIFACT_SUMMARY_CHARS)),
                diff_stat,
            });
        }
    }

    fn handle_tool_result(&mut self, step: Step, out: &mut ParseOutput) {
        let Some(Some(tool)) = self.pending_tools.pop_front() else {
            out.skipped_lines += 1;
            return;
        };
        if step.status.as_deref() == Some("RUNNING") {
            out.skipped_lines += 1;
            return;
        }

        let status_err = matches!(step.status.as_deref(), Some("ERROR" | "INVALID"));
        let exit_code = step.exit_code.or_else(|| {
            if tool.kind == ToolKind::BashCmd {
                step.content.as_deref().and_then(parse_exit_code)
            } else {
                None
            }
        });
        let error_text = step
            .error
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let is_error = status_err || error_text.is_some() || exit_code.is_some_and(|c| c != 0);

        let output_summary = if let Some(err) = error_text {
            Some(truncate_tail(err, MAX_OUTPUT_SUMMARY_CHARS))
        } else if tool.kind == ToolKind::BashCmd {
            step.content
                .as_deref()
                .map(command_output_body)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| truncate_tail(s, MAX_OUTPUT_SUMMARY_CHARS))
        } else if is_error {
            step.content
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| truncate_tail(s, MAX_OUTPUT_SUMMARY_CHARS))
        } else {
            None
        };

        out.events.push(IrEvent::ToolOutcome {
            tool_source_id: tool.id,
            is_error,
            diff_stat: None,
            output_summary,
        });
    }

    fn decode_args(&self, raw: Value) -> Map<String, Value> {
        let Value::Object(map) = raw else {
            return Map::new();
        };
        if !self.compact_args {
            return map;
        }
        map.into_iter()
            .map(|(k, v)| (k, decode_compact_value(v)))
            .collect()
    }

    fn collect_tool_paths(&mut self, name: &str, args: &Map<String, Value>) {
        if has_artifact_metadata(args) {
            return;
        }
        match name {
            "run_command" => {
                if let Some(cwd) = arg_str(args, "Cwd").filter(|p| !is_internal_brain_path(p)) {
                    self.cwd.add_explicit_cwd(cwd);
                }
            }
            "edit_file"
            | "write_file"
            | "replace_file_content"
            | "multi_replace_file_content"
            | "write_to_file" => {
                if let Some(path) = arg_any_str(args, &["Path", "TargetFile"])
                    .filter(|p| !is_internal_brain_path(p))
                {
                    self.cwd.add_write_file(path);
                }
            }
            "read_file" | "view_file" | "view_file_outline" | "view_code_item" => {
                if let Some(path) = arg_any_str(args, &["Path", "AbsolutePath", "File"])
                    .filter(|p| !is_internal_brain_path(p))
                {
                    self.cwd.add_read_file(path);
                }
            }
            "list_dir" => {
                if let Some(dir) =
                    arg_str(args, "DirectoryPath").filter(|p| !is_internal_brain_path(p))
                {
                    self.cwd.add_read_dir(dir);
                }
            }
            "find_by_name" | "FindByName" => {
                if let Some(dir) =
                    arg_str(args, "SearchDirectory").filter(|p| !is_internal_brain_path(p))
                {
                    self.cwd.add_read_dir(dir);
                }
            }
            "grep_search" | "GrepSearch" => {
                if let Some(path) =
                    arg_str(args, "SearchPath").filter(|p| !is_internal_brain_path(p))
                {
                    self.cwd.add_search_path(path);
                }
            }
            _ => {}
        }
    }
}

#[derive(Deserialize)]
struct Step {
    step_index: Option<u64>,
    source: Option<String>,
    #[serde(rename = "type")]
    ty: Option<String>,
    status: Option<String>,
    created_at: Option<DateTime<Utc>>,
    content: Option<String>,
    tool_calls: Option<Vec<RawToolCall>>,
    media: Option<Vec<Value>>,
    error: Option<String>,
    exit_code: Option<i64>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_read_tokens: Option<u64>,
    model: Option<String>,
}

#[derive(Deserialize)]
struct RawToolCall {
    name: Option<String>,
    #[serde(default)]
    args: Value,
}

fn clean_user_prompt(raw: &str, media_count: usize) -> Option<String> {
    let extracted = tag_content(raw, "USER_REQUEST").unwrap_or_else(|| {
        let stripped = raw
            .trim_start()
            .strip_prefix("<USER_REQUEST>")
            .unwrap_or(raw);
        let stripped = strip_tag_blocks(stripped, "ADDITIONAL_METADATA");
        strip_tag_blocks(&stripped, "USER_SETTINGS_CHANGE")
    });
    let text = extracted.trim();
    match (text.is_empty(), media_count) {
        (true, 0) => None,
        (true, n) => Some(format!("[image x{n}]")),
        (false, 0) => Some(truncate_middle(text, MAX_PROMPT_CHARS)),
        (false, n) => Some(format!(
            "{}\n[image x{n}]",
            truncate_middle(text, MAX_PROMPT_CHARS)
        )),
    }
}

fn arg_str<'a>(args: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
}

fn arg_string(args: &Map<String, Value>, key: &str) -> Option<String> {
    arg_str(args, key).map(str::to_string)
}

fn arg_any_str<'a>(args: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| arg_str(args, k))
}

fn arg_any_string(args: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    arg_any_str(args, keys).map(str::to_string)
}

fn arg_lines(args: &Map<String, Value>, key: &str) -> Option<u32> {
    args.get(key).and_then(Value::as_str).map(line_count)
}

fn has_artifact_metadata(args: &Map<String, Value>) -> bool {
    args.get("ArtifactMetadata").is_some_and(|v| !v.is_null())
}

fn artifact_summary(args: &Map<String, Value>) -> Option<String> {
    args.get("ArtifactMetadata")
        .and_then(|v| v.get("Summary"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn path_basename(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    trimmed
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(path)
        .to_string()
}

fn apply_artifact_guard(
    args: &Map<String, Value>,
    kind: ToolKind,
    target: Option<String>,
    diff_stat: Option<DiffStat>,
) -> (ToolKind, Option<String>, Option<String>, Option<DiffStat>) {
    let is_artifact =
        has_artifact_metadata(args) || target.as_deref().is_some_and(is_internal_brain_path);
    if is_artifact {
        (
            ToolKind::Other,
            target.as_deref().map(path_basename),
            artifact_summary(args),
            None,
        )
    } else {
        (kind, target, None, diff_stat)
    }
}

fn multi_chunk_diff_stat(args: &Map<String, Value>) -> Option<DiffStat> {
    let chunks = args.get("ReplacementChunks")?.as_array()?;
    let mut stat = DiffStat::default();
    let mut any = false;
    for chunk in chunks {
        let removed = str_of(chunk, "TargetContent").map(line_count);
        let added = str_of(chunk, "ReplacementContent").map(line_count);
        if removed.is_some() || added.is_some() {
            any = true;
            stat.removed += removed.unwrap_or(0);
            stat.added += added.unwrap_or(0);
        }
    }
    any.then_some(stat)
}

fn map_tool(
    name: &str,
    args: &Map<String, Value>,
) -> (ToolKind, Option<String>, Option<String>, Option<DiffStat>) {
    match name {
        "read_file" => (ToolKind::FileRead, arg_string(args, "Path"), None, None),
        "view_file" | "view_file_outline" => (
            ToolKind::FileRead,
            arg_string(args, "AbsolutePath"),
            None,
            None,
        ),
        "view_code_item" => (ToolKind::FileRead, arg_string(args, "File"), None, None),
        "edit_file" => {
            let target = arg_string(args, "Path");
            let diff = match (arg_lines(args, "Target"), arg_lines(args, "Replacement")) {
                (Some(removed), Some(added)) => Some(DiffStat { added, removed }),
                _ => None,
            };
            apply_artifact_guard(args, ToolKind::FileEdit, target, diff)
        }
        "replace_file_content" => {
            let target = arg_string(args, "TargetFile");
            let diff = match (
                arg_lines(args, "TargetContent"),
                arg_lines(args, "ReplacementContent"),
            ) {
                (Some(removed), Some(added)) => Some(DiffStat { added, removed }),
                _ => None,
            };
            apply_artifact_guard(args, ToolKind::FileEdit, target, diff)
        }
        "multi_replace_file_content" => {
            let target = arg_string(args, "TargetFile");
            let diff = multi_chunk_diff_stat(args);
            apply_artifact_guard(args, ToolKind::FileEdit, target, diff)
        }
        "write_file" => {
            let target = arg_string(args, "Path");
            let diff = arg_lines(args, "Content").map(|added| DiffStat { added, removed: 0 });
            apply_artifact_guard(args, ToolKind::FileWrite, target, diff)
        }
        "write_to_file" => {
            let target = arg_string(args, "TargetFile");
            let diff = arg_lines(args, "CodeContent").map(|added| DiffStat { added, removed: 0 });
            apply_artifact_guard(args, ToolKind::FileWrite, target, diff)
        }
        "run_command" => (
            ToolKind::BashCmd,
            arg_string(args, "CommandLine"),
            arg_any_string(args, &["toolSummary", "toolAction"]),
            None,
        ),
        "search_web" => (ToolKind::WebSearch, arg_string(args, "query"), None, None),
        "read_url_content" => (
            ToolKind::WebFetch,
            arg_any_string(args, &["Url", "url"]),
            None,
            None,
        ),
        "invoke_subagent" => (
            ToolKind::Subagent,
            arg_string(args, "TypeName"),
            arg_any_string(args, &["Role", "Prompt"]),
            None,
        ),
        "browser_subagent" => (
            ToolKind::Subagent,
            arg_string(args, "TaskName"),
            arg_string(args, "Task"),
            None,
        ),
        "swarm" => {
            let action = arg_string(args, "action");
            if action.as_deref() == Some("add") {
                (
                    ToolKind::Subagent,
                    arg_string(args, "name"),
                    arg_any_string(args, &["brief", "title"]),
                    None,
                )
            } else {
                (ToolKind::Other, action, arg_string(args, "name"), None)
            }
        }
        "run_workflow" => (
            ToolKind::Subagent,
            arg_string(args, "ScriptPath"),
            arg_string(args, "Description"),
            None,
        ),
        "grep_search" | "GrepSearch" => (
            ToolKind::Other,
            arg_string(args, "Query"),
            arg_any_string(args, &["SearchPath", "toolSummary"]),
            None,
        ),
        "find_by_name" | "FindByName" => (
            ToolKind::Other,
            arg_string(args, "Pattern"),
            arg_string(args, "SearchDirectory"),
            None,
        ),
        "list_dir" => (
            ToolKind::Other,
            arg_string(args, "DirectoryPath"),
            None,
            None,
        ),
        n if n.starts_with("mcp_") || n.starts_with("mcp__") => {
            (ToolKind::Mcp, None, arg_string(args, "toolSummary"), None)
        }
        _ => (ToolKind::Other, arg_string(args, "toolSummary"), None, None),
    }
}

fn parse_exit_code(content: &str) -> Option<i64> {
    const MARKER: &str = "Command exited with code ";
    let pos = content.find(MARKER)? + MARKER.len();
    let rest = &content[pos..];
    let end = rest
        .char_indices()
        .take_while(|&(i, c)| c.is_ascii_digit() || (i == 0 && c == '-'))
        .last()
        .map(|(i, c)| i + c.len_utf8())?;
    rest[..end].parse::<i64>().ok()
}

fn command_output_body(content: &str) -> &str {
    const MARKER: &str = "Command output:";
    match content.find(MARKER) {
        Some(pos) => &content[pos + MARKER.len()..],
        None => content,
    }
}

fn decode_compact_value(val: Value) -> Value {
    let Value::String(s) = val else {
        return val;
    };
    if s.starts_with('"') || s.starts_with('{') || s.starts_with('[') {
        if let Ok(parsed) = serde_json::from_str::<Value>(&s) {
            return parsed;
        }
        if let Some(stripped) = strip_compact_truncation(&s) {
            if stripped.starts_with('"') {
                if let Some(decoded) = decode_truncated_json_string(stripped) {
                    return Value::String(decoded);
                }
            }
        }
    } else if s == "true" {
        return Value::Bool(true);
    } else if s == "false" {
        return Value::Bool(false);
    } else if s == "null" {
        return Value::Null;
    } else if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(n) = s.parse::<i64>() {
            return Value::Number(n.into());
        }
    }
    Value::String(s)
}

fn strip_compact_truncation(s: &str) -> Option<&str> {
    const SUFFIX: &str = " bytes>";
    const PREFIX: &str = "\n<truncated ";
    let before_suffix = s.strip_suffix(SUFFIX)?;
    let pos = before_suffix.rfind(PREFIX)?;
    let digits = &before_suffix[pos + PREFIX.len()..];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(&s[..pos])
}

fn decode_truncated_json_string(s: &str) -> Option<String> {
    let mut candidate = s;
    for _ in 0..8 {
        let wrapped = format!("{candidate}\"");
        if let Ok(decoded) = serde_json::from_str::<String>(&wrapped) {
            return Some(decoded);
        }
        let (idx, _) = candidate.char_indices().next_back()?;
        candidate = &candidate[..idx];
    }
    s.strip_prefix('"').map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peeks_step_head_and_guards_against_payload_fields() {
        let line = br#"{"step_index":4,"source":"MODEL","type":"GENERIC","status":"DONE","created_at":"2026-11-18T00:00:00Z","content":"hi"}"#;
        assert_eq!(
            peek_step_head(line),
            Some(StepHead {
                ty: "GENERIC",
                status: Some("DONE")
            })
        );
        let weird = br#"{"type":"GENERIC","step_index":4}"#;
        assert_eq!(peek_step_head(weird), None);

        let before_type =
            br#"{"step_index":1,"content":"\"type\":\"FAKE\"","type":"PLANNER_RESPONSE"}"#;
        assert_eq!(peek_step_head(before_type), None);
    }

    #[test]
    fn cleans_user_prompts() {
        let wrapped = "<USER_REQUEST>\nfix the parser\n</USER_REQUEST>\n<ADDITIONAL_METADATA>\nThe current local time is: 10:00\n</ADDITIONAL_METADATA>";
        assert_eq!(
            clean_user_prompt(wrapped, 0).as_deref(),
            Some("fix the parser")
        );

        let missing_close = "<USER_REQUEST>\nfirst line\n<ADDITIONAL_METADATA>\ntime: 10:00";
        assert_eq!(
            clean_user_prompt(missing_close, 1).as_deref(),
            Some("first line\n[image x1]")
        );

        let media_only = "<USER_REQUEST>\n   \n</USER_REQUEST>";
        assert_eq!(
            clean_user_prompt(media_only, 2).as_deref(),
            Some("[image x2]")
        );
        assert_eq!(clean_user_prompt("   ", 0), None);
    }

    #[test]
    fn parses_exit_codes_and_command_output() {
        let ok = "Created At: 1\nCompleted At: 2\nCommand exited with code 0.\n";
        assert_eq!(parse_exit_code(ok), Some(0));
        let fail =
            "Command exited with code 101.\n\nCommand output:\nerror[E0308]: mismatched types";
        assert_eq!(parse_exit_code(fail), Some(101));
        assert_eq!(
            command_output_body(fail).trim(),
            "error[E0308]: mismatched types"
        );
        assert_eq!(parse_exit_code("Command exited with code -9\n"), Some(-9));
        assert_eq!(parse_exit_code("no status line"), None);
    }

    #[test]
    fn decodes_compact_json_and_truncated_strings() {
        assert_eq!(
            decode_compact_value(Value::String("\"/work/app/a.rs\"".into())),
            Value::String("/work/app/a.rs".into())
        );
        assert_eq!(
            decode_compact_value(Value::String("{\"Summary\":\"Plan updated\"}".into())),
            serde_json::json!({"Summary": "Plan updated"})
        );
        assert_eq!(
            decode_compact_value(Value::String(
                "\"line1\\nline2\\\n<truncated 42 bytes>".into()
            )),
            Value::String("line1\nline2".into())
        );
        assert_eq!(
            decode_compact_value(Value::String(
                "\"line1\\nline2\n<truncated 42 bytes>".into()
            )),
            Value::String("line1\nline2".into())
        );
    }
}
