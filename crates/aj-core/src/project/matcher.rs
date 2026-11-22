//! Rule matching: given the features of a session and all rules, decide which project it belongs to.
//!
//! Pure logic, so it is easy to test in isolation and can later back a "which sessions would this rule match" preview in the UI.

use aj_store::{RuleRow, SessionRow};
use anyhow::{bail, Context, Result};
use regex::RegexBuilder;

use super::git::normalize_remote;

pub const RULE_PATH_PREFIX: &str = "path_prefix";
pub const RULE_GIT_REMOTE: &str = "git_remote";
pub const RULE_REGEX: &str = "regex";

pub const REGEX_FIELDS: [&str; 4] = ["title", "first_prompt", "cwd", "any"];

/// Session features used for matching.
#[derive(Debug, Clone, Default)]
pub struct MatchContext<'a> {
    /// Normalized working directory.
    pub cwd: Option<&'a str>,
    pub git_root: Option<&'a str>,
    pub git_remote: Option<&'a str>,
    pub title: Option<&'a str>,
    pub first_prompt: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleMatch {
    pub rule_id: i64,
    pub project_id: i64,
    pub auto_apply: bool,
    /// Reason for the match, shown to the user.
    pub reason: String,
}

fn path_matches(path: &str, prefix: &str) -> bool {
    let prefix = prefix.trim_end_matches('/');
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Validate a rule before adding it. Returns the canonical pattern.
pub fn validate_rule(rule_type: &str, target_field: Option<&str>, pattern: &str) -> Result<String> {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        bail!("rule pattern must not be empty");
    }
    match rule_type {
        RULE_PATH_PREFIX => {
            if !pattern.starts_with('/') {
                bail!("a path rule must be an absolute path: {pattern}");
            }
            Ok(pattern.trim_end_matches('/').to_string())
        }
        RULE_GIT_REMOTE => Ok(normalize_remote(pattern)),
        RULE_REGEX => {
            let field = target_field.unwrap_or("any");
            if !REGEX_FIELDS.contains(&field) {
                bail!(
                    "the field of a regex rule must be one of {}, got {field}",
                    REGEX_FIELDS.join(", ")
                );
            }
            RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .with_context(|| format!("invalid regular expression: {pattern}"))?;
            Ok(pattern.to_string())
        }
        other => bail!("unknown rule type: {other}"),
    }
}

/// Find the best matching rule.
///
/// Priority: path rules (longest prefix first) > remote rules > regex rules (oldest first).
/// Paths and remotes are deterministic evidence, while a regex is a guess about content, so it comes last.
pub fn match_rules(rules: &[RuleRow], ctx: &MatchContext) -> Option<RuleMatch> {
    let hit = |rule: &RuleRow, reason: String| RuleMatch {
        rule_id: rule.id,
        project_id: rule.project_id,
        auto_apply: rule.auto_apply,
        reason,
    };

    let path_hit = rules
        .iter()
        .filter(|r| r.rule_type == RULE_PATH_PREFIX)
        .filter(|r| {
            [ctx.cwd, ctx.git_root]
                .into_iter()
                .flatten()
                .any(|p| path_matches(p, &r.pattern))
        })
        .max_by_key(|r| r.pattern.len());
    if let Some(rule) = path_hit {
        return Some(hit(rule, format!("directory {}", rule.pattern)));
    }

    if let Some(remote) = ctx.git_remote.map(normalize_remote) {
        if let Some(rule) = rules
            .iter()
            .find(|r| r.rule_type == RULE_GIT_REMOTE && normalize_remote(&r.pattern) == remote)
        {
            return Some(hit(rule, format!("repository {}", rule.pattern)));
        }
    }

    for rule in rules.iter().filter(|r| r.rule_type == RULE_REGEX) {
        // Rules are validated before storage; a bad regex is skipped here so that one bad rule cannot break the others.
        let Ok(re) = RegexBuilder::new(&rule.pattern)
            .case_insensitive(true)
            .build()
        else {
            continue;
        };
        let field = rule.target_field.as_deref().unwrap_or("any");
        let candidates: [(&str, &str, Option<&str>); 3] = [
            ("title", "title", ctx.title),
            ("first_prompt", "first input", ctx.first_prompt),
            ("cwd", "directory", ctx.cwd),
        ];
        for (name, label, value) in candidates {
            if (field == "any" || field == name) && value.is_some_and(|v| re.is_match(v)) {
                return Some(hit(rule, format!("{label} matches /{}/", rule.pattern)));
            }
        }
    }
    None
}

