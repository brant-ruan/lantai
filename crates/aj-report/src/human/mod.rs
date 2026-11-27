//! The human journal: day notes, project notes and annotations people write next to the
//! generated reports (docs/SPEC.md, "Human journal").
//!
//! Nodes live in `aj-store`; this module decides where a new node goes, how nodes move among
//! their siblings, and where annotations show in the report version on screen. Siblings are
//! the children of one parent, or, for roots, the notes at one end (top or bottom) of a
//! project's section, or the day's notes, or the annotations shown under one point.

pub mod attach;
pub mod project;

use std::collections::BTreeMap;

use aj_core::order::{key_between, keys_after};
use aj_store::{HumanNodeRow, HumanPosition, HumanRelocation, NewHumanRoot, Store, PROJECT_PERIOD};
use anyhow::{anyhow, bail, Result};

use crate::report_io;
use attach::{Attachment, OutlineSection, Placement};

/// The journal kind of a project's own page: its notes stand apart from every period and its
/// "period" is the project id.
pub const PROJECT_PAGE: &str = "project";

/// The period kind human nodes are stored under for a report kind (`daily` -> `day`), or for
/// [`PROJECT_PAGE`].
pub fn period_kind(report_kind: &str) -> Result<&'static str> {
    match report_kind {
        "daily" => Ok("day"),
        "weekly" => Ok("week"),
        "monthly" => Ok("month"),
        PROJECT_PAGE => Ok(PROJECT_PERIOD),
        other => bail!("no human journal for report kind {other:?}"),
    }
}

/// The report kind a period kind belongs to (`day` -> `daily`).
pub fn report_kind(period_kind: &str) -> Option<&'static str> {
    match period_kind {
        "day" => Some("daily"),
        "week" => Some("weekly"),
        "month" => Some("monthly"),
        _ => None,
    }
}

/// Each root with everything under it, roots in stored order.
pub fn trees(nodes: &[HumanNodeRow]) -> Vec<(&HumanNodeRow, Vec<&HumanNodeRow>)> {
    let mut out = Vec::new();
    for root in nodes.iter().filter(|n| n.parent_id.is_none()) {
        let mut members = vec![root];
        let mut i = 0;
        while i < members.len() {
            let id = members[i].id;
            members.extend(nodes.iter().filter(|n| n.parent_id == Some(id)));
            i += 1;
        }
        out.push((root, members));
    }
    out
}

/// A period's nodes, with where each annotation shows in the current report version.
#[derive(Debug, Clone, Default)]
pub struct HumanView {
    /// Roots first, then children; siblings in order.
    pub nodes: Vec<HumanNodeRow>,
    /// For every annotation root.
    pub placements: BTreeMap<i64, Placement>,
}

/// Where a new node goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewPlace {
    /// At the end of the period's notes that belong to no project.
    DayNote,
    /// At the end of a project's notes at the top or the bottom of its section (always the
    /// bottom on a project's own page, which has no generated text to go around).
    ProjectNote(i64, HumanPosition),
    /// At the end of the annotations on the report point at this display line.
    Annotation(usize),
    /// Right after this node, among its siblings.
    After(i64),
    /// As the last child of this node.
    LastChild(i64),
}

/// One direction of moving a node one step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    /// Under the sibling above it, as its last child.
    Indent,
    /// Out of its parent, right after it.
    Outdent,
}

/// Where dragged nodes are dropped. `after` is the sibling they go right after (the start of
/// the group when `None`); the dragged nodes themselves do not count as siblings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drop {
    /// Among the root notes at one end of a project's section, or among the period's notes
    /// when `project_id` is `None` (those, and a project page's own notes, are always at the
    /// bottom).
    Notes {
        project_id: Option<i64>,
        position: HumanPosition,
        after: Option<i64>,
    },
    /// Among the annotations shown with the annotation `with`.
    Annotations { with: i64, after: Option<i64> },
    /// Among the children of `parent`.
    Children { parent: i64, after: Option<i64> },
}

struct Context {
    kind: &'static str,
    period: String,
    view: HumanView,
    sections: Vec<OutlineSection>,
}

/// The period's nodes and annotation placements.
pub fn load(store: &Store, report_kind: &str, period: &str) -> Result<HumanView> {
    Ok(context(store, report_kind, period)?.view)
}

