//! Projects, rules and assignment: project, rule, assign, unassign, accept.

use aj_adapter::SourceAdapter;
use aj_core::project::matcher::{RULE_GIT_REMOTE, RULE_PATH_PREFIX, RULE_REGEX};
use aj_core::project::{evaluate_all, uncovered_path, validate_rule};
use aj_core::scan::normalized_cwd;
use aj_store::{NewRule, SessionRow};
use anyhow::{bail, Result};
use clap::{Args, Subcommand};

use crate::app::{adapters, find_session, find_slice, short_id, title, App};

#[derive(Subcommand)]
pub(crate) enum ProjectCmd {
    /// List projects
    List,
    /// Create a project, optionally binding directories or repositories
    New {
        name: String,
        /// Project description (also used to let the model judge which sessions belong here)
        #[arg(long)]
        desc: Option<String>,
        #[command(flatten)]
        rule: RuleSpec,
    },
    /// Rename a project
    Rename { name: String, new_name: String },
    /// Change a project's description
    Describe { name: String, desc: String },
    /// Archive a project: its data stays, its rules are paused
    Archive { name: String },
    /// Restore an archived project and resume its rules
    Unarchive { name: String },
    /// Delete a project (its sessions become unassigned)
    Delete { name: String },
    /// List candidate projects: unassigned sessions grouped by repository or directory
    Candidates,
}

