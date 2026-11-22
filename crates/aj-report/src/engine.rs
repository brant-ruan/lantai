//! Engines: hand a prompt to a model and get text back.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::{diagnose, prompts};

pub trait Engine: Sync {
    /// Engine identifier (including the model); part of the cache hash.
    fn name(&self) -> String;

    /// Send a prompt that [`Engine::complete`] has already redacted. Implement this; call
    /// `complete`.
    fn send(&self, prompt: &str) -> Result<String>;

    /// Redact the whole prompt and send it. This is the one gate every model call passes, so
    /// text that was never redacted on its way in (notes, project names and descriptions, user
    /// prompts) cannot reach a model as it is. Redaction is idempotent, so text redacted before
    /// storage is unchanged.
    fn complete(&self, prompt: &str) -> Result<String> {
        self.send(&aj_core::redact::redact(prompt))
    }
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

    fn send(&self, prompt: &str) -> Result<String> {
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
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => anyhow::anyhow!(
                    "command `{}` {}; is Claude Code still installed there?",
                    self.binary,
                    diagnose::COMMAND_NOT_FOUND
                ),
                _ => anyhow::Error::new(e).context(format!("failed to start `{}`", self.binary)),
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

/// Stands in when no model is configured, for work that only needs an engine's name (cache
/// estimates, dry runs). Its empty name matches no cached digest; it cannot complete.
pub struct NoModelEngine;

impl Engine for NoModelEngine {
    fn name(&self) -> String {
        String::new()
    }

    fn send(&self, _prompt: &str) -> Result<String> {
        bail!("{}", diagnose::NO_MODEL)
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

        fn send(&self, prompt: &str) -> Result<String> {
            self.prompts.lock().unwrap().push(prompt.to_string());
            (self.respond)(prompt)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::MockEngine;
    use super::Engine;

    #[test]
    fn complete_redacts_the_whole_prompt() {
        let engine = MockEngine::new(|_| Ok("ok".into()));
        engine
            .complete(
                "User note: DB_PASSWORD=hunter2hunter2\n- \"app\" — key sk-abcdefghijklmnopqrstu\n",
            )
            .unwrap();
        let sent = engine.prompts.lock().unwrap()[0].clone();
        assert!(
            !sent.contains("hunter2") && !sent.contains("sk-abcdef"),
            "{sent}"
        );
        assert!(sent.contains("User note: DB_PASSWORD=[REDACTED]"), "{sent}");
    }
}
