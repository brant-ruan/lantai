//! What a report version was generated from, and how the current material differs. Pure logic.
//!
//! Every generated version records, per project, the content hash of each slice it used
//! and a hash of the section it wrote. Comparing that with the current material tells
//! which projects are stale; comparing the section hash with the stored section tells
//! whether the user edited it.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::digest::hash_parts;

/// Manifest key of the group of sessions that belong to no project.
const UNASSIGNED_KEY: &str = "unassigned";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Hash of the writing instructions the version was generated with.
    pub prompt_hash: String,
    /// Keyed by [`project_key`], so that renaming a project keeps its entry.
    pub projects: BTreeMap<String, ProjectEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEntry {
    /// Project name when the entry was written (display only; headings use the current name).
    pub name: String,
    /// Slice ID to the slice's content hash.
    pub slices: BTreeMap<i64, String>,
    /// Hash of the section body as generated. A stored section that no longer matches was edited.
    pub section_hash: String,
    /// Name of the engine that wrote the section (`None` in manifests from before it was
    /// recorded). Sections kept by Update keep their writer, so one version can have several.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
}

/// The material a report would be generated from now, in manifest form.
pub type CurrentInputs = BTreeMap<String, ProjectEntry>;

/// The stable key of a project in a manifest.
pub fn project_key(project_id: Option<i64>) -> String {
    match project_id {
        Some(id) => id.to_string(),
        None => UNASSIGNED_KEY.into(),
    }
}

/// The project ID behind a manifest key (`None` for the unassigned group).
pub fn project_id_of(key: &str) -> Option<i64> {
    key.parse().ok()
}

/// Hash of a section body, insensitive to surrounding whitespace.
pub fn section_hash(body: &str) -> String {
    hash_parts(&[body.trim()])
}

impl Manifest {
    pub fn parse(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// The sections one engine wrote in a version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionWriter {
    /// `None` for sections kept from a version whose manifest did not record its engine.
    pub engine: Option<String>,
    /// Project keys, in key order; empty when only the version's engine is known.
    pub keys: Vec<String>,
}

/// Who wrote a version's sections, most sections first. A manifest that records no engine
/// at all (or no manifest) falls back to the version's own engine.
pub fn section_writers(
    manifest: Option<&Manifest>,
    version_engine: Option<&str>,
) -> Vec<SectionWriter> {
    let recorded = manifest.filter(|m| m.projects.values().any(|e| e.engine.is_some()));
    let Some(m) = recorded else {
        return version_engine
            .map(|e| SectionWriter {
                engine: Some(e.to_string()),
                keys: manifest
                    .map(|m| m.projects.keys().cloned().collect())
                    .unwrap_or_default(),
            })
            .into_iter()
            .collect();
    };
    let mut groups: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
    for (key, entry) in &m.projects {
        groups
            .entry(entry.engine.clone())
            .or_default()
            .push(key.clone());
    }
    let mut out: Vec<SectionWriter> = groups
        .into_iter()
        .map(|(engine, keys)| SectionWriter { engine, keys })
        .collect();
    // Known engines before the unrecorded group; among them, the one with most sections first.
    out.sort_by_key(|w| (w.engine.is_none(), std::cmp::Reverse(w.keys.len())));
    out
}

/// How the current material differs from a manifest, by project key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManifestDiff {
    pub unchanged: Vec<String>,
    /// In both, with different slices or slice contents.
    pub changed: Vec<String>,
    /// Only in the current material.
    pub added: Vec<String>,
    /// Only in the manifest: every slice was ignored or moved elsewhere.
    pub removed: Vec<String>,
    pub new_slices: usize,
    pub changed_slices: usize,
    pub removed_slices: usize,
}

impl ManifestDiff {
    pub fn is_stale(&self) -> bool {
        !(self.changed.is_empty() && self.added.is_empty() && self.removed.is_empty())
    }

    /// Projects whose sections have to be written again.
    pub fn to_write(&self) -> Vec<String> {
        self.changed.iter().chain(&self.added).cloned().collect()
    }
}

