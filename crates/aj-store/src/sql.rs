//! SQL pieces shared by the queries of several modules: conditions on daily slices, and
//! conversions between columns and Rust values.
//!
//! The slice conditions assume the aliases used throughout: `sds` for `session_daily_slices`
//! and `s` for the slice's row in `sessions`.

use aj_ir::AgentId;
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::Row;

/// True when a slice takes part in reports: its own setting first, otherwise the session's.
/// The SQL form of `!SliceRow::is_ignored`.
pub(crate) const SLICE_INCLUDED: &str =
    "(sds.ignore_state = 'included' OR (sds.ignore_state = 'inherit' AND s.ignored = 0))";

/// The project a slice belongs to: its own override first, otherwise the session's.
/// The SQL form of `SliceRow::effective_project`.
pub(crate) const SLICE_PROJECT: &str = "COALESCE(sds.project_id_override, s.project_id)";

/// True when a slice has no project and its session was not explicitly assigned to none, so a
/// suggestion may still place it. (Spelled out: constants cannot be joined at compile time.)
pub(crate) const SLICE_UNASSIGNED: &str =
    "(COALESCE(sds.project_id_override, s.project_id) IS NULL \
     AND s.assignment_source != 'manual')";

/// A timestamp as stored: fixed-format UTC, so stored values compare correctly as plain text.
pub(crate) fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(crate) fn parse_ts(s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

pub(crate) fn opt_ts(s: Option<String>) -> rusqlite::Result<Option<DateTime<Utc>>> {
    s.as_deref().map(parse_ts).transpose()
}

/// Read an agent identifier column. Unknown identifiers are fine; only a malformed one fails.
pub(crate) fn agent_at(r: &Row, idx: usize) -> rusqlite::Result<AgentId> {
    let agent: String = r.get(idx)?;
    AgentId::parse(&agent).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, e.into())
    })
}
