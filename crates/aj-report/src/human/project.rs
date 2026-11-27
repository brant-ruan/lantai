//! The human journal as a project's page shows it: the notes of the page itself, and the
//! project's notes and annotations of every period of one kind (day, week or month). Each
//! annotation is placed in the project's section of its period, which is the text the page
//! shows for that period. Every change is made in the node's own period with the same
//! functions a report view uses, so both views always show the same journal.

use std::collections::BTreeMap;

use aj_store::{HumanNodeRow, HumanPosition, Store, PROJECT_PERIOD};
use anyhow::{anyhow, bail, Result};

use super::attach::{self, Attachment, OutlineSection, Placement};
use super::{period_kind, report_kind, HumanView, Move, NewPlace, PROJECT_PAGE};
use crate::rollup::Granularity;
use crate::structure::extract_project_section;

/// Where a new node goes on a project page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectPlace {
    /// At the end of the page's own notes.
    PageNote,
    /// At the end of the project's notes of this period, before or after its generated text.
    PeriodNote(String, HumanPosition),
    /// On the point at `line` of the project's section of `period`, as the page shows it.
    Annotation { period: String, line: usize },
    /// Right after this node, among its siblings.
    After(i64),
    /// As the last child of this node.
    LastChild(i64),
}

/// The page's notes and the project's nodes of `granularity`'s periods. Placements give an
/// annotation's line in the project's section of its period (as the rollup shows it).
pub fn load(store: &Store, project_id: i64, granularity: Granularity) -> Result<HumanView> {
    let name = project_name(store, project_id)?;
    let report_kind = granularity.report_kind();
    let kind = period_kind(report_kind)?;
    let page = project_id.to_string();
    let nodes: Vec<HumanNodeRow> = store
        .human_nodes_of_project(project_id)?
        .into_iter()
        .filter(|n| {
            n.period_kind == kind || (n.period_kind == PROJECT_PERIOD && n.period_key == page)
        })
        .collect();
    let mut sections: BTreeMap<String, Vec<OutlineSection>> = BTreeMap::new();
    let mut placements = BTreeMap::new();
    for n in nodes
        .iter()
        .filter(|n| n.parent_id.is_none() && n.period_kind == kind)
    {
        let Some(att) = n.attach_json.as_deref().and_then(Attachment::parse) else {
            continue;
        };
        if !sections.contains_key(&n.period_key) {
            let outline = section_outline(store, report_kind, &n.period_key, &name)?;
            sections.insert(n.period_key.clone(), outline);
        }
        let placement = match attach::place(&sections[&n.period_key], &att, &name) {
            // The page shows the section without its heading line.
            Placement::Point(line) => Placement::Point(line - 1),
            Placement::Section(_) => Placement::Section(0),
            Placement::Detached => Placement::Detached,
        };
        placements.insert(n.id, placement);
    }
    Ok(HumanView { nodes, placements })
}

/// Add a node and return its id.
pub fn create(
    store: &Store,
    project_id: i64,
    granularity: Granularity,
    place: ProjectPlace,
    body: &str,
) -> Result<i64> {
    let report_kind = granularity.report_kind();
    match place {
        ProjectPlace::PageNote => super::create(
            store,
            PROJECT_PAGE,
            &project_id.to_string(),
            NewPlace::ProjectNote(project_id, HumanPosition::Bottom),
            body,
        ),
        ProjectPlace::PeriodNote(period, position) => super::create(
            store,
            report_kind,
            &period,
            NewPlace::ProjectNote(project_id, position),
            body,
        ),
        ProjectPlace::Annotation { period, line } => {
            let name = project_name(store, project_id)?;
            let shown = section_outline(store, report_kind, &period, &name)?;
            let att = attach::attachment_at(&shown, line + 1)
                .ok_or_else(|| anyhow!("line {line} of the {period} section is not a point"))?;
            // The same point in the report as its own view shows it.
            let full = super::context(store, report_kind, &period)?;
            match attach::place(&full.sections, &att, &name) {
                Placement::Point(at) => {
                    super::create(store, report_kind, &period, NewPlace::Annotation(at), body)
                }
                _ => bail!("the point is not in the {period} report"),
            }
        }
        ProjectPlace::After(id) => {
            let (kind, period) = node_period(store, id)?;
            super::create(store, kind, &period, NewPlace::After(id), body)
        }
        ProjectPlace::LastChild(id) => {
            let (kind, period) = node_period(store, id)?;
            super::create(store, kind, &period, NewPlace::LastChild(id), body)
        }
    }
}

pub fn update_body(store: &Store, id: i64, body: &str) -> Result<()> {
    let (kind, period) = node_period(store, id)?;
    super::update_body(store, kind, &period, id, body)
}

