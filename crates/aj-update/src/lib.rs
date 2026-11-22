//! Checks GitHub Releases for a newer Lantai (docs/SPEC.md, "Distribution"). It only reads the
//! public release list; it downloads and installs nothing. Comparing versions and reading
//! release notes is pure ([`evaluate`]); [`check`] adds the one network request.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use semver::Version;
use serde::{Deserialize, Serialize};

/// The repository releases are published in.
pub const REPOSITORY: &str = "brant-ruan/lantai";

/// How many recent releases one check looks at, enough to cover a long gap between updates.
const RELEASES_PER_CHECK: u32 = 50;

const TIMEOUT: Duration = Duration::from_secs(15);

/// One release as the GitHub API returns it (only the fields used here).
#[derive(Debug, Clone, Deserialize)]
pub struct GithubRelease {
    pub tag_name: String,
    pub html_url: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub published_at: Option<String>,
}

/// The result of a check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum UpdateCheck {
    /// No published release is newer than the running version.
    UpToDate { current: String },
    /// A newer release exists.
    #[serde(rename_all = "camelCase")]
    Available {
        current: String,
        latest: String,
        /// Release page of the latest version.
        url: String,
        published_at: Option<String>,
        /// Whether any release between the running one and the latest lists breaking changes.
        breaking: bool,
    },
}

/// Compare the running version with a list of releases.
pub fn evaluate(current: &str, releases: &[GithubRelease]) -> Result<UpdateCheck> {
    let current_version =
        Version::parse(current).with_context(|| format!("invalid current version {current}"))?;
    let newer: Vec<(Version, &GithubRelease)> = releases
        .iter()
        .filter(|r| !r.draft && !r.prerelease)
        .filter_map(|r| Some((parse_tag(&r.tag_name)?, r)))
        .filter(|(v, _)| *v > current_version)
        .collect();
    let Some((latest, release)) = newer.iter().max_by(|a, b| a.0.cmp(&b.0)) else {
        return Ok(UpdateCheck::UpToDate {
            current: current.to_string(),
        });
    };
    Ok(UpdateCheck::Available {
        current: current.to_string(),
        latest: latest.to_string(),
        url: release.html_url.clone(),
        published_at: release.published_at.clone(),
        breaking: newer
            .iter()
            .any(|(_, r)| r.body.as_deref().is_some_and(has_breaking_section)),
    })
}

/// A release tag such as `v0.2.0`; tags that are not versions are skipped.
fn parse_tag(tag: &str) -> Option<Version> {
    Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()
}

/// Whether release notes (a changelog section) have a "Breaking" heading.
fn has_breaking_section(notes: &str) -> bool {
    notes.lines().any(|line| {
        let line = line.trim();
        line.starts_with('#') && line.trim_start_matches('#').trim() == "Breaking"
    })
}

/// Ask GitHub for the recent releases and compare them with `current`.
pub fn check(current: &str) -> Result<UpdateCheck> {
    let url =
        format!("https://api.github.com/repos/{REPOSITORY}/releases?per_page={RELEASES_PER_CHECK}");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", format!("Lantai/{current}"))
        .call()
        .map_err(|e| anyhow::anyhow!("could not reach GitHub: {e}"))?;
    let status = response.status().as_u16();
    let text = response
        .body_mut()
        .read_to_string()
        .context("failed to read GitHub's response")?;
    if status == 403 || status == 429 {
        bail!("GitHub is limiting requests from this network; try again later");
    }
    if status >= 400 {
        bail!("GitHub returned {status}");
    }
    let releases: Vec<GithubRelease> =
        serde_json::from_str(&text).context("GitHub's response is not a release list")?;
    evaluate(current, &releases)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, body: &str) -> GithubRelease {
        GithubRelease {
            tag_name: tag.to_string(),
            html_url: format!("https://example.com/{tag}"),
            body: Some(body.to_string()),
            draft: false,
            prerelease: false,
            published_at: None,
        }
    }

    #[test]
    fn up_to_date_when_nothing_is_newer() {
        let releases = [release("v0.1.0", ""), release("v0.0.9", "")];
        assert_eq!(
            evaluate("0.1.0", &releases).unwrap(),
            UpdateCheck::UpToDate {
                current: "0.1.0".into()
            }
        );
        assert!(matches!(
            evaluate("0.1.0", &[]).unwrap(),
            UpdateCheck::UpToDate { .. }
        ));
    }

    #[test]
    fn picks_the_highest_newer_version() {
        let releases = [
            release("v0.2.0", ""),
            release("v0.10.0", ""),
            release("v0.3.0", ""),
        ];
        let UpdateCheck::Available { latest, url, .. } = evaluate("0.1.0", &releases).unwrap()
        else {
            panic!("expected an update");
        };
        assert_eq!(latest, "0.10.0");
        assert_eq!(url, "https://example.com/v0.10.0");
    }

    #[test]
    fn skips_drafts_prereleases_and_odd_tags() {
        let mut draft = release("v0.3.0", "");
        draft.draft = true;
        let mut pre = release("v0.4.0-beta.1", "");
        pre.prerelease = true;
        let releases = [
            draft,
            pre,
            release("website-2026", ""),
            release("0.2.0", ""),
        ];
        let UpdateCheck::Available { latest, .. } = evaluate("0.1.0", &releases).unwrap() else {
            panic!("expected an update");
        };
        assert_eq!(latest, "0.2.0");
    }

    #[test]
    fn breaking_notes_in_any_skipped_release_count() {
        let releases = [
            release("v0.3.0", "### Added\n\n- Things."),
            release("v0.2.0", "### Breaking\n\n- New database format."),
            release("v0.1.0", "### Breaking\n\n- Old news."),
        ];
        let UpdateCheck::Available { breaking, .. } = evaluate("0.1.0", &releases).unwrap() else {
            panic!("expected an update");
        };
        assert!(breaking);

        let UpdateCheck::Available { breaking, .. } = evaluate("0.2.0", &releases).unwrap() else {
            panic!("expected an update");
        };
        assert!(!breaking, "the running version's own notes do not count");
    }

    #[test]
    fn breaking_heading_must_be_a_heading() {
        assert!(has_breaking_section("## Breaking\n- x"));
        assert!(has_breaking_section("  ### Breaking  "));
        assert!(!has_breaking_section("- Breaking news about nothing"));
        assert!(!has_breaking_section("### Breaking changes are rare"));
    }

    #[test]
    fn invalid_current_version_is_an_error() {
        assert!(evaluate("dev", &[]).is_err());
    }
}