/// Return `session`'s repository root (or working directory if not in a repo) when no
/// `path_prefix` rule covers it yet, so the UI or CLI can suggest creating a rule.
pub fn uncovered_path(rules: &[RuleRow], session: &SessionRow) -> Option<String> {
    let root = session
        .git_root
        .as_deref()
        .or(session.detected_cwd.as_deref())?;
    let covered = rules
        .iter()
        .any(|r| r.rule_type == RULE_PATH_PREFIX && path_matches(root, &r.pattern));
    (!covered).then(|| root.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(
        id: i64,
        project_id: i64,
        rule_type: &str,
        field: Option<&str>,
        pattern: &str,
    ) -> RuleRow {
        RuleRow {
            id,
            project_id,
            rule_type: rule_type.into(),
            target_field: field.map(str::to_string),
            pattern: pattern.into(),
            auto_apply: false,
        }
    }

    #[test]
    fn longest_path_wins_and_prefix_respects_boundaries() {
        let rules = vec![
            rule(1, 10, RULE_PATH_PREFIX, None, "/work/app"),
            rule(2, 20, RULE_PATH_PREFIX, None, "/work/app/docs"),
        ];
        let at = |cwd| {
            match_rules(
                &rules,
                &MatchContext {
                    cwd: Some(cwd),
                    ..Default::default()
                },
            )
            .map(|m| m.project_id)
        };
        assert_eq!(at("/work/app/docs/a"), Some(20));
        assert_eq!(at("/work/app/src"), Some(10));
        assert_eq!(at("/work/application"), None);
        // The working directory is outside the rule, but the root of its repository matches.
        let ctx = MatchContext {
            cwd: Some("/elsewhere/wt"),
            git_root: Some("/work/app"),
            ..Default::default()
        };
        assert_eq!(match_rules(&rules, &ctx).unwrap().project_id, 10);
    }

    #[test]
    fn deterministic_evidence_beats_regex() {
        let rules = vec![
            rule(1, 30, RULE_REGEX, Some("title"), "thesis|paper"),
            rule(2, 20, RULE_GIT_REMOTE, None, "github.com/owner/repo"),
            rule(3, 10, RULE_PATH_PREFIX, None, "/work/app"),
        ];
        let mut ctx = MatchContext {
            cwd: Some("/work/app"),
            git_remote: Some("git@github.com:owner/repo.git"),
            title: Some("Paper correctness review"),
            ..Default::default()
        };
        assert_eq!(match_rules(&rules, &ctx).unwrap().project_id, 10);
        ctx.cwd = Some("/other");
        assert_eq!(match_rules(&rules, &ctx).unwrap().project_id, 20);
        ctx.git_remote = None;
        let m = match_rules(&rules, &ctx).unwrap();
        assert_eq!(m.project_id, 30);
        assert_eq!(m.reason, "title matches /thesis|paper/");
    }

    #[test]
    fn regex_field_is_respected() {
        let rules = vec![rule(1, 1, RULE_REGEX, Some("first_prompt"), "overleaf")];
        let by_title = MatchContext {
            title: Some("Overleaf sync"),
            ..Default::default()
        };
        assert_eq!(match_rules(&rules, &by_title), None);
        let by_prompt = MatchContext {
            first_prompt: Some("sync the Overleaf version to local"),
            ..Default::default()
        };
        assert!(match_rules(&rules, &by_prompt).is_some());
    }

    #[test]
    fn validation_normalizes_and_rejects() {
        assert_eq!(
            validate_rule(RULE_PATH_PREFIX, None, "/work/app/").unwrap(),
            "/work/app"
        );
        assert!(validate_rule(RULE_PATH_PREFIX, None, "work/app").is_err());
        assert_eq!(
            validate_rule(RULE_GIT_REMOTE, None, "git@github.com:O/R.git").unwrap(),
            "github.com/o/r"
        );
        assert!(validate_rule(RULE_REGEX, Some("title"), "(unclosed").is_err());
        assert!(validate_rule(RULE_REGEX, Some("body"), "x").is_err());
        assert!(validate_rule("keyword", None, "x").is_err());
    }

    #[test]
    fn uncovered_path_detects_missing_and_covered_roots() {
        let mut session = SessionRow {
            id: 1,
            agent: aj_ir::AgentKind::Claude,
            source_session_id: "s1".into(),
            project_id: None,
            assignment_source: "unassigned".into(),
            suggested_project_id: None,
            suggestion_source: None,
            suggestion_reason: None,
            git_root: Some("/work/app/sub".into()),
            git_remote: None,
            detected_cwd: Some("/work/app/sub/deep".into()),
            git_branch: None,
            title: None,
            user_note: None,
            ignored: false,
            first_active_at: None,
            last_active_at: None,
        };
        let rules = vec![rule(1, 10, RULE_PATH_PREFIX, None, "/work/app")];
        assert_eq!(uncovered_path(&rules, &session), None);

        session.git_root = Some("/work/other".into());
        assert_eq!(
            uncovered_path(&rules, &session).as_deref(),
            Some("/work/other")
        );

        session.git_root = None;
        session.detected_cwd = None;
        assert_eq!(uncovered_path(&rules, &session), None);
    }
}
