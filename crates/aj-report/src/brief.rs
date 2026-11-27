//! Cross-session clustering of one project's daily material into task clusters.
//!
//! A project with many slices, or a day whose material exceeds the stage 2 budget, is
//! condensed first: the model groups the project's slices into a few task clusters
//! (structured JSON), and the report is written from the rendered clusters instead of
//! the full material. Clusters keep the source anchors, so traceability is unaffected,
//! and they are the unit later layers (workstreams, weekly reports) build on.

use std::fmt::Write;

use aj_core::INTERNAL_TASK_FINGERPRINT;
use aj_store::ProjectBriefRow;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::digest::{extract_json_object, hash_parts};
use crate::engine::Engine;
use crate::lang::Language;
use crate::material::ProjectMaterial;

/// Bump when the project brief prompt or the cluster structure changes, to invalidate cached briefs.
pub const BRIEF_PROMPT_VERSION: &str = "6";

/// Condense a project into task clusters before stage 2 once it has at least this many slices.
pub const SLICE_CLUSTER_THRESHOLD: usize = 5;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClusterPoint {
    pub text: String,
    /// Source anchors such as `S12.T5` or `S12`.
    #[serde(default)]
    pub anchors: Vec<String>,
}

/// One task of a project on one day, possibly spanning several sessions.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskCluster {
    pub title: String,
    #[serde(default)]
    pub points: Vec<ClusterPoint>,
    #[serde(default)]
    pub slice_ids: Vec<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectClusters {
    pub clusters: Vec<TaskCluster>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedBrief {
    pub project: String,
    pub input_hash: String,
    pub clusters_json: String,
}

impl CachedBrief {
    /// The briefs among stored `rows` that were written in `lang`; the others are misses.
    pub fn usable(rows: Vec<ProjectBriefRow>, lang: &Language) -> Vec<CachedBrief> {
        rows.into_iter()
            .filter(|b| b.language == lang.tag)
            .map(|b| CachedBrief {
                project: b.project,
                input_hash: b.input_hash,
                clusters_json: b.clusters_json,
            })
            .collect()
    }

    /// The row that stores this brief, written in `lang` by the engine named `engine_name`.
    pub fn to_row(&self, lang: &Language, engine_name: &str) -> ProjectBriefRow {
        ProjectBriefRow {
            project: self.project.clone(),
            input_hash: self.input_hash.clone(),
            clusters_json: self.clusters_json.clone(),
            language: lang.tag.to_string(),
            engine: Some(engine_name.to_string()),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Condensed {
    /// Number of projects whose material was replaced by clusters (cached or generated).
    pub projects: usize,
    pub cached: usize,
    pub generated: usize,
    /// Newly generated briefs to persist in cache, with the index of their material.
    pub to_save: Vec<(usize, CachedBrief)>,
    /// Projects whose clustering failed and whose full material was kept, with the reason.
    pub failures: Vec<String>,
}

pub fn needs_clustering(material: &ProjectMaterial) -> bool {
    material.slice_count >= SLICE_CLUSTER_THRESHOLD
}

/// Parse the clusters from model output, rejecting an empty result.
pub fn parse_clusters(output: &str) -> Result<ProjectClusters> {
    let parsed: ProjectClusters = serde_json::from_str(extract_json_object(output)?)
        .context("failed to parse the task clusters JSON")?;
    anyhow::ensure!(
        parsed.clusters.iter().any(|c| !c.title.trim().is_empty()),
        "the model returned no task clusters"
    );
    Ok(parsed)
}

/// Render clusters as stage 2 material.
pub fn render_clusters(clusters: &ProjectClusters) -> String {
    let mut out = String::from("\nTask clusters condensed from this project's material:\n");
    for c in &clusters.clusters {
        let _ = write!(out, "- {}", c.title.trim());
        if !c.slice_ids.is_empty() {
            let slices: Vec<String> = c.slice_ids.iter().map(|id| format!("S{id}")).collect();
            let _ = write!(out, " (slices: {})", slices.join(", "));
        }
        out.push('\n');
        for p in &c.points {
            let _ = write!(out, "  - {}", p.text.trim());
            for a in &p.anchors {
                let a = a.trim().trim_matches(['[', ']']).trim_start_matches('#');
                if !a.is_empty() {
                    let _ = write!(out, " [#{a}]");
                }
            }
            out.push('\n');
        }
    }
    out
}

fn brief_hash(engine_name: &str, material: &ProjectMaterial) -> String {
    hash_parts(&[
        BRIEF_PROMPT_VERSION,
        engine_name,
        &material.project,
        &material.identity,
    ])
}

/// Clusters for one material: from the cache, or from the model.
enum Outcome {
    Cached(ProjectClusters),
    Generated(String, ProjectClusters),
    Failed(String),
}

fn cluster_one(
    engine: &dyn Engine,
    engine_name: &str,
    lang: &Language,
    material: &ProjectMaterial,
    cached: &[CachedBrief],
) -> Outcome {
    let input_hash = brief_hash(engine_name, material);
    let hit = cached
        .iter()
        .find(|c| c.project == material.project && c.input_hash == input_hash)
        .and_then(|c| serde_json::from_str::<ProjectClusters>(&c.clusters_json).ok());
    if let Some(clusters) = hit {
        return Outcome::Cached(clusters);
    }
    match engine
        .complete(&build_prompt(lang, &material.project, &material.text))
        .and_then(|out| parse_clusters(&out))
    {
        Ok(clusters) => Outcome::Generated(input_hash, clusters),
        Err(e) => Outcome::Failed(format!("{}: {e:#}", material.project)),
    }
}

/// Condense the materials at `indices`, all at once (they are independent requests).
#[allow(clippy::too_many_arguments)]
fn condense_many(
    engine: &dyn Engine,
    engine_name: &str,
    lang: &Language,
    materials: &mut [ProjectMaterial],
    indices: &[usize],
    cached: &[CachedBrief],
    result: &mut Condensed,
    done: &mut [bool],
) {
    let shared: &[ProjectMaterial] = materials;
    let outcomes: Vec<(usize, Outcome)> = std::thread::scope(|scope| {
        let handles: Vec<_> = indices
            .iter()
            .map(|&i| {
                scope.spawn(move || {
                    (
                        i,
                        cluster_one(engine, engine_name, lang, &shared[i], cached),
                    )
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("clustering thread panicked"))
            .collect()
    });
    for (i, outcome) in outcomes {
        done[i] = true;
        let material = &mut materials[i];
        match outcome {
            Outcome::Cached(clusters) => {
                material.text = render_clusters(&clusters);
                result.projects += 1;
                result.cached += 1;
            }
            Outcome::Generated(input_hash, clusters) => {
                material.text = render_clusters(&clusters);
                result.to_save.push((
                    i,
                    CachedBrief {
                        project: material.project.clone(),
                        input_hash,
                        clusters_json: serde_json::to_string(&clusters).unwrap_or_default(),
                    },
                ));
                result.projects += 1;
                result.generated += 1;
            }
            Outcome::Failed(reason) => result.failures.push(reason),
        }
    }
}

fn total_chars(materials: &[ProjectMaterial]) -> usize {
    materials.iter().map(|m| m.text.chars().count()).sum()
}

/// Condense projects with many slices into task clusters, and if the total material
/// still exceeds `budget`, condense the largest remaining projects until it fits.
/// `cached` must hold only briefs written in `lang`.
pub fn condense_if_needed(
    engine: &dyn Engine,
    engine_name: &str,
    lang: &Language,
    materials: &mut [ProjectMaterial],
    cached: &[CachedBrief],
    budget: usize,
) -> Condensed {
    let mut result = Condensed::default();
    let mut done = vec![false; materials.len()];

    let busy: Vec<usize> = (0..materials.len())
        .filter(|&i| needs_clustering(&materials[i]))
        .collect();
    condense_many(
        engine,
        engine_name,
        lang,
        materials,
        &busy,
        cached,
        &mut result,
        &mut done,
    );

    // Still too large: condense the largest remaining materials, so the fewest lose
    // detail. Pick, largest first, enough of them to fit if each shrank to nothing, and
    // run those at once; repeat in case the clusters themselves are still too large.
    loop {
        let total = total_chars(materials);
        if total <= budget {
            break;
        }
        let mut order: Vec<usize> = (0..materials.len()).filter(|&i| !done[i]).collect();
        if order.is_empty() {
            break;
        }
        order.sort_by_key(|&i| std::cmp::Reverse(materials[i].text.chars().count()));
        let mut over = total - budget;
        let mut pick = Vec::new();
        for i in order {
            pick.push(i);
            let size = materials[i].text.chars().count();
            if size >= over {
                break;
            }
            over -= size;
        }
        condense_many(
            engine,
            engine_name,
            lang,
            materials,
            &pick,
            cached,
            &mut result,
            &mut done,
        );
    }
    result
}

fn build_prompt(lang: &Language, project: &str, material: &str) -> String {
    format!(
        "{INTERNAL_TASK_FINGERPRINT}\n{}\n{}\n\nMaterial:\n\n## Project: {project}\n{material}",
        lang.prompts().project_brief.trim_end(),
        lang.json_instruction()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testing::MockEngine;
    use crate::lang::EN;

    const CLUSTERS: &str = r##"{"clusters":[{"title":"Fix login","points":[{"text":"use a lock","anchors":["S1.T2","#S3"]}],"slice_ids":[1,3]}]}"##;

    fn pm(project: &str, text: String, slice_count: usize) -> ProjectMaterial {
        ProjectMaterial {
            project: project.into(),
            identity: format!("id-{project}-{}", text.len()),
            text,
            slice_count,
        }
    }

    fn materials() -> Vec<ProjectMaterial> {
        vec![
            pm("small", "s".repeat(100), 1),
            pm("huge", "h".repeat(5_000), 2),
            pm("big", "b".repeat(2_000), 1),
        ]
    }

    #[test]
    fn parses_and_renders_clusters_with_normalized_anchors() {
        let c = parse_clusters(&format!("```json\n{CLUSTERS}\n```")).unwrap();
        assert_eq!(c.clusters[0].slice_ids, vec![1, 3]);
        let text = render_clusters(&c);
        assert!(text.contains("- Fix login (slices: S1, S3)\n"));
        assert!(text.contains("  - use a lock [#S1.T2] [#S3]\n"));
        assert!(!text.contains("status"));
        assert!(parse_clusters(r#"{"clusters":[]}"#).is_err());
        assert!(parse_clusters("- a markdown list").is_err());
    }

    #[test]
    fn material_within_budget_is_left_alone() {
        let e = MockEngine::new(|_| Ok(CLUSTERS.into()));
        let mut m = materials();
        let r = condense_if_needed(&e, "mock", &EN, &mut m, &[], 10_000);
        assert_eq!(r, Condensed::default());
        assert_eq!(e.call_count(), 0);
        assert_eq!(m, materials());
    }

    #[test]
    fn largest_projects_are_condensed_first_until_it_fits() {
        let e = MockEngine::new(|_| Ok(CLUSTERS.into()));
        let mut m = materials();
        let r = condense_if_needed(&e, "mock", &EN, &mut m, &[], 3_000);
        assert_eq!((r.projects, r.cached, r.generated), (1, 0, 1));
        assert_eq!(r.to_save.len(), 1);
        assert!(m[1].text.contains("[#S1.T2]"));
        assert_eq!(m[2].text.len(), 2_000);
        assert!(e.prompts.lock().unwrap()[0].contains("## Project: huge"));

        // Re-running with the saved clusters hits the cache and avoids calling the model.
        assert_eq!(r.to_save[0].0, 1);
        let saved: Vec<CachedBrief> = r.to_save.iter().map(|(_, b)| b.clone()).collect();
        let mut again = materials();
        let r_cached = condense_if_needed(&e, "mock", &EN, &mut again, &saved, 3_000);
        assert_eq!((r_cached.cached, r_cached.generated), (1, 0));
        assert_eq!(e.call_count(), 1);
        assert_eq!(again, m);

        // Display details may change freely; only the identity decides the cache.
        let mut retimed = materials();
        retimed[1].text.push_str(" (now 12 active min)");
        let r_retimed = condense_if_needed(&e, "mock", &EN, &mut retimed, &saved, 3_000);
        assert_eq!((r_retimed.cached, r_retimed.generated), (1, 0));

        let mut changed = materials();
        changed[1].identity.push_str("-new digest");
        let r_miss = condense_if_needed(&e, "mock", &EN, &mut changed, &saved, 3_000);
        assert_eq!((r_miss.cached, r_miss.generated), (0, 1));
        assert_eq!(e.call_count(), 2);
    }

    #[test]
    fn failed_or_unparseable_clusters_keep_the_full_material() {
        let e = MockEngine::new(|_| Ok("- not json".into()));
        let mut m = materials();
        let r = condense_if_needed(&e, "mock", &EN, &mut m, &[], 3_000);
        assert_eq!(r.projects, 0);
        assert_eq!(r.failures.len(), 3);
        assert_eq!(m, materials());
    }

    #[test]
    fn project_with_many_slices_is_clustered_even_within_budget() {
        let e = MockEngine::new(|_| Ok(CLUSTERS.into()));
        let mut m = vec![
            pm("small", "x".repeat(10), 1),
            pm("busy", "y".repeat(10), SLICE_CLUSTER_THRESHOLD),
        ];
        let r = condense_if_needed(&e, "mock", &EN, &mut m, &[], 50_000);
        assert_eq!((r.projects, r.generated), (1, 1));
        assert!(m[1].text.contains("Fix login"));
        assert_eq!(m[0].text, "x".repeat(10));
    }
}
