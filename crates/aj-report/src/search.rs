//! Search across both layers: the journal (sections of the current global daily, weekly and
//! monthly reports, and the notes people wrote next to them) and session text (via
//! `aj_core::search`), with each session hit linked to the reports whose anchors cite it.

use std::collections::HashMap;

use aj_core::search::{
    count_matches, covers_all, parse_terms, search_sessions, snippet, SessionQuery, SessionSearch,
    SnippetPart,
};
use aj_store::{HumanNodeRow, ReportStatus, ReportTextRow, SliceFilter, Store};
use anyhow::Result;
use chrono::NaiveDate;

use crate::anchors;
use crate::human;
use crate::period::{Month, Week};
use crate::rollup::Granularity;

/// Most candidate turns scanned per search.
pub const DEFAULT_TURN_LIMIT: usize = 400;
/// Most report sections returned per search.
const REPORT_SECTION_LIMIT: usize = 100;
/// Snippets kept per report section.
const SNIPPETS_PER_SECTION: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchLayer {
    #[default]
    All,
    Reports,
    Sessions,
}

impl SearchLayer {
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "all" => Ok(Self::All),
            "reports" => Ok(Self::Reports),
            "sessions" => Ok(Self::Sessions),
            other => {
                anyhow::bail!("unknown search layer {other:?}; expected all, reports, or sessions")
            }
        }
    }

    fn reports(self) -> bool {
        self != Self::Sessions
    }

    fn sessions(self) -> bool {
        self != Self::Reports
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchRequest {
    pub query: String,
    pub layer: SearchLayer,
    /// Dates, project and ignore state apply to both layers (a project keeps only its report
    /// sections); the agent applies to sessions only.
    pub filter: SliceFilter,
    pub include_tool_output: bool,
    pub turn_limit: usize,
}

/// A report a session hit is cited by.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReportRef {
    pub kind: String,
    pub period_key: String,
}

/// One matching `## <section>` of a report, or one matching note (a root with everything
/// under it) written for a period.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportHit {
    pub report: ReportRef,
    /// The report's status; `None` for a note.
    pub status: Option<ReportStatus>,
    /// The section heading or the note's project; `None` for text before the first section
    /// and for notes of no project.
    pub section: Option<String>,
    pub snippets: Vec<Vec<SnippetPart>>,
    pub match_count: usize,
    /// Written by a person rather than generated.
    pub human: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    pub terms: Vec<String>,
    pub reports: Vec<ReportHit>,
    pub reports_truncated: bool,
    pub sessions: SessionSearch,
    /// Reports citing a turn, keyed by `(slice_id, turn_index)`; a whole-slice anchor is
    /// recorded under `(slice_id, None)`.
    pub citations: HashMap<(i64, Option<i64>), Vec<ReportRef>>,
}

impl SearchResults {
    /// Reports citing this turn or its whole slice, without duplicates.
    pub fn cited_by(&self, slice_id: i64, turn_index: Option<i64>) -> Vec<ReportRef> {
        let mut out: Vec<ReportRef> = Vec::new();
        let mut keys = vec![(slice_id, None)];
        if turn_index.is_some() {
            keys.insert(0, (slice_id, turn_index));
        }
        for key in &keys {
            for r in self.citations.get(key).into_iter().flatten() {
                if !out.contains(r) {
                    out.push(r.clone());
                }
            }
        }
        out
    }
}

/// The first day of a report's period, for ordering reports of different kinds.
fn period_start(kind: Granularity, key: &str) -> Option<NaiveDate> {
    match kind {
        Granularity::Daily => key.parse().ok(),
        Granularity::Weekly => Week::parse(key).ok().map(|w| w.monday()),
        Granularity::Monthly => Month::parse(key).ok().map(|m| m.first_day()),
    }
}

