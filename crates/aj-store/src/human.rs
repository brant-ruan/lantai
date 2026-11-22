//! Human journal nodes: what people write next to the generated reports.
//!
//! Every node belongs to a period (`day` / `week` / `month` plus its key), or to a project's own
//! page ([`PROJECT_PERIOD`] with the project id as key). A root node with `attach_json` is an
//! annotation on a point of the generated report; otherwise a root with a project is a project
//! note and one without is a day note. Children copy their root's period,
//! project, kind and position, so lookups need no tree walk. A project note sits at the top or
//! the bottom of its project's section ([`HumanPosition`]); everything else is at the bottom.
//! Sibling order is a fractional `sort_key` computed by the caller.

use anyhow::{bail, Result};
use chrono::Utc;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::{params, OptionalExtension, Row, ToSql, Transaction};

use crate::{ts, Store};

/// Where a project note shows in its project's section: before or after the generated points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HumanPosition {
    Top,
    #[default]
    Bottom,
}

impl HumanPosition {
    /// The stored value.
    pub fn as_str(self) -> &'static str {
        match self {
            HumanPosition::Top => "top",
            HumanPosition::Bottom => "bottom",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "top" => Some(HumanPosition::Top),
            "bottom" => Some(HumanPosition::Bottom),
            _ => None,
        }
    }
}

