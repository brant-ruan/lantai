//! Engines: hand a prompt to a model and get text back.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::prompts;

pub trait Engine: Sync {
    /// Engine identifier (including the model); part of the cache hash.
    fn name(&self) -> String;
    fn complete(&self, prompt: &str) -> Result<String>;
}

/// Generates through the local `claude` CLI, reusing an existing subscription.
///
/// - The prompt goes through stdin and never appears in command-line arguments.
/// - `--tools ""` disables every built-in tool, so session content is data only.
/// - `--no-session-persistence` keeps the call off disk, so our own scanner never reads it.
/// - It runs in a dedicated empty directory, unaffected by any project configuration.
pub struct ClaudeCliEngine {
    pub binary: String,
    pub model: Option<String>,
    pub runner_dir: PathBuf,
}

impl Engine for ClaudeCliEngine {
    fn name(&self) -> String {
        format!("claude-cli:{}", self.model.as_deref().unwrap_or("default"))
    }

    fn complete(&self, prompt: &str) -> Result<String> {
        std::fs::create_dir_all(&self.runner_dir)
            .with_context(|| format!("failed to create {}", self.runner_dir.display()))?;
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
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| {
                format!(
                    "failed to start `{}`; make sure the Claude Code CLI is installed",
                    self.binary
                )
            })?;

        // Write stdin on a separate thread so a large prompt cannot deadlock against the child's output.
        let mut stdin = child
            .stdin
            .take()
            .context("failed to get the child's stdin")?;
        let output = std::thread::scope(|scope| {
            scope.spawn(move || {
                let _ = stdin.write_all(prompt.as_bytes());
            });
            child.wait_with_output()
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!(
                "claude exited with code {:?}: {}",
                output.status.code(),
                format!("{stderr}{stdout}").trim()
            );
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            bail!("claude returned empty content");
        }
        Ok(text)
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use std::sync::Mutex;

    type Responder = Box<dyn Fn(&str) -> Result<String> + Sync>;

    /// Test engine: records the prompts it receives and answers through a closure.
    pub struct MockEngine {
        pub prompts: Mutex<Vec<String>>,
        pub respond: Responder,
    }

    impl MockEngine {
        pub fn new(respond: impl Fn(&str) -> Result<String> + Sync + 'static) -> Self {
            Self {
                prompts: Mutex::new(vec![]),
                respond: Box::new(respond),
            }
        }

        pub fn call_count(&self) -> usize {
            self.prompts.lock().unwrap().len()
        }
    }

    impl Engine for MockEngine {
        fn name(&self) -> String {
            "mock".into()
        }

        fn complete(&self, prompt: &str) -> Result<String> {
            self.prompts.lock().unwrap().push(prompt.to_string());
            (self.respond)(prompt)
        }
    }
}
