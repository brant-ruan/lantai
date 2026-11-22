//! Progress of a report generation, as structured events: the command line prints them in
//! English, the desktop app sends them to the frontend, which words them in the interface
//! language.

use std::fmt;

use serde::Serialize;

/// One step of a generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "stage",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Progress {
    /// Stage 1 starts: this many slices need a digest.
    DigestsPending { pending: usize, cached: usize },
    /// One slice was digested; `parts` when it was split, `incremental` when only new
    /// turns were added to its previous digest.
    SliceDigested {
        slice_id: i64,
        title: String,
        parts: Option<usize>,
        incremental: bool,
    },
    /// Projects of a day are being clustered into tasks before the report is written.
    ClusteringProjects,
    /// Busy days of each project are being clustered (weekly reports).
    ClusteringDays,
    /// The daily report is being written, from this much material; `sections` is the
    /// number of changed sections when only those are rewritten.
    WritingDaily {
        sections: Option<usize>,
        chars: usize,
    },
    /// Project sections of a weekly or monthly report are being written.
    WritingProjects { count: usize, concurrency: usize },
    /// One project section of a weekly or monthly report is done.
    ProjectWritten { project: String, ok: bool },
    /// The model's report broke the required structure; asking it to rewrite.
    RepairingStructure,
}

impl fmt::Display for Progress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DigestsPending { pending, cached } => write!(
                f,
                "Stage 1: {pending} slices need a digest ({cached} cached)"
            ),
            Self::SliceDigested {
                slice_id,
                title,
                parts,
                incremental,
            } => {
                write!(f, "  S{slice_id} {title}")?;
                match (parts, incremental) {
                    (Some(n), _) => write!(f, " ({n} parts)"),
                    (None, true) => write!(f, " (updated with new turns)"),
                    (None, false) => Ok(()),
                }
            }
            Self::ClusteringProjects => {
                write!(
                    f,
                    "Stage 2: clustering multi-session projects before writing the report"
                )
            }
            Self::ClusteringDays => write!(f, "Clustering busy days"),
            Self::WritingDaily { sections, chars } => match sections {
                Some(n) => write!(
                    f,
                    "Stage 2: writing {n} changed project sections ({chars} chars of material)"
                ),
                None => write!(
                    f,
                    "Stage 2: writing the daily report ({chars} chars of material)"
                ),
            },
            Self::WritingProjects { count, concurrency } => write!(
                f,
                "Writing {count} project section(s), {concurrency} at a time"
            ),
            Self::ProjectWritten { project, ok } => {
                write!(f, "  {project}: {}", if *ok { "written" } else { "failed" })
            }
            Self::RepairingStructure => {
                write!(
                    f,
                    "Report structure invalid, requesting a structured rewrite"
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_a_stage_code_with_parameters() {
        let p = Progress::SliceDigested {
            slice_id: 12,
            title: "fix login".into(),
            parts: None,
            incremental: true,
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap(),
            serde_json::json!({
                "stage": "sliceDigested",
                "sliceId": 12,
                "title": "fix login",
                "parts": null,
                "incremental": true
            })
        );
        assert_eq!(p.to_string(), "  S12 fix login (updated with new turns)");
        assert_eq!(
            serde_json::to_value(Progress::ClusteringDays).unwrap(),
            serde_json::json!({ "stage": "clusteringDays" })
        );
    }
}