#[derive(Subcommand)]
pub(crate) enum RuleCmd {
    /// List all rules
    List,
    /// Add rules to a project
    Add {
        project: String,
        #[command(flatten)]
        rule: RuleSpec,
    },
    /// Delete a rule
    Rm { id: i64 },
    /// Choose whether a matching rule assigns directly (on) or only suggests (off)
    Auto {
        id: i64,
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
}

/// Rule content. Any of the three kinds, several at once if needed.
#[derive(Args, Default)]
pub(crate) struct RuleSpec {
    /// Directory prefix (absolute path); repeatable
    #[arg(long = "path")]
    paths: Vec<String>,
    /// Git repository URL; repeatable
    #[arg(long = "remote")]
    remotes: Vec<String>,
    /// Regular expression (case-insensitive); repeatable
    #[arg(long = "regex")]
    regexes: Vec<String>,
    /// Field the regex is matched against
    #[arg(long, default_value = "any", value_parser = ["title", "first_prompt", "cwd", "any"])]
    field: String,
    /// Assign directly on a match, without confirming each session
    #[arg(long)]
    auto: bool,
}

#[derive(Args)]
pub(crate) struct AssignArgs {
    /// Project name; `none` means explicitly no project
    pub project: String,
    /// Session ID prefixes; or S<slice number> to reassign that day only
    #[arg(required = true)]
    pub targets: Vec<String>,
}

pub(crate) fn cmd_project(app: &mut App, cmd: ProjectCmd) -> Result<()> {
    match cmd {
        ProjectCmd::List => {
            let projects = app.store.project_summaries()?;
            if projects.is_empty() {
                println!("No projects yet. See what could become one with `aj project candidates`, then create it with `aj project new <name>`.");
            }
            for p in projects {
                println!(
                    "{}{}  {} slices, {} suggested slices pending, {} rules{}",
                    p.project.name,
                    if p.project.archived {
                        " [archived]"
                    } else {
                        ""
                    },
                    p.slices,
                    p.suggested,
                    p.rules,
                    p.project
                        .description
                        .map(|d| format!("\n    {d}"))
                        .unwrap_or_default()
                );
            }
            Ok(())
        }
        ProjectCmd::New { name, desc, rule } => {
            let id = app.store.create_project(&name, desc.as_deref())?;
            println!("Created project \"{}\"", name.trim());
            let added = add_rules(app, id, &rule)?;
            if added > 0 {
                reevaluate(app)?;
            }
            Ok(())
        }
        ProjectCmd::Rename { name, new_name } => {
            let p = app.store.require_project(&name)?;
            let rewritten = app.store.rename_project(
                p.id,
                &new_name,
                aj_report::structure::rename_project_heading,
            )?;
            println!(
                "Renamed: {} → {} ({rewritten} report versions updated)",
                p.name,
                new_name.trim()
            );
            Ok(())
        }
        ProjectCmd::Describe { name, desc } => {
            let p = app.store.require_project(&name)?;
            app.store.set_project_description(p.id, Some(desc.trim()))?;
            println!("Updated the description of \"{}\"", p.name);
            Ok(())
        }
        ProjectCmd::Archive { name } => set_archived(app, &name, true),
        ProjectCmd::Unarchive { name } => set_archived(app, &name, false),
        ProjectCmd::Delete { name } => {
            let p = app.store.require_project(&name)?;
            let today = crate::app::resolve_date(app, None);
            app.store.delete_project(p.id, &today)?;
            println!("Deleted project \"{}\"", p.name);
            reevaluate(app)
        }
        ProjectCmd::Candidates => {
            let candidates = app.store.project_candidates()?;
            if candidates.is_empty() {
                println!("No candidates: every session is assigned or already has a suggestion.");
            }
            for c in candidates {
                println!("{:>4} sessions  {}", c.sessions, c.root);
                if let Some(remote) = c.git_remote {
                    println!("              repository {remote}");
                }
                for t in c.sample_titles {
                    println!("              · {t}");
                }
            }
            Ok(())
        }
    }
}

fn set_archived(app: &mut App, name: &str, archived: bool) -> Result<()> {
    let p = app.store.require_project(name)?;
    app.store.set_project_archived(p.id, archived)?;
    if archived {
        println!("Archived \"{}\"; its rules are paused", p.name);
    } else {
        println!("Restored \"{}\"", p.name);
    }
    reevaluate(app)
}

pub(crate) fn cmd_rule(app: &mut App, cmd: RuleCmd) -> Result<()> {
    match cmd {
        RuleCmd::List => {
            let rules = app.store.rules()?;
            if rules.is_empty() {
                println!("No rules yet.");
            }
            for r in rules {
                let project = app
                    .store
                    .project(r.project_id)?
                    .map(|p| p.name)
                    .unwrap_or_default();
                let kind = match r.rule_type.as_str() {
                    RULE_PATH_PREFIX => "directory".to_string(),
                    RULE_GIT_REMOTE => "repository".to_string(),
                    _ => format!("regex({})", r.target_field.as_deref().unwrap_or("any")),
                };
                println!(
                    "#{:<3} {} → {}  {} {}",
                    r.id,
                    kind,
                    project,
                    r.pattern,
                    if r.auto_apply {
                        "[auto-assign]"
                    } else {
                        "[suggest only]"
                    }
                );
            }
            Ok(())
        }
        RuleCmd::Add { project, rule } => {
            let p = app.store.require_project(&project)?;
            if add_rules(app, p.id, &rule)? == 0 {
                bail!("give the rule content with --path, --remote or --regex");
            }
            reevaluate(app)
        }
        RuleCmd::Rm { id } => {
            if !app.store.delete_rule(id)? {
                bail!("no rule with ID {id}");
            }
            println!("Deleted rule #{id}");
            reevaluate(app)
        }
        RuleCmd::Auto { id, state } => {
            if !app.store.set_rule_auto_apply(id, state == "on")? {
                bail!("no rule with ID {id}");
            }
            reevaluate(app)
        }
    }
}

fn add_rules(app: &App, project_id: i64, spec: &RuleSpec) -> Result<usize> {
    let groups: [(&str, &[String], Option<&str>); 3] = [
        (RULE_PATH_PREFIX, &spec.paths, None),
        (RULE_GIT_REMOTE, &spec.remotes, None),
        (RULE_REGEX, &spec.regexes, Some(spec.field.as_str())),
    ];
    let mut added = 0;
    for (rule_type, patterns, field) in groups {
        for pattern in patterns {
            let pattern = validate_rule(rule_type, field, pattern)?;
            let id = app.store.add_rule(
                project_id,
                &NewRule {
                    rule_type,
                    target_field: field,
                    pattern: &pattern,
                    auto_apply: spec.auto,
                },
            )?;
            println!("Added rule #{id}: {pattern}");
            added += 1;
        }
    }
    Ok(added)
}

/// After rules or projects change, re-evaluate every session that was not assigned manually.
fn reevaluate(app: &App) -> Result<()> {
    let adapters = adapters(app)?;
    let refs: Vec<&dyn SourceAdapter> = adapters.iter().map(|a| a.as_ref()).collect();
    let stats = evaluate_all(&app.store, &|s| normalized_cwd(&refs, s))?;
    println!(
        "Re-evaluated by rule: {} sessions assigned, {} suggestions pending",
        stats.assigned, stats.suggested
    );
    Ok(())
}

fn is_slice_ref(target: &str) -> bool {
    target.len() > 1
        && target.starts_with(['S', 's'])
        && target[1..].chars().all(|c| c.is_ascii_digit())
}

pub(crate) fn cmd_assign(app: &App, args: AssignArgs) -> Result<()> {
    let project = if args.project.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(app.store.require_project(&args.project)?)
    };
    let label = project
        .as_ref()
        .map(|p| p.name.as_str())
        .unwrap_or("no project");
    let project_id = project.as_ref().map(|p| p.id);

