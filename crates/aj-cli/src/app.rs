//! Context and small helpers shared by the commands.

use std::path::PathBuf;

use aj_adapter::SourceAdapter;
use aj_core::slice::local_date;
use aj_core::Config;
use aj_report::lang::{self, Language};
use aj_report::{AppConfig, KeyStore};
use aj_store::{SessionRow, SliceRow, Store};
use anyhow::{bail, Context, Result};
use chrono::{Local, Utc};

pub(crate) struct App {
    pub store: Store,
    pub cfg: Config,
    pub data_dir: PathBuf,
}

impl App {
    /// Generation config, with keys from the platform key store.
    pub(crate) fn load_config(&self) -> Result<AppConfig> {
        AppConfig::load(
            &self.data_dir.join("config.toml"),
            KeyStore::platform_default(&self.data_dir),
        )
    }

    /// The language to write reports in: `requested` (from `--lang`) if given, otherwise
    /// the configured one. An installation that has not chosen one yet gets its initial
    /// language saved to `config.toml`, as the app does.
    pub(crate) fn report_language(&self, requested: Option<&str>) -> Result<&'static Language> {
        if let Some(tag) = requested {
            return Language::get(tag);
        }
        let mut config = self.load_config()?;
        let (lang, just_chosen) =
            config.report_language(self.store.has_reports()?, &lang::env_locales());
        if just_chosen {
            config.save(&self.data_dir.join("config.toml"))?;
        }
        Ok(lang)
    }
}

/// Adapters for every supported agent, enabled or not (reading already-stored sessions).
pub(crate) fn adapters(app: &App) -> Result<Vec<Box<dyn SourceAdapter>>> {
    aj_sources::build_adapters(&app.data_dir, |_| true)
}

/// Adapters for the agents enabled in `config.toml` (what a scan reads).
pub(crate) fn enabled_adapters(app: &App) -> Result<Vec<Box<dyn SourceAdapter>>> {
    let cfg = app.load_config()?;
    aj_sources::build_adapters(&app.data_dir, |kind| cfg.agent(kind).enabled)
}

pub(crate) fn resolve_date(app: &App, date: Option<String>) -> String {
    date.unwrap_or_else(|| local_date(Utc::now(), &Local, app.cfg.day_boundary_hour).to_string())
}

pub(crate) fn title(s: &SessionRow) -> String {
    s.title.clone().unwrap_or_else(|| "(untitled)".into())
}

pub(crate) fn short_id(s: &SessionRow) -> &str {
    &s.source_session_id[..s.source_session_id.len().min(8)]
}

pub(crate) fn find_session(app: &App, prefix: &str) -> Result<SessionRow> {
    let mut found = app.store.find_sessions_by_prefix(prefix)?;
    match found.len() {
        0 => bail!("no session has an ID starting with {prefix}"),
        1 => Ok(found.remove(0)),
        _ => bail!(
            "several sessions have an ID starting with {prefix}; type a few more characters: {}",
            found.iter().map(short_id).collect::<Vec<_>>().join(", ")
        ),
    }
}

pub(crate) fn find_slice(app: &App, reference: &str) -> Result<SliceRow> {
    let id: i64 = reference
        .trim_start_matches(['S', 's'])
        .parse()
        .with_context(|| format!("a slice reference looks like S12, got {reference}"))?;
    app.store
        .slice(id)?
        .with_context(|| format!("no slice S{id}"))
}

pub(crate) fn clip(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let head: String = text.chars().take(max_chars).collect();
    format!("{head}…")
}
