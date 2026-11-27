//! Applies rule matches to sessions: auto-apply rules assign directly, the rest become suggestions awaiting confirmation.

use std::collections::HashSet;
use std::path::Path;

use aj_store::{AssignmentSource, RuleRow, SessionRow, Store, SuggestionSource};
use anyhow::Result;

use super::git;
use super::matcher::{match_rules, MatchContext};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EvaluateStats {
    pub evaluated: u64,
    pub assigned: u64,
    pub suggested: u64,
}

/// The rules in effect: those of archived projects are paused.
pub struct ActiveRules {
    rules: Vec<RuleRow>,
    archived: HashSet<i64>,
}

impl ActiveRules {
    pub fn load(store: &Store) -> Result<Self> {
        let archived: HashSet<i64> = store.archived_project_ids()?.into_iter().collect();
        let rules = store
            .rules()?
            .into_iter()
            .filter(|r| !archived.contains(&r.project_id))
            .collect();
        Ok(Self { rules, archived })
    }
}

/// Re-evaluate the assignment of one session. Manually assigned sessions are left alone, and so
/// are sessions a rule assigned to a project that has since been archived.
pub fn evaluate_session(
    store: &Store,
    active: &ActiveRules,
    session: &SessionRow,
    cwd: Option<String>,
    stats: &mut EvaluateStats,
) -> Result<()> {
    if session.assignment_source == AssignmentSource::Manual {
        return Ok(());
    }
    if session.assignment_source == AssignmentSource::Rule
        && session
            .project_id
            .is_some_and(|p| active.archived.contains(&p))
    {
        return Ok(());
    }
    let rules = active.rules.as_slice();
    stats.evaluated += 1;

    // Repository info can only be probed while the directory still exists; otherwise keep what was recorded before.
    let mut git_root = session.git_root.clone();
    let mut git_remote = session.git_remote.clone();
    if let Some(cwd) = &cwd {
        if let Some(root) = git::find_git_root(Path::new(cwd)) {
            git_remote = git::read_remote(&root).or(git_remote);
            git_root = Some(root.to_string_lossy().into_owned());
        }
        if git_root != session.git_root || git_remote != session.git_remote {
            store.set_session_git(session.id, git_root.as_deref(), git_remote.as_deref())?;
        }
    }

    let first_prompt = store.first_prompt(session.id)?;
    let matched = match_rules(
        rules,
        &MatchContext {
            cwd: cwd.as_deref(),
            git_root: git_root.as_deref(),
            git_remote: git_remote.as_deref(),
            title: session.title.as_deref(),
            first_prompt: first_prompt.as_deref(),
        },
    );

    match matched {
        Some(m) if m.auto_apply => {
            store.assign_by_rule(session.id, m.project_id)?;
            stats.assigned += 1;
        }
        Some(m) => {
            // Previously assigned by an auto-apply rule, now only a suggestion: back to unassigned.
            store.unassign_rule(session.id)?;
            store.set_suggestion(
                session.id,
                Some((m.project_id, SuggestionSource::Rule, &m.reason)),
            )?;
            stats.suggested += 1;
        }
        None => {
            store.unassign_rule(session.id)?;
            // Clear only suggestions produced by rules; suggestions from the model are unaffected by rule changes.
            if session.suggestion_source == Some(SuggestionSource::Rule) {
                store.set_suggestion(session.id, None)?;
            }
        }
    }
    Ok(())
}