fn context(store: &Store, report_kind: &str, period: &str) -> Result<Context> {
    let kind = period_kind(report_kind)?;
    let sections = if kind == PROJECT_PERIOD {
        // A project page has no report to annotate.
        Vec::new()
    } else {
        match store.current_report_version(report_kind, period, None)? {
            Some(v) => attach::outline(&report_io::display_markdown(
                report_kind,
                period,
                &v.markdown,
                &v.language,
                &report_io::project_order(store)?,
            )),
            None => Vec::new(),
        }
    };
    let nodes = store.human_nodes(kind, period)?;
    let mut placements = BTreeMap::new();
    for n in nodes.iter().filter(|n| n.parent_id.is_none()) {
        let Some(att) = n.attach_json.as_deref().and_then(Attachment::parse) else {
            continue;
        };
        let section = match n.project_id {
            Some(id) => store.project(id)?.map(|p| p.name),
            None => None,
        }
        .unwrap_or_else(|| att.section.clone());
        placements.insert(n.id, attach::place(&sections, &att, &section));
    }
    Ok(Context {
        kind,
        period: period.to_string(),
        view: HumanView { nodes, placements },
        sections,
    })
}

/// Add a node and return its id.
pub fn create(
    store: &Store,
    report_kind: &str,
    period: &str,
    place: NewPlace,
    body: &str,
) -> Result<i64> {
    let cx = context(store, report_kind, period)?;
    let root = |project_id: Option<i64>,
                attach_json: Option<&str>,
                position: HumanPosition,
                sort_key: &str| {
        store.insert_human_root(&NewHumanRoot {
            period_kind: cx.kind,
            period_key: &cx.period,
            project_id,
            attach_json,
            position,
            sort_key,
            body,
        })
    };
    let bottom = HumanPosition::Bottom;
    if cx.kind == PROJECT_PERIOD {
        let own = matches!(place, NewPlace::ProjectNote(id, _) if id.to_string() == cx.period);
        if !own && !matches!(place, NewPlace::After(_) | NewPlace::LastChild(_)) {
            bail!("a project page only holds notes of its own project");
        }
    }
    match place {
        NewPlace::DayNote => root(
            None,
            None,
            bottom,
            &key_at_end(&notes_of(&cx.view, None, bottom)),
        ),
        NewPlace::ProjectNote(id, position) => {
            let position = if cx.kind == PROJECT_PERIOD {
                bottom
            } else {
                position
            };
            let group = notes_of(&cx.view, Some(id), position);
            root(Some(id), None, position, &key_at_end(&group))
        }
        NewPlace::Annotation(line) => {
            let att = attach::attachment_at(&cx.sections, line)
                .ok_or_else(|| anyhow!("line {line} of the report is not a point"))?;
            let project_id = project_named(store, &att.section)?;
            let group = annotations_at(&cx.view, Placement::Point(line));
            root(
                project_id,
                Some(&att.to_json()),
                bottom,
                &key_at_end(&group),
            )
        }
        NewPlace::After(id) => {
            let node = find(&cx.view, id)?;
            let group = siblings(&cx.view, node);
            let key = key_after_member(store, &group, id)?;
            match node.parent_id {
                Some(parent) => store.insert_human_child(parent, &key, body),
                None => root(
                    node.project_id,
                    node.attach_json.as_deref(),
                    node.position,
                    &key,
                ),
            }
        }
        NewPlace::LastChild(id) => {
            find(&cx.view, id)?;
            store.insert_human_child(id, &key_at_end(&children_of(&cx.view, id)), body)
        }
    }
}

/// Change a node's text. An annotation whose point is on screen is re-anchored to it, so it
/// keeps following the point as the report changes.
pub fn update_body(
    store: &Store,
    report_kind: &str,
    period: &str,
    id: i64,
    body: &str,
) -> Result<()> {
    let cx = context(store, report_kind, period)?;
    store.update_human_body(id, body)?;
    if let Some(Placement::Point(line)) = cx.view.placements.get(&id) {
        if let Some(att) = attach::attachment_at(&cx.sections, *line) {
            store.update_human_attach(id, &att.to_json())?;
        }
    }
    Ok(())
}