    for target in &args.targets {
        if is_slice_ref(target) {
            let slice = find_slice(app, target)?;
            app.store.set_slice_project_override(slice.id, project_id)?;
            println!(
                "S{} ({}) → {label}, for this day only",
                slice.id, slice.local_date
            );
            continue;
        }
        let session = find_session(app, target)?;
        app.store.assign_manual(session.id, project_id)?;
        println!("{} {} → {label}", short_id(&session), title(&session));
        hint_rule(app, &session, project.as_ref().map(|p| p.name.as_str()))?;
    }
    Ok(())
}

/// When the session's directory is not covered by any rule yet, hint that it can be turned into one.
fn hint_rule(app: &App, session: &SessionRow, project: Option<&str>) -> Result<()> {
    let Some(project) = project else {
        return Ok(());
    };
    if let Some(root) = uncovered_path(&app.store.rules()?, session) {
        println!("    Hint: to suggest this project for future sessions under {root}, run\n    aj rule add \"{project}\" --path \"{root}\"");
    }
    Ok(())
}

pub(crate) fn cmd_unassign(app: &App, targets: &[String]) -> Result<()> {
    for target in targets {
        if is_slice_ref(target) {
            let slice = find_slice(app, target)?;
            app.store.set_slice_project_override(slice.id, None)?;
            println!("S{} follows its session's project again", slice.id);
        } else {
            let session = find_session(app, target)?;
            app.store.clear_manual(session.id)?;
            println!(
                "{} {}: manual assignment removed",
                short_id(&session),
                title(&session)
            );
        }
    }
    reevaluate(app)
}

/// Accept suggestions. Without targets, accept those of the given date (or all of them).
pub(crate) fn cmd_accept(
    app: &App,
    targets: &[String],
    date: Option<&str>,
    project: Option<&str>,
    all: bool,
) -> Result<()> {
    let only_project = project
        .map(|p| app.store.require_project(p))
        .transpose()?
        .map(|p| p.id);
    let sessions: Vec<SessionRow> = if !targets.is_empty() {
        targets
            .iter()
            .map(|t| find_session(app, t))
            .collect::<Result<_>>()?
    } else if all {
        app.store.all_sessions()?
    } else if let Some(date) = date {
        let mut out = Vec::new();
        for slice in app.store.slices_for_date(date)? {
            if let Some(s) = app.store.session(slice.session_id)? {
                out.push(s);
            }
        }
        out
    } else {
        bail!("name some sessions, or give a range with --date / --all");
    };

    let mut accepted = 0;
    for s in sessions {
        let Some(suggested) = s.suggested_project_id else {
            continue;
        };
        if only_project.is_some_and(|p| p != suggested) || s.project_id.is_some() {
            continue;
        }
        if app.store.accept_suggestion(s.id)? {
            let name = app
                .store
                .project(suggested)?
                .map(|p| p.name)
                .unwrap_or_default();
            println!("{} {} → {name}", short_id(&s), title(&s));
            accepted += 1;
        }
    }
    println!("Accepted {accepted} suggestions");
    Ok(())
}