/// Re-evaluate every session (call after rules change). `normalized_cwd` is supplied by the caller
/// to consult the adapters; the core layer does not care which agent a session came from.
pub fn evaluate_all(
    store: &Store,
    normalized_cwd: &dyn Fn(&SessionRow) -> Option<String>,
) -> Result<EvaluateStats> {
    let active = ActiveRules::load(store)?;
    let mut stats = EvaluateStats::default();
    for session in store.all_sessions()? {
        let cwd = normalized_cwd(&session);
        evaluate_session(store, &active, &session, cwd, &mut stats)?;
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aj_ir::{AgentId, IrEvent};
    use aj_store::NewRule;

    fn setup() -> (Store, i64) {
        let mut store = Store::open_in_memory().unwrap();
        let events = vec![
            IrEvent::SessionMeta {
                cwd: Some("/nonexistent/work/app/src".into()),
                git_branch: None,
                agent_version: None,
                parent_session_id: None,
            },
            IrEvent::SessionTitle {
                title: "Paper correctness review".into(),
            },
            IrEvent::TurnStart {
                source_event_id: "u1".into(),
                timestamp: "2026-11-16T01:00:00Z".parse().unwrap(),
                user_prompt: "check the paper".into(),
            },
        ];
        let sid = store
            .ingest(&AgentId::CLAUDE, "s1", &events)
            .unwrap()
            .session_id;
        (store, sid)
    }

    fn info(s: &SessionRow) -> Option<String> {
        s.detected_cwd.clone()
    }

    fn path_rule(auto_apply: bool) -> NewRule<'static> {
        NewRule {
            rule_type: "path_prefix",
            target_field: None,
            pattern: "/nonexistent/work/app",
            auto_apply,
        }
    }

    #[test]
    fn plain_rule_suggests_and_auto_rule_assigns() {
        let (store, sid) = setup();
        let pid = store.create_project("App", None).unwrap();
        let rule_id = store.add_rule(pid, &path_rule(false)).unwrap();

        let stats = evaluate_all(&store, &info).unwrap();
        assert_eq!(
            stats,
            EvaluateStats {
                evaluated: 1,
                assigned: 0,
                suggested: 1
            }
        );
        let s = store.session(sid).unwrap().unwrap();
        assert_eq!((s.project_id, s.suggested_project_id), (None, Some(pid)));
        assert_eq!(
            s.suggestion_reason.as_deref(),
            Some("directory /nonexistent/work/app")
        );

        store.set_rule_auto_apply(rule_id, true).unwrap();
        evaluate_all(&store, &info).unwrap();
        let s = store.session(sid).unwrap().unwrap();
        assert_eq!(
            (s.project_id, s.assignment_source),
            (Some(pid), AssignmentSource::Rule)
        );

        // Once the rule is deleted, the assignment it caused is undone.
        store.delete_rule(rule_id).unwrap();
        evaluate_all(&store, &info).unwrap();
        let s = store.session(sid).unwrap().unwrap();
        assert_eq!((s.project_id, s.suggested_project_id), (None, None));
    }

    #[test]
    fn manual_assignment_and_llm_suggestion_survive_reevaluation() {
        let (store, sid) = setup();
        let app = store.create_project("App", None).unwrap();
        let paper = store.create_project("Paper", None).unwrap();

        // With no rule matching, the model's suggestion is kept.
        store
            .set_suggestion(
                sid,
                Some((paper, SuggestionSource::Llm, "content is a paper review")),
            )
            .unwrap();
        evaluate_all(&store, &info).unwrap();
        assert_eq!(
            store.session(sid).unwrap().unwrap().suggested_project_id,
            Some(paper)
        );

        // After a manual assignment, not even an auto-apply rule may change it.
        store.assign_manual(sid, Some(paper)).unwrap();
        store.add_rule(app, &path_rule(true)).unwrap();
        let stats = evaluate_all(&store, &info).unwrap();
        assert_eq!(stats.evaluated, 0);
        assert_eq!(store.session(sid).unwrap().unwrap().project_id, Some(paper));
    }

    #[test]
    fn archiving_pauses_rules_but_keeps_their_assignments() {
        let (mut store, sid) = setup();
        let pid = store.create_project("App", None).unwrap();
        let rule_id = store.add_rule(pid, &path_rule(true)).unwrap();
        evaluate_all(&store, &info).unwrap();

        // A session the rule already assigned stays with the archived project.
        store.set_project_archived(pid, true).unwrap();
        let stats = evaluate_all(&store, &info).unwrap();
        assert_eq!(stats.evaluated, 0);
        let s = store.session(sid).unwrap().unwrap();
        assert_eq!(
            (s.project_id, s.assignment_source),
            (Some(pid), AssignmentSource::Rule)
        );

        // A paused rule neither assigns nor suggests.
        store.set_rule_auto_apply(rule_id, false).unwrap();
        store.unassign_rule(sid).unwrap();
        evaluate_all(&store, &info).unwrap();
        let s = store.session(sid).unwrap().unwrap();
        assert_eq!((s.project_id, s.suggested_project_id), (None, None));

        // Restoring the project brings its rules back.
        store.set_project_archived(pid, false).unwrap();
        evaluate_all(&store, &info).unwrap();
        let s = store.session(sid).unwrap().unwrap();
        assert_eq!(s.suggested_project_id, Some(pid));
    }
}