/// Move a node one step; a move with nowhere to go is ignored. A project note at the end of
/// its group crosses the generated points: up from the first bottom note to the end of the
/// top notes, down from the last top note to the start of the bottom notes.
pub fn move_node(
    store: &mut Store,
    report_kind: &str,
    period: &str,
    id: i64,
    m: Move,
) -> Result<()> {
    let cx = context(store, report_kind, period)?;
    let node = find(&cx.view, id)?;
    let group = siblings(&cx.view, node);
    let idx = group
        .iter()
        .position(|n| n.id == id)
        .expect("a node is among its siblings");
    let crosses = node.parent_id.is_none()
        && node.attach_json.is_none()
        && node.project_id.is_some()
        && cx.kind != PROJECT_PERIOD;
    match m {
        Move::Up if idx == 0 && crosses && node.position == HumanPosition::Bottom => {
            let top = notes_of(&cx.view, node.project_id, HumanPosition::Top);
            store.set_human_position(id, HumanPosition::Top, &key_at_end(&top))
        }
        Move::Down if idx + 1 == group.len() && crosses && node.position == HumanPosition::Top => {
            let bottom = notes_of(&cx.view, node.project_id, HumanPosition::Bottom);
            let first = bottom.iter().map(|n| n.sort_key.as_str()).min();
            store.set_human_position(id, HumanPosition::Bottom, &key_between(None, first))
        }
        Move::Up | Move::Down => {
            // The key between the two nodes the moved one will sit between.
            let (lo, hi) = match m {
                Move::Up if idx == 0 => return Ok(()),
                Move::Up => (idx.checked_sub(2), Some(idx - 1)),
                _ if idx + 1 == group.len() => return Ok(()),
                _ => (Some(idx + 1), (idx + 2 < group.len()).then_some(idx + 2)),
            };
            let group = renumber_if_needed(store, group)?;
            let key = key_between(
                lo.map(|i| group[i].sort_key.as_str()),
                hi.map(|i| group[i].sort_key.as_str()),
            );
            store.move_human_node(id, node.parent_id, &key)
        }
        Move::Indent => {
            let Some(prev) = idx.checked_sub(1).map(|i| group[i]) else {
                return Ok(());
            };
            let key = key_at_end(&children_of(&cx.view, prev.id));
            store.move_human_node(id, Some(prev.id), &key)
        }
        Move::Outdent => {
            let Some(parent) = node.parent_id.map(|p| find(&cx.view, p)).transpose()? else {
                return Ok(());
            };
            let parent_group = siblings(&cx.view, parent);
            let key = key_after_member(store, &parent_group, parent.id)?;
            store.move_human_node(id, parent.parent_id, &key)
        }
    }
}

/// Move nodes, each with everything under it, to `drop`, in the order given. A node below
/// another one that moves goes along with it. Nodes stay in their period and project: notes
/// among notes (top or bottom of the same section), annotations among the annotations on the
/// same point, and under any node of the same project. Anything else is refused.
pub fn move_to(
    store: &mut Store,
    report_kind: &str,
    period: &str,
    ids: &[i64],
    drop: Drop,
) -> Result<()> {
    let cx = context(store, report_kind, period)?;
    let view = &cx.view;
    let picked = ids
        .iter()
        .map(|&id| find(view, id))
        .collect::<Result<Vec<_>>>()?;
    // Only the topmost of the picked nodes move; the others come along under them.
    let moved: Vec<&HumanNodeRow> = picked
        .iter()
        .copied()
        .filter(|n| {
            !picked
                .iter()
                .any(|a| a.id != n.id && is_below(view, n.id, a.id))
        })
        .collect();
    if moved.is_empty() {
        return Ok(());
    }
    let is_moved = |id: i64| moved.iter().any(|m| m.id == id || is_below(view, id, m.id));
    let annotation_root = |n: &HumanNodeRow| n.parent_id.is_none() && n.attach_json.is_some();

    let (group, parent, position, keep_attach): (Vec<&HumanNodeRow>, _, _, _) = match drop {
        Drop::Notes {
            project_id,
            position,
            ..
        } => {
            if moved
                .iter()
                .any(|n| n.project_id != project_id || annotation_root(n))
            {
                bail!("only notes of the same project can go among its notes");
            }
            let position = if project_id.is_none() || cx.kind == PROJECT_PERIOD {
                HumanPosition::Bottom
            } else {
                position
            };
            (notes_of(view, project_id, position), None, position, false)
        }
        Drop::Annotations { with, .. } => {
            let at = view
                .placements
                .get(&with)
                .copied()
                .ok_or_else(|| anyhow!("human node {with} is not an annotation"))?;
            if moved
                .iter()
                .any(|n| view.placements.get(&n.id) != Some(&at))
            {
                bail!("annotations only move among the annotations on the same point");
            }
            (annotations_at(view, at), None, HumanPosition::Bottom, true)
        }
        Drop::Children { parent, .. } => {
            let p = find(view, parent)?;
            if is_moved(parent) {
                bail!("a human node cannot move under itself");
            }
            if moved.iter().any(|n| n.project_id != p.project_id) {
                bail!("a human node only moves within its project");
            }
            (children_of(view, parent), Some(parent), p.position, false)
        }
    };
    let after = match drop {
        Drop::Notes { after, .. }
        | Drop::Annotations { after, .. }
        | Drop::Children { after, .. } => after,
    };
    let rest: Vec<&HumanNodeRow> = group.into_iter().filter(|n| !is_moved(n.id)).collect();
    let rest = renumber_if_needed(store, rest)?;
    let idx = match after {
        None => 0,
        Some(a) => {
            rest.iter()
                .position(|n| n.id == a)
                .ok_or_else(|| anyhow!("human node {a} is not where the nodes are dropped"))?
                + 1
        }
    };
    let hi = rest.get(idx).map(|n| n.sort_key.clone());
    let mut lo = idx.checked_sub(1).map(|i| rest[i].sort_key.clone());
    let moves = moved
        .iter()
        .map(|n| {
            let key = key_between(lo.as_deref(), hi.as_deref());
            lo = Some(key.clone());
            HumanRelocation {
                id: n.id,
                parent_id: parent,
                attach_json: if keep_attach {
                    n.attach_json.clone()
                } else {
                    None
                },
                position,
                sort_key: key,
            }
        })
        .collect::<Vec<_>>();
    store.relocate_human_nodes(&moves)
}

