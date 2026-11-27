//! Shared application state and thin environment helpers for the desktop shell.
//!
//! Resolves data and export directories, names the files in the data directory, opens SQLite
//! connections, loads `config.toml`,
//! and provides source adapters without holding any business rules.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use aj_adapter::SourceAdapter;
use aj_backup::DataFiles;
use aj_core::{data_files, Config};
use aj_report::lang::{self, Language};
use aj_report::{AppConfig, KeyStore};
use aj_store::Store;
use anyhow::{Context, Result};

/// The database's file name in the data directory.
pub const DB_FILE: &str = data_files::DATABASE;

/// What a backup holds of the data directory `dir`.
pub fn data_files(dir: &Path) -> DataFiles<'_> {
    DataFiles {
        dir,
        database: DB_FILE,
        files: data_files::CONFIG_FILES,
    }
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub export_dir: PathBuf,
    /// Running report generations, keyed by report scope (see `cmd_report::job_key`),
    /// each with its own cancellation flag.
    pub report_jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Serializes read-modify-write cycles on `config.toml` / `secrets.toml`.
    pub config_lock: Mutex<()>,
}

impl AppState {
    pub fn new() -> Result<Self> {
        let data_dir = match std::env::var_os("AJ_DATA_DIR") {
            Some(d) => PathBuf::from(d),
            None => dirs::data_dir()
                .context("cannot determine the application data directory")?
                .join(aj_core::APP_DATA_DIR_NAME),
        };
        let export_dir = dirs::home_dir()
            .context("cannot determine the home directory")?
            .join("WorkJournal");

        // The database is not opened here: it may first need the person's go-ahead to be
        // upgraded (`cmd_database`), which also syncs built-in prompts once it is ready.
        Ok(Self {
            data_dir,
            export_dir,
            report_jobs: Mutex::new(HashMap::new()),
            config_lock: Mutex::new(()),
        })
    }

    pub fn open_store(&self) -> Result<Store> {
        Store::open(&self.data_dir.join(DB_FILE))
    }

    pub fn load_config(&self) -> Result<AppConfig> {
        AppConfig::load(
            &self.config_path(),
            KeyStore::platform_default(&self.data_dir),
        )
    }

    /// Locked load-edit-save of `config.toml`; returns the saved configuration.
    pub fn modify_config(
        &self,
        edit: impl FnOnce(&mut AppConfig) -> Result<()>,
    ) -> Result<AppConfig> {
        let _guard = self
            .config_lock
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let mut cfg = self.load_config()?;
        edit(&mut cfg)?;
        cfg.save(&self.config_path())?;
        Ok(cfg)
    }

    /// The language reports are written in. An installation that has not chosen one gets
    /// its initial language (from `system_locales`, the system's preferred languages most
    /// preferred first, else the environment) saved now.
    pub fn report_language(&self, system_locales: &[String]) -> Result<&'static Language> {
        if let Some(l) = self.load_config()?.chosen_report_language() {
            return Ok(l);
        }
        let has_reports = self.open_store()?.has_reports()?;
        let locales = if system_locales.is_empty() {
            lang::env_locales()
        } else {
            system_locales.to_vec()
        };
        let cfg = self.modify_config(|c| {
            c.report_language(has_reports, &locales);
            Ok(())
        })?;
        cfg.chosen_report_language()
            .context("the report language was not saved")
    }

    /// Slicing parameters from `config.toml` (read on every call, so a settings change applies at once).
    pub fn core_config(&self) -> Result<Config> {
        Ok(self.load_config()?.core_config())
    }

    pub fn config_path(&self) -> PathBuf {
        self.data_dir.join(data_files::CONFIG)
    }

    /// Adapters for every supported agent, enabled or not (reading already-stored sessions).
    pub fn adapters(&self) -> Result<Vec<Box<dyn SourceAdapter>>> {
        aj_sources::build_adapters(&self.data_dir, |_| true)
    }
}
