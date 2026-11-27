//! Core layer: scan orchestration, daily slicing, project assignment, dehydration and redaction.
//!
//! This layer depends only on the IR and the storage layer and knows nothing about any agent's file format.

pub mod dehydrate;
pub mod order;
pub mod overview;
pub mod project;
pub mod redact;
pub mod scan;
pub mod search;
pub mod slice;
pub mod stats;
pub mod task_link;

/// Folder name under the system application data directory (`~/Library/Application Support/lantai` on macOS).
pub const APP_DATA_DIR_NAME: &str = "lantai";

/// Files Lantai keeps in its data directory, named once for the app and the CLI.
pub mod data_files {
    /// The database.
    pub const DATABASE: &str = "journal.sqlite";
    /// Providers, models and settings.
    pub const CONFIG: &str = "config.toml";
    /// The chosen app icon.
    pub const APP_ICON: &str = "app-icon";
    /// The configuration files a backup carries besides the database. Add new user
    /// configuration files here.
    pub const CONFIG_FILES: &[&str] = &[CONFIG, APP_ICON];
}

/// Fingerprint written at the top of report-generation prompts. A session containing it was started by this tool and is ignored on ingestion.
pub const INTERNAL_TASK_FINGERPRINT: &str = "<!-- AGENT_JOURNAL_INTERNAL_TASK -->";

#[derive(Debug, Clone)]
pub struct Config {
    /// Day boundary (hour of local time). Activity before it counts towards the previous day.
    pub day_boundary_hour: u32,
    /// A gap between adjacent activity longer than this many minutes counts as idle and is excluded from active time.
    pub idle_gap_minutes: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            day_boundary_hour: 0,
            idle_gap_minutes: 30,
        }
    }
}