/// Whether `id` lies somewhere below `ancestor`.
fn is_below(view: &HumanView, id: i64, ancestor: i64) -> bool {
    let mut cur = view
        .nodes
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.parent_id);
    while let Some(p) = cur {
        if p == ancestor {
            return true;
        }
        cur = view
            .nodes
            .iter()
            .find(|n| n.id == p)
            .and_then(|n| n.parent_id);
    }
    false
}

/// Delete nodes and everything under them; returns what was removed, for [`restore`].
pub fn delete(store: &mut Store, ids: &[i64]) -> Result<Vec<HumanNodeRow>> {
    store.delete_human_nodes(ids)
}

/// Put back nodes removed by [`delete`] (an undo).
pub fn restore(store: &mut Store, rows: &[HumanNodeRow]) -> Result<Vec<i64>> {
    store.restore_human_nodes(rows)
}

fn find(view: &HumanView, id: i64) -> Result<&HumanNodeRow> {
    view.nodes
        .iter()
        .find(|n| n.id == id)
        .ok_or_else(|| anyhow!("human node {id} is not in this period"))
}

fn children_of(view: &HumanView, parent: i64) -> Vec<&HumanNodeRow> {
    view.nodes
        .iter()
        .filter(|n| n.parent_id == Some(parent))
        .collect()
}

/// Root notes (not annotations) at one end of a project's section, or of the period when
/// `project_id` is `None`.
fn notes_of(
    view: &HumanView,
    project_id: Option<i64>,
    position: HumanPosition,
) -> Vec<&HumanNodeRow> {
    view.nodes
        .iter()
        .filter(|n| {
            n.parent_id.is_none()
                && n.attach_json.is_none()
                && n.project_id == project_id
                && n.position == position
        })
        .collect()
}

fn annotations_at(view: &HumanView, at: Placement) -> Vec<&HumanNodeRow> {
    view.nodes
        .iter()
        .filter(|n| view.placements.get(&n.id) == Some(&at))
        .collect()
}

fn siblings<'a>(view: &'a HumanView, node: &HumanNodeRow) -> Vec<&'a HumanNodeRow> {
    match (node.parent_id, view.placements.get(&node.id)) {
        (Some(parent), _) => children_of(view, parent),
        (None, Some(&at)) => annotations_at(view, at),
        (None, None) => notes_of(view, node.project_id, node.position),
    }
}

fn key_at_end(group: &[&HumanNodeRow]) -> String {
    key_between(group.iter().map(|n| n.sort_key.as_str()).max(), None)
}

