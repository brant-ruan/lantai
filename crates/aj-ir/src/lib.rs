//! Lantai unified intermediate representation (IR): the event layer.
//!
//! Adapters translate each agent's raw records into a stream of [`IrEvent`]s; the core layer merges events into storage.
//! This crate does no IO and contains no agent-specific fields.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// IR version. Adding an optional field bumps the minor version; a breaking change bumps the major version.
pub const IR_VERSION: &str = "0.2.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    Claude,
    Codex,
    Antigravity,
    /// DeepSeek Harness.
    Dsh,
    WorkBuddy,
    OpenCode,
}

impl AgentKind {
    /// Every agent kind, in display order.
    pub const ALL: [AgentKind; 6] = [
        AgentKind::Claude,
        AgentKind::Codex,
        AgentKind::Antigravity,
        AgentKind::Dsh,
        AgentKind::WorkBuddy,
        AgentKind::OpenCode,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AgentKind::Claude => "claude",
            AgentKind::Codex => "codex",
            AgentKind::Antigravity => "antigravity",
            AgentKind::Dsh => "dsh",
            AgentKind::WorkBuddy => "workbuddy",
            AgentKind::OpenCode => "opencode",
        }
    }

    /// Product name shown to users (sessions list, slice details, diagnostics).
    pub fn display_name(self) -> &'static str {
        match self {
            AgentKind::Claude => "Claude Code",
            AgentKind::Codex => "Codex CLI",
            AgentKind::Antigravity => "Antigravity",
            AgentKind::Dsh => "DeepSeek Harness",
            AgentKind::WorkBuddy => "WorkBuddy",
            AgentKind::OpenCode => "OpenCode",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "claude" => Some(AgentKind::Claude),
            "codex" => Some(AgentKind::Codex),
            "antigravity" => Some(AgentKind::Antigravity),
            "dsh" => Some(AgentKind::Dsh),
            "workbuddy" => Some(AgentKind::WorkBuddy),
            "opencode" => Some(AgentKind::OpenCode),
            _ => None,
        }
    }
}

/// Normalized tool category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    FileRead,
    FileEdit,
    FileWrite,
    BashCmd,
    WebSearch,
    WebFetch,
    Subagent,
    Mcp,
    Other,
}

impl ToolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolKind::FileRead => "file_read",
            ToolKind::FileEdit => "file_edit",
            ToolKind::FileWrite => "file_write",
            ToolKind::BashCmd => "bash_cmd",
            ToolKind::WebSearch => "web_search",
            ToolKind::WebFetch => "web_fetch",
            ToolKind::Subagent => "subagent",
            ToolKind::Mcp => "mcp",
            ToolKind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "file_read" => ToolKind::FileRead,
            "file_edit" => ToolKind::FileEdit,
            "file_write" => ToolKind::FileWrite,
            "bash_cmd" => ToolKind::BashCmd,
            "web_search" => ToolKind::WebSearch,
            "web_fetch" => ToolKind::WebFetch,
            "subagent" => ToolKind::Subagent,
            "mcp" => ToolKind::Mcp,
            _ => ToolKind::Other,
        }
    }

    /// Whether this kind of tool modifies files.
    pub fn modifies_files(self) -> bool {
        matches!(self, ToolKind::FileEdit | ToolKind::FileWrite)
    }
}

/// Lines added and removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DiffStat {
    pub added: u32,
    pub removed: u32,
}

/// Tokens of one model call, as four non-overlapping counts.
///
/// `input_tokens` excludes cache hits and cache writes; `output_tokens` includes reasoning.
/// Adapters convert source semantics (e.g. OpenAI-style input that includes cached tokens).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl Usage {
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_read_tokens + self.cache_write_tokens
    }

    pub fn is_zero(&self) -> bool {
        self.total() == 0
    }
}

/// Event layer: the output of an adapter.
///
/// Events are emitted in source-file order. `source_event_id` is a stable ID from the source,
/// used for deduplication (ingesting the same event twice must be idempotent) and for traceability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum IrEvent {
    /// Session metadata. May repeat; later non-empty fields override earlier values.
    SessionMeta {
        cwd: Option<String>,
        git_branch: Option<String>,
        agent_version: Option<String>,
        parent_session_id: Option<String>,
    },
    /// Session title given explicitly by the source.
    SessionTitle { title: String },
    /// A human input, which starts a new turn.
    TurnStart {
        source_event_id: String,
        timestamp: DateTime<Utc>,
        user_prompt: String,
    },
    /// A piece of agent text output, belonging to the current turn.
    AgentMessage {
        source_event_id: String,
        timestamp: DateTime<Utc>,
        text: String,
    },
    /// Token usage of one model call, belonging to the current turn. The same call may be
    /// reported more than once; `source_event_id` is stable across batches so storage deduplicates.
    TokenUsage {
        source_event_id: String,
        timestamp: DateTime<Utc>,
        model: Option<String>,
        usage: Usage,
    },
    /// A tool call, belonging to the current turn. Its result arrives in a later [`IrEvent::ToolOutcome`].
    ToolAction {
        /// Tool call ID; [`IrEvent::ToolOutcome`] refers to it.
        source_event_id: String,
        timestamp: DateTime<Utc>,
        kind: ToolKind,
        /// Tool name as the source calls it, for display only.
        tool_name: String,
        /// What the tool acted on: a file path, command, URL, query, etc.
        target: Option<String>,
        /// Extra description (e.g. the purpose of a command, the task of a subagent).
        summary: Option<String>,
        diff_stat: Option<DiffStat>,
    },
    /// Result of a tool call. May be parsed in a different batch from its ToolAction.
    ToolOutcome {
        tool_source_id: String,
        is_error: bool,
        diff_stat: Option<DiffStat>,
        /// Truncated summary of the output.
        output_summary: Option<String>,
    },
    /// Unrecognized record type: counted, never an error.
    Unknown { raw_type: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_roundtrip() {
        let e = IrEvent::ToolAction {
            source_event_id: "t1".into(),
            timestamp: "2026-11-16T01:02:03Z".parse().unwrap(),
            kind: ToolKind::FileEdit,
            tool_name: "Edit".into(),
            target: Some("/a/b.rs".into()),
            summary: None,
            diff_stat: Some(DiffStat {
                added: 3,
                removed: 1,
            }),
        };
        let s = serde_json::to_string(&e).unwrap();
        assert!(s.contains(r#""event":"tool_action""#));
        assert!(s.contains(r#""kind":"file_edit""#));
        assert_eq!(serde_json::from_str::<IrEvent>(&s).unwrap(), e);
    }

    #[test]
    fn kind_str_roundtrip() {
        for k in [
            ToolKind::FileRead,
            ToolKind::FileEdit,
            ToolKind::FileWrite,
            ToolKind::BashCmd,
            ToolKind::WebSearch,
            ToolKind::WebFetch,
            ToolKind::Subagent,
            ToolKind::Mcp,
            ToolKind::Other,
        ] {
            assert_eq!(ToolKind::parse(k.as_str()), k);
        }
    }
}
