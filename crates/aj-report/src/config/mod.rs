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
//! [providers.claude]              # an agent CLI, added by finding local tools
//! type = "claude_cli"              # or "codex_cli"
//! binary = "/path/to/claude"       # optional; `claude` (or `codex`) on PATH when unset
//!
//! [models.flash]                   # an alias the user picks in the app or with --model
//! provider = "gateway"
//! model = "model-a"
//! reasoning_effort = "high"        # optional; the provider's default when unset
//! temperature = 0.3                # optional, 0-2; OpenAI-compatible providers only
//!
//! [models."Claude Code"]           # no `model`: the CLI's own default
//! provider = "claude"
//! reasoning_effort = "high"        # optional; one of `engine::CLAUDE_EFFORTS`
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
//! Nothing is built in: without aliases, [`AppConfig::build_engine`] fails with
//! [`crate::diagnose::NO_MODEL`]. A model alias resolves to an [`Engine`](crate::Engine). Keys never appear in `config.toml`;
//! a provider without a stored key is called without authentication. Edits are in-memory except
//! for key changes, which go to the key store immediately; [`AppConfig::save`]
//! writes `config.toml`.
//!
//! The general settings are here; `providers.rs` holds the providers and their keys, and
//! `models.rs` the model aliases and the engines they build.

mod models;
mod providers;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use aj_ir::AgentId;

use crate::keystore::KeyStore;
use crate::lang::{self, Language};
use crate::summarize::Budgets;

use models::{known_models, validate_model};
pub use models::{validate_model_alias, ModelConfig, MAX_TEMPERATURE};
pub use providers::{validate_provider_name, KeyUpdate, ProviderConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderConfig>,
    #[serde(default)]
    pub models: BTreeMap<String, ModelConfig>,
    /// Per-agent source settings, keyed by [`AgentId::as_str`].
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
    pub fn default_for(kind: &AgentId) -> Self {
        Self {
            enabled: *kind == AgentId::CLAUDE,
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

/// A form field as saved: trimmed, and `None` when empty.
fn clean(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

impl AppConfig {
    /// Load the configuration. A missing file yields no providers and no models.
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
        // Only registered agents can be scanned, so other keys are mistakes.
        let registered = |k: &str| AgentId::parse(k).is_ok_and(|a| a.registration().is_some());
        if let Some(unknown) = cfg.agents.keys().find(|k| !registered(k)) {
            bail!("unknown agent [agents.{unknown}] in {}", path.display());
        }
        if let Some(d) = cfg
            .default_model
            .as_deref()
            .filter(|d| !cfg.models.contains_key(*d))
        {
            bail!(
                "default_model {d} is not defined under [models] in {}",
                path.display()
            );
        }
        for (alias, m) in &cfg.models {
            let Some(provider) = cfg.providers.get(&m.provider) else {
                bail!(
                    "model {alias} uses provider {} which is not defined in {}",
                    m.provider,
                    path.display()
                );
            };
            validate_model(m, provider, &known_models(provider))
                .with_context(|| format!("invalid model {alias} in {}", path.display()))?;
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

    /// The agent's settings, or its defaults when `config.toml` does not mention it.
    pub fn agent(&self, kind: &AgentId) -> AgentSettings {
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

    pub fn set_agent(&mut self, kind: AgentId, settings: AgentSettings) {
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
}