impl ToSql for HumanPosition {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for HumanPosition {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let s = value.as_str()?;
        HumanPosition::parse(s)
            .ok_or_else(|| FromSqlError::Other(format!("unknown note position {s:?}").into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanNodeRow {
    pub id: i64,
    /// `day` | `week` | `month`
    pub period_kind: String,
    pub period_key: String,
    pub project_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub sort_key: String,
    /// Where an annotation hangs (JSON owned by `aj-report`); `None` for notes and children.
    pub attach_json: Option<String>,
    pub position: HumanPosition,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Period kind of the notes written on a project's own page; the key is the project id.
pub const PROJECT_PERIOD: &str = "project";

/// A new root node. Children are added with [`Store::insert_human_child`].
#[derive(Debug, Clone)]
pub struct NewHumanRoot<'a> {
    pub period_kind: &'a str,
    pub period_key: &'a str,
    pub project_id: Option<i64>,
    pub attach_json: Option<&'a str>,
    pub position: HumanPosition,
    pub sort_key: &'a str,
    pub body: &'a str,
}

/// Where [`Store::relocate_human_nodes`] puts a node: under `parent_id` (taking the parent's
/// position, without an attachment), or as a root with `attach_json` and `position`.
#[derive(Debug, Clone)]
pub struct HumanRelocation {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub attach_json: Option<String>,
    pub position: HumanPosition,
    pub sort_key: String,
}

/// Number of nodes per period key, for one period kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HumanCountRow {
    pub period_key: String,
    pub notes: u64,
}

const COLUMNS: &str = "id, period_kind, period_key, project_id, parent_id, sort_key, attach_json, \
                       position, body, created_at, updated_at";

fn row(r: &Row) -> rusqlite::Result<HumanNodeRow> {
    Ok(HumanNodeRow {
        id: r.get(0)?,
        period_kind: r.get(1)?,
        period_key: r.get(2)?,
        project_id: r.get(3)?,
        parent_id: r.get(4)?,
        sort_key: r.get(5)?,
        attach_json: r.get(6)?,
        position: r.get(7)?,
        body: r.get(8)?,
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
    })
}

impl Store {
    /// Every node of a period, roots and children alike, in sibling order.
    pub fn human_nodes(&self, period_kind: &str, period_key: &str) -> Result<Vec<HumanNodeRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM human_nodes WHERE period_kind = ?1 AND period_key = ?2
             ORDER BY parent_id IS NOT NULL, parent_id, sort_key, id"
        ))?;
        let rows = stmt
            .query_map(params![period_kind, period_key], row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Every node of every period, for search.
    pub fn all_human_nodes(&self) -> Result<Vec<HumanNodeRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM human_nodes
             ORDER BY period_kind, period_key, parent_id IS NOT NULL, parent_id, sort_key, id"
        ))?;
        let rows = stmt
            .query_map([], row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Every node that carries a project (its notes and the annotations on its sections),
    /// across periods, in period and sibling order.
    pub fn human_nodes_of_project(&self, project_id: i64) -> Result<Vec<HumanNodeRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM human_nodes WHERE project_id = ?1
             ORDER BY period_kind, period_key, parent_id IS NOT NULL, parent_id, sort_key, id"
        ))?;
        let rows = stmt
            .query_map([project_id], row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Top-level notes and annotations per period key of one kind (for the sidebar); the
    /// blocks under them are part of the same note.
    pub fn human_counts(&self, period_kind: &str) -> Result<Vec<HumanCountRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT period_key, COUNT(*) FROM human_nodes
             WHERE period_kind = ?1 AND parent_id IS NULL
             GROUP BY period_key ORDER BY period_key",
        )?;
        let rows = stmt
            .query_map([period_kind], |r| {
                Ok(HumanCountRow {
                    period_key: r.get(0)?,
                    notes: r.get::<_, i64>(1)? as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn human_node(&self, id: i64) -> Result<Option<HumanNodeRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLUMNS} FROM human_nodes WHERE id = ?1"),
                [id],
                row,
            )
            .optional()?)
    }

    pub fn insert_human_root(&self, n: &NewHumanRoot) -> Result<i64> {
        let now = ts(Utc::now());
        self.conn.execute(
            "INSERT INTO human_nodes (period_kind, period_key, project_id, parent_id, sort_key,
                                      attach_json, position, body, created_at, updated_at)
             VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                n.period_kind,
                n.period_key,
                n.project_id,
                n.sort_key,
                n.attach_json,
                n.position,
                n.body,
                now
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Add a child under `parent_id`, which lends it its period, project and position.
    pub fn insert_human_child(&self, parent_id: i64, sort_key: &str, body: &str) -> Result<i64> {
        let now = ts(Utc::now());
        let n = self.conn.execute(
            "INSERT INTO human_nodes (period_kind, period_key, project_id, parent_id, sort_key,
                                      attach_json, position, body, created_at, updated_at)
             SELECT period_kind, period_key, project_id, id, ?2, NULL, position, ?3, ?4, ?4
             FROM human_nodes WHERE id = ?1",
            params![parent_id, sort_key, body, now],
        )?;
        if n == 0 {
            bail!("human node {parent_id} does not exist");
        }
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_human_body(&self, id: i64, body: &str) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE human_nodes SET body = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, body, ts(Utc::now())],
        )?;
        if n == 0 {
            bail!("human node {id} does not exist");
        }
        Ok(())
    }

    /// Give a node a new position among its siblings (used to renumber a group whose keys
    /// collided).
    pub fn set_human_sort_key(&self, id: i64, sort_key: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE human_nodes SET sort_key = ?2 WHERE id = ?1",
            params![id, sort_key],
        )?;
        Ok(())
    }

    /// Move a root note with everything under it to the other end of its project's section,
    /// at `sort_key` among the notes there.
    pub fn set_human_position(
        &mut self,
        id: i64,
        position: HumanPosition,
        sort_key: &str,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let n = tx.execute(
            "UPDATE human_nodes SET sort_key = ?2, updated_at = ?3
             WHERE id = ?1 AND parent_id IS NULL",
            params![id, sort_key, ts(Utc::now())],
        )?;
        if n == 0 {
            bail!("human node {id} is not a root");
        }
        set_subtree_position(&tx, id, position)?;
        tx.commit()?;
        Ok(())
    }

    /// Re-anchor an annotation (after the point it hangs on was found again elsewhere).
    pub fn update_human_attach(&self, id: i64, attach_json: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE human_nodes SET attach_json = ?2 WHERE id = ?1 AND parent_id IS NULL",
            params![id, attach_json],
        )?;
        Ok(())
    }

    /// Move a node under another parent (or back to the roots of its group) with a new sort
    /// key. A node cannot move under itself or its own descendants, nor across periods. A
    /// child that becomes a root joins the group of the root it came from (the same project
    /// and position, or the same annotated point); a root that becomes a child drops its
    /// attachment and takes its new parent's position, with everything under it.
    pub fn move_human_node(
        &mut self,
        id: i64,
        parent_id: Option<i64>,
        sort_key: &str,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let Some(node) = tx
            .query_row(
                &format!("SELECT {COLUMNS} FROM human_nodes WHERE id = ?1"),
                [id],
                row,
            )
            .optional()?
        else {
            bail!("human node {id} does not exist");
        };
        if let Some(p) = parent_id {
            let parent = tx
                .query_row(
                    &format!("SELECT {COLUMNS} FROM human_nodes WHERE id = ?1"),
                    [p],
                    row,
                )
                .optional()?;
            let Some(parent) = parent else {
                bail!("human node {p} does not exist");
            };
            if parent.period_kind != node.period_kind || parent.period_key != node.period_key {
                bail!("a human node cannot move to another period");
            }
            let inside: bool = tx.query_row(
                "WITH RECURSIVE up(id, parent_id) AS (
                     SELECT id, parent_id FROM human_nodes WHERE id = ?1
                     UNION ALL
                     SELECT n.id, n.parent_id FROM human_nodes n JOIN up ON n.id = up.parent_id
                 ) SELECT EXISTS (SELECT 1 FROM up WHERE id = ?2)",
                params![p, id],
                |r| r.get(0),
            )?;
            if inside {
                bail!("a human node cannot move under itself");
            }
        } else if node.parent_id.is_some() {
            // Becoming a root again: keep the group of the root it came from.
            let attach: Option<String> = tx.query_row(
                "WITH RECURSIVE up(id, parent_id, attach_json) AS (
                     SELECT id, parent_id, attach_json FROM human_nodes WHERE id = ?1
                     UNION ALL
                     SELECT n.id, n.parent_id, n.attach_json FROM human_nodes n
                     JOIN up ON n.id = up.parent_id
                 ) SELECT attach_json FROM up WHERE parent_id IS NULL",
                [id],
                |r| r.get(0),
            )?;
            tx.execute(
                "UPDATE human_nodes SET attach_json = ?2 WHERE id = ?1",
                params![id, attach],
            )?;
        }
        tx.execute(
            "UPDATE human_nodes SET parent_id = ?2, sort_key = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, parent_id, sort_key, ts(Utc::now())],
        )?;
        if let Some(p) = parent_id {
            tx.execute(
                "UPDATE human_nodes SET attach_json = NULL WHERE id = ?1",
                [id],
            )?;
            let position: HumanPosition =
                tx.query_row("SELECT position FROM human_nodes WHERE id = ?1", [p], |r| {
                    r.get(0)
                })?;
            set_subtree_position(&tx, id, position)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Move several nodes, each with everything under it, in one transaction (a drag). Every
    /// node stays in its period and cannot go under itself or its own descendants.
    pub fn relocate_human_nodes(&mut self, moves: &[HumanRelocation]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let get = |id: i64| -> Result<HumanNodeRow> {
            tx.query_row(
                &format!("SELECT {COLUMNS} FROM human_nodes WHERE id = ?1"),
                [id],
                row,
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("human node {id} does not exist"))
        };
        let now = ts(Utc::now());
        for m in moves {
            let node = get(m.id)?;
            let (attach, position) = match m.parent_id {
                Some(p) => {
                    let parent = get(p)?;
                    if parent.period_kind != node.period_kind
                        || parent.period_key != node.period_key
                    {
                        bail!("a human node cannot move to another period");
                    }
                    let inside: bool = tx.query_row(
                        "WITH RECURSIVE up(id, parent_id) AS (
                             SELECT id, parent_id FROM human_nodes WHERE id = ?1
                             UNION ALL
                             SELECT n.id, n.parent_id FROM human_nodes n JOIN up ON n.id = up.parent_id
                         ) SELECT EXISTS (SELECT 1 FROM up WHERE id = ?2)",
                        params![p, m.id],
                        |r| r.get(0),
                    )?;
                    if inside {
                        bail!("a human node cannot move under itself");
                    }
                    (None, parent.position)
                }
                None => (m.attach_json.clone(), m.position),
            };
            tx.execute(
                "UPDATE human_nodes SET parent_id = ?2, sort_key = ?3, attach_json = ?4,
                                        updated_at = ?5
                 WHERE id = ?1",
                params![m.id, m.parent_id, m.sort_key, attach, now],
            )?;
            set_subtree_position(&tx, m.id, position)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Delete nodes and everything under them. Returns the removed rows (each subtree parent
    /// first), for [`Store::restore_human_nodes`] to put back.
    pub fn delete_human_nodes(&mut self, ids: &[i64]) -> Result<Vec<HumanNodeRow>> {
        let tx = self.conn.transaction()?;
        let mut removed: Vec<HumanNodeRow> = Vec::new();
        for &id in ids {
            if removed.iter().any(|n| n.id == id) {
                continue;
            }
            let mut stmt = tx.prepare(&format!(
                "WITH RECURSIVE down(id, depth) AS (
                     SELECT id, 0 FROM human_nodes WHERE id = ?1
                     UNION ALL
                     SELECT n.id, down.depth + 1 FROM human_nodes n JOIN down ON n.parent_id = down.id
                 )
                 SELECT {COLUMNS} FROM human_nodes WHERE id IN (SELECT id FROM down)
                 ORDER BY (SELECT depth FROM down WHERE down.id = human_nodes.id), sort_key, id"
            ))?;
            let rows = stmt
                .query_map([id], row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(stmt);
            // A node already removed with an earlier one's subtree adds nothing.
            removed.extend(rows);
            tx.execute("DELETE FROM human_nodes WHERE id = ?1", [id])?;
        }
        tx.commit()?;
        Ok(removed)
    }

    /// Put back rows returned by [`Store::delete_human_nodes`]. A row keeps its id when it is
    /// still free and gets a new one otherwise; children follow their parent either way, in
    /// whatever order the rows come. A project deleted in the meantime leaves the note without
    /// one. Returns the id each row now has, in the same order.
    pub fn restore_human_nodes(&mut self, rows: &[HumanNodeRow]) -> Result<Vec<i64>> {
        let tx = self.conn.transaction()?;
        let exists = |id: i64| -> rusqlite::Result<bool> {
            tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM human_nodes WHERE id = ?1)",
                [id],
                |r| r.get(0),
            )
        };
        // Old id -> new id, for the rows put back so far.
        let mut ids: Vec<Option<i64>> = vec![None; rows.len()];
        let new_id =
            |ids: &[Option<i64>], old: i64| rows.iter().position(|r| r.id == old).map(|i| ids[i]);
        loop {
            let mut progressed = false;
            for (i, n) in rows.iter().enumerate() {
                if ids[i].is_some() {
                    continue;
                }
                let parent_id = match n.parent_id {
                    None => None,
                    Some(p) => match new_id(&ids, p) {
                        // The parent is among the rows: wait until it is back.
                        Some(None) => continue,
                        Some(Some(id)) => Some(id),
                        None if exists(p)? => Some(p),
                        None => bail!("the parent of human node {} no longer exists", n.id),
                    },
                };
                let taken = exists(n.id)?;
                tx.execute(
                    "INSERT INTO human_nodes (id, period_kind, period_key, project_id, parent_id,
                                              sort_key, attach_json, position, body, created_at,
                                              updated_at)
                     VALUES (?1, ?2, ?3, (SELECT id FROM logical_projects WHERE id = ?4), ?5,
                             ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        (!taken).then_some(n.id),
                        n.period_kind,
                        n.period_key,
                        n.project_id,
                        parent_id,
                        n.sort_key,
                        n.attach_json,
                        n.position,
                        n.body,
                        n.created_at,
                        n.updated_at
                    ],
                )?;
                ids[i] = Some(tx.last_insert_rowid());
                progressed = true;
            }
            if ids.iter().all(Option::is_some) {
                break;
            }
            if !progressed {
                bail!("human nodes to restore form a cycle");
            }
        }
        tx.commit()?;
        Ok(ids.into_iter().flatten().collect())
    }
}

