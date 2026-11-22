//! One-way Markdown export with configurable source-anchor rendering and external-edit detection.
//!
//! Tracks the SHA-256 hash of each exported file in `<data_dir>/exports.json` so re-exporting
//! asks before overwriting a file that was edited outside Lantai.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{bail, Context, Result};
use regex::Regex;
use sha2::{Digest as _, Sha256};

use crate::digest::hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnchorExportMode {
    #[default]
    Keep,
    Footnotes,
    Strip,
}

impl AnchorExportMode {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "keep" => Ok(Self::Keep),
            "footnotes" => Ok(Self::Footnotes),
            "strip" => Ok(Self::Strip),
            other => bail!("unknown anchor export mode: {other}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportOutcome {
    Written {
        path: PathBuf,
        bytes: usize,
    },
    Conflict {
        path: PathBuf,
        existing_markdown: String,
    },
}

fn canonical_anchor_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r" ?\[#?S(\d+)(?:\.T(\d+))?\]").expect("canonical anchor regex"))
}

/// Transform canonical `[#S12]` / `[#S12.T5]` source anchors according to `mode`.
pub fn format_anchors(markdown: &str, mode: AnchorExportMode) -> String {
    match mode {
        AnchorExportMode::Keep => markdown.to_string(),
        AnchorExportMode::Strip => canonical_anchor_re().replace_all(markdown, "").into_owned(),
        AnchorExportMode::Footnotes => {
            let mut footnotes: Vec<(String, String)> = Vec::new();
            let mut seen: HashSet<String> = HashSet::new();
            let body = canonical_anchor_re().replace_all(markdown, |caps: &regex::Captures| {
                let lead = if caps[0].starts_with(' ') { " " } else { "" };
                let slice = &caps[1];
                let (label, desc) = match caps.get(2) {
                    Some(turn) => (
                        format!("S{slice}.T{}", turn.as_str()),
                        format!("Slice S{slice}, turn T{}", turn.as_str()),
                    ),
                    None => (format!("S{slice}"), format!("Slice S{slice}")),
                };
                if seen.insert(label.clone()) {
                    footnotes.push((label.clone(), desc));
                }
                format!("{lead}[^{label}]")
            });
            if footnotes.is_empty() {
                return body.into_owned();
            }
            let mut out = body.trim_end().to_string();
            out.push_str("\n\n## Sources\n\n");
            for (label, desc) in footnotes {
                out.push_str(&format!("[^{label}]: {desc}\n"));
            }
            out
        }
    }
}

pub fn default_daily_export_path(base_dir: &Path, date: &str) -> PathBuf {
    base_dir.join("daily").join(format!("{date}.md"))
}

pub fn default_weekly_export_path(base_dir: &Path, week: &str) -> PathBuf {
    base_dir.join("weekly").join(format!("{week}.md"))
}

pub fn default_monthly_export_path(base_dir: &Path, month: &str) -> PathBuf {
    base_dir.join("monthly").join(format!("{month}.md"))
}

pub fn default_project_export_path(
    base_dir: &Path,
    project_name: &str,
    period_key: &str,
) -> PathBuf {
    let safe_project = project_name.replace(['/', '\\'], "-");
    base_dir
        .join("projects")
        .join(safe_project)
        .join(format!("{period_key}-summary.md"))
}