/// Split a report into `(heading, body)` sections at `## ` headings. The `# ` title line is
/// dropped; text before the first section has no heading.
pub fn sections(markdown: &str) -> Vec<(Option<String>, String)> {
    let mut out: Vec<(Option<String>, String)> = vec![(None, String::new())];
    for line in markdown.lines() {
        let trimmed = line.trim();
        if let Some(h) = trimmed.strip_prefix("## ") {
            out.push((Some(h.trim().to_string()), String::new()));
        } else if !trimmed.starts_with("# ") {
            let body = &mut out.last_mut().expect("never empty").1;
            body.push_str(line);
            body.push('\n');
        }
    }
    out.retain(|(h, body)| h.is_some() || !body.trim().is_empty());
    out
}

/// A piece of the journal that is searched as one: a report section or a note.
struct Unit {
    kind: Granularity,
    report: ReportRef,
    status: Option<ReportStatus>,
    section: Option<String>,
    text: String,
    human: bool,
}

fn report_units(reports: &[ReportTextRow]) -> Vec<Unit> {
    let mut out = Vec::new();
    for r in reports {
        let Ok(kind) = Granularity::parse(&r.report_type) else {
            continue;
        };
        for (heading, body) in sections(&r.markdown) {
            out.push(Unit {
                kind,
                report: ReportRef {
                    kind: r.report_type.clone(),
                    period_key: r.period_key.clone(),
                },
                status: Some(r.status),
                section: heading,
                text: anchors::strip(&body),
                human: false,
            });
        }
    }
    out
}