/// The key right after `id` within `group` (sorted by key).
fn key_after_member(store: &Store, group: &[&HumanNodeRow], id: i64) -> Result<String> {
    let group = renumber_if_needed(store, group.to_vec())?;
    let idx = group
        .iter()
        .position(|n| n.id == id)
        .expect("member of its group");
    Ok(key_between(
        Some(&group[idx].sort_key),
        group.get(idx + 1).map(|n| n.sort_key.as_str()),
    ))
}

/// Keys of a group must be strictly increasing to place anything between them. Annotations
/// written on different points can end up under one point after the report changes, with
/// equal keys; such a group is given fresh keys in its current order.
fn renumber_if_needed(store: &Store, group: Vec<&HumanNodeRow>) -> Result<Vec<HumanNodeRow>> {
    let ordered = group.windows(2).all(|w| w[0].sort_key < w[1].sort_key);
    let mut out: Vec<HumanNodeRow> = group.into_iter().cloned().collect();
    if !ordered {
        let keys = keys_after(None, out.len());
        for (n, key) in out.iter_mut().zip(keys) {
            store.set_human_sort_key(n.id, &key)?;
            n.sort_key = key;
        }
    }
    Ok(out)
}

fn project_named(store: &Store, name: &str) -> Result<Option<i64>> {
    Ok(store.find_project(name)?.map(|p| p.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aj_store::NewReportVersion;

    const DAY: &str = "2026-11-21";

    fn store_with_report() -> (Store, i64) {
        let mut store = Store::open_in_memory().unwrap();
        let app = store.create_project("App", None).unwrap();
        store
            .save_report_version(&NewReportVersion {
                report_type: "daily",
                period_key: DAY,
                project_id: None,
                origin: aj_store::ReportOrigin::Generated,
                markdown:
                    "# x\n\n## App\n- Built the search view [#S1.T2]\n- Fixed the menu [#S2.T1]",
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

    fn bodies(store: &Store) -> Vec<(String, Option<i64>)> {
        let view = load(store, "daily", DAY).unwrap();
        // Depth-first, as shown.
        fn walk(
            view: &HumanView,
            parent: Option<i64>,
            depth: usize,
            out: &mut Vec<(String, Option<i64>)>,
        ) {
            let mut kids: Vec<&HumanNodeRow> = view
                .nodes
                .iter()
                .filter(|n| n.parent_id == parent)
                .collect();
            kids.sort_by(|a, b| (a.project_id, &a.sort_key).cmp(&(b.project_id, &b.sort_key)));
            for n in kids {
                out.push((format!("{}{}", "  ".repeat(depth), n.body), n.project_id));
                walk(view, Some(n.id), depth + 1, out);
            }
        }
        let mut out = Vec::new();
        walk(&view, None, 0, &mut out);
        out
    }

    #[test]
    fn notes_are_created_and_moved_like_an_outline() {
        let (mut store, app) = store_with_report();
        let a = create(
            &store,
            "daily",
            DAY,
            NewPlace::ProjectNote(app, HumanPosition::Bottom),
            "a",
        )
        .unwrap();
        let c = create(
            &store,
            "daily",
            DAY,
            NewPlace::ProjectNote(app, HumanPosition::Bottom),
            "c",
        )
        .unwrap();
        let b = create(&store, "daily", DAY, NewPlace::After(a), "b").unwrap();
        create(&store, "daily", DAY, NewPlace::DayNote, "day").unwrap();
        let names = |s: &Store| bodies(s).into_iter().map(|(b, _)| b).collect::<Vec<_>>();
        assert_eq!(names(&store), vec!["day", "a", "b", "c"]);

        move_node(&mut store, "daily", DAY, b, Move::Indent).unwrap();
        move_node(&mut store, "daily", DAY, c, Move::Indent).unwrap();
        assert_eq!(names(&store), vec!["day", "a", "  b", "  c"]);

        move_node(&mut store, "daily", DAY, c, Move::Up).unwrap();
        assert_eq!(names(&store), vec!["day", "a", "  c", "  b"]);

        move_node(&mut store, "daily", DAY, c, Move::Outdent).unwrap();
        assert_eq!(names(&store), vec!["day", "a", "  b", "c"]);

        let d = create(&store, "daily", DAY, NewPlace::LastChild(b), "d").unwrap();
        create(&store, "daily", DAY, NewPlace::After(d), "e").unwrap();
        assert_eq!(
            names(&store),
            vec!["day", "a", "  b", "    d", "    e", "c"]
        );
        assert!(bodies(&store).iter().skip(1).all(|(_, p)| *p == Some(app)));
    }

    #[test]
    fn project_notes_sit_at_either_end_and_cross_the_points() {
        let (mut store, app) = store_with_report();
        let top = HumanPosition::Top;
        let bottom = HumanPosition::Bottom;
        let b1 = create(
            &store,
            "daily",
            DAY,
            NewPlace::ProjectNote(app, bottom),
            "b1",
        )
        .unwrap();
        let t1 = create(&store, "daily", DAY, NewPlace::ProjectNote(app, top), "t1").unwrap();
        let t2 = create(&store, "daily", DAY, NewPlace::After(t1), "t2").unwrap();
        let child = create(&store, "daily", DAY, NewPlace::LastChild(t2), "c").unwrap();
        // Each end in order, as shown: (body, position).
        let ends = |s: &Store| {
            let view = load(s, "daily", DAY).unwrap();
            [top, bottom].map(|p| {
                notes_of(&view, Some(app), p)
                    .iter()
                    .map(|n| n.body.clone())
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(ends(&store), [vec!["t1", "t2"], vec!["b1"]]);
        let pos = |s: &Store, id| s.human_node(id).unwrap().unwrap().position;
        assert_eq!(pos(&store, child), top);

        // Down from the last top note: to the start of the bottom notes, with its child.
        move_node(&mut store, "daily", DAY, t2, Move::Down).unwrap();
        assert_eq!(ends(&store), [vec!["t1"], vec!["t2", "b1"]]);
        assert_eq!(pos(&store, child), bottom);
        // Up from the first bottom note: back to the end of the top notes.
        move_node(&mut store, "daily", DAY, t2, Move::Up).unwrap();
        assert_eq!(ends(&store), [vec!["t1", "t2"], vec!["b1"]]);
        // Up from the first top note, down from the last bottom note: nowhere to go.
        move_node(&mut store, "daily", DAY, t1, Move::Up).unwrap();
        move_node(&mut store, "daily", DAY, b1, Move::Down).unwrap();
        assert_eq!(ends(&store), [vec!["t1", "t2"], vec!["b1"]]);

        // Day notes and a project's own page have no top.
        let page = create(
            &store,
            PROJECT_PAGE,
            &app.to_string(),
            NewPlace::ProjectNote(app, top),
            "plan",
        )
        .unwrap();
        assert_eq!(pos(&store, page), bottom);
    }

    #[test]
    fn dragged_notes_move_together_and_stay_out_of_other_groups() {
        let (mut store, app) = store_with_report();
        let top = HumanPosition::Top;
        let bottom = HumanPosition::Bottom;
        let mk = |s: &Store, body: &str, p| {
            create(s, "daily", DAY, NewPlace::ProjectNote(app, p), body).unwrap()
        };
        let a = mk(&store, "a", bottom);
        let b = mk(&store, "b", bottom);
        let c = mk(&store, "c", bottom);
        let b1 = create(&store, "daily", DAY, NewPlace::LastChild(b), "b1").unwrap();
        let t = mk(&store, "t", top);
        let ends = |s: &Store| {
            let view = load(s, "daily", DAY).unwrap();
            [top, bottom].map(|p| {
                let mut group = notes_of(&view, Some(app), p);
                group.sort_by(|x, y| x.sort_key.cmp(&y.sort_key));
                group.iter().map(|n| n.body.clone()).collect::<Vec<_>>()
            })
        };
        let drop_notes = |position, after| Drop::Notes {
            project_id: Some(app),
            position,
            after,
        };

        // c and b (with b1, also picked) to the top, after t, in the order given.
        move_to(
            &mut store,
            "daily",
            DAY,
            &[c, b, b1],
            drop_notes(top, Some(t)),
        )
        .unwrap();
        assert_eq!(ends(&store), [vec!["t", "c", "b"], vec!["a"]]);
        let node = |s: &Store, id| s.human_node(id).unwrap().unwrap();
        assert_eq!(node(&store, b1).parent_id, Some(b), "b1 came along under b");
        assert_eq!(node(&store, b1).position, top);

        // t under a (at the bottom), as its first child.
        move_to(
            &mut store,
            "daily",
            DAY,
            &[t],
            Drop::Children {
                parent: a,
                after: None,
            },
        )
        .unwrap();
        assert_eq!(node(&store, t).parent_id, Some(a));
        assert_eq!(node(&store, t).position, bottom);
        // Back to the start of the top notes.
        move_to(&mut store, "daily", DAY, &[t], drop_notes(top, None)).unwrap();
        assert_eq!(ends(&store), [vec!["t", "c", "b"], vec!["a"]]);

        // Refused: under itself, into the day's notes, after a node that moves.
        let under_self = Drop::Children {
            parent: b1,
            after: None,
        };
        assert!(move_to(&mut store, "daily", DAY, &[b], under_self).is_err());
        let day = Drop::Notes {
            project_id: None,
            position: bottom,
            after: None,
        };
        assert!(move_to(&mut store, "daily", DAY, &[a], day).is_err());
        assert!(move_to(&mut store, "daily", DAY, &[c], drop_notes(top, Some(c))).is_err());
        assert_eq!(ends(&store), [vec!["t", "c", "b"], vec!["a"]]);
    }

    #[test]
    fn annotations_hang_on_points_and_are_reanchored_on_edit() {
        let (store, app) = store_with_report();
        let display = report_io::display_markdown(
            "daily",
            DAY,
            &store
                .current_report_version("daily", DAY, None)
                .unwrap()
                .unwrap()
                .markdown,
            "en",
            &report_io::project_order(&store).unwrap(),
        );
        let line = display
            .split('\n')
            .position(|l| l.contains("Fixed the menu"))
            .unwrap();
        let ann = create(
            &store,
            "daily",
            DAY,
            NewPlace::Annotation(line),
            "Mostly by hand",
        )
        .unwrap();
        let second = create(&store, "daily", DAY, NewPlace::After(ann), "And tested").unwrap();

        let view = load(&store, "daily", DAY).unwrap();
        assert_eq!(view.placements.get(&ann), Some(&Placement::Point(line)));
        assert_eq!(view.placements.get(&second), Some(&Placement::Point(line)));
        let node = view.nodes.iter().find(|n| n.id == ann).unwrap();
        assert_eq!(node.project_id, Some(app));

        // Not a point: refused.
        assert!(create(&store, "daily", DAY, NewPlace::Annotation(0), "x").is_err());

        update_body(&store, "daily", DAY, ann, "Mostly by hand, twice").unwrap();
        let view = load(&store, "daily", DAY).unwrap();
        assert_eq!(view.placements.get(&ann), Some(&Placement::Point(line)));
    }

    #[test]
    fn colliding_keys_are_renumbered_before_placing_between_them() {
        let (store, app) = store_with_report();
        for body in ["x", "y"] {
            store
                .insert_human_root(&NewHumanRoot {
                    period_kind: "day",
                    period_key: DAY,
                    project_id: Some(app),
                    attach_json: None,
                    position: HumanPosition::Bottom,
                    sort_key: "V",
                    body,
                })
                .unwrap();
        }
        let first = load(&store, "daily", DAY).unwrap().nodes[0].id;
        create(&store, "daily", DAY, NewPlace::After(first), "between").unwrap();
        let names: Vec<String> = bodies(&store).into_iter().map(|(b, _)| b).collect();
        assert_eq!(names, vec!["x", "between", "y"]);
    }

    #[test]
    fn a_project_page_holds_only_its_own_notes_apart_from_periods() {
        let (mut store, app) = store_with_report();
        let other = store.create_project("Other", None).unwrap();
        let page = app.to_string();
        let a = create(
            &store,
            PROJECT_PAGE,
            &page,
            NewPlace::ProjectNote(app, HumanPosition::Bottom),
            "plan",
        )
        .unwrap();
        create(&store, PROJECT_PAGE, &page, NewPlace::LastChild(a), "step").unwrap();
        for wrong in [
            NewPlace::DayNote,
            NewPlace::ProjectNote(other, HumanPosition::Bottom),
            NewPlace::Annotation(3),
        ] {
            assert!(create(&store, PROJECT_PAGE, &page, wrong, "x").is_err());
        }
        assert_eq!(load(&store, PROJECT_PAGE, &page).unwrap().nodes.len(), 2);
        // Not part of any period.
        assert!(load(&store, "daily", DAY).unwrap().nodes.is_empty());

        // Deleted with its children, and put back.
        let removed = delete(&mut store, &[a]).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(load(&store, PROJECT_PAGE, &page).unwrap().nodes.is_empty());
        restore(&mut store, &removed).unwrap();
        assert_eq!(load(&store, PROJECT_PAGE, &page).unwrap().nodes.len(), 2);
    }
}
