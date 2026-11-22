//! Where annotations hang in a generated report. Pure logic.
//!
//! An annotation records the point it was written on: the section heading, the point's text
//! without anchors, and the anchors of the point and everything under it. Update and
//! regeneration rewrite the report, so the point is found again in the version on screen by
//! its anchors first (turns cited are more stable than wording) and by its text second. A point
//! that cannot be found leaves the annotation at the end of its section, never lost.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::anchors;
use crate::structure::strip_heading_wrappers;
use crate::textsim::similarity;

/// What an annotation was written on (stored as the node's `attach_json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Heading of the project section the point was in.
    pub section: String,
    /// The point's text without anchors.
    pub quote: String,
    /// Anchors of the point and everything under it, as `S12` or `S12.T5`.
    pub anchors: Vec<String>,
}

impl Attachment {
    pub fn parse(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Where an annotation shows in the displayed report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Under the point on this line.
    Point(usize),
    /// At the end of the section whose heading is on this line: the point is gone.
    Section(usize),
    /// The report has no such section (or there is no report).
    Detached,
}

/// A bullet of the displayed report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    pub line: usize,
    pub text: String,
    pub anchors: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineSection {
    pub heading: String,
    pub heading_line: usize,
    pub points: Vec<Point>,
}

/// A point is found again from this anchor overlap (Jaccard) or this text similarity up.
const ANCHOR_MATCH: f64 = 0.5;
const TEXT_MATCH: f64 = 0.6;

/// The sections and bullets of a displayed report, each bullet carrying the anchors of
/// itself and everything nested under it.
pub fn outline(display: &str) -> Vec<OutlineSection> {
    let mut sections: Vec<OutlineSection> = Vec::new();
    // Indent and index (in the current section) of the open bullets above this line.
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for (line, raw) in display.split('\n').enumerate() {
        let trimmed = raw.trim();
        if let Some(h2) = trimmed.strip_prefix("## ") {
            sections.push(OutlineSection {
                heading: strip_heading_wrappers(h2),
                heading_line: line,
                points: Vec::new(),
            });
            stack.clear();
            continue;
        }
        let Some(sec) = sections.last_mut() else {
            continue;
        };
        let Some((indent, content)) = bullet(raw) else {
            continue;
        };
        let own: BTreeSet<String> = anchors::references(content)
            .into_iter()
            .map(|(slice, turn)| anchor_key(slice, turn))
            .collect();
        while stack.last().is_some_and(|&(i, _)| i >= indent) {
            stack.pop();
        }
        for &(_, idx) in &stack {
            sec.points[idx].anchors.extend(own.iter().cloned());
        }
        stack.push((indent, sec.points.len()));
        sec.points.push(Point {
            line,
            text: anchors::strip(content).trim().to_string(),
            anchors: own,
        });
    }
    sections
}

/// What an annotation on the bullet at `line` records; `None` when no bullet is there.
pub fn attachment_at(sections: &[OutlineSection], line: usize) -> Option<Attachment> {
    sections.iter().find_map(|sec| {
        sec.points
            .iter()
            .find(|p| p.line == line)
            .map(|p| Attachment {
                section: sec.heading.clone(),
                quote: p.text.clone(),
                anchors: p.anchors.iter().cloned().collect(),
            })
    })
}

/// Find an annotation's point in the displayed report. `section` is the name to look for:
/// the project's current name when the annotation belongs to a project (it may have been
/// renamed), otherwise the recorded heading.
pub fn place(sections: &[OutlineSection], att: &Attachment, section: &str) -> Placement {
    let wanted = section.to_lowercase();
    let Some(sec) = sections.iter().find(|s| s.heading.to_lowercase() == wanted) else {
        return Placement::Detached;
    };
    if let Some(p) = sec.points.iter().find(|p| p.text == att.quote) {
        return Placement::Point(p.line);
    }
    let recorded: BTreeSet<&str> = att.anchors.iter().map(String::as_str).collect();
    let best = sec
        .points
        .iter()
        .map(|p| {
            let anchors = jaccard(&recorded, &p.anchors);
            let text = similarity(&att.quote, &p.text);
            (p.line, anchors, text)
        })
        .filter(|&(_, a, t)| a >= ANCHOR_MATCH || t >= TEXT_MATCH)
        .max_by(|x, y| (x.1 + x.2).total_cmp(&(y.1 + y.2)));
    match best {
        Some((line, _, _)) => Placement::Point(line),
        None => Placement::Section(sec.heading_line),
    }
}

fn jaccard(a: &BTreeSet<&str>, b: &BTreeSet<String>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let common = b.iter().filter(|x| a.contains(x.as_str())).count();
    common as f64 / (a.len() + b.len() - common) as f64
}

fn anchor_key(slice: i64, turn: Option<i64>) -> String {
    match turn {
        Some(t) => format!("S{slice}.T{t}"),
        None => format!("S{slice}"),
    }
}

/// Indent width and content of a bullet line.
fn bullet(line: &str) -> Option<(usize, &str)> {
    let rest = line.trim_start_matches([' ', '\t']);
    let indent: usize = line[..line.len() - rest.len()]
        .chars()
        .map(|c| if c == '\t' { 2 } else { 1 })
        .sum();
    let content = rest
        .strip_prefix("- ")
        .or_else(|| rest.strip_prefix("* "))
        .or_else(|| rest.strip_prefix("\u{2022} "))?;
    Some((indent, content.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = "# 2026-11-21 · Sat\n\n## App\n- Built the search view [#S1.T2]\n  - Scans with LIKE [#S1.T3]\n- Fixed the menu [#S2.T1]\n\n## Docs\n- Wrote the spec [#S3]";

    #[test]
    fn outline_collects_anchors_from_nested_points() {
        let sections = outline(REPORT);
        assert_eq!(sections.len(), 2);
        let first = &sections[0].points[0];
        assert_eq!(first.line, 3);
        assert_eq!(first.text, "Built the search view");
        assert_eq!(
            first.anchors.iter().map(String::as_str).collect::<Vec<_>>(),
            vec!["S1.T2", "S1.T3"]
        );
        assert_eq!(sections[1].heading, "Docs");
    }

    #[test]
    fn annotations_follow_their_point_through_rewrites() {
        let att = attachment_at(&outline(REPORT), 3).unwrap();
        assert_eq!(att.section, "App");

        // Same report: the same line.
        assert_eq!(place(&outline(REPORT), &att, "App"), Placement::Point(3));

        // Reworded and moved after another point, same turns: found by anchors.
        let updated = "# T\n\n## App\n- Fixed the menu [#S2.T1]\n- Added a search screen for reports [#S1.T2]\n  - LIKE scan [#S1.T3]";
        assert_eq!(place(&outline(updated), &att, "App"), Placement::Point(4));

        // The project was renamed: looked up under its new name.
        let renamed = updated.replace("## App", "## Lantai");
        assert_eq!(
            place(&outline(&renamed), &att, "Lantai"),
            Placement::Point(4)
        );

        // The point is gone: the end of its section.
        let gone = "# T\n\n## App\n- Fixed the menu [#S2.T1]";
        assert_eq!(place(&outline(gone), &att, "App"), Placement::Section(2));

        // No such section at all.
        assert_eq!(
            place(&outline("# T\n\n## Docs\n- x"), &att, "App"),
            Placement::Detached
        );
    }

    #[test]
    fn points_without_anchors_are_found_by_text() {
        let report = "# T\n\n## App\n- Planned the human journal";
        let att = attachment_at(&outline(report), 3).unwrap();
        let reworded = "# T\n\n## App\n- Planned the human journal design";
        assert_eq!(place(&outline(reworded), &att, "App"), Placement::Point(3));
        assert_eq!(
            Attachment::parse(&att.to_json()).as_ref(),
            Some(&att),
            "round-trips through JSON"
        );
    }
}
