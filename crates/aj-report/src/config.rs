//! Generation configuration: providers, model aliases and budgets in
//! `<data dir>/config.toml`; API keys in a [`KeyStore`].
//!
//! ```toml
//! default_model = "flash"          # alias used when none is chosen explicitly
//!
//! [providers.gateway]              # a connection
//! type = "openai_compat"
//! base_url = "https://example.com/v1"
//! timeout_secs = 300               # optional
//!
//! [providers.claude]               # built in; always present
//! type = "claude_cli"
//! # binary = "/path/to/claude"
//!
//! [models.flash]                   # an alias the user picks in the app or with --model
//! provider = "gateway"
//! model = "model-a"
//! reasoning_effort = "high"
//!
//! [models.claude]                  # built in; the CLI's own default model
//! provider = "claude"
//!
//! [agents.claude]      # optional; which session sources to use
//! enabled = true       # scan this agent's sessions (default: only Claude Code)
//! pre_digest = false   # summarize its pending slices in the background
//!
//! [slicing]            # optional
//! day_boundary_hour = 0  # local hour a new day starts; earlier activity counts towards the previous day
//!
//! [report]             # optional
//! language = "zh-Hans" # report language (a tag from `lang::LANGUAGES`); chosen on first start
//!
//! [ui]                 # optional
//! language = "auto"    # interface language: "auto" follows the system, or a language tag
//! check_updates = false # look for a new release on GitHub on start and once a day
//!
//! [budgets]            # optional
//! chunk_chars = 40000  # longest record in one digest request (characters)
//! stage2_chars = 60000 # most material in one report request (characters)
//! quiet_minutes = 20   # a slice active more recently than this is live (not pre-digested)
//! max_incremental_updates = 3  # incremental digest updates before a full digest
//! batch_concurrency = 2        # concurrent daily reports generated in a sidebar batch
//! ```
//!
//! A model alias resolves to an [`Engine`]. Keys never appear in `config.toml`;
//! a provider without a stored key is called without authentication. Edits are in-memory except
//! for key changes, which go to the key store immediately; [`AppConfig::save`]
//! writes `config.toml`.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use aj_ir::AgentKind;

use crate::engine::{ClaudeCliEngine, Engine};
use crate::keystore::KeyStore;
use crate::lang::{self, Language};
use crate::openai::{ApiKey, OpenAiCompatEngine};
use crate::summarize::Budgets;

/// Name of the built-in provider and of the built-in model alias that uses it.
pub const BUILTIN: &str = "claude";

/// Model ids the Claude CLI accepts as `--model` shorthands.
pub const CLAUDE_CLI_MODELS: &[&str] = &["haiku", "opus", "sonnet"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderConfig>,
    #[serde(default)]
    pub models: BTreeMap<String, ModelConfig>,
    /// Per-agent source settings, keyed by [`AgentKind::as_str`].
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub agents: BTreeMap<String, AgentSettings>,
    /// Size limits; tune them to the context window of the model in use.
    #[serde(default)]
    pub budgets: BudgetConfig,
    /// How activity is cut into days.
    #[serde(default)]
    pub slicing: SlicingConfig,
    /// What reports are written in.
    #[serde(default, skip_serializing_if = "ReportConfig::is_empty")]
    pub report: ReportConfig,
    /// The desktop app's own settings.
    #[serde(default, skip_serializing_if = "UiConfig::is_empty")]
    pub ui: UiConfig,
    #[serde(skip, default = "unset_keystore")]
    keystore: KeyStore,
}

fn unset_keystore() -> KeyStore {
    KeyStore::File(std::path::PathBuf::new())
}

/// Whether an agent's sessions are scanned and pre-digested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSettings {
    pub enabled: bool,
    #[serde(default)]
    pub pre_digest: bool,
}

impl AgentSettings {
    /// Claude Code is on by default; other agents are opt-in.
    pub fn default_for(kind: AgentKind) -> Self {
        Self {
            enabled: kind == AgentKind::Claude,
            pre_digest: false,
        }
    }
}

/// Latest local hour allowed as the day boundary.
pub const MAX_DAY_BOUNDARY_HOUR: u32 = 12;

/// Optional overrides of the default slicing parameters in [`aj_core::Config`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlicingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day_boundary_hour: Option<u32>,
}

