//! Generates through the local `codex` CLI (`codex exec`), reusing an existing sign-in.
//!
//! Codex has no switch that turns every tool off, so the call turns off each feature that adds
//! one and leaves only declarations that cannot act (`docs/SPEC.md`, "Agent CLIs"):
//!
//! - The prompt goes through stdin; `--ephemeral` keeps the call from leaving a session.
//! - `--disable` and `--strict-config` fail on an unknown name, so an update that renames a
//!   feature stops generation instead of quietly bringing a tool back.
//! - A read-only sandbox and `approval_policy = "never"` stop anything left from writing.
//! - `model_instructions_file` replaces Codex's built-in instructions with Lantai's system prompt.
//! - User configuration and execution-policy rules are ignored; it runs in a dedicated empty
//!   directory.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde_json::Value;

use super::{engine_name, Engine, ModelInfo};
use crate::{process, prompts};

/// Features that add tools (or instructions about them), turned off on every call.
pub const DISABLED_FEATURES: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "view_image",
    "multi_agent",
    "multi_agent_v2",
    "goals",
    "sleep_tool",
    "apps",
    "plugins",
    "remote_plugin",
    "browser_use",
    "browser_use_external",
    "computer_use",
    "in_app_browser",
    "image_generation",
    "tool_suggest",
    "skill_search",
    "skill_mcp_dependency_install",
    "code_mode_host",
    "hooks",
    "workspace_dependencies",
    "memories",
];

/// Configuration overrides on every call (`-c key=value`, values in TOML).
pub const CONFIG_OVERRIDES: &[&str] = &[
    "approval_policy=\"never\"",
    "web_search=\"disabled\"",
    "agents.enabled=false",
    "tools.update_plan.enabled=false",
    "tools.experimental_request_user_input.enabled=false",
    "include_permissions_instructions=false",
    "include_collaboration_mode_instructions=false",
    "include_apps_instructions=false",
];

/// File in the runner directory holding the system prompt Codex reads.
pub(super) const INSTRUCTIONS_FILE: &str = "codex-instructions.md";

/// How long listing models may take.
const LIST_TIMEOUT: Duration = Duration::from_secs(20);

pub struct CodexCliEngine {
    pub binary: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub runner_dir: PathBuf,
    /// More `-c` overrides after the fixed ones; the tool audit points Codex at a stand-in server.
    pub(crate) extra_config: Vec<String>,
}

impl CodexCliEngine {
    pub fn new(
        binary: String,
        model: Option<String>,
        effort: Option<String>,
        runner_dir: PathBuf,
    ) -> Self {
        Self {
            binary,
            model,
            effort,
            runner_dir,
            extra_config: Vec::new(),
        }
    }

    /// The models Codex offers (`codex debug models`, those listed in its picker), each with the
    /// reasoning effort values it accepts.
    pub fn models(binary: &str) -> Result<Vec<ModelInfo>> {
        let mut cmd = Command::new(binary);
        cmd.args(["debug", "models"])
            .current_dir(std::env::temp_dir());
        let output =
            process::run(&mut cmd, None, Some(LIST_TIMEOUT), "Codex CLI")?.with_context(|| {
                format!(
                    "`codex debug models` did not answer within {} seconds",
                    LIST_TIMEOUT.as_secs()
                )
            })?;
        if !output.status.success() {
            bail!(
                "`codex debug models` exited with code {:?}: {}",
                output.status.code(),
                error_lines(&output.stderr_text())
            );
        }
        parse_models(&output.stdout_text())
    }

    pub(super) fn command(&self, instructions: &Path) -> Command {
        let mut cmd = Command::new(&self.binary);
        cmd.current_dir(&self.runner_dir).args([
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "--ignore-user-config",
            "--ignore-rules",
            "--strict-config",
            "--color",
            "never",
            "--sandbox",
            "read-only",
        ]);
        cmd.arg("--cd").arg(&self.runner_dir);
        for feature in DISABLED_FEATURES {
            cmd.args(["--disable", feature]);
        }
        for value in CONFIG_OVERRIDES {
            cmd.args(["-c", value]);
        }
        cmd.arg("-c").arg(format!(
            "model_instructions_file={}",
            toml_string(&instructions.to_string_lossy())
        ));
        if let Some(effort) = &self.effort {
            cmd.arg("-c")
                .arg(format!("model_reasoning_effort={}", toml_string(effort)));
        }
        for value in &self.extra_config {
            cmd.args(["-c", value]);
        }
        if let Some(model) = &self.model {
            cmd.args(["--model", model]);
        }
        // `-` reads the prompt from stdin.
        cmd.arg("-");
        cmd
    }
}

impl Engine for CodexCliEngine {
    fn name(&self) -> String {
        engine_name(
            "codex-cli",
            self.model.as_deref(),
            std::slice::from_ref(&self.effort),
        )
    }

    fn send(&self, prompt: &str) -> Result<String> {
        std::fs::create_dir_all(&self.runner_dir)
            .with_context(|| format!("failed to create {}", self.runner_dir.display()))?;
        let instructions = self.runner_dir.join(INSTRUCTIONS_FILE);
        write_if_changed(&instructions, prompts::SYSTEM.trim())?;
        let output = process::run(
            &mut self.command(&instructions),
            Some(prompt.as_bytes()),
            None,
            "Codex CLI",
        )?
        .context("codex did not finish")?;
        if !output.status.success() {
            // stderr echoes the prompt, so only its error lines go into the message.
            bail!(
                "codex exited with code {:?}: {}",
                output.status.code(),
                error_lines(&output.stderr_text())
            );
        }
        let text = output.stdout_text().trim().to_string();
        if text.is_empty() {
            bail!(
                "codex returned empty content: {}",
                error_lines(&output.stderr_text())
            );
        }
        Ok(text)
    }
}

