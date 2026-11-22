//! Lantai unified intermediate representation (IR): the event layer.
//!
//! Adapters translate each agent's raw records into a stream of [`IrEvent`]s; the core layer merges events into storage.
//! This crate does no IO and contains no agent-specific fields.

use std::borrow::Cow;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// IR version. Adding an optional field bumps the minor version; a breaking change bumps the major version.
pub const IR_VERSION: &str = "0.3.0";

/// The agent that produced a session: an open identifier, not a closed list, so data from agents
/// without an adapter can still be stored, counted and exported (docs/SPEC.md, "Agent
/// identifiers").
///
/// Lowercase ASCII starting with a letter, then letters, digits, `-`, `_` or `.`, at most 64
/// characters. Registered identifiers are in [`REGISTRY`]; others start with `x-` or are a
/// reverse domain name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AgentId(Cow<'static, str>);

impl AgentId {
    pub const CLAUDE: AgentId = AgentId(Cow::Borrowed("claude"));
    pub const CODEX: AgentId = AgentId(Cow::Borrowed("codex"));
    pub const ANTIGRAVITY: AgentId = AgentId(Cow::Borrowed("antigravity"));
    /// DeepSeek Harness.
    pub const DSH: AgentId = AgentId(Cow::Borrowed("dsh"));
    pub const WORKBUDDY: AgentId = AgentId(Cow::Borrowed("workbuddy"));
    pub const OPENCODE: AgentId = AgentId(Cow::Borrowed("opencode"));

    /// Checks the form; any well-formed identifier is accepted, registered or not.
    pub fn parse(s: &str) -> Result<Self, InvalidAgentId> {
        let mut chars = s.chars();
        let well_formed = s.len() <= 64
            && chars.next().is_some_and(|c| c.is_ascii_lowercase())
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_.".contains(c));
        if !well_formed {
            return Err(InvalidAgentId(s.to_string()));
        }
        Ok(REGISTRY
            .iter()
            .find(|r| r.id.as_str() == s)
            .map_or_else(|| AgentId(Cow::Owned(s.to_string())), |r| r.id.clone()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn registration(&self) -> Option<&'static Registration> {
        REGISTRY.iter().find(|r| r.id == *self)
    }

    /// The product name of a registered agent, or the identifier itself.
    pub fn product_name(&self) -> &str {
        self.registration().map_or(self.as_str(), |r| r.product)
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for AgentId {
    type Error = InvalidAgentId;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        AgentId::parse(&s)
    }
}

impl From<AgentId> for String {
    fn from(id: AgentId) -> String {
        id.0.into_owned()
    }
}

/// An agent identifier that is not well formed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidAgentId(pub String);

impl std::fmt::Display for InvalidAgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid agent identifier {:?}", self.0)
    }
}

impl std::error::Error for InvalidAgentId {}

/// A registered agent identifier.
#[derive(Debug)]
pub struct Registration {
    pub id: AgentId,
    /// Product name shown to users.
    pub product: &'static str,
    /// IR version in which the identifier was registered.
    pub since_ir: &'static str,
}

/// Registered agents, in display order. Identifiers are never renamed or reused.
pub const REGISTRY: [Registration; 6] = [
    Registration {
        id: AgentId::CLAUDE,
        product: "Claude Code",
        since_ir: "0.3.0",
    },
    Registration {
        id: AgentId::CODEX,
        product: "Codex CLI",
        since_ir: "0.3.0",
    },
    Registration {
        id: AgentId::ANTIGRAVITY,
        product: "Antigravity",
        since_ir: "0.3.0",
    },
    Registration {
        id: AgentId::DSH,
        product: "DeepSeek Harness",
        since_ir: "0.3.0",
    },
    Registration {
        id: AgentId::WORKBUDDY,
        product: "WorkBuddy",
        since_ir: "0.3.0",
    },
    Registration {
        id: AgentId::OPENCODE,
        product: "OpenCode",
        since_ir: "0.3.0",
    },
];

/// Header line opening a session in the exchange format (docs/SPEC.md, "Exchange format"); the
/// event lines after it belong to this session until the next header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record", rename = "session")]
pub struct SessionHeader {
    pub ir_version: String,
    pub agent: AgentId,
    pub session_id: String,
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
    fn agent_ids() {
        assert_eq!(AgentId::parse("claude").unwrap(), AgentId::CLAUDE);
        assert_eq!(AgentId::CLAUDE.product_name(), "Claude Code");
        let custom = AgentId::parse("x-mytool").unwrap();
        assert_eq!(custom.product_name(), "x-mytool");
        assert!(custom.registration().is_none());
        assert!(AgentId::parse("com.example.tool").is_ok());
        for bad in ["", "Claude", "1abc", "has space", "a/b", &"a".repeat(65)] {
            assert!(AgentId::parse(bad).is_err(), "{bad}");
        }
        assert!(REGISTRY
            .iter()
            .all(|r| AgentId::parse(r.id.as_str()).is_ok() && !r.id.as_str().contains('.')));
        let json = serde_json::to_string(&AgentId::WORKBUDDY).unwrap();
        assert_eq!(json, r#""workbuddy""#);
        assert!(serde_json::from_str::<AgentId>(r#""Bad""#).is_err());
    }

    #[test]
    fn session_header_shape() {
        let h = SessionHeader {
            ir_version: IR_VERSION.into(),
            agent: AgentId::CODEX,
            session_id: "s1".into(),
        };
        let s = serde_json::to_string(&h).unwrap();
        assert_eq!(
            s,
            format!(
                r#"{{"record":"session","ir_version":"{IR_VERSION}","agent":"codex","session_id":"s1"}}"#
            )
        );
        assert_eq!(serde_json::from_str::<SessionHeader>(&s).unwrap(), h);
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
