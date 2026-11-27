//! Generates through the local `claude` CLI, reusing an existing subscription.
//!
//! - The prompt goes through stdin and never appears in command-line arguments.
//! - `--tools ""` disables every built-in tool, so session content is data only.
//! - `--no-session-persistence` keeps the call off disk, so our own scanner never reads it.
//! - It runs in a dedicated empty directory, unaffected by any project configuration.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};

use super::{engine_name, Engine, ModelInfo};
use crate::{process, prompts};

/// Shorthands `--model` accepts for the latest model of each family; a full model name works too.
pub const CLAUDE_MODELS: &[&str] = &["fable", "opus", "sonnet", "haiku"];

/// Values `--effort` accepts.
pub const CLAUDE_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh", "max"];

pub struct ClaudeCliEngine {
    pub binary: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub runner_dir: PathBuf,
}

impl ClaudeCliEngine {
    /// The models offered in the settings: the shorthands, each with every effort value.
    pub fn models() -> Vec<ModelInfo> {
        CLAUDE_MODELS
            .iter()
            .map(|id| ModelInfo {
                id: id.to_string(),
                efforts: CLAUDE_EFFORTS.iter().map(|e| e.to_string()).collect(),
                default_effort: None,
            })
            .collect()
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(&self.binary);
        cmd.current_dir(&self.runner_dir)
            .args([
                "-p",
                "--tools",
                "",
                "--no-session-persistence",
                "--output-format",
                "text",
            ])
            .args(["--system-prompt", prompts::SYSTEM.trim()]);
        if let Some(model) = &self.model {
            cmd.args(["--model", model]);
        }
        if let Some(effort) = &self.effort {
            cmd.args(["--effort", effort]);
        }
        cmd
    }
}

impl Engine for ClaudeCliEngine {
    fn name(&self) -> String {
        engine_name(
            "claude-cli",
            self.model.as_deref(),
            std::slice::from_ref(&self.effort),
        )
    }

    fn send(&self, prompt: &str) -> Result<String> {
        std::fs::create_dir_all(&self.runner_dir)
            .with_context(|| format!("failed to create {}", self.runner_dir.display()))?;
        let output = process::run(
            &mut self.command(),
            Some(prompt.as_bytes()),
            None,
            "Claude Code",
        )?
        .context("claude did not finish")?;
        if !output.status.success() {
            bail!(
                "claude exited with code {:?}: {}",
                output.status.code(),
                format!("{}{}", output.stderr_text(), output.stdout_text()).trim()
            );
        }
        let text = output.stdout_text().trim().to_string();
        if text.is_empty() {
            bail!("claude returned empty content");
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(model: Option<&str>, effort: Option<&str>) -> ClaudeCliEngine {
        ClaudeCliEngine {
            binary: "claude".into(),
            model: model.map(str::to_string),
            effort: effort.map(str::to_string),
            runner_dir: PathBuf::from("/tmp/runner"),
        }
    }

    fn args(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn options_appear_only_when_set() {
        let plain = engine(None, None);
        assert_eq!(plain.name(), "claude-cli:default");
        let a = args(&plain.command());
        assert!(a.windows(2).any(|w| w == ["--tools", ""]), "{a:?}");
        assert!(!a.iter().any(|x| x == "--model" || x == "--effort"));

        let set = engine(Some("opus"), Some("high"));
        assert_eq!(set.name(), "claude-cli:opus:high");
        let a = args(&set.command());
        assert!(a.windows(2).any(|w| w == ["--model", "opus"]), "{a:?}");
        assert!(a.windows(2).any(|w| w == ["--effort", "high"]), "{a:?}");
    }
}
