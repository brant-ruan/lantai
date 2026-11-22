//! Which lines of a report the user wrote rather than the model. Pure logic.
//!
//! The baseline of each `## <project>` section is the text the model wrote for it: the
//! manifest records a hash of every section as generated (kept across user edits and
//! across updates that leave the project alone), so the newest version whose section
//! hashes to one of those values holds the generated text. The displayed section is then
//! compared with it line by line.

use std::collections::BTreeSet;

use crate::manifest::{self, Manifest};
use crate::structure;
use crate::textsim::similarity;

/// How the displayed report differs from what the model wrote. Line numbers are 0-based
/// indexes into the displayed Markdown split on `\n`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportEdits {
    /// Lines the user added or rewrote, in order.
    pub lines: Vec<LineEdit>,
    /// Generated lines the user deleted, per section.
    pub removed: Vec<RemovedLines>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEdit {
    pub line: usize,
    /// The generated line this one rewrote; `None` for a line the user added.
    pub original: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedLines {
    /// Line of the section's `## <project>` heading.
    pub heading_line: usize,
    pub lines: Vec<String>,
}

impl ReportEdits {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.removed.is_empty()
    }
}

/// Two lines in the same changed region count as a rewrite of one another from this
/// similarity up; below it they are an addition and a deletion.
const REWRITE_SIMILARITY: f64 = 0.3;

/// The user's edits in `display` (the version as shown: canonical title, sections in
/// project order). `manifest_json` is the version's manifest and `history` the markdown
/// of the report's versions, newest first, which must include the version itself.
/// `None` when the version records no manifest, so nothing can be told apart.
pub fn report_edits(
    display: &str,
    manifest_json: Option<&str>,
    history: &[&str],
) -> Option<ReportEdits> {
    let manifest = Manifest::parse(manifest_json?)?;
    let generated: BTreeSet<&str> = manifest
        .projects
        .values()
        .map(|e| e.section_hash.as_str())
        .filter(|h| !h.is_empty())
        .collect();

    let mut out = ReportEdits::default();
    for section in display_sections(display) {
        let Some(baseline) = history.iter().find_map(|md| {
            structure::extract_project_section(md, &section.project)
                .filter(|body| generated.contains(manifest::section_hash(body).as_str()))
        }) else {
            continue;
        };
        let base: Vec<&str> = baseline
            .lines()
            .map(str::trim_end)
            .filter(|l| comparable(l))
            .collect();
        let shown: Vec<&str> = section.lines.iter().map(|(_, l)| *l).collect();
        let (edits, removed) = diff_lines(&base, &shown);
        out.lines
            .extend(edits.into_iter().map(|(i, original)| LineEdit {
                line: section.lines[i].0,
                original: original.map(str::to_string),
            }));
        if !removed.is_empty() {
            out.removed.push(RemovedLines {
                heading_line: section.heading_line,
                lines: removed.into_iter().map(str::to_string).collect(),
            });
        }
    }
    Some(out)
}

/// Lines that take part in the comparison: not blank, and not the stats line the program
/// writes under weekly and monthly headings.
fn comparable(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && !t.starts_with("> ")
}

struct DisplaySection<'a> {
    project: String,
    heading_line: usize,
    /// Comparable lines with their line numbers.
    lines: Vec<(usize, &'a str)>,
}

fn display_sections(display: &str) -> Vec<DisplaySection<'_>> {
    let mut out: Vec<DisplaySection> = Vec::new();
    for (idx, raw) in display.split('\n').enumerate() {
        let line = raw.trim_end_matches('\r').trim_end();
        let trimmed = line.trim();
        if let Some(h2) = trimmed.strip_prefix("## ") {
            out.push(DisplaySection {
                project: structure::strip_heading_wrappers(h2),
                heading_line: idx,
                lines: Vec::new(),
            });
            continue;
        }
        if trimmed.starts_with("# ") {
            continue;
        }
        if let Some(sec) = out.last_mut() {
            if comparable(line) {
                sec.lines.push((idx, line));
            }
        }
    }
    out
}

/// Compare `shown` with `base`. Returns the indexes into `shown` of the lines the user
/// wrote (with the base line each one rewrote, if any) and the base lines deleted.
fn diff_lines<'a>(
    base: &[&'a str],
    shown: &[&'a str],
) -> (Vec<(usize, Option<&'a str>)>, Vec<&'a str>) {
    let (n, m) = (base.len(), shown.len());
    // lcs[i][j]: length of the longest common subsequence of base[i..] and shown[j..].
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if base[i] == shown[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }

    let mut edits = Vec::new();
    let mut removed = Vec::new();
    let (mut deleted, mut inserted): (Vec<&str>, Vec<usize>) = (Vec::new(), Vec::new());
    let mut flush = |deleted: &mut Vec<&'a str>, inserted: &mut Vec<usize>| {
        pair_region(deleted, inserted, shown, &mut edits, &mut removed);
        deleted.clear();
        inserted.clear();
    };
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && base[i] == shown[j] {
            flush(&mut deleted, &mut inserted);
            i += 1;
            j += 1;
        } else if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
            inserted.push(j);
            j += 1;
        } else {
            deleted.push(base[i]);
            i += 1;
        }
    }
    flush(&mut deleted, &mut inserted);
    (edits, removed)
}

