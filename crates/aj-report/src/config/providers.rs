//! Providers in `config.toml`: connections to an API or an agent CLI, and their keys in the
//! key store.

use std::path::Path;

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use super::{clean, AppConfig, ModelConfig};
use crate::engine::{ClaudeCliEngine, CodexCliEngine, ModelInfo};
use crate::local_tools::{ToolDescriptor, ToolEngine};
use crate::openai::ApiKey;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderConfig {
    ClaudeCli {
        #[serde(skip_serializing_if = "Option::is_none")]
        binary: Option<String>,
    },
    CodexCli {
        #[serde(skip_serializing_if = "Option::is_none")]
        binary: Option<String>,
    },
    OpenaiCompat {
        /// Like `https://host/v1`, without `/chat/completions`.
        base_url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_secs: Option<u64>,
    },
}

/// What to do with the stored key when a provider is saved.
#[derive(Debug, Clone)]
pub enum KeyUpdate {
    Keep,
    Set(String),
    Clear,
}

impl ProviderConfig {
    /// The `type` written in `config.toml`, also what the app calls the kind.
    pub fn kind(&self) -> &'static str {
        match self {
            ProviderConfig::ClaudeCli { .. } => "claude_cli",
            ProviderConfig::CodexCli { .. } => "codex_cli",
            ProviderConfig::OpenaiCompat { .. } => "openai_compat",
        }
    }

    /// A provider of `kind` from the fields of a settings form; fields the kind does not use
    /// are dropped.
    pub fn from_parts(
        kind: &str,
        base_url: Option<String>,
        timeout_secs: Option<u64>,
        binary: Option<String>,
    ) -> Result<Self> {
        Ok(match kind {
            "claude_cli" => ProviderConfig::ClaudeCli { binary },
            "codex_cli" => ProviderConfig::CodexCli { binary },
            "openai_compat" => ProviderConfig::OpenaiCompat {
                base_url: base_url.unwrap_or_default(),
                timeout_secs,
            },
            other => bail!("unknown provider type: {other}"),
        })
    }

    /// For an agent CLI, the command it runs: the configured path, else the command name.
    pub fn cli_command(&self) -> Option<&str> {
        match self {
            ProviderConfig::ClaudeCli { binary } => Some(binary.as_deref().unwrap_or("claude")),
            ProviderConfig::CodexCli { binary } => Some(binary.as_deref().unwrap_or("codex")),
            ProviderConfig::OpenaiCompat { .. } => None,
        }
    }

    /// For an agent CLI, the path configured for it, if any.
    pub fn cli_binary(&self) -> Option<&str> {
        match self {
            ProviderConfig::ClaudeCli { binary } | ProviderConfig::CodexCli { binary } => {
                binary.as_deref()
            }
            ProviderConfig::OpenaiCompat { .. } => None,
        }
    }

    fn normalized(self) -> Result<Self> {
        Ok(match self {
            ProviderConfig::ClaudeCli { binary } => ProviderConfig::ClaudeCli {
                binary: clean(binary),
            },
            ProviderConfig::CodexCli { binary } => ProviderConfig::CodexCli {
                binary: clean(binary),
            },
            ProviderConfig::OpenaiCompat {
                base_url,
                timeout_secs,
            } => {
                let base_url = base_url.trim().trim_end_matches('/').to_string();
                if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
                    bail!("base URL must start with http:// or https://");
                }
                if timeout_secs == Some(0) {
                    bail!("timeout must be at least 1 second");
                }
                ProviderConfig::OpenaiCompat {
                    base_url,
                    timeout_secs,
                }
            }
        })
    }
}

/// Provider names are Keychain accounts and `security` script tokens, so they
/// stay plain identifiers.
pub fn validate_provider_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 40 {
        bail!("provider name must be 1-40 characters");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("provider name may only contain letters, digits, '-' and '_'");
    }
    Ok(())
}

impl AppConfig {
    /// Add or replace a provider; `original` is its previous name (`None` creates one).
    /// Renaming moves its stored key and repoints the models that use it.
    pub fn upsert_provider(
        &mut self,
        original: Option<&str>,
        name: &str,
        provider: ProviderConfig,
        key: KeyUpdate,
    ) -> Result<()> {
        let name = name.trim();
        validate_provider_name(name)?;
        let provider = provider.normalized()?;
        if original != Some(name) && self.providers.contains_key(name) {
            bail!("a provider named {name} already exists");
        }
        if let Some(old) = original {
            if !self.providers.contains_key(old) {
                bail!("no provider named {old}");
            }
        }
        if let Some(old) = original.filter(|o| *o != name) {
            self.providers.remove(old);
            self.keystore.rename(old, name)?;
            for m in self.models.values_mut().filter(|m| m.provider == old) {
                m.provider = name.to_string();
            }
        }
        let is_cli = provider.cli_command().is_some();
        self.providers.insert(name.to_string(), provider);
        match key {
            _ if is_cli => self.keystore.remove(name)?,
            KeyUpdate::Keep => {}
            KeyUpdate::Set(k) => self.keystore.set(name, &k)?,
            KeyUpdate::Clear => self.keystore.remove(name)?,
        }
        Ok(())
    }

