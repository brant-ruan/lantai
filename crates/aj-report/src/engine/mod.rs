//! Engines: hand a prompt to a model and get text back. The trait and what every engine shares
//! live here; each agent CLI has its own module with its invocation and model list.

mod claude_cli;
mod codex_cli;
#[cfg(test)]
mod tool_audit;

use anyhow::{bail, Result};
use serde::Serialize;

use crate::diagnose;

pub use claude_cli::{ClaudeCliEngine, CLAUDE_EFFORTS, CLAUDE_MODELS};
pub use codex_cli::CodexCliEngine;

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

/// A model a provider offers, as listed in the settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    /// Reasoning effort values the model accepts; empty when unknown.
    pub efforts: Vec<String>,
    /// The effort used when none is set, when known.
    pub default_effort: Option<String>,
}

impl ModelInfo {
    /// A model known only by its ID.
    pub fn id_only(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            efforts: Vec::new(),
            default_effort: None,
        }
    }
}

/// An engine's name for cache keys: `kind:model`, followed by each option that is set, so an
/// alias without options keeps the name (and the cached digests) it always had.
pub(crate) fn engine_name(kind: &str, model: Option<&str>, options: &[Option<String>]) -> String {
    let mut name = format!("{kind}:{}", model.unwrap_or("default"));
    for option in options.iter().flatten() {
        name.push(':');
        name.push_str(option);
    }
    name
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
        name: String,
    }

    impl MockEngine {
        pub fn new(respond: impl Fn(&str) -> Result<String> + Sync + 'static) -> Self {
            Self {
                prompts: Mutex::new(vec![]),
                respond: Box::new(respond),
                name: "mock".into(),
            }
        }

        /// The same engine under another name, as if another model answered.
        pub fn named(mut self, name: &str) -> Self {
            self.name = name.into();
            self
        }

        pub fn call_count(&self) -> usize {
            self.prompts.lock().unwrap().len()
        }
    }

    impl Engine for MockEngine {
        fn name(&self) -> String {
            self.name.clone()
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