/// Within one changed region, pair deleted and inserted lines in order as rewrites while
/// they are similar enough; the rest are additions and deletions.
fn pair_region<'a>(
    deleted: &[&'a str],
    inserted: &[usize],
    shown: &[&'a str],
    edits: &mut Vec<(usize, Option<&'a str>)>,
    removed: &mut Vec<&'a str>,
) {
    let mut d = 0;
    for &j in inserted {
        let pair = deleted[d..]
            .iter()
            .position(|old| similarity(old, shown[j]) >= REWRITE_SIMILARITY);
        match pair {
            Some(k) => {
                removed.extend_from_slice(&deleted[d..d + k]);
                edits.push((j, Some(deleted[d + k])));
                d += k + 1;
            }
            None => edits.push((j, None)),
        }
    }
    removed.extend_from_slice(&deleted[d..]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{Manifest, ProjectEntry};

    fn manifest_for(sections: &[&str]) -> String {
        let mut m = Manifest::default();
        for (i, body) in sections.iter().enumerate() {
            m.projects.insert(
                i.to_string(),
                ProjectEntry {
                    name: format!("p{i}"),
                    section_hash: manifest::section_hash(body),
                    ..Default::default()
                },
            );
        }
        m.to_json()
    }

    const GENERATED: &str = "# Title\n\n## App\n- Built the search view [#S1.T2]\n  - Scans with LIKE\n- Fixed the menu\n\n## Docs\n- Wrote the spec";

    #[test]
    fn unedited_report_has_no_edits() {
        let manifest = manifest_for(&[
            "- Built the search view [#S1.T2]\n  - Scans with LIKE\n- Fixed the menu",
            "- Wrote the spec",
        ]);
        let edits = report_edits(GENERATED, Some(&manifest), &[GENERATED]).unwrap();
        assert!(edits.is_empty());
    }

    #[test]
    fn marks_rewritten_added_and_removed_lines() {
        let manifest = manifest_for(&[
            "- Built the search view [#S1.T2]\n  - Scans with LIKE\n- Fixed the menu",
            "- Wrote the spec",
        ]);
        let edited = "# Title\n\n## App\n- Built the search view for reports [#S1.T2]\n  - Scans with LIKE\n  - Shipped to everyone\n\n## Docs\n- Wrote the spec";
        let edits = report_edits(edited, Some(&manifest), &[edited, GENERATED]).unwrap();
        assert_eq!(
            edits.lines,
            vec![
                LineEdit {
                    line: 3,
                    original: Some("- Built the search view [#S1.T2]".into()),
                },
                LineEdit {
                    line: 5,
                    original: None,
                },
            ]
        );
        assert_eq!(
            edits.removed,
            vec![RemovedLines {
                heading_line: 2,
                lines: vec!["- Fixed the menu".into()],
            }]
        );
    }

    #[test]
    fn baseline_survives_an_update_that_copied_the_edit() {
        // v1 generated, v2 edited App, v3 an update that rewrote only Docs and copied the
        // edited App section verbatim. App's manifest entry still carries v1's hash.
        let v1 = GENERATED;
        let v2 = "# Title\n\n## App\n- Built the search view [#S1.T2]\n  - Scans with LIKE\n- Fixed the menu bar shortcuts\n\n## Docs\n- Wrote the spec";
        let v3 = "# Title\n\n## App\n- Built the search view [#S1.T2]\n  - Scans with LIKE\n- Fixed the menu bar shortcuts\n\n## Docs\n- Wrote the spec and the changelog";
        let manifest = manifest_for(&[
            "- Built the search view [#S1.T2]\n  - Scans with LIKE\n- Fixed the menu",
            "- Wrote the spec and the changelog",
        ]);
        let edits = report_edits(v3, Some(&manifest), &[v3, v2, v1]).unwrap();
        assert_eq!(
            edits.lines,
            vec![LineEdit {
                line: 5,
                original: Some("- Fixed the menu".into()),
            }]
        );
        assert!(edits.removed.is_empty());
    }

    #[test]
    fn section_without_baseline_is_not_marked() {
        let manifest = manifest_for(&["- Wrote the spec"]);
        let md = "# Title\n\n## Garden\n- Planted tomatoes\n\n## Docs\n- Wrote the spec";
        let edits = report_edits(md, Some(&manifest), &[md]).unwrap();
        assert!(edits.is_empty());
        assert!(report_edits(md, None, &[md]).is_none());
    }

    #[test]
    fn dissimilar_lines_are_added_and_removed_not_rewritten() {
        let (edits, removed) = diff_lines(
            &["- Fixed the menu"],
            &["- Talked through the experiment plan with my advisor"],
        );
        assert_eq!(edits, vec![(0, None)]);
        assert_eq!(removed, vec!["- Fixed the menu"]);
    }

    #[test]
    fn stats_lines_are_ignored() {
        let (edits, removed) = diff_lines(&["- A"], &["- A"]);
        assert!(edits.is_empty() && removed.is_empty());
        assert!(!comparable("> 3 active days"));
    }
}
