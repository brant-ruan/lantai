//! Agent source commands: which coding agents are known, whether their session
//! directories exist on this machine, and the per-agent enable / pre-digest
//! switches stored in `config.toml`.

use aj_adapter::{CompatReason, Compatibility, SourceProbe};
use aj_ir::AgentId;
use aj_report::{AgentSettings, AppConfig};
use anyhow::bail;
use serde::Serialize;

use crate::error::CmdResult;
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRootDto {
    pub path: String,
    pub exists: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSourceDto {
    pub agent: String,
    pub name: String,
    pub roots: Vec<SourceRootDto>,
    pub detected: bool,
    pub enabled: bool,
    pub pre_digest: bool,
    /// `unchecked` | `supported` | `partial` | `unsupported`: whether the local data's format
    /// (agent version) is one the adapter reads.
    pub compatibility: &'static str,
    /// Agent versions seen in the local data.
    pub versions: Vec<String>,
    pub compat_reason: Option<CompatReasonDto>,
    /// Sessions already in the database. They stay visible when the agent is disabled; a
    /// disabled agent just stops being scanned, so they are no longer updated.
    pub stored_sessions: u64,
}

/// Why the local data is not fully read; the frontend words it in the interface language.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CompatReasonDto {
    NewerFormat,
    SessionsInNewerFormat { count: u64 },
    UntestedVersions { versions: Vec<String> },
    UnrecognizedLayout,
    ProbeFailed { message: String },
}

impl From<CompatReason> for CompatReasonDto {
    fn from(r: CompatReason) -> Self {
        match r {
            CompatReason::NewerFormat => Self::NewerFormat,
            CompatReason::SessionsInNewerFormat(count) => Self::SessionsInNewerFormat { count },
            CompatReason::UntestedVersions(versions) => Self::UntestedVersions { versions },
            CompatReason::UnrecognizedLayout => Self::UnrecognizedLayout,
            CompatReason::ProbeFailed(message) => Self::ProbeFailed { message },
        }
    }
}

fn compatibility_str(c: Compatibility) -> &'static str {
    match c {
        Compatibility::Unchecked => "unchecked",
        Compatibility::Supported => "supported",
        Compatibility::Partial => "partial",
        Compatibility::Unsupported => "unsupported",
    }
}

fn sources_dto(state: &AppState, cfg: &AppConfig) -> CmdResult<Vec<AgentSourceDto>> {
    let probes = aj_sources::probe_all(&state.data_dir)?;
    let stored = state
        .open_store()
        .and_then(|s| s.session_counts_by_agent())?;
    Ok(aj_sources::all_sources()
        .into_iter()
        .map(|src| {
            let settings = cfg.agent(&src.agent);
            let probe = probes
                .iter()
                .find(|(k, _)| *k == src.agent)
                .map(|(_, p)| p.clone())
                .unwrap_or_else(SourceProbe::default);
            AgentSourceDto {
                agent: src.agent.as_str().to_string(),
                name: src.agent.product_name().to_string(),
                detected: src.detected(),
                roots: src
                    .roots
                    .iter()
                    .map(|r| SourceRootDto {
                        path: r.to_string_lossy().into_owned(),
                        exists: r.is_dir(),
                    })
                    .collect(),
                enabled: settings.enabled,
                pre_digest: settings.enabled && settings.pre_digest,
                compatibility: compatibility_str(probe.compatibility),
                versions: probe.versions,
                compat_reason: probe.reason.map(Into::into),
                stored_sessions: stored
                    .iter()
                    .find(|(k, _)| *k == src.agent)
                    .map_or(0, |(_, n)| *n),
            }
        })
        .collect())
}

// Probing reads (and for OpenCode copies) local data, so these run off the main thread.
#[tauri::command(async)]
pub fn agent_sources(state: tauri::State<'_, AppState>) -> CmdResult<Vec<AgentSourceDto>> {
    let cfg = state.load_config()?;
    sources_dto(&state, &cfg)
}

#[tauri::command(async)]
pub fn set_agent_settings(
    state: tauri::State<'_, AppState>,
    agent: String,
    enabled: bool,
    pre_digest: bool,
) -> CmdResult<Vec<AgentSourceDto>> {
    let cfg = state.modify_config(|cfg| {
        let kind = AgentId::parse(&agent)?;
        if !aj_sources::is_supported(&kind) {
            bail!("Lantai cannot read {} yet", kind.product_name());
        }
        cfg.set_agent(
            kind,
            AgentSettings {
                enabled,
                pre_digest,
            },
        );
        Ok(())
    })?;
    sources_dto(&state, &cfg)
}
