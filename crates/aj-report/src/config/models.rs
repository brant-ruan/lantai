//! Model aliases in `config.toml`: which provider, model and options each one uses, the
//! default alias, and the engine an alias builds.

use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use super::{clean, AppConfig, ProviderConfig};
use crate::diagnose;
use crate::engine::{ClaudeCliEngine, CodexCliEngine, Engine, ModelInfo, NoModelEngine};
use crate::openai::{ApiKey, OpenAiCompatEngine};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    pub provider: String,
    /// Model id at the provider; required except for the Claude CLI (its default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Valid values are defined by the server or CLI (e.g. `none`, `low`, `high`, `max`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    /// Sampling temperature, 0-2; only OpenAI-compatible providers take one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
}

/// Highest temperature accepted.
pub const MAX_TEMPERATURE: f64 = 2.0;

/// Check a model alias's options against its provider: an API needs a model ID, a temperature
/// is 0-2 and only for APIs, and the reasoning effort must be one `listed` gives (the models from
/// [`AppConfig::effort_reference`]; empty skips the check).
pub(super) fn validate_model(
    model: &ModelConfig,
    provider: &ProviderConfig,
    listed: &[ModelInfo],
) -> Result<()> {
    if let Some(t) = model.temperature {
        if !matches!(provider, ProviderConfig::OpenaiCompat { .. }) {
            bail!(
                "provider {} is an agent CLI, which has no temperature setting",
                model.provider
            );
        }
        if !(0.0..=MAX_TEMPERATURE).contains(&t) {
            bail!("temperature must be between 0 and {MAX_TEMPERATURE}, got {t}");
        }
    }
    if matches!(provider, ProviderConfig::OpenaiCompat { .. }) && model.model.is_none() {
        bail!("choose a model id for provider {}", model.provider)
    }
    if let Some(effort) = model.reasoning_effort.as_deref() {
        check_listed_effort(listed, model.model.as_deref(), effort)?;
    }
    Ok(())
}

/// Whether `effort` is one the model takes according to `listed`: the model's own efforts when
/// it is listed with some, else any listed model's (the provider's default model, or one the
/// list does not show). Nothing to compare against passes.
fn check_listed_effort(listed: &[ModelInfo], model: Option<&str>, effort: &str) -> Result<()> {
    let own = model
        .and_then(|id| listed.iter().find(|m| m.id == id))
        .filter(|m| !m.efforts.is_empty());
    let accepted: Vec<&str> = match own {
        Some(m) => m.efforts.iter().map(String::as_str).collect(),
        None => {
            let mut all: Vec<&str> = Vec::new();
            for e in listed.iter().flat_map(|m| &m.efforts) {
                if !all.contains(&e.as_str()) {
                    all.push(e);
                }
            }
            all
        }
    };
    if accepted.is_empty() || accepted.contains(&effort) {
        return Ok(());
    }
    match own {
        Some(m) => bail!(
            "model {} takes reasoning effort {}, not {effort}",
            m.id,
            accepted.join(", ")
        ),
        None => bail!(
            "reasoning effort is one of {}, not {effort}",
            accepted.join(", ")
        ),
    }
}

/// The models an alias's effort is checked against without running anything: Claude Code's
/// fixed list; nothing for other providers.
pub(super) fn known_models(provider: &ProviderConfig) -> Vec<ModelInfo> {
    match provider {
        ProviderConfig::ClaudeCli { .. } => ClaudeCliEngine::models(),
        _ => Vec::new(),
    }
}

/// Model aliases are display names chosen by the user (quoted TOML keys), so
/// any text works as long as it is non-empty, single-line and reasonably short.
pub fn validate_model_alias(alias: &str) -> Result<()> {
    if alias.is_empty() {
        bail!("model name must not be empty");
    }
    if alias.chars().count() > 80 {
        bail!("model name must be at most 80 characters");
    }
    if alias.chars().any(char::is_control) {
        bail!("model name must not contain control characters");
    }
    Ok(())
}

impl AppConfig {
    /// The alias set as default, else the first alias by name; `None` without any alias.
    pub fn default_model_name(&self) -> Option<&str> {
        self.default_model
            .as_deref()
            .or_else(|| self.models.keys().next().map(String::as_str))
    }