/// A TOML basic string, for `-c` values.
fn toml_string(s: &str) -> String {
    toml::Value::String(s.to_string()).to_string()
}

/// Write `content` to `path` unless it is already there. Writes go through a temporary file and
/// a rename, so a parallel call never reads a half-written file.
fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|c| c == content) {
        return Ok(());
    }
    let tmp = path.with_extension(format!(
        "tmp-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&tmp, content).with_context(|| format!("failed to write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("failed to write {}", path.display()))
}

/// The lines of Codex's stderr that report errors; the last few lines when none do. Codex
/// echoes the prompt on stderr, which must not end up in error messages.
fn error_lines(stderr: &str) -> String {
    let errors: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| {
            let lower = l.to_ascii_lowercase();
            lower.starts_with("error") || lower.starts_with("fatal")
        })
        .collect();
    if !errors.is_empty() {
        return errors.join("\n");
    }
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    lines[lines.len().saturating_sub(3)..].join("\n")
}

/// Parse `codex debug models`: `{"models": [{"slug", "visibility", "supported_reasoning_levels":
/// [{"effort"}], "default_reasoning_level"}]}`. Hidden models are skipped.
fn parse_models(json: &str) -> Result<Vec<ModelInfo>> {
    let value: Value =
        serde_json::from_str(json).context("`codex debug models` did not print JSON")?;
    let models = value["models"]
        .as_array()
        .context("`codex debug models` printed no model list")?;
    Ok(models
        .iter()
        .filter(|m| m["visibility"].as_str().is_none_or(|v| v == "list"))
        .filter_map(|m| {
            let id = m["slug"].as_str()?.to_string();
            let efforts = m["supported_reasoning_levels"]
                .as_array()
                .map(|levels| {
                    levels
                        .iter()
                        .filter_map(|l| l["effort"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let default_effort = m["default_reasoning_level"].as_str().map(str::to_string);
            Some(ModelInfo {
                id,
                efforts,
                default_effort,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn command_turns_tools_off_and_reads_stdin() {
        let engine = CodexCliEngine::new(
            "codex".into(),
            Some("m1".into()),
            Some("high".into()),
            PathBuf::from("/tmp/runner"),
        );
        let a = args(&engine.command(Path::new("/tmp/runner/i \"x\".md")));
        for flag in ["--ephemeral", "--ignore-user-config", "--strict-config"] {
            assert!(a.iter().any(|x| x == flag), "{flag} in {a:?}");
        }
        assert!(a.windows(2).any(|w| w == ["--sandbox", "read-only"]));
        assert!(a.windows(2).any(|w| w == ["--disable", "shell_tool"]));
        assert!(a.windows(2).any(|w| w == ["-c", "web_search=\"disabled\""]));
        let file = a
            .iter()
            .find(|x| x.starts_with("model_instructions_file="))
            .unwrap();
        let parsed: toml::Table = toml::from_str(file).unwrap();
        assert_eq!(
            parsed["model_instructions_file"].as_str(),
            Some("/tmp/runner/i \"x\".md")
        );
        assert!(a
            .windows(2)
            .any(|w| w == ["-c", "model_reasoning_effort=\"high\""]));
        assert!(a.windows(2).any(|w| w == ["--model", "m1"]));
        assert_eq!(a.last().map(String::as_str), Some("-"));
        assert_eq!(engine.name(), "codex-cli:m1:high");

        let plain = CodexCliEngine::new("codex".into(), None, None, PathBuf::from("/r"));
        let a = args(&plain.command(Path::new("/r/i.md")));
        assert!(!a
            .iter()
            .any(|x| x == "--model" || x.starts_with("model_reasoning")));
        assert_eq!(plain.name(), "codex-cli:default");
    }

    #[test]
    fn parses_listed_models_with_efforts() {
        let json = r#"{"models": [
            {"slug": "a", "visibility": "list", "default_reasoning_level": "low",
             "supported_reasoning_levels": [{"effort": "low"}, {"effort": "high"}]},
            {"slug": "hidden", "visibility": "hide"},
            {"slug": "b"}
        ]}"#;
        let models = parse_models(json).unwrap();
        assert_eq!(
            models,
            vec![
                ModelInfo {
                    id: "a".into(),
                    efforts: vec!["low".into(), "high".into()],
                    default_effort: Some("low".into()),
                },
                ModelInfo::id_only("b"),
            ]
        );
        assert!(parse_models("not json").is_err());
        assert!(parse_models("{}").is_err());
    }

    #[test]
    fn error_text_leaves_the_echoed_prompt_out() {
        let stderr = "OpenAI Codex v0.1\n--------\nuser\nsecret session text\n\nERROR: You've hit your usage limit\n";
        assert_eq!(error_lines(stderr), "ERROR: You've hit your usage limit");
        assert_eq!(
            error_lines("Error: Unknown feature flag: x\n"),
            "Error: Unknown feature flag: x"
        );
        assert_eq!(error_lines("a\nb\nc\nd\n"), "b\nc\nd");
    }

    #[test]
    fn instructions_file_is_written_once() {
        let dir = std::env::temp_dir().join(format!("aj-codex-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(INSTRUCTIONS_FILE);
        write_if_changed(&path, "one").unwrap();
        write_if_changed(&path, "one").unwrap();
        write_if_changed(&path, "two").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