/// Report settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportConfig {
    /// Report language tag; unset until chosen (see [`AppConfig::report_language`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

impl ReportConfig {
    fn is_empty(&self) -> bool {
        self.language.is_none()
    }
}

/// Interface language value that follows the system.
pub const UI_LANGUAGE_AUTO: &str = "auto";

/// Desktop app settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiConfig {
    /// `"auto"` or a language tag the interface is translated into (the app decides which
    /// tags it knows; an unknown one shows English).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Look for a new release on start and once a day (off unless turned on).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub check_updates: bool,
}

impl UiConfig {
    fn is_empty(&self) -> bool {
        self.language.is_none() && !self.check_updates
    }
}

/// Optional overrides of the default [`Budgets`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage2_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quiet_minutes: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_incremental_updates: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_concurrency: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderConfig {
    ClaudeCli {
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    pub provider: String,
    /// Model id at the provider; required except for the Claude CLI (its default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Valid values are defined by the server (e.g. `none`, `low`, `high`, `max`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

/// What to do with the stored key when a provider is saved.
#[derive(Debug, Clone)]
pub enum KeyUpdate {
    Keep,
    Set(String),
    Clear,
}

fn clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

impl ProviderConfig {
    fn normalized(self) -> Result<Self> {
        Ok(match self {
            ProviderConfig::ClaudeCli { binary } => ProviderConfig::ClaudeCli {
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
    /// Load the configuration. A missing file yields only the built-in provider and model.
    pub fn load(path: &Path, keystore: KeyStore) -> Result<Self> {
        let mut cfg = match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str::<AppConfig>(&text)
                .with_context(|| format!("failed to parse {}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => AppConfig {
                default_model: None,
                providers: BTreeMap::new(),
                models: BTreeMap::new(),
                agents: BTreeMap::new(),
                budgets: BudgetConfig::default(),
                slicing: SlicingConfig::default(),
                report: ReportConfig::default(),
                ui: UiConfig::default(),
                keystore: unset_keystore(),
            },
            Err(e) => return Err(e).with_context(|| format!("failed to read {}", path.display())),
        };
        cfg.keystore = keystore;
        cfg.providers
            .entry(BUILTIN.to_string())
            .or_insert(ProviderConfig::ClaudeCli { binary: None });
        cfg.models
            .entry(BUILTIN.to_string())
            .or_insert(ModelConfig {
                provider: BUILTIN.to_string(),
                model: None,
                reasoning_effort: None,
            });
        if let Some(h) = cfg.slicing.day_boundary_hour {
            if h > MAX_DAY_BOUNDARY_HOUR {
                bail!(
                    "day_boundary_hour must be 0-{MAX_DAY_BOUNDARY_HOUR}, got {h} in {}",
                    path.display()
                );
            }
        }
        if let Some(tag) = &cfg.report.language {
            Language::get(tag)
                .with_context(|| format!("invalid [report] language in {}", path.display()))?;
        }
        if cfg
            .ui
            .language
            .as_deref()
            .is_some_and(|l| l.trim().is_empty())
        {
            bail!("[ui] language must not be empty in {}", path.display());
        }
        if let Some(unknown) = cfg.agents.keys().find(|k| AgentKind::parse(k).is_none()) {
            bail!("unknown agent [agents.{unknown}] in {}", path.display());
        }
        for (alias, m) in &cfg.models {
            if !cfg.providers.contains_key(&m.provider) {
                bail!(
                    "model {alias} uses provider {} which is not defined in {}",
                    m.provider,
                    path.display()
                );
            }
        }
        Ok(cfg)
    }

    /// Write `config.toml`. Comments in it are not preserved.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("failed to create {}", dir.display()))?;
        }
        let text = toml::to_string(self).context("failed to serialize the configuration")?;
        std::fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn keystore(&self) -> &KeyStore {
        &self.keystore
    }

    /// The size limits to use: defaults with the configured overrides applied.
    /// The agent's settings, or its defaults when `config.toml` does not mention it.
    pub fn agent(&self, kind: AgentKind) -> AgentSettings {
        self.agents
            .get(kind.as_str())
            .copied()
            .unwrap_or_else(|| AgentSettings::default_for(kind))
    }

    /// Slicing parameters for the core layer, with defaults for anything not set.
    pub fn core_config(&self) -> aj_core::Config {
        let mut cfg = aj_core::Config::default();
        if let Some(h) = self.slicing.day_boundary_hour {
            cfg.day_boundary_hour = h;
        }
        cfg
    }

    pub fn set_day_boundary_hour(&mut self, hour: u32) -> Result<()> {
        if hour > MAX_DAY_BOUNDARY_HOUR {
            bail!("day boundary must be 0-{MAX_DAY_BOUNDARY_HOUR}, got {hour}");
        }
        self.slicing.day_boundary_hour = Some(hour);
        Ok(())
    }

    /// The chosen report language, if one has been chosen.
    pub fn chosen_report_language(&self) -> Option<&'static Language> {
        self.report.language.as_deref().and_then(Language::find)
    }

    /// The report language: the chosen one, or the initial choice for an installation that
    /// has not chosen yet (see [`lang::initial`]). Returns whether it was just chosen, in
    /// which case the caller should save the configuration so the choice sticks.
    pub fn report_language(
        &mut self,
        has_reports: bool,
        system_locales: &[String],
    ) -> (&'static Language, bool) {
        if let Some(l) = self.chosen_report_language() {
            return (l, false);
        }
        let l = lang::initial(has_reports, system_locales);
        self.report.language = Some(l.tag.to_string());
        (l, true)
    }

    pub fn set_report_language(&mut self, tag: &str) -> Result<()> {
        self.report.language = Some(Language::get(tag)?.tag.to_string());
        Ok(())
    }

    /// The interface language setting: `"auto"` or a language tag.
    pub fn ui_language(&self) -> &str {
        self.ui.language.as_deref().unwrap_or(UI_LANGUAGE_AUTO)
    }

    pub fn set_ui_language(&mut self, value: &str) -> Result<()> {
        let value = value.trim();
        if value.is_empty() {
            bail!("the interface language must not be empty");
        }
        self.ui.language = (value != UI_LANGUAGE_AUTO).then(|| value.to_string());
        Ok(())
    }

    pub fn check_updates(&self) -> bool {
        self.ui.check_updates
    }

    pub fn set_check_updates(&mut self, on: bool) {
        self.ui.check_updates = on;
    }

    pub fn set_agent(&mut self, kind: AgentKind, settings: AgentSettings) {
        self.agents.insert(kind.as_str().to_string(), settings);
    }

    pub fn budgets(&self) -> Budgets {
        let defaults = Budgets::default();
        Budgets {
            chunk_chars: self.budgets.chunk_chars.unwrap_or(defaults.chunk_chars),
            stage2_chars: self.budgets.stage2_chars.unwrap_or(defaults.stage2_chars),
            quiet_minutes: self.budgets.quiet_minutes.unwrap_or(defaults.quiet_minutes),
            max_incremental_updates: self
                .budgets
                .max_incremental_updates
                .unwrap_or(defaults.max_incremental_updates),
            batch_concurrency: self
                .budgets
                .batch_concurrency
                .unwrap_or(defaults.batch_concurrency)
                .max(1),
            ..defaults
        }
    }

    pub fn default_model_name(&self) -> &str {
        self.default_model.as_deref().unwrap_or(BUILTIN)
    }

    // ---- providers ----

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
            let Some(existing) = self.providers.get(old) else {
                bail!("no provider named {old}");
            };
            if old == BUILTIN
                && std::mem::discriminant(existing) != std::mem::discriminant(&provider)
            {
                bail!("the built-in provider {BUILTIN} must stay a Claude CLI provider");
            }
        }
        if let Some(old) = original.filter(|o| *o != name) {
            if old == BUILTIN {
                bail!("the built-in provider {BUILTIN} cannot be renamed");
            }
            self.providers.remove(old);
            self.keystore.rename(old, name)?;
            for m in self.models.values_mut().filter(|m| m.provider == old) {
                m.provider = name.to_string();
            }
        }
        let is_cli = matches!(provider, ProviderConfig::ClaudeCli { .. });
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
        if name == BUILTIN {
            bail!("the built-in provider {BUILTIN} cannot be deleted");
        }
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

    /// List the model ids a provider offers. `draft` describes a provider still
    /// being edited (not yet saved); `typed_key` is a key typed but not saved.
    /// Without a typed key, the saved provider's key (if `saved_name`) is used.
    pub fn list_provider_models(
        &self,
        saved_name: Option<&str>,
        draft: ProviderConfig,
        typed_key: Option<&str>,
    ) -> Result<Vec<String>> {
        match draft.normalized()? {
            ProviderConfig::ClaudeCli { .. } => {
                Ok(CLAUDE_CLI_MODELS.iter().map(|s| s.to_string()).collect())
            }
            ProviderConfig::OpenaiCompat { base_url, .. } => {
                let key = match typed_key.map(str::trim).filter(|k| !k.is_empty()) {
                    Some(k) => ApiKey::Value(k.to_string()),
                    None => saved_name.map_or(ApiKey::None, |n| self.api_key(n)),
                };
                crate::openai::list_models(&base_url, &key)
            }
        }
    }

    // ---- model aliases ----

    /// Add or replace a model alias; `original` is its previous alias (`None` creates one).
    pub fn upsert_model(
        &mut self,
        original: Option<&str>,
        alias: &str,
        model: ModelConfig,
    ) -> Result<()> {
        let alias = alias.trim();
        validate_model_alias(alias)?;
        let model = ModelConfig {
            provider: model.provider.trim().to_string(),
            model: clean(model.model),
            reasoning_effort: clean(model.reasoning_effort),
        };
        let Some(provider) = self.providers.get(&model.provider) else {
            bail!("no provider named {}", model.provider);
        };
        let model = match provider {
            ProviderConfig::OpenaiCompat { .. } if model.model.is_none() => {
                bail!("choose a model id for provider {}", model.provider)
            }
            // The CLI has no reasoning-effort setting.
            ProviderConfig::ClaudeCli { .. } => ModelConfig {
                reasoning_effort: None,
                ..model
            },
            _ => model,
        };
        if original != Some(alias) && self.models.contains_key(alias) {
            bail!("a model named {alias} already exists");
        }
        if let Some(old) = original.filter(|o| *o != alias) {
            if old == BUILTIN {
                bail!("the built-in model {BUILTIN} cannot be renamed");
            }
            if self.models.remove(old).is_none() {
                bail!("no model named {old}");
            }
            if self.default_model.as_deref() == Some(old) {
                self.default_model = Some(alias.to_string());
            }
        }
        self.models.insert(alias.to_string(), model);
        Ok(())
    }

    pub fn remove_model(&mut self, alias: &str) -> Result<()> {
        if alias == BUILTIN {
            bail!("the built-in model {BUILTIN} cannot be deleted");
        }
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
        self.default_model = (alias != BUILTIN).then(|| alias.to_string());
        Ok(())
    }

    /// One-line description of a model alias; never contains a key.
    pub fn describe_model(&self, alias: &str) -> String {
        let Some(m) = self.models.get(alias) else {
            return format!("unknown model {alias}");
        };
        let model = m.model.as_deref().unwrap_or("default");
        match self.providers.get(&m.provider) {
            Some(ProviderConfig::ClaudeCli { binary }) => format!(
                "{} (claude_cli, command {})  model {model}",
                m.provider,
                binary.as_deref().unwrap_or("claude"),
            ),
            Some(ProviderConfig::OpenaiCompat { base_url, .. }) => format!(
                "{} ({base_url})  model {model}  reasoning effort {}  key {}",
                m.provider,
                m.reasoning_effort.as_deref().unwrap_or("default"),
                if self.has_key(&m.provider) {
                    "stored"
                } else {
                    "none"
                }
            ),
            None => format!("{} (missing provider)", m.provider),
        }
    }

    /// Build the engine for a model alias (the default alias when `None`).
    pub fn build_engine(&self, alias: Option<&str>, runner_dir: &Path) -> Result<Box<dyn Engine>> {
        let alias = alias.unwrap_or_else(|| self.default_model_name());
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
            ProviderConfig::ClaudeCli { binary } => Box::new(ClaudeCliEngine {
                binary: binary.clone().unwrap_or_else(|| "claude".into()),
                model: m.model.clone(),
                runner_dir: runner_dir.to_path_buf(),
            }),
            ProviderConfig::OpenaiCompat {
                base_url,
                timeout_secs,
                ..
            } => Box::new(OpenAiCompatEngine {
                base_url: base_url.clone(),
                api_key: self.api_key(&m.provider),
                model: m.model.clone().unwrap_or_default(),
                reasoning_effort: m.reasoning_effort.clone(),
                timeout: Duration::from_secs(timeout_secs.unwrap_or(300)),
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aj-config-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file_store(dir: &Path) -> KeyStore {
        KeyStore::File(dir.join(crate::keystore::SECRETS_FILE))
    }

    fn openai() -> ProviderConfig {
        ProviderConfig::OpenaiCompat {
            base_url: "https://example.com/v1/".into(),
            timeout_secs: None,
        }
    }

    fn model(provider: &str, id: &str) -> ModelConfig {
        ModelConfig {
            provider: provider.into(),
            model: Some(id.into()),
            reasoning_effort: Some("high".into()),
        }
    }

    #[test]
    fn parses_config_and_adds_builtins() {
        let dir = temp_dir("parse");
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            r#"
default_model = "flash"

[providers.school]
type = "openai_compat"
base_url = "https://example.com/v1/"

[models.flash]
provider = "school"
model = "m1"
reasoning_effort = "high"

[agents.codex]
enabled = true
pre_digest = true

[budgets]
chunk_chars = 1234
batch_concurrency = 4
"#,
        )
        .unwrap();
        let cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
        // Unlisted agents use their defaults: only Claude Code is on.
        assert!(cfg.agent(AgentKind::Claude).enabled);
        assert!(!cfg.agent(AgentKind::Antigravity).enabled);
        assert!(cfg.agent(AgentKind::Codex).pre_digest);
        assert_eq!(cfg.budgets().chunk_chars, 1234);
        assert_eq!(cfg.budgets().batch_concurrency, 4);
        assert_eq!(cfg.budgets().stage2_chars, Budgets::default().stage2_chars);
        assert_eq!(cfg.default_model_name(), "flash");
        assert!(cfg.providers.contains_key(BUILTIN) && cfg.models.contains_key(BUILTIN));
        assert!(cfg.describe_model("flash").contains("key none"));

        let engine = cfg.build_engine(None, &dir).unwrap();
        assert_eq!(engine.name(), "openai:m1:high");
        assert!(matches!(cfg.api_key("school"), ApiKey::None));
        assert!(cfg.build_engine(Some("nope"), &dir).is_err());

        // Without a config file, fall back to the built-in model; bad files are errors.
        let empty = AppConfig::load(&dir.join("missing.toml"), file_store(&dir)).unwrap();
        assert_eq!(empty.default_model_name(), BUILTIN);
        assert_eq!(empty.budgets().batch_concurrency, 2);
        std::fs::write(
            &path,
            "[providers.x]\ntype = \"openai_compat\"\nbase_url = \"http://h\"\napi_key_env = \"K\"",
        )
        .unwrap();
        assert!(AppConfig::load(&path, file_store(&dir)).is_err());
        std::fs::write(&path, "[agents.cursor]\nenabled = true").unwrap();
        assert!(AppConfig::load(&path, file_store(&dir)).is_err());
        std::fs::write(&path, "[models.x]\nprovider = \"ghost\"\nmodel = \"m\"").unwrap();
        assert!(AppConfig::load(&path, file_store(&dir)).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn day_boundary_defaults_to_midnight_and_round_trips() {
        let dir = temp_dir("slicing");
        let path = dir.join("config.toml");
        let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
        assert_eq!(cfg.core_config().day_boundary_hour, 0);
        assert!(cfg
            .set_day_boundary_hour(MAX_DAY_BOUNDARY_HOUR + 1)
            .is_err());
        cfg.set_day_boundary_hour(4).unwrap();
        cfg.save(&path).unwrap();
        let loaded = AppConfig::load(&path, file_store(&dir)).unwrap();
        assert_eq!(loaded.core_config().day_boundary_hour, 4);
        std::fs::write(&path, "[slicing]\nday_boundary_hour = 13").unwrap();
        assert!(AppConfig::load(&path, file_store(&dir)).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn update_checks_are_off_by_default_and_round_trip() {
        let dir = temp_dir("updates");
        let path = dir.join("config.toml");
        let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
        assert!(!cfg.check_updates());
        cfg.save(&path).unwrap();
        assert!(!std::fs::read_to_string(&path)
            .unwrap()
            .contains("check_updates"));
        cfg.set_check_updates(true);
        cfg.save(&path).unwrap();
        assert!(AppConfig::load(&path, file_store(&dir))
            .unwrap()
            .check_updates());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn edits_round_trip_and_keep_keys_out_of_config() {
        let dir = temp_dir("edit");
        let path = dir.join("config.toml");
        let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();

        cfg.upsert_provider(None, "p", openai(), KeyUpdate::Set("sk-secret".into()))
            .unwrap();
        cfg.upsert_model(None, "fast", model("p", "m1")).unwrap();
        cfg.set_default_model("fast").unwrap();
        // Renaming a provider carries its key and repoints models; renaming a model keeps the default.
        cfg.upsert_provider(Some("p"), "q", openai(), KeyUpdate::Keep)
            .unwrap();
        cfg.upsert_model(None, "Fast Model (v2)", model("q", "m3"))
            .unwrap();
        cfg.upsert_model(Some("fast"), "fast2", model("q", "m2"))
            .unwrap();
        cfg.save(&path).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("sk-secret"));
        assert!(text.contains("base_url = \"https://example.com/v1\""));

        let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
        assert_eq!(cfg.default_model_name(), "fast2");
        assert_eq!(cfg.models["fast2"].provider, "q");
        assert_eq!(cfg.models["Fast Model (v2)"].model.as_deref(), Some("m3"));
        assert!(cfg
            .upsert_model(None, "two\nlines", model("q", "m"))
            .is_err());
        assert!(cfg.has_key("q"));
        assert!(matches!(cfg.api_key("q"), ApiKey::Value(k) if k == "sk-secret"));
        assert!(cfg.describe_model("fast2").contains("key stored"));

        // A provider in use cannot be deleted; models need a known provider and a model id.
        assert!(cfg.remove_provider("q").is_err());
        assert!(cfg.upsert_model(None, "x", model("ghost", "m")).is_err());
        assert!(cfg
            .upsert_model(
                None,
                "x",
                ModelConfig {
                    model: None,
                    ..model("q", "m")
                }
            )
            .is_err());
        assert!(cfg
            .upsert_provider(None, "bad name", openai(), KeyUpdate::Keep)
            .is_err());
        assert!(cfg.remove_model(BUILTIN).is_err());

        cfg.remove_model("fast2").unwrap();
        cfg.remove_model("Fast Model (v2)").unwrap();
        assert_eq!(cfg.default_model_name(), BUILTIN);
        cfg.upsert_provider(Some("q"), "q", openai(), KeyUpdate::Clear)
            .unwrap();
        assert!(!cfg.has_key("q"));
        cfg.remove_provider("q").unwrap();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn claude_cli_lists_shorthand_models() {
        let dir = temp_dir("cli");
        let cfg = AppConfig::load(&dir.join("none.toml"), file_store(&dir)).unwrap();
        let ids = cfg
            .list_provider_models(None, ProviderConfig::ClaudeCli { binary: None }, None)
            .unwrap();
        assert!(ids.contains(&"sonnet".to_string()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn languages_are_chosen_once_saved_and_validated() {
        let dir = temp_dir("lang");
        let path = dir.join("config.toml");
        let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
        assert!(cfg.chosen_report_language().is_none());
        assert_eq!(cfg.ui_language(), UI_LANGUAGE_AUTO);

        // An installation with reports keeps writing Chinese; the choice is made once.
        let (l, chosen) = cfg.report_language(true, &["en-US".to_string()]);
        assert_eq!((l.tag, chosen), ("zh-Hans", true));
        let (l, chosen) = cfg.report_language(false, &["en-US".to_string()]);
        assert_eq!((l.tag, chosen), ("zh-Hans", false));

        cfg.set_report_language("en").unwrap();
        assert!(cfg.set_report_language("xx").is_err());
        cfg.set_ui_language("zh-Hans").unwrap();
        cfg.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("[report]") && text.contains("[ui]"), "{text}");

        let mut cfg = AppConfig::load(&path, file_store(&dir)).unwrap();
        assert_eq!(cfg.chosen_report_language().unwrap().tag, "en");
        assert_eq!(cfg.ui_language(), "zh-Hans");
        cfg.set_ui_language("auto").unwrap();
        assert!(cfg.ui.language.is_none());

        std::fs::write(&path, "[report]\nlanguage = \"xx\"\n").unwrap();
        assert!(AppConfig::load(&path, file_store(&dir)).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