    /// Remove a provider and its stored key. Fails while a model alias still uses it.
    pub fn remove_provider(&mut self, name: &str) -> Result<()> {
        let users: Vec<&str> = self
            .models
            .iter()
            .filter(|(_, m)| m.provider == name)
            .map(|(alias, _)| alias.as_str())
            .collect();
        if !users.is_empty() {
            bail!(
                "provider {name} is still used by model(s): {}",
                users.join(", ")
            );
        }
        if self.providers.remove(name).is_none() {
            bail!("no provider named {name}");
        }
        self.keystore.remove(name)
    }

    /// Whether a key is stored for the named provider.
    pub fn has_key(&self, provider: &str) -> bool {
        matches!(
            self.providers.get(provider),
            Some(ProviderConfig::OpenaiCompat { .. })
        ) && self.keystore.has(provider)
    }

    /// The key to send to the named provider. An OpenAI-compatible provider
    /// without a stored key is called without authentication (local models).
    pub fn api_key(&self, provider: &str) -> ApiKey {
        match self.providers.get(provider) {
            Some(ProviderConfig::OpenaiCompat { .. }) => match self.keystore.get(provider) {
                Ok(Some(k)) => ApiKey::Value(k),
                Ok(None) => ApiKey::None,
                Err(e) => ApiKey::Missing(format!(
                    "could not read the key for provider {provider}: {e:#}"
                )),
            },
            _ => ApiKey::None,
        }
    }

    /// List the models a provider offers. `draft` describes a provider still
    /// being edited (not yet saved); `typed_key` is a key typed but not saved.
    /// Without a typed key, the saved provider's key (if `saved_name`) is used.
    pub fn list_provider_models(
        &self,
        saved_name: Option<&str>,
        draft: ProviderConfig,
        typed_key: Option<&str>,
    ) -> Result<Vec<ModelInfo>> {
        let draft = draft.normalized()?;
        match &draft {
            ProviderConfig::ClaudeCli { .. } => Ok(ClaudeCliEngine::models()),
            ProviderConfig::CodexCli { .. } => {
                CodexCliEngine::models(draft.cli_command().unwrap_or("codex"))
            }
            ProviderConfig::OpenaiCompat { base_url, .. } => {
                let key = match typed_key.map(str::trim).filter(|k| !k.is_empty()) {
                    Some(k) => ApiKey::Value(k.to_string()),
                    None => saved_name.map_or(ApiKey::None, |n| self.api_key(n)),
                };
                crate::openai::list_models(base_url, &key)
            }
        }
    }

    /// Add a local agent CLI found on this machine: a provider named after its agent identifier
    /// (with a numeric suffix if taken) running `binary`, and a model alias named after the
    /// product. Returns the alias. A tool already configured with the same command is not added
    /// twice; its existing alias is returned.
    pub fn add_local_tool(&mut self, tool: &ToolDescriptor, binary: &Path) -> Result<String> {
        let Some(engine) = tool.engine else {
            bail!("{} cannot generate reports yet", tool.agent.product_name());
        };
        let binary = binary.to_string_lossy().into_owned();
        let provider = match engine {
            ToolEngine::ClaudeCli => ProviderConfig::ClaudeCli {
                binary: Some(binary),
            },
            ToolEngine::CodexCli => ProviderConfig::CodexCli {
                binary: Some(binary),
            },
        };
        if let Some((name, _)) = self.providers.iter().find(|(_, p)| **p == provider) {
            if let Some((alias, _)) = self.models.iter().find(|(_, m)| m.provider == *name) {
                return Ok(alias.clone());
            }
        }
        let name = free_name(tool.agent.as_str(), |n| self.providers.contains_key(n));
        self.upsert_provider(None, &name, provider, KeyUpdate::Keep)?;
        let alias = free_name(tool.agent.product_name(), |a| self.models.contains_key(a));
        self.upsert_model(
            None,
            &alias,
            ModelConfig {
                provider: name,
                model: None,
                reasoning_effort: None,
                temperature: None,
            },
            &[],
        )?;
        Ok(alias)
    }
}

/// `base`, or `base-2`, `base-3`, … (`base 2` for aliases with spaces) — the first not `taken`.
fn free_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    let sep = if base.contains(' ') { " " } else { "-" };
    std::iter::once(base.to_string())
        .chain((2..).map(|i| format!("{base}{sep}{i}")))
        .find(|n| !taken(n))
        .expect("an unbounded sequence has a free name")
}
