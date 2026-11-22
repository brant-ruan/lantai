//! Application layer: slice digests (stage 1), report aggregation (stage 2), engine dispatch and source anchor verification.

pub mod anchors;
pub mod batch;
pub mod brief;
pub mod chunk;
pub mod config;
pub mod daily;
pub mod digest;
pub mod edits;
pub mod engine;
pub mod export;
pub mod human;
pub mod keystore;
pub mod lang;
pub mod manifest;
pub mod material;
pub mod monthly;
pub mod openai;
pub mod period;
pub mod presets;
pub mod progress;
pub mod prompt_library;
pub mod prompts;
pub mod report_io;
pub mod rollup;
pub mod search;
pub mod self_stats;
pub mod stage1;
pub mod structure;
pub mod summarize;
mod textsim;
pub mod weekly;

pub use config::{
    AgentSettings, AppConfig, KeyUpdate, ModelConfig, ProviderConfig, MAX_DAY_BOUNDARY_HOUR,
};
pub use daily::{generate_daily, DailyOptions, DailyResult};
pub use digest::{Digest, Point, StoredDigest};
pub use engine::{ClaudeCliEngine, Engine};
pub use keystore::KeyStore;
pub use report_io::ReportMode;
pub use summarize::Budgets;
