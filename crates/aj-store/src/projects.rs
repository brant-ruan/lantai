//! Projects, match rules and session assignment.
//!
//! Projects are created by the user. Rules only produce suggestions unless they are marked auto-apply.

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use rusqlite::{params, OptionalExtension, Row};

use crate::sessions::{session_from_row, SESSION_COLS};
use crate::sql::{ts, SLICE_PROJECT, SLICE_UNASSIGNED};
use crate::{SessionRow, Store};

#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    /// Archived projects keep their data, but their rules are paused and they are not offered
    /// for new assignments.
    pub archived: bool,
}

/// The columns [`project_from_row`] reads, in its order (table alias `p`).
const PROJECT_COLS: &str = "p.id, p.name, p.description, p.is_archived";

/// The user's project order.
const PROJECT_ORDER: &str = "p.sort_order, p.name COLLATE NOCASE";

fn project_from_row(r: &Row) -> rusqlite::Result<ProjectRow> {
    Ok(ProjectRow {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        archived: r.get(3)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleRow {
    pub id: i64,
    pub project_id: i64,
    /// `path_prefix` | `git_remote` | `regex`
    pub rule_type: String,
    /// `regex` only: `title` | `first_prompt` | `cwd` | `any`
    pub target_field: Option<String>,
    pub pattern: String,
    /// Assign directly on a match instead of only suggesting.
    pub auto_apply: bool,
}

#[derive(Debug, Clone)]
pub struct NewRule<'a> {
    pub rule_type: &'a str,
    pub target_field: Option<&'a str>,
    pub pattern: &'a str,
    pub auto_apply: bool,
}

#[derive(Debug, Clone)]
pub struct ProjectSummary {
    pub project: ProjectRow,
    /// Daily slices in the project (slice override first, else the session's project).
    pub slices: u64,
    /// Unassigned daily slices whose session is suggested for the project.
    pub suggested: u64,
    pub rules: u64,
}

/// What deleting a project takes with it or changes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeleteImpact {
    /// Sessions assigned to the project; they become unassigned.
    pub sessions: u64,
    /// Project-specific reports (with all their versions); deleted.
    pub reports: u64,
    /// Assignment rules; deleted.
    pub rules: u64,
    /// Project notes; they become notes of their period (notes written on the project's own
    /// page become notes of the day the project is deleted).
    pub notes: u64,
}

/// A candidate project: a group of unassigned sessions in the same repository or directory.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub root: String,
    pub git_remote: Option<String>,
    pub sessions: u64,
    pub last_active_at: Option<String>,
    pub sample_titles: Vec<String>,
}

impl Store {
    // ---- Projects ----------------------------------------------------------

