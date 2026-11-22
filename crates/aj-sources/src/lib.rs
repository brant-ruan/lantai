//! Where each registered agent keeps its sessions and how to build its adapter: the session
//! roots the shells (CLI, desktop) scan and the adapters they scan with.
//!
//! This is the only place that maps an [`AgentId`] to a concrete adapter, so adding an agent
//! means adding its adapter crate and one row to [`SOURCES`]. Identifiers and product names are
//! registered in `aj_ir`.

use std::path::{Path, PathBuf};

use aj_adapter::{CompatReason, SourceAdapter, SourceProbe};
use aj_adapter_antigravity::AntigravityAdapter;
use aj_adapter_claude::ClaudeAdapter;
use aj_adapter_codex::CodexAdapter;
use aj_adapter_dsh::DshAdapter;
use aj_adapter_opencode::OpenCodeAdapter;
use aj_adapter_workbuddy::WorkBuddyAdapter;
use aj_ir::AgentId;
use anyhow::{Context, Result};

/// One agent's session source on this machine.
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub agent: AgentId,
    /// Directories the agent writes sessions to (the first is the primary one).
    pub roots: Vec<PathBuf>,
}

impl SourceInfo {
    /// True when at least one of the session directories exists.
    pub fn detected(&self) -> bool {
        self.roots.iter().any(|r| r.is_dir())
    }
}

/// Builds an adapter from its session roots and Lantai's data directory.
type Build = fn(Vec<PathBuf>, &Path) -> Result<Box<dyn SourceAdapter>>;

/// How Lantai reads one registered agent.
struct Source {
    agent: AgentId,
    /// Environment variable whose value replaces all default roots.
    env: &'static str,
    default_roots: fn(home: &Path) -> Vec<PathBuf>,
    build: Build,
}

fn first(roots: Vec<PathBuf>, what: &str) -> Result<PathBuf> {
    roots
        .into_iter()
        .next()
        .with_context(|| format!("no {what} directory"))
}

/// Agents with an adapter, in display order.
const SOURCES: [Source; 6] = [
    Source {
        agent: AgentId::CLAUDE,
        env: "AJ_CLAUDE_ROOT",
        default_roots: |_| ClaudeAdapter::default_root().into_iter().collect(),
        build: |roots, _| {
            Ok(Box::new(ClaudeAdapter::new(first(
                roots,
                "Claude Code session",
            )?)))
        },
    },
    Source {
        agent: AgentId::CODEX,
        env: "AJ_CODEX_ROOT",
        default_roots: |home| {
            vec![
                home.join(".codex").join("sessions"),
                home.join(".codex").join("archived_sessions"),
            ]
        },
        build: |roots, _| Ok(Box::new(CodexAdapter::new(roots))),
    },
    Source {
        agent: AgentId::ANTIGRAVITY,
        env: "AJ_ANTIGRAVITY_ROOT",
        default_roots: |home| {
            vec![
                home.join(".gemini").join("antigravity").join("brain"),
                home.join(".gemini").join("antigravity-ide").join("brain"),
            ]
        },
        build: |roots, _| Ok(Box::new(AntigravityAdapter::new(roots))),
    },
    Source {
        agent: AgentId::DSH,
        env: "AJ_DSH_ROOT",
        default_roots: |home| vec![home.join(".dsh").join("sessions")],
        build: |roots, _| {
            Ok(Box::new(DshAdapter::new(first(
                roots,
                "DeepSeek Harness session",
            )?)))
        },
    },
    Source {
        agent: AgentId::WORKBUDDY,
        env: "AJ_WORKBUDDY_ROOT",
        default_roots: |home| vec![home.join(".workbuddy").join("projects")],
        build: |roots, _| {
            Ok(Box::new(WorkBuddyAdapter::new(first(
                roots,
                "WorkBuddy session",
            )?)))
        },
    },
    Source {
        agent: AgentId::OPENCODE,
        env: "AJ_OPENCODE_ROOT",
        // The directory holding `opencode.db`; OpenCode follows XDG on macOS too.
        default_roots: |home| match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
            Some(d) => vec![PathBuf::from(d).join("opencode")],
            None => vec![home.join(".local").join("share").join("opencode")],
        },
        // OpenCode's database is copied before it is opened; the copy lives under `cache/`.
        build: |roots, data_dir| {
            let cache = data_dir.join("cache").join("opencode");
            Ok(Box::new(OpenCodeAdapter::new(
                first(roots, "OpenCode data")?,
                cache,
            )))
        },
    },
];

/// Session directories of one source: its environment override, or the defaults.
fn roots(src: &Source) -> Vec<PathBuf> {
    match std::env::var_os(src.env) {
        Some(r) => vec![PathBuf::from(r)],
        None => (src.default_roots)(&dirs::home_dir().unwrap_or_default()),
    }
}

/// Every agent Lantai can read, in display order.
pub fn all_sources() -> Vec<SourceInfo> {
    SOURCES
        .iter()
        .map(|src| SourceInfo {
            agent: src.agent.clone(),
            roots: roots(src),
        })
        .collect()
}

/// Whether Lantai has an adapter for this agent.
pub fn is_supported(agent: &AgentId) -> bool {
    SOURCES.iter().any(|s| s.agent == *agent)
}

/// Adapters for the agents that `include` accepts. `data_dir` is Lantai's data directory;
/// adapters that must copy their source before reading it keep the copy under `cache/`.
pub fn build_adapters(
    data_dir: &Path,
    include: impl Fn(&AgentId) -> bool,
) -> Result<Vec<Box<dyn SourceAdapter>>> {
    SOURCES
        .iter()
        .filter(|src| include(&src.agent))
        .map(|src| (src.build)(roots(src), data_dir))
        .collect()
}

/// Compatibility of each agent's local data, for the settings page. A probe that fails (e.g. a
/// database being written) is reported in the note instead of failing the listing.
pub fn probe_all(data_dir: &Path) -> Result<Vec<(AgentId, SourceProbe)>> {
    Ok(build_adapters(data_dir, |_| true)?
        .iter()
        .map(|a| {
            let probe = a.probe().unwrap_or_else(|e| SourceProbe {
                reason: Some(CompatReason::ProbeFailed(format!("{e:#}"))),
                ..Default::default()
            });
            (a.agent(), probe)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_source_is_registered_and_builds() {
        let all = all_sources();
        assert_eq!(all.len(), SOURCES.len());
        assert!(all.iter().all(|s| !s.roots.is_empty()));
        assert!(all.iter().all(|s| s.agent.registration().is_some()));
        let built = build_adapters(Path::new("/nonexistent"), |_| true).unwrap();
        assert_eq!(built.len(), all.len());
        assert!(built.iter().zip(&all).all(|(a, s)| a.agent() == s.agent));
        assert!(build_adapters(Path::new("/nonexistent"), |_| false)
            .unwrap()
            .is_empty());
        assert!(!is_supported(&AgentId::parse("x-mytool").unwrap()));
    }
}