/// Give a node and everything under it `position`.
fn set_subtree_position(tx: &Transaction, id: i64, position: HumanPosition) -> Result<()> {
    tx.execute(
        "WITH RECURSIVE down(id) AS (
             SELECT ?1
             UNION ALL
             SELECT n.id FROM human_nodes n JOIN down ON n.parent_id = down.id
         )
         UPDATE human_nodes SET position = ?2 WHERE id IN (SELECT id FROM down)",
        params![id, position],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root<'a>(project_id: Option<i64>, key: &'a str, body: &'a str) -> NewHumanRoot<'a> {
        NewHumanRoot {
            period_kind: "day",
            period_key: "2026-11-21",
            project_id,
            attach_json: None,
            position: HumanPosition::Bottom,
            sort_key: key,
            body,
        }
    }

    #[test]
    fn children_inherit_and_follow_their_root() {
        let mut store = Store::open_in_memory().unwrap();
        let p = store.create_project("Garden", None).unwrap();
        let a = store
            .insert_human_root(&root(Some(p), "V", "Planted tomatoes"))
            .unwrap();
        let b = store
            .insert_human_root(&root(Some(p), "k", "Bought seeds"))
            .unwrap();
        let c = store.insert_human_child(a, "V", "Two rows").unwrap();
        store
            .insert_human_root(&root(None, "V", "A quiet day"))
            .unwrap();

        let child = store.human_node(c).unwrap().unwrap();
        assert_eq!((child.project_id, child.parent_id), (Some(p), Some(a)));
        assert_eq!(child.period_key, "2026-11-21");

        let roots: Vec<i64> = store
            .human_nodes("day", "2026-11-21")
            .unwrap()
            .into_iter()
            .filter(|r| r.parent_id.is_none() && r.project_id == Some(p))
            .map(|r| r.id)
            .collect();
        assert_eq!(roots, vec![a, b]);

        // Indent b under a, then cannot move a under its own descendant.
        store.move_human_node(b, Some(a), "k").unwrap();
        assert!(store.move_human_node(a, Some(b), "V").is_err());
        // Outdent c back to the project's roots.
        store.move_human_node(c, None, "z").unwrap();
        assert_eq!(store.human_node(c).unwrap().unwrap().parent_id, None);

        let removed = store.delete_human_nodes(&[a, b]).unwrap();
        assert_eq!(
            removed.iter().map(|n| n.id).collect::<Vec<_>>(),
            vec![a, b],
            "a parent comes before its children, and nothing twice"
        );
        assert!(
            store.human_node(b).unwrap().is_none(),
            "children go with their parent"
        );
        // Undo: both come back with their ids, the child under its parent.
        assert_eq!(store.restore_human_nodes(&removed).unwrap(), vec![a, b]);
        assert_eq!(store.human_node(b).unwrap().unwrap().parent_id, Some(a));
        // Children listed before their parent wait for it.
        let removed = store.delete_human_nodes(&[b, a]).unwrap();
        assert_eq!(store.restore_human_nodes(&removed).unwrap(), vec![b, a]);
        assert_eq!(store.human_node(b).unwrap().unwrap().parent_id, Some(a));
        store.delete_human_nodes(&[a]).unwrap();
        assert_eq!(
            store.human_counts("day").unwrap(),
            vec![HumanCountRow {
                period_key: "2026-11-21".into(),
                notes: 2,
            }]
        );
    }

    #[test]
    fn positions_follow_the_root_and_move_with_the_subtree() {
        let mut store = Store::open_in_memory().unwrap();
        let p = store.create_project("Garden", None).unwrap();
        let top = store
            .insert_human_root(&NewHumanRoot {
                position: HumanPosition::Top,
                ..root(Some(p), "V", "Before the points")
            })
            .unwrap();
        let bottom = store
            .insert_human_root(&root(Some(p), "V", "After the points"))
            .unwrap();
        let child = store.insert_human_child(bottom, "V", "Detail").unwrap();
        let pos = |s: &Store, id| s.human_node(id).unwrap().unwrap().position;
        assert_eq!(pos(&store, child), HumanPosition::Bottom);

        store
            .set_human_position(bottom, HumanPosition::Top, "k")
            .unwrap();
        assert_eq!(pos(&store, bottom), HumanPosition::Top);
        assert_eq!(pos(&store, child), HumanPosition::Top, "children go along");
        assert!(store
            .set_human_position(child, HumanPosition::Bottom, "k")
            .is_err());

        // Indenting takes the new parent's position.
        let other = store
            .insert_human_root(&root(Some(p), "z", "Later"))
            .unwrap();
        store.move_human_node(other, Some(top), "V").unwrap();
        assert_eq!(pos(&store, other), HumanPosition::Top);

        let removed = store.delete_human_nodes(&[bottom]).unwrap();
        store.restore_human_nodes(&removed).unwrap();
        assert_eq!(
            pos(&store, child),
            HumanPosition::Top,
            "undo keeps the position"
        );
    }

    #[test]
    fn annotations_group_by_attachment_and_lose_it_when_nested() {
        let mut store = Store::open_in_memory().unwrap();
        let p = store.create_project("App", None).unwrap();
        let attach = r#"{"section":"App"}"#;
        let new = |key: &'static str| NewHumanRoot {
            attach_json: Some(attach),
            ..root(Some(p), key, "Agree")
        };
        let a = store.insert_human_root(&new("V")).unwrap();
        let b = store.insert_human_root(&new("k")).unwrap();
        store
            .insert_human_root(&root(Some(p), "V", "A note"))
            .unwrap();
        let group: Vec<i64> = store
            .human_nodes("day", "2026-11-21")
            .unwrap()
            .into_iter()
            .filter(|r| r.attach_json.as_deref() == Some(attach))
            .map(|r| r.id)
            .collect();
        assert_eq!(group, vec![a, b]);

        store.move_human_node(b, Some(a), "V").unwrap();
        assert_eq!(store.human_node(b).unwrap().unwrap().attach_json, None);
        store.move_human_node(b, None, "z").unwrap();
        assert_eq!(
            store.human_node(b).unwrap().unwrap().attach_json.as_deref(),
            Some(attach),
            "an outdented child rejoins the annotated point"
        );
    }

    #[test]
    fn deleting_a_project_turns_its_notes_into_day_notes() {
        let mut store = Store::open_in_memory().unwrap();
        let p = store.create_project("Garden", None).unwrap();
        let a = store
            .insert_human_root(&root(Some(p), "V", "Planted tomatoes"))
            .unwrap();
        let c = store.insert_human_child(a, "V", "Two rows").unwrap();
        let ann = store
            .insert_human_root(&NewHumanRoot {
                attach_json: Some(r#"{"section":"Garden"}"#),
                ..root(Some(p), "V", "Nice")
            })
            .unwrap();
        let key = p.to_string();
        let page = store
            .insert_human_root(&NewHumanRoot {
                period_kind: PROJECT_PERIOD,
                period_key: &key,
                ..root(Some(p), "V", "Plan")
            })
            .unwrap();
        store.delete_project(p, "2026-11-22").unwrap();

        let page = store.human_node(page).unwrap().unwrap();
        assert_eq!(
            (
                page.period_kind.as_str(),
                page.period_key.as_str(),
                page.body.as_str()
            ),
            ("day", "2026-11-22", "Garden: Plan"),
            "notes of the project page join the day it is deleted"
        );
        let note = store.human_node(a).unwrap().unwrap();
        assert_eq!(note.project_id, None);
        assert_eq!(note.body, "Garden: Planted tomatoes");
        assert_eq!(store.human_node(c).unwrap().unwrap().body, "Two rows");
        assert_eq!(store.human_node(ann).unwrap().unwrap().body, "Nice");
    }
}