/// Export `markdown` to `target_path`, returning `Conflict` if `target_path` exists and was modified externally.
pub fn export_markdown(
    data_dir: &Path,
    target_path: &Path,
    markdown: &str,
    mode: AnchorExportMode,
    force: bool,
) -> Result<ExportOutcome> {
    let content = format_anchors(markdown, mode);
    let new_hash = hex(&Sha256::digest(content.as_bytes()));
    let manifest_path = data_dir.join("exports.json");
    let mut manifest = load_manifest(&manifest_path)?;

    if target_path.exists() {
        let existing_markdown = std::fs::read_to_string(target_path)
            .with_context(|| format!("failed to read {}", target_path.display()))?;
        let existing_hash = hex(&Sha256::digest(existing_markdown.as_bytes()));
        let key = export_key(target_path);
        let recorded = manifest.get(&key).map(String::as_str);
        if !force && existing_hash != new_hash && recorded != Some(existing_hash.as_str()) {
            return Ok(ExportOutcome::Conflict {
                path: target_path.to_path_buf(),
                existing_markdown,
            });
        }
    }

    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(target_path, &content)
        .with_context(|| format!("failed to write {}", target_path.display()))?;

    let key = export_key(target_path);
    manifest.insert(key, new_hash);
    save_manifest(&manifest_path, &manifest)?;

    Ok(ExportOutcome::Written {
        path: target_path.to_path_buf(),
        bytes: content.len(),
    })
}

fn export_key(path: &Path) -> String {
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        if let Ok(canon) = std::fs::canonicalize(parent) {
            return canon.join(name).to_string_lossy().into_owned();
        }
    }
    path.to_string_lossy().into_owned()
}

fn load_manifest(path: &Path) -> Result<BTreeMap<String, String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .with_context(|| format!("failed to parse {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn save_manifest(path: &Path, manifest: &BTreeMap<String, String>) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(manifest)?;
    std::fs::write(path, json).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_anchors_for_keep_strip_and_footnotes() {
        let md = "# Daily\n- Fixed login [#S12.T5] [#S12]\n- Reviewed PR [#S12.T5]\n";
        assert_eq!(format_anchors(md, AnchorExportMode::Keep), md);
        assert_eq!(
            format_anchors(md, AnchorExportMode::Strip),
            "# Daily\n- Fixed login\n- Reviewed PR\n"
        );
        assert_eq!(
            format_anchors(md, AnchorExportMode::Footnotes),
            "# Daily\n- Fixed login [^S12.T5] [^S12]\n- Reviewed PR [^S12.T5]\n\n## Sources\n\n[^S12.T5]: Slice S12, turn T5\n[^S12]: Slice S12\n"
        );
    }

    #[test]
    fn detects_external_edits_and_allows_clean_reexports() {
        let root = std::env::temp_dir().join(format!("aj-export-test-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let data_dir = root.join("data");
        let out_path = default_daily_export_path(&root.join("WorkJournal"), "2026-11-16");

        // Initial export writes the file cleanly.
        let first = export_markdown(
            &data_dir,
            &out_path,
            "# Report v1 [#S1]\n",
            AnchorExportMode::Keep,
            false,
        )
        .unwrap();
        assert!(matches!(first, ExportOutcome::Written { .. }));

        // Re-exporting an updated version succeeds because the file matches our recorded hash.
        let second = export_markdown(
            &data_dir,
            &out_path,
            "# Report v2 [#S1]\n",
            AnchorExportMode::Keep,
            false,
        )
        .unwrap();
        assert!(matches!(second, ExportOutcome::Written { .. }));

        // External edit triggers a conflict unless force is true.
        std::fs::write(&out_path, "# Hand-edited externally\n").unwrap();
        let conflict = export_markdown(
            &data_dir,
            &out_path,
            "# Report v3 [#S1]\n",
            AnchorExportMode::Keep,
            false,
        )
        .unwrap();
        assert_eq!(
            conflict,
            ExportOutcome::Conflict {
                path: out_path.clone(),
                existing_markdown: "# Hand-edited externally\n".into()
            }
        );

        let forced = export_markdown(
            &data_dir,
            &out_path,
            "# Report v3 [#S1]\n",
            AnchorExportMode::Strip,
            true,
        )
        .unwrap();
        assert!(matches!(forced, ExportOutcome::Written { .. }));
        assert_eq!(std::fs::read_to_string(&out_path).unwrap(), "# Report v3\n");

        std::fs::remove_dir_all(&root).ok();
    }
}
