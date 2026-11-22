//! Project assignment.
//!
//! Three parts, each doing one thing:
//! - [`matcher`]: rule matching. Pure functions; no file or database access.
//! - [`git`]: probes the repository root and remote from a working directory. Read-only on the file system.
//! - [`assign`]: writes the results of the other two back to the storage layer.

pub mod assign;
pub mod git;
pub mod matcher;

pub use assign::{evaluate_all, evaluate_session, ActiveRules, EvaluateStats};
pub use matcher::{match_rules, uncovered_path, validate_rule, MatchContext, RuleMatch};