pub fn diff(manifest: &Manifest, current: &CurrentInputs) -> ManifestDiff {
    let mut d = ManifestDiff::default();
    for (key, entry) in current {
        match manifest.projects.get(key) {
            Some(old) if old.slices == entry.slices => d.unchanged.push(key.clone()),
            Some(_) => d.changed.push(key.clone()),
            None => d.added.push(key.clone()),
        }
    }
    for key in manifest.projects.keys() {
        if !current.contains_key(key) {
            d.removed.push(key.clone());
        }
    }

    // Slice counts across projects, so that a slice moved between projects counts as changed.
    let old: BTreeMap<i64, &String> = manifest
        .projects
        .values()
        .flat_map(|p| p.slices.iter().map(|(id, h)| (*id, h)))
        .collect();
    let now: BTreeMap<i64, &String> = current
        .values()
        .flat_map(|p| p.slices.iter().map(|(id, h)| (*id, h)))
        .collect();
    let moved: BTreeSet<i64> = manifest
        .projects
        .iter()
        .flat_map(|(key, p)| {
            p.slices
                .keys()
                .filter(move |id| current.get(key).is_none_or(|c| !c.slices.contains_key(id)))
        })
        .copied()
        .collect();
    for (id, hash) in &now {
        match old.get(id) {
            None => d.new_slices += 1,
            Some(h) if h != hash || moved.contains(id) => d.changed_slices += 1,
            Some(_) => {}
        }
    }
    d.removed_slices = old.keys().filter(|id| !now.contains_key(id)).count();
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, slices: &[(i64, &str)]) -> ProjectEntry {
        ProjectEntry {
            name: name.into(),
            slices: slices.iter().map(|(id, h)| (*id, h.to_string())).collect(),
            section_hash: String::new(),
            engine: None,
        }
    }

    fn manifest(projects: &[(&str, ProjectEntry)]) -> Manifest {
        Manifest {
            prompt_hash: "p".into(),
            projects: projects
                .iter()
                .map(|(k, e)| (k.to_string(), e.clone()))
                .collect(),
        }
    }

    #[test]
    fn keys_survive_renames_and_cover_unassigned() {
        assert_eq!(project_key(Some(7)), "7");
        assert_eq!(project_key(None), "unassigned");
        assert_eq!(project_id_of("7"), Some(7));
        assert_eq!(project_id_of("unassigned"), None);
    }

    #[test]
    fn identical_inputs_are_not_stale() {
        let m = manifest(&[("1", entry("A", &[(10, "h")]))]);
        let current: CurrentInputs = m.projects.clone();
        let d = diff(&m, &current);
        assert!(!d.is_stale());
        assert_eq!(d.unchanged, vec!["1"]);
    }

    #[test]
    fn classifies_projects_and_counts_slices() {
        let m = manifest(&[
            ("1", entry("A", &[(10, "a"), (11, "b")])),
            ("2", entry("B", &[(20, "c")])),
            ("3", entry("C", &[(30, "d")])),
        ]);
        let current: CurrentInputs = [
            // A: slice 11 grew, slice 12 is new.
            (
                "1".to_string(),
                entry("A", &[(10, "a"), (11, "b2"), (12, "e")]),
            ),
            // B unchanged.
            ("2".to_string(), entry("B", &[(20, "c")])),
            // C's only slice moved to the unassigned group.
            ("unassigned".to_string(), entry("Unassigned", &[(30, "d")])),
        ]
        .into_iter()
        .collect();
        let d = diff(&m, &current);
        assert_eq!(d.changed, vec!["1"]);
        assert_eq!(d.unchanged, vec!["2"]);
        assert_eq!(d.added, vec!["unassigned"]);
        assert_eq!(d.removed, vec!["3"]);
        assert_eq!(
            (d.new_slices, d.changed_slices, d.removed_slices),
            (1, 2, 0)
        );
        assert_eq!(d.to_write(), vec!["1", "unassigned"]);
        assert!(d.is_stale());
    }

    #[test]
    fn ignored_slice_counts_as_removed() {
        let m = manifest(&[("1", entry("A", &[(10, "a"), (11, "b")]))]);
        let current: CurrentInputs = [("1".to_string(), entry("A", &[(10, "a")]))]
            .into_iter()
            .collect();
        let d = diff(&m, &current);
        assert_eq!(d.changed, vec!["1"]);
        assert_eq!(d.removed_slices, 1);
    }

    fn written_by(mut e: ProjectEntry, engine: &str) -> ProjectEntry {
        e.engine = Some(engine.into());
        e
    }

    #[test]
    fn writers_group_sections_by_engine() {
        let m = manifest(&[
            ("1", written_by(entry("A", &[]), "x")),
            ("2", written_by(entry("B", &[]), "y")),
            ("3", written_by(entry("C", &[]), "y")),
            ("4", entry("D", &[])),
        ]);
        let w = section_writers(Some(&m), Some("y"));
        let summary: Vec<(Option<&str>, Vec<&str>)> = w
            .iter()
            .map(|w| {
                (
                    w.engine.as_deref(),
                    w.keys.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (Some("y"), vec!["2", "3"]),
                (Some("x"), vec!["1"]),
                (None, vec!["4"]),
            ]
        );
    }

    #[test]
    fn writers_fall_back_to_the_version_engine() {
        let m = manifest(&[("1", entry("A", &[])), ("2", entry("B", &[]))]);
        let w = section_writers(Some(&m), Some("v"));
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].engine.as_deref(), Some("v"));
        assert_eq!(w[0].keys, vec!["1", "2"]);
        assert_eq!(
            section_writers(None, Some("v"))[0].keys,
            Vec::<String>::new()
        );
        assert!(section_writers(Some(&m), None).is_empty());
        assert!(section_writers(None, None).is_empty());
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let m = manifest(&[("1", entry("A", &[(10, "a")]))]);
        assert_eq!(Manifest::parse(&m.to_json()), Some(m.clone()));
        // Older manifests have no engine; it is left out when absent.
        assert!(!m.to_json().contains("engine"));
        let written = manifest(&[("1", written_by(entry("A", &[]), "x"))]);
        assert_eq!(Manifest::parse(&written.to_json()), Some(written));
        assert_eq!(Manifest::parse("not json"), None);
        assert_eq!(section_hash("- a\n"), section_hash("  - a"));
    }
}