    /// The models to check `model`'s reasoning effort against before saving it: for Codex, the
    /// models `codex debug models` lists (this runs Codex, so call it outside any lock); Claude
    /// Code's fixed list; nothing for APIs, whose lists rarely give efforts. Empty when there is
    /// no effort to check or Codex cannot list its models, so the effort is saved as typed.
    pub fn effort_reference(&self, model: &ModelConfig) -> Vec<ModelInfo> {
        let Some(provider) = self.providers.get(model.provider.trim()) else {
            return Vec::new();
        };
        if clean(model.reasoning_effort.clone()).is_none() {
            return Vec::new();
        }
        match provider {
            ProviderConfig::CodexCli { .. } => {
                CodexCliEngine::models(provider.cli_command().unwrap_or("codex"))
                    .unwrap_or_default()
            }
            _ => known_models(provider),
        }
    }

    /// Add or replace a model alias; `original` is its previous alias (`None` creates one).
    /// `listed` comes from [`Self::effort_reference`] and checks the alias's reasoning effort.
    pub fn upsert_model(
        &mut self,
        original: Option<&str>,
        alias: &str,
        model: ModelConfig,
        listed: &[ModelInfo],
    ) -> Result<()> {
        let alias = alias.trim();
        validate_model_alias(alias)?;
        let model = ModelConfig {
            provider: model.provider.trim().to_string(),
            model: clean(model.model),
            reasoning_effort: clean(model.reasoning_effort),
            temperature: model.temperature,
        };
        let Some(provider) = self.providers.get(&model.provider) else {
            bail!("no provider named {}", model.provider);
        };
        validate_model(&model, provider, listed)?;
        if original != Some(alias) && self.models.contains_key(alias) {
            bail!("a model named {alias} already exists");
        }
        if let Some(old) = original.filter(|o| *o != alias) {
            if self.models.remove(old).is_none() {
                bail!("no model named {old}");
            }
            if self.default_model.as_deref() == Some(old) {
                self.default_model = Some(alias.to_string());
            }
        }
        // The first alias becomes the default, so later additions do not take its place.
        if self.default_model.is_none() && self.models.is_empty() {
            self.default_model = Some(alias.to_string());
        }
        self.models.insert(alias.to_string(), model);
        Ok(())
    }

    pub fn remove_model(&mut self, alias: &str) -> Result<()> {
        if self.models.remove(alias).is_none() {
            bail!("no model named {alias}");
        }
        if self.default_model.as_deref() == Some(alias) {
            self.default_model = None;
        }
        Ok(())
    }

    pub fn set_default_model(&mut self, alias: &str) -> Result<()> {
        if !self.models.contains_key(alias) {
            bail!("no model named {alias}");
        }
        self.default_model = Some(alias.to_string());
        Ok(())
    }

    /// One-line description of a model alias; never contains a key.
    pub fn describe_model(&self, alias: &str) -> String {
        let Some(m) = self.models.get(alias) else {
            return format!("unknown model {alias}");
        };
        let model = m.model.as_deref().unwrap_or("default");
        let effort = m.reasoning_effort.as_deref().unwrap_or("default");
        match self.providers.get(&m.provider) {
            Some(ProviderConfig::OpenaiCompat { base_url, .. }) => {
                let temperature = m
                    .temperature
                    .map_or_else(|| "default".to_string(), |t| t.to_string());
                let key = if self.has_key(&m.provider) {
                    "stored"
                } else {
                    "none"
                };
                format!(
                    "{} ({base_url})  model {model}  reasoning effort {effort}  temperature {temperature}  key {key}",
                    m.provider,
                )
            }
            Some(p) => format!(
                "{} ({}, command {})  model {model}  reasoning effort {effort}",
                m.provider,
                p.kind(),
                p.cli_command().unwrap_or_default(),
            ),
            None => format!("{} (missing provider)", m.provider),
        }
    }