pub fn move_node(store: &mut Store, id: i64, m: Move) -> Result<()> {
    let (kind, period) = node_period(store, id)?;
    super::move_node(store, kind, &period, id, m)
}

/// Move dragged nodes (all of one period) to `drop`, in that period.
pub fn move_to(store: &mut Store, ids: &[i64], drop: super::Drop) -> Result<()> {
    let Some(&first) = ids.first() else {
        return Ok(());
    };
    let (kind, period) = node_period(store, first)?;
    super::move_to(store, kind, &period, ids, drop)
}

/// The journal kind and period a node belongs to.
fn node_period(store: &Store, id: i64) -> Result<(&'static str, String)> {
    let node = store
        .human_node(id)?
        .ok_or_else(|| anyhow!("human node {id} does not exist"))?;
    let kind = if node.period_kind == PROJECT_PERIOD {
        PROJECT_PAGE
    } else {
        report_kind(&node.period_kind)
            .ok_or_else(|| anyhow!("unknown period kind {:?}", node.period_kind))?
    };
    Ok((kind, node.period_key))
}

fn project_name(store: &Store, project_id: i64) -> Result<String> {
    Ok(store
        .project(project_id)?
        .ok_or_else(|| anyhow!("project {project_id} does not exist"))?
        .name)
}

/// The outline of the project's section of a period as the rollup shows it, under a heading
/// line of its own (so the section's line `n` is outline line `n + 1`).
fn section_outline(
    store: &Store,
    report_kind: &str,
    period: &str,
    name: &str,
) -> Result<Vec<OutlineSection>> {
    let body = store
        .current_report_version(report_kind, period, None)?
        .and_then(|v| extract_project_section(&v.markdown, name));
    Ok(match body {
        Some(body) => attach::outline(&format!("## {name}\n{body}")),
        None => Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aj_store::NewReportVersion;

    const DAY: &str = "2026-11-21";

    fn store() -> (Store, i64) {
        let mut store = Store::open_in_memory().unwrap();
        let other = store.create_project("Other", None).unwrap();
        let app = store.create_project("App", None).unwrap();
        assert!(other < app);
        store
            .save_report_version(&NewReportVersion {
                report_type: "daily",
                period_key: DAY,
                project_id: None,
                origin: aj_store::ReportOrigin::Generated,
                markdown: "# x\n\n## Other\n- Elsewhere [#S9.T1]\n\n## App\n\
                           - Built the search view [#S1.T2]\n- Fixed the menu [#S2.T1]",
                engine: None,
                input_hash: None,
                prompt_id: None,
                prompt_name: None,
                prompt_snapshot: None,
                input_manifest: None,
                language: "en",
            })
            .unwrap();
        (store, app)
    }

    #[test]
    fn the_page_edits_the_same_journal_as_the_reports() {
        let (mut store, app) = store();
        let day = Granularity::Daily;
        let page = create(&store, app, day, ProjectPlace::PageNote, "plan").unwrap();
        let note = create(
            &store,
            app,
            day,
            ProjectPlace::PeriodNote(DAY.into(), HumanPosition::Bottom),
            "called",
        )
        .unwrap();
        let next = create(&store, app, day, ProjectPlace::After(note), "then").unwrap();
        let ann = create(
            &store,
            app,
            day,
            ProjectPlace::Annotation {
                period: DAY.into(),
                line: 1,
            },
            "twice",
        )
        .unwrap();
        // Not a point of the section.
        let bad = ProjectPlace::Annotation {
            period: DAY.into(),
            line: 5,
        };
        assert!(create(&store, app, day, bad, "x").is_err());

        let view = load(&store, app, day).unwrap();
        let ids: Vec<i64> = view.nodes.iter().map(|n| n.id).collect();
        for id in [page, note, next, ann] {
            assert!(ids.contains(&id));
        }
        // On the page: the second line of the App section.
        assert_eq!(view.placements.get(&ann), Some(&Placement::Point(1)));
        // In the daily report: the same point, on its own line there.
        let report = super::super::load(&store, "daily", DAY).unwrap();
        let shown = report.nodes.iter().map(|n| n.id).collect::<Vec<_>>();
        assert!(shown.contains(&note) && shown.contains(&next) && !shown.contains(&page));
        assert!(matches!(report.placements.get(&ann), Some(Placement::Point(l)) if *l > 1));

        // Changes go to the node's own period.
        update_body(&store, ann, "three times").unwrap();
        move_node(&mut store, next, Move::Indent).unwrap();
        let moved = store.human_node(next).unwrap().unwrap();
        assert_eq!(moved.parent_id, Some(note));
        let ann_row = store.human_node(ann).unwrap().unwrap();
        assert_eq!(ann_row.body, "three times");

        // Weekly granularity: only the page notes.
        let week = load(&store, app, Granularity::Weekly).unwrap();
        assert_eq!(
            week.nodes.iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![page]
        );
    }
}