    pub fn create_project(&self, name: &str, description: Option<&str>) -> Result<i64> {
        let name = name.trim();
        if name.is_empty() {
            bail!("project name must not be empty");
        }
        if self.find_project(name)?.is_some() {
            bail!("a project named \"{name}\" already exists");
        }
        self.conn.execute(
            "INSERT INTO logical_projects (name, description, sort_order, created_at)
             VALUES (?1, ?2, (SELECT COALESCE(MAX(sort_order), -1) + 1 FROM logical_projects), ?3)",
            params![name, description, ts(Utc::now())],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn project(&self, id: i64) -> Result<Option<ProjectRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {PROJECT_COLS} FROM logical_projects p WHERE p.id = ?1"),
                [id],
                project_from_row,
            )
            .optional()?)
    }

    /// Find a project by name, case-insensitively.
    pub fn find_project(&self, name: &str) -> Result<Option<ProjectRow>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {PROJECT_COLS} FROM logical_projects p WHERE p.name = ?1 COLLATE NOCASE"
                ),
                [name.trim()],
                project_from_row,
            )
            .optional()?)
    }

    pub fn require_project(&self, name: &str) -> Result<ProjectRow> {
        self.find_project(name)?.with_context(|| {
            let names: Vec<String> = self
                .projects()
                .map(|l| l.into_iter().map(|p| p.name).collect())
                .unwrap_or_default();
            if names.is_empty() {
                format!("no project named \"{name}\" (there are no projects yet)")
            } else {
                format!(
                    "no project named \"{name}\"; existing projects: {}",
                    names.join(", ")
                )
            }
        })
    }

    /// Every project, in the user's order.
    pub fn projects(&self) -> Result<Vec<ProjectRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {PROJECT_COLS} FROM logical_projects p ORDER BY {PROJECT_ORDER}"
        ))?;
        let rows = stmt
            .query_map([], project_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Every project's name by its ID.
    pub fn project_names(&self) -> Result<HashMap<i64, String>> {
        Ok(self
            .projects()?
            .into_iter()
            .map(|p| (p.id, p.name))
            .collect())
    }

    /// Every project, in the user's order, with its slice, suggestion and rule counts. Counting
    /// reads every slice; use [`Store::projects`] when the counts are not needed.
    pub fn project_summaries(&self) -> Result<Vec<ProjectSummary>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {PROJECT_COLS},
                    (SELECT COUNT(*) FROM session_daily_slices sds
                       JOIN sessions s ON s.id = sds.session_id
                      WHERE {SLICE_PROJECT} = p.id),
                    (SELECT COUNT(*) FROM session_daily_slices sds
                       JOIN sessions s ON s.id = sds.session_id
                      WHERE {SLICE_UNASSIGNED} AND s.suggested_project_id = p.id),
                    (SELECT COUNT(*) FROM project_match_rules r WHERE r.project_id = p.id)
             FROM logical_projects p ORDER BY {PROJECT_ORDER}"
        ))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(ProjectSummary {
                    project: project_from_row(r)?,
                    slices: r.get::<_, i64>(4)? as u64,
                    suggested: r.get::<_, i64>(5)? as u64,
                    rules: r.get::<_, i64>(6)? as u64,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Set the user-defined display order: `ids` first, in that order; projects
    /// missing from `ids` keep their relative order after them.
    pub fn reorder_projects(&mut self, ids: &[i64]) -> Result<()> {
        let mut order: Vec<i64> = Vec::new();
        for id in ids {
            if !order.contains(id) {
                order.push(*id);
            }
        }
        for p in self.projects()? {
            if !order.contains(&p.id) {
                order.push(p.id);
            }
        }
        let tx = self.conn.transaction()?;
        for (pos, id) in order.iter().enumerate() {
            tx.execute(
                "UPDATE logical_projects SET sort_order = ?1 WHERE id = ?2",
                params![pos as i64, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Rename a project and, in the same transaction, rewrite every saved report
    /// version through `rewrite(markdown, old_name, new_name)` (which returns
    /// `None` when a version needs no change), so reports keep pointing at the
    /// project. Names are unique (case-insensitive). Returns the number of
    /// report versions rewritten.
    pub fn rename_project(
        &mut self,
        id: i64,
        new_name: &str,
        rewrite: impl Fn(&str, &str, &str) -> Option<String>,
    ) -> Result<usize> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            bail!("project name must not be empty");
        }
        let old = self
            .project(id)?
            .with_context(|| format!("no project with ID {id}"))?;
        if self.find_project(new_name)?.is_some_and(|p| p.id != id) {
            bail!("a project named \"{new_name}\" already exists");
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE logical_projects SET name = ?2 WHERE id = ?1",
            params![id, new_name],
        )?;
        let versions: Vec<(i64, String)> = {
            let mut stmt = tx.prepare("SELECT id, markdown_content FROM report_versions")?;
            let rows = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            rows
        };
        let mut rewritten = 0;
        for (vid, markdown) in versions {
            if let Some(updated) = rewrite(&markdown, &old.name, new_name) {
                tx.execute(
                    "UPDATE report_versions SET markdown_content = ?2 WHERE id = ?1",
                    params![vid, updated],
                )?;
                rewritten += 1;
            }
        }
        tx.commit()?;
        Ok(rewritten)
    }

    pub fn set_project_description(&self, id: i64, description: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE logical_projects SET description = ?2 WHERE id = ?1",
            params![id, description],
        )?;
        Ok(())
    }

    /// Archive or restore a project. Archiving keeps every assignment, report, note and rule; it
    /// drops the suggestions pointing at the project, since its rules are paused while archived
    /// (the caller re-evaluates assignments afterwards).
    pub fn set_project_archived(&mut self, id: i64, archived: bool) -> Result<()> {
        let tx = self.conn.transaction()?;
        let changed = tx.execute(
            "UPDATE logical_projects SET is_archived = ?2 WHERE id = ?1",
            params![id, archived],
        )?;
        if changed == 0 {
            bail!("no project with ID {id}");
        }
        if archived {
            tx.execute(
                "UPDATE sessions
                 SET suggested_project_id = NULL, suggestion_source = NULL, suggestion_reason = NULL
                 WHERE suggested_project_id = ?1",
                [id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// IDs of archived projects.
    pub fn archived_project_ids(&self) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM logical_projects WHERE is_archived = 1")?;
        let ids = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    /// What deleting a project would affect, for the confirmation shown before it.
    pub fn project_delete_impact(&self, id: i64) -> Result<DeleteImpact> {
        let count = |sql: &str| -> Result<u64> {
            Ok(self.conn.query_row(sql, [id], |r| r.get::<_, i64>(0))? as u64)
        };
        Ok(DeleteImpact {
            sessions: count("SELECT COUNT(*) FROM sessions WHERE project_id = ?1")?,
            reports: count("SELECT COUNT(*) FROM reports WHERE project_id = ?1")?,
            rules: count("SELECT COUNT(*) FROM project_match_rules WHERE project_id = ?1")?,
            notes: count(
                "SELECT COUNT(*) FROM human_nodes
                 WHERE project_id = ?1 AND parent_id IS NULL AND attach_json IS NULL",
            )?,
        })
    }

    /// Delete a project. Its sessions become unassigned; its rules and project-specific reports are deleted with it.
    ///
    /// `today` is the period key the notes written on the project's own page move to.
    pub fn delete_project(&mut self, id: i64, today: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE sessions SET assignment_source = 'unassigned' WHERE project_id = ?1",
            [id],
        )?;
        tx.execute(
            "UPDATE sessions SET suggestion_source = NULL, suggestion_reason = NULL
             WHERE suggested_project_id = ?1",
            [id],
        )?;
        // The project's notes become notes of their period, led by the project's name, which
        // have no top or bottom of a section. Annotations keep the section name in their
        // attachment and stay where they are.
        tx.execute(
            "UPDATE human_nodes SET position = 'bottom' WHERE project_id = ?1",
            [id],
        )?;
        tx.execute(
            "UPDATE human_nodes
             SET body = (SELECT name FROM logical_projects WHERE id = ?1) || ': ' || body
             WHERE project_id = ?1 AND parent_id IS NULL AND attach_json IS NULL",
            [id],
        )?;
        // Notes of the project's own page have no period of their own: they join today's.
        tx.execute(
            "UPDATE human_nodes SET period_kind = 'day', period_key = ?2
             WHERE period_kind = ?3 AND period_key = CAST(?1 AS TEXT)",
            params![id, today, crate::human::PROJECT_PERIOD],
        )?;
        // project_id (also on human nodes), suggested_project_id and slice overrides are
        // cleared by ON DELETE SET NULL.
        tx.execute("DELETE FROM logical_projects WHERE id = ?1", [id])?;
        tx.commit()?;
        Ok(())
    }

    // ---- Rules -------------------------------------------------------------

    pub fn add_rule(&self, project_id: i64, rule: &NewRule) -> Result<i64> {
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT project_id FROM project_match_rules WHERE rule_type = ?1 AND pattern = ?2",
                params![rule.rule_type, rule.pattern],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(owner) = existing {
            let name = self.project(owner)?.map(|p| p.name).unwrap_or_default();
            bail!("this rule already exists and belongs to project \"{name}\"");
        }
        self.conn.execute(
            "INSERT INTO project_match_rules (project_id, rule_type, target_field, pattern, auto_apply)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![project_id, rule.rule_type, rule.target_field, rule.pattern, rule.auto_apply],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn rules(&self) -> Result<Vec<RuleRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, rule_type, target_field, pattern, auto_apply
             FROM project_match_rules ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(RuleRow {
                    id: r.get(0)?,
                    project_id: r.get(1)?,
                    rule_type: r.get(2)?,
                    target_field: r.get(3)?,
                    pattern: r.get(4)?,
                    auto_apply: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn delete_rule(&self, id: i64) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM project_match_rules WHERE id = ?1", [id])?
            > 0)
    }

    pub fn set_rule_auto_apply(&self, id: i64, auto_apply: bool) -> Result<bool> {
        Ok(self.conn.execute(
            "UPDATE project_match_rules SET auto_apply = ?2 WHERE id = ?1",
            params![id, auto_apply],
        )? > 0)
    }

    // ---- Assignment --------------------------------------------------------

    /// Assign manually (`None` means explicitly no project). Rules never override this afterwards.
    pub fn assign_manual(&self, session_id: i64, project_id: Option<i64>) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET project_id = ?2, assignment_source = 'manual',
                    suggested_project_id = NULL, suggestion_source = NULL, suggestion_reason = NULL
             WHERE id = ?1",
            params![session_id, project_id],
        )?;
        Ok(())
    }

    /// Undo a manual assignment so that rules decide again.
    pub fn clear_manual(&self, session_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET project_id = NULL, assignment_source = 'unassigned'
             WHERE id = ?1 AND assignment_source = 'manual'",
            [session_id],
        )?;
        Ok(())
    }

    /// Assign through an auto-apply rule. Manually assigned sessions are not affected.
    pub fn assign_by_rule(&self, session_id: i64, project_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET project_id = ?2, assignment_source = 'rule',
                    suggested_project_id = NULL, suggestion_source = NULL, suggestion_reason = NULL
             WHERE id = ?1 AND assignment_source != 'manual'",
            params![session_id, project_id],
        )?;
        Ok(())
    }

    /// Undo an assignment made by a rule (when the rule is deleted or no longer matches).
    pub fn unassign_rule(&self, session_id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET project_id = NULL, assignment_source = 'unassigned'
             WHERE id = ?1 AND assignment_source = 'rule'",
            [session_id],
        )?;
        Ok(())
    }

    /// Set or clear the suggestion. Only meaningful for sessions that are not assigned yet.
    pub fn set_suggestion(
        &self,
        session_id: i64,
        suggestion: Option<(i64, &str, &str)>,
    ) -> Result<()> {
        let (project_id, source, reason) = match suggestion {
            Some((p, s, r)) => (Some(p), Some(s), Some(r)),
            None => (None, None, None),
        };
        self.conn.execute(
            "UPDATE sessions SET suggested_project_id = ?2, suggestion_source = ?3, suggestion_reason = ?4
             WHERE id = ?1",
            params![session_id, project_id, source, reason],
        )?;
        Ok(())
    }

    /// Accept the suggestion: the suggested project becomes a manually confirmed assignment. Returns whether there was one to accept.
    pub fn accept_suggestion(&self, session_id: i64) -> Result<bool> {
        Ok(self.conn.execute(
            "UPDATE sessions SET project_id = suggested_project_id, assignment_source = 'manual',
                    suggested_project_id = NULL, suggestion_source = NULL, suggestion_reason = NULL
             WHERE id = ?1 AND project_id IS NULL AND suggested_project_id IS NOT NULL",
            [session_id],
        )? > 0)
    }

    pub fn set_session_git(
        &self,
        session_id: i64,
        git_root: Option<&str>,
        git_remote: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET git_root = ?2, git_remote = ?3 WHERE id = ?1",
            params![session_id, git_root, git_remote],
        )?;
        Ok(())
    }

    /// Reassign a single day's slice (`None` restores following the session).
    pub fn set_slice_project_override(&self, slice_id: i64, project_id: Option<i64>) -> Result<()> {
        self.conn.execute(
            "UPDATE session_daily_slices SET project_id_override = ?2 WHERE id = ?1",
            params![slice_id, project_id],
        )?;
        Ok(())
    }

    pub fn all_sessions(&self) -> Result<Vec<SessionRow>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {SESSION_COLS} FROM sessions ORDER BY id"))?;
        let rows = stmt
            .query_map([], session_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// First user input of a session (for regex rules to match against).
    pub fn first_prompt(&self, session_id: i64) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT user_prompt FROM turns WHERE session_id = ?1 AND user_prompt != ''
                 ORDER BY turn_index LIMIT 1",
                [session_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Sessions with neither an assignment nor a suggestion, grouped by repository root (or working directory when there is none).
    pub fn project_candidates(&self) -> Result<Vec<Candidate>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(git_root, detected_cwd) AS root, MAX(git_remote), COUNT(*), MAX(last_active_at),
                    GROUP_CONCAT(COALESCE(title, fallback_title, ''), char(31))
             FROM sessions
             WHERE project_id IS NULL AND suggested_project_id IS NULL AND ignored = 0
                   AND assignment_source != 'manual' AND COALESCE(git_root, detected_cwd) IS NOT NULL
             GROUP BY root ORDER BY COUNT(*) DESC, root",
        )?;
        let rows = stmt
            .query_map([], |r| {
                let titles: Option<String> = r.get(4)?;
                Ok(Candidate {
                    root: r.get(0)?,
                    git_remote: r.get(1)?,
                    sessions: r.get::<_, i64>(2)? as u64,
                    last_active_at: r.get(3)?,
                    sample_titles: titles
                        .unwrap_or_default()
                        .split('\u{1f}')
                        .filter(|t| !t.is_empty())
                        .take(3)
                        .map(str::to_string)
                        .collect(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NewSlice;
    use aj_ir::{AgentId, IrEvent};

    fn store_with_session() -> (Store, i64) {
        let mut store = Store::open_in_memory().unwrap();
        let events = vec![IrEvent::TurnStart {
            source_event_id: "u1".into(),
            timestamp: "2026-11-16T01:00:00Z".parse().unwrap(),
            user_prompt: "hello".into(),
        }];
        let id = store
            .ingest(&AgentId::CLAUDE, "s1", &events)
            .unwrap()
            .session_id;
        (store, id)
    }

    #[test]
    fn projects_keep_creation_order_until_reordered() {
        let (mut store, _) = store_with_session();
        let b = store.create_project("Beta", None).unwrap();
        let a = store.create_project("alpha", None).unwrap();
        let c = store.create_project("Gamma", None).unwrap();
        let names = |s: &Store| -> Vec<String> {
            s.projects().unwrap().into_iter().map(|p| p.name).collect()
        };
        assert_eq!(names(&store), ["Beta", "alpha", "Gamma"]);
        // Unlisted projects follow the listed ones; duplicates are ignored.
        store.reorder_projects(&[c, a, c]).unwrap();
        assert_eq!(names(&store), ["Gamma", "alpha", "Beta"]);
        let _ = b;
    }

    #[test]
    fn project_names_are_unique_ignoring_case() {
        let (mut store, _) = store_with_session();
        let id = store.create_project("Atlas", Some("paper")).unwrap();
        assert!(store.create_project("atlas", None).is_err());
        assert!(store.create_project("  ", None).is_err());
        assert_eq!(store.find_project("ATLAS").unwrap().unwrap().id, id);
        let no_rewrite = |_: &str, _: &str, _: &str| None;
        store.rename_project(id, "atlas", no_rewrite).unwrap(); // changing only the case is allowed
        let other = store.create_project("Other", None).unwrap();
        assert!(store.rename_project(other, "Atlas", no_rewrite).is_err());
        assert!(store
            .require_project("nope")
            .unwrap_err()
            .to_string()
            .contains("Other"));
    }

    #[test]
    fn rename_rewrites_saved_reports_in_one_transaction() {
        let (mut store, _) = store_with_session();
        let id = store.create_project("Old", None).unwrap();
        let version = |md: &'static str| crate::NewReportVersion {
            report_type: "daily",
            period_key: "2026-11-17",
            project_id: None,
            origin: "generated",
            markdown: md,
            engine: None,
            input_hash: None,
            prompt_id: None,
            prompt_name: None,
            prompt_snapshot: None,
            input_manifest: None,
            language: "en",
        };
        store
            .save_report_version(&version("## Old\n- a\n"))
            .unwrap();
        store
            .save_report_version(&version("## Other\n- b\n"))
            .unwrap();
        let rewrite = |md: &str, old: &str, new: &str| {
            md.contains(&format!("## {old}\n"))
                .then(|| md.replace(&format!("## {old}\n"), &format!("## {new}\n")))
        };
        assert_eq!(store.rename_project(id, "New", rewrite).unwrap(), 1);
        assert_eq!(store.project(id).unwrap().unwrap().name, "New");
        let texts: Vec<String> = store
            .report_versions("daily", "2026-11-17", None)
            .unwrap()
            .into_iter()
            .map(|v| v.markdown)
            .collect();
        assert_eq!(texts, ["## New\n- a\n", "## Other\n- b\n"]);
    }

    #[test]
    fn archiving_drops_suggestions_and_impact_counts_what_deletion_takes() {
        let (mut store, sid) = store_with_session();
        let pid = store.create_project("App", None).unwrap();
        store
            .add_rule(
                pid,
                &NewRule {
                    rule_type: "path_prefix",
                    target_field: None,
                    pattern: "/work/app",
                    auto_apply: false,
                },
            )
            .unwrap();
        store
            .set_suggestion(sid, Some((pid, "llm", "judged by model")))
            .unwrap();

        store.set_project_archived(pid, true).unwrap();
        assert!(store.project(pid).unwrap().unwrap().archived);
        assert_eq!(store.archived_project_ids().unwrap(), vec![pid]);
        let s = store.session(sid).unwrap().unwrap();
        assert_eq!((s.suggested_project_id, s.suggestion_source), (None, None));

        store.assign_manual(sid, Some(pid)).unwrap();
        assert_eq!(
            store.project_delete_impact(pid).unwrap(),
            DeleteImpact {
                sessions: 1,
                reports: 0,
                rules: 1,
                notes: 0
            }
        );

        store.set_project_archived(pid, false).unwrap();
        assert!(store.archived_project_ids().unwrap().is_empty());
        assert!(store.set_project_archived(pid + 100, true).is_err());
    }

    #[test]
    fn suggestion_then_accept_becomes_manual() {
        let (mut store, sid) = store_with_session();
        let slice = |date: &str, start: &str| NewSlice {
            local_date: date.into(),
            start_turn_index: 0,
            end_turn_index: 0,
            slice_start_at: start.parse().unwrap(),
            slice_end_at: start.parse().unwrap(),
            active_minutes: 1,
            active_intervals_json: "[]".into(),
            files_modified_json: "[]".into(),
        };
        store
            .replace_slices(
                sid,
                &[
                    slice("2026-11-15", "2026-11-15T23:50:00Z"),
                    slice("2026-11-16", "2026-11-16T01:00:00Z"),
                ],
            )
            .unwrap();
        let pid = store.create_project("P", None).unwrap();
        store
            .set_suggestion(sid, Some((pid, "rule", "directory /work/p")))
            .unwrap();
        let s = store.session(sid).unwrap().unwrap();
        assert_eq!((s.project_id, s.suggested_project_id), (None, Some(pid)));
        assert_eq!(store.project_summaries().unwrap()[0].suggested, 2);
        assert_eq!(store.project_summaries().unwrap()[0].slices, 0);

        assert!(store.accept_suggestion(sid).unwrap());
        let s = store.session(sid).unwrap().unwrap();
        assert_eq!((s.project_id, s.suggested_project_id), (Some(pid), None));
        // Counts are per daily slice: one session across two days is two.
        let summary = &store.project_summaries().unwrap()[0];
        assert_eq!((summary.slices, summary.suggested), (2, 0));
        assert_eq!(s.assignment_source, "manual");
        assert!(!store.accept_suggestion(sid).unwrap());
    }

    #[test]
    fn rule_assignment_never_overrides_manual() {
        let (store, sid) = store_with_session();
        let a = store.create_project("A", None).unwrap();
        let b = store.create_project("B", None).unwrap();
        store.assign_manual(sid, Some(a)).unwrap();
        store.assign_by_rule(sid, b).unwrap();
        store.unassign_rule(sid).unwrap();
        assert_eq!(store.session(sid).unwrap().unwrap().project_id, Some(a));
    }

    #[test]
    fn deleting_project_releases_sessions_and_rules() {
        let (mut store, sid) = store_with_session();
        let pid = store.create_project("P", None).unwrap();
        let rule = NewRule {
            rule_type: "path_prefix",
            target_field: None,
            pattern: "/work/p",
            auto_apply: true,
        };
        store.add_rule(pid, &rule).unwrap();
        assert!(store.add_rule(pid, &rule).is_err());
        store.assign_by_rule(sid, pid).unwrap();

        store.delete_project(pid, "2026-11-21").unwrap();
        let s = store.session(sid).unwrap().unwrap();
        assert_eq!(
            (s.project_id, s.assignment_source.as_str()),
            (None, "unassigned")
        );
        assert!(store.rules().unwrap().is_empty());
    }
}
