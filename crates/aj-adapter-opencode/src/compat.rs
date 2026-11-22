//! Which OpenCode data this adapter can read (pure logic over the database shape).
//!
//! Only the 1.x storage (`session` / `message` / `part`) is read. OpenCode 2.x moved sessions to
//! `session_v2` / `session_message`; a store may hold both after an upgrade.

use aj_adapter::{CompatReason, Compatibility, SourceProbe};

use crate::db::StoreShape;

/// Versions the 1.x reader was verified against start with this major version.
const READ_MAJOR: &str = "1.";
/// Development builds report this instead of a version number.
const DEV_VERSION: &str = "local";

pub(crate) fn assess(shape: &StoreShape) -> SourceProbe {
    let mut versions = shape.versions.clone();
    versions.sort_by_key(|v| version_key(v));
    let unknown_versions: Vec<&String> = versions
        .iter()
        .filter(|v| !v.starts_with(READ_MAJOR) && v.as_str() != DEV_VERSION)
        .collect();

    let (compatibility, reason) = if !shape.v1 && shape.v2 {
        (Compatibility::Unsupported, Some(CompatReason::NewerFormat))
    } else if !shape.v1 {
        (
            Compatibility::Unsupported,
            Some(CompatReason::UnrecognizedLayout),
        )
    } else if shape.v2_only > 0 {
        (
            Compatibility::Partial,
            Some(CompatReason::SessionsInNewerFormat(shape.v2_only)),
        )
    } else if !unknown_versions.is_empty() {
        (
            Compatibility::Partial,
            Some(CompatReason::UntestedVersions(
                unknown_versions.into_iter().cloned().collect(),
            )),
        )
    } else {
        (Compatibility::Supported, None)
    };
    SourceProbe {
        compatibility,
        versions,
        reason,
    }
}

/// Numeric sort key for versions like `1.18.35`; non-numeric parts sort last.
fn version_key(v: &str) -> Vec<u64> {
    v.split('.')
        .map(|p| p.parse().unwrap_or(u64::MAX))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(v1: bool, versions: &[&str], v2: bool, v2_only: u64) -> StoreShape {
        StoreShape {
            v1,
            versions: versions.iter().map(|v| v.to_string()).collect(),
            v2,
            v2_only,
        }
    }

    #[test]
    fn plain_1x_store_is_supported_and_versions_are_sorted() {
        let p = assess(&shape(true, &["1.18.35", "local", "1.9.2"], false, 0));
        assert_eq!(p.compatibility, Compatibility::Supported);
        assert_eq!(p.versions, ["1.9.2", "1.18.35", "local"]);
    }

    #[test]
    fn a_2x_store_is_never_claimed() {
        assert_eq!(
            assess(&shape(false, &[], true, 12)).compatibility,
            Compatibility::Unsupported
        );
        // Upgraded store: old sessions still readable, new ones not.
        let p = assess(&shape(true, &["1.18.35"], true, 3));
        assert_eq!(p.compatibility, Compatibility::Partial);
        assert_eq!(p.reason, Some(CompatReason::SessionsInNewerFormat(3)));
        // Upgraded but every 2.x session is also in the 1.x tables.
        assert_eq!(
            assess(&shape(true, &["1.18.35"], true, 0)).compatibility,
            Compatibility::Supported
        );
    }

    #[test]
    fn unknown_major_versions_are_flagged() {
        let p = assess(&shape(true, &["1.18.35", "2.0.12"], false, 0));
        assert_eq!(p.compatibility, Compatibility::Partial);
        assert_eq!(
            p.reason,
            Some(CompatReason::UntestedVersions(vec!["2.0.12".into()]))
        );
    }
}