/// One unit per note tree; `project_name` gives a project's current name.
fn note_units(nodes: &[HumanNodeRow], project_name: &dyn Fn(i64) -> Option<String>) -> Vec<Unit> {
    let mut out = Vec::new();
    for (root, members) in human::trees(nodes) {
        let Some(report_kind) = human::report_kind(&root.period_kind) else {
            continue;
        };
        let Ok(kind) = Granularity::parse(report_kind) else {
            continue;
        };
        let text = members
            .iter()
            .map(|n| n.body.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        out.push(Unit {
            kind,
            report: ReportRef {
                kind: report_kind.to_string(),
                period_key: root.period_key.clone(),
            },
            status: None,
            section: root.project_id.and_then(project_name),
            text,
            human: true,
        });
    }
    out
}

/// Matching units, newest period first (daily before weekly before monthly on the same start
/// day, and reports before notes).
fn search_units(
    units: Vec<Unit>,
    terms: &[String],
    filter: &SliceFilter,
    project: Option<&str>,
) -> (Vec<ReportHit>, bool) {
    let mut hits: Vec<(NaiveDate, usize, bool, ReportHit)> = Vec::new();
    for u in units {
        let since = u.kind.bound(filter.since.as_deref());
        let until = u.kind.bound(filter.until.as_deref());
        let key = &u.report.period_key;
        if since.is_some_and(|s| *key < s) || until.is_some_and(|until| *key > until) {
            continue;
        }
        let Some(start) = period_start(u.kind, key) else {
            continue;
        };
        if project.is_some() && u.section.as_deref() != project {
            continue;
        }
        let lines: Vec<&str> = u.text.lines().filter(|l| !l.trim().is_empty()).collect();
        let in_heading = u.section.iter().map(String::as_str);
        if !covers_all(lines.iter().copied().chain(in_heading), terms) {
            continue;
        }
        let match_count = lines.iter().map(|l| count_matches(l, terms)).sum::<usize>()
            + u.section.as_deref().map_or(0, |h| count_matches(h, terms));
        let mut snippets: Vec<Vec<SnippetPart>> = lines
            .iter()
            .filter_map(|l| snippet(l, terms))
            .take(SNIPPETS_PER_SECTION)
            .collect();
        if snippets.is_empty() {
            // Only the heading matched: show the start of the section.
            snippets.extend(lines.first().map(|l| {
                vec![SnippetPart {
                    text: l.trim().to_string(),
                    hit: false,
                }]
            }));
        }
        hits.push((
            start,
            u.kind as usize,
            u.human,
            ReportHit {
                report: u.report,
                status: u.status,
                section: u.section,
                snippets,
                match_count,
                human: u.human,
            },
        ));
    }
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let truncated = hits.len() > REPORT_SECTION_LIMIT;
    let hits = hits
        .into_iter()
        .take(REPORT_SECTION_LIMIT)
        .map(|(_, _, _, h)| h)
        .collect();
    (hits, truncated)
}

/// Which reports cite each slice or turn.
fn citations(reports: &[ReportTextRow]) -> HashMap<(i64, Option<i64>), Vec<ReportRef>> {
    let mut map: HashMap<(i64, Option<i64>), Vec<ReportRef>> = HashMap::new();
    for r in reports {
        let rr = ReportRef {
            kind: r.report_type.clone(),
            period_key: r.period_key.clone(),
        };
        for key in anchors::references(&r.markdown) {
            let list = map.entry(key).or_default();
            if !list.contains(&rr) {
                list.push(rr.clone());
            }
        }
    }
    map
}

pub fn search(store: &Store, req: &SearchRequest) -> Result<SearchResults> {
    let terms = parse_terms(&req.query);
    if terms.is_empty() {
        return Ok(SearchResults::default());
    }
    let reports = store.current_report_texts()?;
    let mut out = SearchResults {
        terms: terms.clone(),
        ..Default::default()
    };
    if req.layer.reports() {
        let project = match req.filter.project_id {
            Some(id) => Some(store.project(id)?.map(|p| p.name).unwrap_or_default()),
            None => None,
        };
        let names = store.project_names()?;
        let mut units = report_units(&reports);
        units.extend(note_units(&store.all_human_nodes()?, &|id| {
            names.get(&id).cloned()
        }));
        (out.reports, out.reports_truncated) =
            search_units(units, &terms, &req.filter, project.as_deref());
    }
    if req.layer.sessions() {
        let limit = if req.turn_limit == 0 {
            DEFAULT_TURN_LIMIT
        } else {
            req.turn_limit
        };
        out.sessions = search_sessions(
            store,
            &SessionQuery {
                terms,
                filter: req.filter.clone(),
                include_tool_output: req.include_tool_output,
                limit,
            },
        )?;
        out.citations = citations(&reports);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: &str, key: &str, md: &str) -> ReportTextRow {
        ReportTextRow {
            report_type: kind.into(),
            period_key: key.into(),
            status: ReportStatus::Draft,
            markdown: md.into(),
        }
    }

    #[test]
    fn sections_split_at_h2_and_drop_the_title() {
        let md =
            "# 2026-11-14\n> stats\n\n## Lantai\n- search [#S1.T2]\n### Sub\n- x\n## Other\n- y\n";
        let s = sections(md);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0], (None, "> stats\n\n".to_string()));
        assert_eq!(s[1].0.as_deref(), Some("Lantai"));
        assert!(s[1].1.contains("### Sub"));
    }

    #[test]
    fn reports_match_per_section_filter_by_project_and_date_and_sort_newest_first() {
        let reports = vec![
            row(
                "daily",
                "2026-11-10",
                "# t\n## Lantai\n- keychain storage [#S1.T1]\n## Web\n- keychain\n",
            ),
            row(
                "daily",
                "2026-11-14",
                "# t\n## Lantai\n- keychain and search\n",
            ),
            row("weekly", "2026-W46", "# t\n## Lantai\n- keychain\n"),
        ];
        let terms = parse_terms("keychain");
        let (hits, truncated) = search_units(
            report_units(&reports),
            &terms,
            &SliceFilter::default(),
            None,
        );
        assert!(!truncated);
        let keys: Vec<_> = hits
            .iter()
            .map(|h| {
                (
                    h.report.period_key.as_str(),
                    h.section.as_deref().unwrap_or(""),
                )
            })
            .collect();
        assert_eq!(
            keys,
            vec![
                ("2026-11-14", "Lantai"),
                ("2026-11-10", "Lantai"),
                ("2026-11-10", "Web"),
                ("2026-W46", "Lantai"),
            ]
        );

        let (hits, _) = search_units(
            report_units(&reports),
            &terms,
            &SliceFilter::default(),
            Some("Web"),
        );
        assert_eq!(hits.len(), 1);

        let since = SliceFilter {
            since: Some("2026-11-12".into()),
            ..Default::default()
        };
        let (hits, _) = search_units(report_units(&reports), &terms, &since, None);
        let keys: Vec<_> = hits.iter().map(|h| h.report.period_key.as_str()).collect();
        assert_eq!(keys, vec!["2026-11-14", "2026-W46"]);

        // Every term must occur in the same section; anchors are not searchable text.
        let (hits, _) = search_units(
            report_units(&reports),
            &parse_terms("keychain search"),
            &SliceFilter::default(),
            None,
        );
        assert_eq!(hits.len(), 1);
        let (hits, _) = search_units(
            report_units(&reports),
            &parse_terms("S1"),
            &SliceFilter::default(),
            None,
        );
        assert!(hits.is_empty());
    }

    fn node(
        id: i64,
        kind: &str,
        key: &str,
        project: Option<i64>,
        parent: Option<i64>,
        body: &str,
    ) -> HumanNodeRow {
        HumanNodeRow {
            id,
            period_kind: kind.into(),
            period_key: key.into(),
            project_id: project,
            parent_id: parent,
            sort_key: "V".into(),
            attach_json: None,
            position: aj_store::HumanPosition::Bottom,
            body: body.into(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn notes_match_as_whole_trees_under_their_project() {
        let reports = vec![row("daily", "2026-11-14", "# t\n## Lantai\n- keychain\n")];
        let nodes = vec![
            node(
                1,
                "day",
                "2026-11-14",
                Some(7),
                None,
                "met the **keychain** team",
            ),
            node(2, "day", "2026-11-14", Some(7), Some(1), "agreed on search"),
            node(3, "week", "2026-W46", None, None, "keychain review"),
            node(4, "day", "2026-11-10", None, None, "unrelated"),
        ];
        let name = |id: i64| (id == 7).then(|| "Lantai".to_string());
        let mut units = report_units(&reports);
        units.extend(note_units(&nodes, &name));

        // A child's text counts toward its root; the hit is one per tree.
        let (hits, _) = search_units(
            units,
            &parse_terms("keychain search"),
            &SliceFilter::default(),
            None,
        );
        assert_eq!(hits.len(), 1);
        assert!(hits[0].human);
        assert_eq!(hits[0].section.as_deref(), Some("Lantai"));
        assert_eq!(hits[0].report.kind, "daily");

        let mut units = report_units(&reports);
        units.extend(note_units(&nodes, &name));
        let (hits, _) = search_units(
            units,
            &parse_terms("keychain"),
            &SliceFilter::default(),
            None,
        );
        let got: Vec<_> = hits
            .iter()
            .map(|h| (h.report.period_key.as_str(), h.human))
            .collect();
        // Reports come before notes on the same day; the week starts earlier.
        assert_eq!(
            got,
            vec![
                ("2026-11-14", false),
                ("2026-11-14", true),
                ("2026-W46", true)
            ]
        );
    }

    #[test]
    fn citations_index_turn_and_slice_anchors() {
        let reports = vec![
            row(
                "daily",
                "2026-11-14",
                "## A\n- x [#S5.T2] [#S5.T2]\n- y [#S6]\n",
            ),
            row("weekly", "2026-W46", "## A\n- x [#S5.T2]\n"),
        ];
        let res = SearchResults {
            citations: citations(&reports),
            ..Default::default()
        };
        assert_eq!(res.cited_by(5, Some(2)).len(), 2);
        assert_eq!(res.cited_by(6, Some(9)).len(), 1);
        assert!(res.cited_by(7, Some(1)).is_empty());
    }
}