    /// The engine for estimates and dry runs: [`Self::build_engine`], except that with no model
    /// configured and none picked it is [`NoModelEngine`], so a period's state still loads and
    /// nothing counts as cached (which is what generating would find).
    pub fn estimate_engine(
        &self,
        alias: Option<&str>,
        runner_dir: &Path,
    ) -> Result<Box<dyn Engine>> {
        if alias.is_none() && self.default_model_name().is_none() {
            return Ok(Box::new(NoModelEngine));
        }
        self.build_engine(alias, runner_dir)
    }

    /// Build the engine for a model alias (the default alias when `None`).
    pub fn build_engine(&self, alias: Option<&str>, runner_dir: &Path) -> Result<Box<dyn Engine>> {
        self.engine_for(alias, runner_dir, |provider| self.api_key(provider))
    }

    /// The aliases whose engine is named `engine_name` (as stored with digests, briefs and report
    /// versions), in alias order. Keys are not read. Empty when no alias produces that name any
    /// more.
    pub fn aliases_for_engine(&self, engine_name: &str) -> Vec<String> {
        self.models
            .keys()
            .filter(|alias| {
                self.engine_for(Some(alias), Path::new(""), |_| ApiKey::None)
                    .is_ok_and(|e| e.name() == engine_name)
            })
            .cloned()
            .collect()
    }

    /// The engine of `alias` (else the default), with its key from `api_key`.
    fn engine_for(
        &self,
        alias: Option<&str>,
        runner_dir: &Path,
        api_key: impl Fn(&str) -> ApiKey,
    ) -> Result<Box<dyn Engine>> {
        let Some(alias) = alias.or_else(|| self.default_model_name()) else {
            bail!(
                "{}; add one in Settings → Models (or run `aj models --find`)",
                diagnose::NO_MODEL
            );
        };
        let Some(m) = self.models.get(alias) else {
            bail!(
                "no model named {alias}; available: {}",
                self.models.keys().cloned().collect::<Vec<_>>().join(", ")
            );
        };
        let Some(provider) = self.providers.get(&m.provider) else {
            bail!("model {alias} uses unknown provider {}", m.provider);
        };
        Ok(match provider {
            ProviderConfig::ClaudeCli { .. } => Box::new(ClaudeCliEngine {
                binary: provider.cli_command().unwrap_or_default().to_string(),
                model: m.model.clone(),
                effort: m.reasoning_effort.clone(),
                runner_dir: runner_dir.to_path_buf(),
            }),
            ProviderConfig::CodexCli { .. } => Box::new(CodexCliEngine::new(
                provider.cli_command().unwrap_or_default().to_string(),
                m.model.clone(),
                m.reasoning_effort.clone(),
                runner_dir.to_path_buf(),
            )),
            ProviderConfig::OpenaiCompat {
                base_url,
                timeout_secs,
                ..
            } => Box::new(OpenAiCompatEngine {
                base_url: base_url.clone(),
                api_key: api_key(&m.provider),
                model: m.model.clone().unwrap_or_default(),
                reasoning_effort: m.reasoning_effort.clone(),
                temperature: m.temperature,
                timeout: Duration::from_secs(timeout_secs.unwrap_or(300)),
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effort_is_checked_against_listed_models() {
        let info = |id: &str, efforts: &[&str]| ModelInfo {
            id: id.into(),
            efforts: efforts.iter().map(|e| e.to_string()).collect(),
            default_effort: None,
        };
        let listed = [
            info("old", &["low", "medium", "high"]),
            info("new", &["low", "high", "ultra"]),
            info("bare", &[]),
        ];
        // A listed model takes only its own efforts.
        assert!(check_listed_effort(&listed, Some("new"), "ultra").is_ok());
        let err = check_listed_effort(&listed, Some("old"), "ultra").unwrap_err();
        assert_eq!(
            err.to_string(),
            "model old takes reasoning effort low, medium, high, not ultra"
        );
        // The default model, an unlisted model or one listed without efforts: any listed effort.
        assert!(check_listed_effort(&listed, None, "medium").is_ok());
        assert!(check_listed_effort(&listed, Some("custom"), "ultra").is_ok());
        assert!(check_listed_effort(&listed, Some("bare"), "high").is_ok());
        assert!(check_listed_effort(&listed, None, "bogus").is_err());
        // Nothing to compare against: taken as typed.
        assert!(check_listed_effort(&[], Some("old"), "bogus").is_ok());
    }
}
