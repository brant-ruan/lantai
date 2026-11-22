//! Registry of the coding agents Lantai knows about: where each one keeps its
//! session files, whether an adapter exists for it yet, and construction of the
//! adapters the shells (CLI, desktop) scan with.
//!
//! This is the only place that maps an [`AgentKind`] to a concrete adapter, so
//! adding an agent means adding its adapter crate and one arm here.

use std::path::{Path, PathBuf};

use aj_adapter::{CompatReason, SourceAdapter, SourceProbe};
use aj_adapter_antigravity::AntigravityAdapter;
use aj_adapter_claude::ClaudeAdapter;
use aj_adapter_codex::CodexAdapter;
use aj_adapter_dsh::DshAdapter;
use aj_adapter_opencode::OpenCodeAdapter;
use aj_adapter_workbuddy::WorkBuddyAdapter;
use aj_ir::AgentKind;
use anyhow::{Context, Result};

/// One agent's session source on this machine.
#[derive(Debug, Clone)]
pub struct SourceInfo {
    pub kind: AgentKind,
    /// Directories the agent writes sessions to (the first is the primary one).
    pub roots: Vec<PathBuf>,
    /// Whether Lantai has an adapter for this agent yet.
    pub supported: bool,
}

impl SourceInfo {
    /// True when at least one of the session directories exists.
    pub fn detected(&self) -> bool {
        self.roots.iter().any(|r| r.is_dir())
    }
}

/// Session directories per agent. `AJ_CLAUDE_ROOT`, `AJ_CODEX_ROOT`, `AJ_ANTIGRAVITY_ROOT`, `AJ_DSH_ROOT`, `AJ_WORKBUDDY_ROOT`, and `AJ_OPENCODE_ROOT` override
/// the default roots (a Codex or Antigravity override replaces the multiple default directories).
fn roots(kind: AgentKind) -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    match kind {
        AgentKind::Claude => match std::env::var_os("AJ_CLAUDE_ROOT") {
            Some(r) => vec![PathBuf::from(r)],
            None => ClaudeAdapter::default_root().into_iter().collect(),
        },
        AgentKind::Codex => match std::env::var_os("AJ_CODEX_ROOT") {
            Some(r) => vec![PathBuf::from(r)],
            None => vec![
                home.join(".codex").join("sessions"),
                home.join(".codex").join("archived_sessions"),
            ],
        },
        AgentKind::Antigravity => match std::env::var_os("AJ_ANTIGRAVITY_ROOT") {
            Some(r) => vec![PathBuf::from(r)],
            None => vec![
                home.join(".gemini").join("antigravity").join("brain"),
                home.join(".gemini").join("antigravity-ide").join("brain"),
            ],
        },
        AgentKind::Dsh => match std::env::var_os("AJ_DSH_ROOT") {
            Some(r) => vec![PathBuf::from(r)],
            None => vec![home.join(".dsh").join("sessions")],
        },
        AgentKind::WorkBuddy => match std::env::var_os("AJ_WORKBUDDY_ROOT") {
            Some(r) => vec![PathBuf::from(r)],
            None => vec![home.join(".workbuddy").join("projects")],
        },
        // The directory holding `opencode.db`; OpenCode follows XDG on macOS too.
        AgentKind::OpenCode => match std::env::var_os("AJ_OPENCODE_ROOT") {
            Some(r) => vec![PathBuf::from(r)],
            None => match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
                Some(d) => vec![PathBuf::from(d).join("opencode")],
                None => vec![home.join(".local").join("share").join("opencode")],
            },
        },
    }
}

fn supported(kind: AgentKind) -> bool {
    matches!(
        kind,
        AgentKind::Claude
            | AgentKind::Codex
            | AgentKind::Antigravity
            | AgentKind::Dsh
            | AgentKind::WorkBuddy
            | AgentKind::OpenCode
    )
}

/// Every known agent, in display order.
pub fn all_sources() -> Vec<SourceInfo> {
    AgentKind::ALL
        .iter()
        .map(|&kind| SourceInfo {
            kind,
            roots: roots(kind),
            supported: supported(kind),
        })
        .collect()
}

/// Adapters for the supported agents that `include` accepts. `data_dir` is Lantai's data
/// directory; adapters that must copy their source before reading it keep the copy under `cache/`.
pub fn build_adapters(
    data_dir: &Path,
    include: impl Fn(AgentKind) -> bool,
) -> Result<Vec<Box<dyn SourceAdapter>>> {
    let mut out: Vec<Box<dyn SourceAdapter>> = Vec::new();
    for src in all_sources() {
        if !src.supported || !include(src.kind) {
            continue;
        }
        match src.kind {
            AgentKind::Claude => {
                let root = src
                    .roots
                    .into_iter()
                    .next()
                    .context("no Claude Code session directory")?;
                out.push(Box::new(ClaudeAdapter::new(root)));
            }
            AgentKind::Codex => out.push(Box::new(CodexAdapter::new(src.roots))),
            AgentKind::Antigravity => out.push(Box::new(AntigravityAdapter::new(src.roots))),
            AgentKind::Dsh => {
                let root = src
                    .roots
                    .into_iter()
                    .next()
                    .context("no DeepSeek Harness session directory")?;
                out.push(Box::new(DshAdapter::new(root)));
            }
            AgentKind::WorkBuddy => {
                let root = src
                    .roots
                    .into_iter()
                    .next()
                    .context("no WorkBuddy session directory")?;
                out.push(Box::new(WorkBuddyAdapter::new(root)));
            }
            AgentKind::OpenCode => {
                let root = src
                    .roots
                    .into_iter()
                    .next()
                    .context("no OpenCode data directory")?;
                let cache = data_dir.join("cache").join("opencode");
                out.push(Box::new(OpenCodeAdapter::new(root, cache)));
            }
        }
    }
    Ok(out)
}

/// Compatibility of each supported agent's local data, for the settings page. A probe that
/// fails (e.g. a database being written) is reported in the note instead of failing the listing.
pub fn probe_all(data_dir: &Path) -> Result<Vec<(AgentKind, SourceProbe)>> {
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
    fn lists_every_agent_and_builds_only_supported_ones() {
        let all = all_sources();
        assert_eq!(all.len(), AgentKind::ALL.len());
        assert!(all.iter().all(|s| !s.roots.is_empty()));
        let built = build_adapters(Path::new("/nonexistent"), |_| true).unwrap();
        assert_eq!(built.len(), all.iter().filter(|s| s.supported).count());
        assert!(build_adapters(Path::new("/nonexistent"), |_| false)
            .unwrap()
            .is_empty());
    }
}
