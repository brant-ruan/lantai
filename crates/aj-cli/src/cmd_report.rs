//! Reports and models: report daily/weekly/monthly/history/show, models.

use std::path::PathBuf;

use aj_report::local_tools;
use aj_report::monthly::{generate_monthly, MonthlyOptions};
use aj_report::period::{Month, Week};
use aj_report::prompt_library::{KIND_DAILY, KIND_MONTHLY, KIND_WEEKLY};
use aj_report::weekly::{generate_weekly, WeeklyOptions};
use aj_report::{generate_daily, DailyOptions, ReportMode};
use aj_store::ReportVersionRow;
use anyhow::{bail, Context, Result};
use chrono::Local;
use clap::Subcommand;

use crate::app::{clip, resolve_date, App};

#[derive(Subcommand)]
pub(crate) enum ReportKind {
    /// Generate a daily report
    Daily {
        /// Date as YYYY-MM-DD; defaults to today
        #[arg(long)]
        date: Option<String>,
        /// Only include this project
        #[arg(long)]
        project: Option<String>,
        /// Prompt to use (see `aj prompt list`); defaults to the default daily prompt
        #[arg(long)]
        prompt: Option<String>,
        /// Language to write the report in (en, zh-Hans); defaults to `[report] language`
        #[arg(long)]
        lang: Option<String>,
        /// Print the material that would be sent to the model without calling it
        #[arg(long)]
        dry_run: bool,
        /// Only rewrite the projects whose material changed since the current version,
        /// keeping the other sections (including your edits) as they are
        #[arg(long)]
        update: bool,
        /// Model alias to use (see `aj models`); defaults to default_model
        #[arg(long, env = "AJ_MODEL")]
        model: Option<String>,
        /// Number of concurrent model calls in the digest stage
        #[arg(long, default_value_t = 3)]
        concurrency: usize,
        /// Write the report to a file
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Generate a weekly report: one request per project clusters its week into threads
    Weekly {
        /// Week as 2026-W47; defaults to the week of --date
        #[arg(long)]
        week: Option<String>,
        /// Any date in the week (YYYY-MM-DD); defaults to today
        #[arg(long, conflicts_with = "week")]
        date: Option<String>,
        /// Only include this project
        #[arg(long)]
        project: Option<String>,
        /// Prompt to use (see `aj prompt list`); defaults to the default weekly prompt
        #[arg(long)]
        prompt: Option<String>,
        /// Language to write the report in (en, zh-Hans); defaults to `[report] language`
        #[arg(long)]
        lang: Option<String>,
        /// Print the requests that would be sent without calling the model
        #[arg(long)]
        dry_run: bool,
        /// Only rewrite the projects whose material changed since the current version
        #[arg(long)]
        update: bool,
        /// Model alias to use (see `aj models`); defaults to default_model
        #[arg(long, env = "AJ_MODEL")]
        model: Option<String>,
        /// Number of concurrent model calls (digests, then project sections)
        #[arg(long, default_value_t = 3)]
        concurrency: usize,
        /// Write the report to a file
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Generate a monthly report: one request per project merges its weekly reports
    Monthly {
        /// Month as 2026-11; defaults to the month of --date
        #[arg(long)]
        month: Option<String>,
        /// Any date in the month (YYYY-MM-DD); defaults to today
        #[arg(long, conflicts_with = "month")]
        date: Option<String>,
        /// Only include this project
        #[arg(long)]
        project: Option<String>,
        /// Prompt to use (see `aj prompt list --kind monthly`); defaults to the default monthly prompt
        #[arg(long)]
        prompt: Option<String>,
        /// Language to write the report in (en, zh-Hans); defaults to `[report] language`
        #[arg(long)]
        lang: Option<String>,
        /// Print the requests that would be sent without calling the model
        #[arg(long)]
        dry_run: bool,
        /// Only rewrite the projects whose weekly report sections changed since the current version
        #[arg(long)]
        update: bool,
        /// Model alias to use (see `aj models`); defaults to default_model
        #[arg(long, env = "AJ_MODEL")]
        model: Option<String>,
        /// Number of concurrent model calls for project sections
        #[arg(long, default_value_t = 3)]
        concurrency: usize,
        /// Write the report to a file
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// List the versions of a report
    History {
        /// Date as YYYY-MM-DD; defaults to today
        #[arg(long)]
        date: Option<String>,
        /// A weekly report instead (2026-W47)
        #[arg(long, conflicts_with = "date")]
        week: Option<String>,
        /// A monthly report instead (2026-11)
        #[arg(long, conflicts_with_all = ["date", "week"])]
        month: Option<String>,
        /// Versions of the report for this project only
        #[arg(long)]
        project: Option<String>,
    },
    /// Print a saved report version
    Show {
        /// Date as YYYY-MM-DD; defaults to today
        #[arg(long)]
        date: Option<String>,
        /// A weekly report instead (2026-W47)
        #[arg(long, conflicts_with = "date")]
        week: Option<String>,
        /// A monthly report instead (2026-11)
        #[arg(long, conflicts_with_all = ["date", "week"])]
        month: Option<String>,
        #[arg(long)]
        project: Option<String>,
        /// Version number from `aj report history`; defaults to the current version
        #[arg(long)]
        version: Option<u64>,
        /// Print the prompt the version was generated with instead of the report
        #[arg(long)]
        prompt: bool,
    },
}

pub(crate) fn cmd_report(app: &mut App, kind: ReportKind) -> Result<()> {
    match kind {
        ReportKind::Daily {
            date,
            project,
            prompt,
            lang,
            dry_run,
            update,
            model,
            concurrency,
            out,
        } => {
            let date = resolve_date(app, date);
            cmd_report_daily(
                app,
                DailyArgs {
                    date,
                    project,
                    prompt,
                    lang,
                    concurrency,
                    dry_run,
                    update,
                },
                model,
                out,
            )
        }
        ReportKind::Weekly {
            week,
            date,
            project,
            prompt,
            lang,
            dry_run,
            update,
            model,
            concurrency,
            out,
        } => {
            let week = resolve_week(app, week, date)?;
            cmd_report_weekly(
                app,
                WeeklyArgs {
                    week,
                    project,
                    prompt,
                    lang,
                    concurrency,
                    dry_run,
                    update,
                },
                model,
                out,
            )
        }
        ReportKind::Monthly {
            month,
            date,
            project,
            prompt,
            lang,
            dry_run,
            update,
            model,
            concurrency,
            out,
        } => {
            let month = resolve_month(app, month, date)?;
            cmd_report_monthly(
                app,
                MonthlyArgs {
                    month,
                    project,
                    prompt,
                    lang,
                    concurrency,
                    dry_run,
                    update,
                },
                model,
                out,
            )
        }
        ReportKind::History {
            date,
            week,
            month,
            project,
        } => {
            let (kind, period) = resolve_period(app, date, week, month)?;
            let versions = report_versions(app, kind, &period, project.as_deref())?;
            if versions.is_empty() {
                println!("No {kind} report for {period} yet.");
            }
            for v in versions {
                println!(
                    "{} v{:<3} {}  {:<10} {:<8} prompt: {:<12} engine: {}  {} chars",
                    if v.is_current { "*" } else { " " },
                    v.number,
                    v.created_at,
                    v.origin,
                    v.language,
                    v.prompt_name.as_deref().unwrap_or("-"),
                    v.engine.as_deref().unwrap_or("-"),
                    v.markdown.chars().count()
                );
            }
            Ok(())
        }
        ReportKind::Show {
            date,
            week,
            month,
            project,
            version,
            prompt,
        } => {
            let (kind, period) = resolve_period(app, date, week, month)?;
            let versions = report_versions(app, kind, &period, project.as_deref())?;
            let chosen = match version {
                Some(n) => versions.into_iter().find(|v| v.number == n),
                None => versions.into_iter().find(|v| v.is_current),
            };
            let Some(v) = chosen else {
                bail!("no such report version for {period}; see `aj report history`");
            };
            if prompt {
                println!(
                    "{}",
                    v.prompt_snapshot
                        .as_deref()
                        .unwrap_or("(no prompt recorded)")
                );
            } else {
                println!("{}", v.markdown);
            }
            Ok(())
        }
    }
}

fn report_versions(
    app: &App,
    kind: &str,
    period: &str,
    project: Option<&str>,
) -> Result<Vec<ReportVersionRow>> {
    let project_id = project
        .map(|p| app.store.require_project(p))
        .transpose()?
        .map(|p| p.id);
    app.store.report_versions(kind, period, project_id)
}

/// The week given, or the week of the date given (today by default).
fn resolve_week(app: &App, week: Option<String>, date: Option<String>) -> Result<Week> {
    match week {
        Some(w) => Week::parse(&w),
        None => {
            let date = resolve_date(app, date);
            Ok(Week::containing(date.parse().with_context(|| {
                format!("a date looks like 2026-11-17, got {date}")
            })?))
        }
    }
}

/// The month given, or the month of the date given (today by default).
fn resolve_month(app: &App, month: Option<String>, date: Option<String>) -> Result<Month> {
    match month {
        Some(m) => Month::parse(&m),
        None => {
            let date = resolve_date(app, date);
            let d: chrono::NaiveDate = date
                .parse()
                .with_context(|| format!("a date looks like 2026-11-17, got {date}"))?;
            Ok(Month::containing(d))
        }
    }
}

/// Which report `--date` / `--week` / `--month` select.
fn resolve_period(
    app: &App,
    date: Option<String>,
    week: Option<String>,
    month: Option<String>,
) -> Result<(&'static str, String)> {
    if let Some(m) = month {
        return Ok((KIND_MONTHLY, Month::parse(&m)?.key()));
    }
    if let Some(w) = week {
        return Ok((KIND_WEEKLY, Week::parse(&w)?.key()));
    }
    Ok((KIND_DAILY, resolve_date(app, date)))
}

struct MonthlyArgs {
    month: Month,
    project: Option<String>,
    prompt: Option<String>,
    lang: Option<String>,
    concurrency: usize,
    dry_run: bool,
    update: bool,
}

fn cmd_report_monthly(
    app: &mut App,
    args: MonthlyArgs,
    model: Option<String>,
    out: Option<PathBuf>,
) -> Result<()> {
    let config = app.load_config()?;
    let engine = report_engine(&config, model.as_deref(), args.dry_run, app)?;
    let opts = MonthlyOptions {
        language: app.report_language(args.lang.as_deref())?,
        month: args.month,
        project: args.project,
        prompt: args.prompt,
        concurrency: args.concurrency,
        dry_run: args.dry_run,
        budgets: config.budgets(),
        cancel: None,
        mode: if args.update {
            ReportMode::Update
        } else {
            ReportMode::Regenerate
        },
        now: chrono::Utc::now(),
    };
    if !opts.dry_run {
        eprintln!("Engine: {}", engine.name());
    }
    let started = std::time::Instant::now();
    let r = generate_monthly(&mut app.store, engine.as_ref(), &Local, &opts, &mut |msg| {
        eprintln!("{msg}")
    })?;
    println!("{}", r.markdown);
    if opts.dry_run {
        eprintln!(
            "\n[preview] {} weekly report(s) ({} slices, {} ignored); {} chars of requests. No model was called.",
            r.weeks_used, r.slices_used, r.slices_ignored, r.stage2_prompt_chars
        );
        return Ok(());
    }
    eprintln!(
        "\n{} weekly report(s) ({} slices, {} ignored); {} chars of requests; {} anchors, {} invalid ones removed; {:.0}s",
        r.weeks_used,
        r.slices_used,
        r.slices_ignored,
        r.stage2_prompt_chars,
        r.anchors.total,
        r.anchors.invalid,
        started.elapsed().as_secs_f64()
    );
    if r.updated {
        eprintln!(
            "Updated the previous version: rewrote {} ({}), kept {} sections unchanged",
            r.projects_written.len(),
            r.projects_written.join(", "),
            r.sections_kept
        );
    }
    eprintln!("Prompt: {}", r.prompt_name);
    for w in &r.warnings {
        eprintln!("  warning: {w}");
    }
    if let Some(path) = out {
        std::fs::write(&path, &r.markdown)
            .with_context(|| format!("failed to write {}", path.display()))?;
        eprintln!("Written to {}", path.display());
    }
    Ok(())
}

struct WeeklyArgs {
    week: Week,
    project: Option<String>,
    prompt: Option<String>,
    lang: Option<String>,
    concurrency: usize,
    dry_run: bool,
    update: bool,
}

fn cmd_report_weekly(
    app: &mut App,
    args: WeeklyArgs,
    model: Option<String>,
    out: Option<PathBuf>,
) -> Result<()> {
    let config = app.load_config()?;
    let engine = report_engine(&config, model.as_deref(), args.dry_run, app)?;
    let opts = WeeklyOptions {
        language: app.report_language(args.lang.as_deref())?,
        week: args.week,
        project: args.project,
        prompt: args.prompt,
        concurrency: args.concurrency,
        dry_run: args.dry_run,
        budgets: config.budgets(),
        cancel: None,
        mode: if args.update {
            ReportMode::Update
        } else {
            ReportMode::Regenerate
        },
        now: chrono::Utc::now(),
    };
    if !opts.dry_run {
        eprintln!("Engine: {}", engine.name());
    }
    let started = std::time::Instant::now();
    let r = generate_weekly(&mut app.store, engine.as_ref(), &Local, &opts, &mut |msg| {
        eprintln!("{msg}")
    })?;
    println!("{}", r.markdown);
    if opts.dry_run {
        eprintln!(
            "\n[preview] {} slices ({} ignored), {} with a cached digest; {} chars of requests. No model was called.",
            r.slices_used, r.slices_ignored, r.stage1.digests_cached, r.stage2_prompt_chars
        );
        return Ok(());
    }
    eprintln!(
        "\n{} slices ({} ignored): {} digests generated, {} cached; {} project-day(s) clustered ({} cached); {} chars of requests; {} anchors, {} invalid ones removed; {:.0}s",
        r.slices_used,
        r.slices_ignored,
        r.stage1.digests_generated,
        r.stage1.digests_cached,
        r.days_condensed,
        r.briefs_cached,
        r.stage2_prompt_chars,
        r.anchors.total,
        r.anchors.invalid,
        started.elapsed().as_secs_f64()
    );
    if r.updated {
        eprintln!(
            "Updated the previous version: rewrote {} ({}), kept {} sections unchanged",
            r.projects_written.len(),
            r.projects_written.join(", "),
            r.sections_kept
        );
    }
    eprintln!("Prompt: {}", r.prompt_name);
    for w in &r.warnings {
        eprintln!("  warning: {w}");
    }
    if let Some(path) = out {
        std::fs::write(&path, &r.markdown)
            .with_context(|| format!("failed to write {}", path.display()))?;
        eprintln!("Written to {}", path.display());
    }
    Ok(())
}

/// What the user asked for on the command line; budgets and the engine come from the config file.
struct DailyArgs {
    date: String,
    project: Option<String>,
    prompt: Option<String>,
    lang: Option<String>,
    concurrency: usize,
    dry_run: bool,
    update: bool,
}

fn cmd_report_daily(
    app: &mut App,
    args: DailyArgs,
    model: Option<String>,
    out: Option<PathBuf>,
) -> Result<()> {
    let config = app.load_config()?;
    let engine = report_engine(&config, model.as_deref(), args.dry_run, app)?;
    let opts = DailyOptions {
        language: app.report_language(args.lang.as_deref())?,
        date: args.date,
        project: args.project,
        prompt: args.prompt,
        concurrency: args.concurrency,
        dry_run: args.dry_run,
        budgets: config.budgets(),
        cancel: None,
        mode: if args.update {
            ReportMode::Update
        } else {
            ReportMode::Regenerate
        },
        now: chrono::Utc::now(),
    };
    if !opts.dry_run {
        eprintln!("Engine: {}", engine.name());
    }
    let r = generate_daily(&mut app.store, engine.as_ref(), &Local, &opts, &mut |msg| {
        eprintln!("{msg}")
    })?;

    println!("{}", r.markdown);
    if opts.dry_run {
        eprintln!(
            "\n[preview] {} slices ({} ignored), {} with a cached digest; {} chars of material. No model was called.",
            r.slices_used, r.slices_ignored, r.stage1.digests_cached, r.stage2_prompt_chars
        );
        return Ok(());
    }
    eprintln!(
        "\n{} slices ({} ignored): {} digests generated, {} updated incrementally, {} cached, {} short slices passed through; {} anchors, {} invalid ones removed",
        r.slices_used,
        r.slices_ignored,
        r.stage1.digests_generated,
        r.stage1.digests_incremental,
        r.stage1.digests_cached,
        r.stage1.digests_passthrough,
        r.anchors.total,
        r.anchors.invalid
    );
    if r.updated {
        eprintln!(
            "Updated the previous version: rewrote {} ({}), kept {} sections unchanged",
            r.projects_written.len(),
            r.projects_written.join(", "),
            r.sections_kept
        );
    }
    eprintln!("Prompt: {}", r.prompt_name);
    if r.stage1.slices_split > 0 {
        eprintln!(
            "{} long slices were digested in parts: {} parts generated, {} cached",
            r.stage1.slices_split, r.stage1.parts_generated, r.stage1.parts_cached
        );
    }
    if r.projects_condensed > 0 {
        eprintln!(
            "{} projects were clustered into tasks before writing ({} cached)",
            r.projects_condensed, r.briefs_cached
        );
    }
    if r.stage1.suggestions_made > 0 {
        eprintln!(
            "Suggested projects for {} unassigned sessions (see `aj sessions` or `aj accept`)",
            r.stage1.suggestions_made
        );
    }
    for w in &r.warnings {
        eprintln!("  warning: {w}");
    }
    if let Some(path) = out {
        std::fs::write(&path, &r.markdown)
            .with_context(|| format!("failed to write {}", path.display()))?;
        eprintln!("Written to {}", path.display());
    }
    Ok(())
}

/// The engine a report command runs with. A dry run never calls a model, so it works without one.
fn report_engine(
    config: &aj_report::AppConfig,
    model: Option<&str>,
    dry_run: bool,
    app: &App,
) -> Result<Box<dyn aj_report::Engine>> {
    let runner = app.data_dir.join("runner");
    if dry_run {
        config.estimate_engine(model, &runner)
    } else {
        config.build_engine(model, &runner)
    }
}

pub(crate) fn cmd_models(app: &App, test: bool, only: Option<&str>) -> Result<()> {
    let path = app.data_dir.join("config.toml");
    let config = app.load_config()?;
    println!(
        "Config file: {}{}",
        path.display(),
        if path.exists() { "" } else { " (missing)" }
    );
    println!("Key store: {}", config.keystore().describe());
    if config.models.is_empty() {
        println!("No model is configured. Run `aj models --find` to look for local agent CLIs,");
        println!("or add a provider in the app's Settings → Models.");
    }
    for name in config.models.keys() {
        if only.is_some_and(|o| o != name) {
            continue;
        }
        let mark = if Some(name.as_str()) == config.default_model_name() {
            "*"
        } else {
            " "
        };
        println!("{mark} {name}: {}", config.describe_model(name));
        if test {
            let engine = config.build_engine(Some(name), &app.data_dir.join("runner"))?;
            let started = std::time::Instant::now();
            match engine.complete("Reply with exactly one word: OK") {
                Ok(reply) => println!(
                    "    ✓ {:.1}s, reply: {}",
                    started.elapsed().as_secs_f64(),
                    clip(&reply, 40)
                ),
                Err(e) => println!("    ✗ {e:#}"),
            }
        }
    }
    Ok(())
}

/// `aj models --find`: known agent CLIs on PATH, and whether each can generate reports.
pub(crate) fn cmd_find_tools() -> Result<()> {
    let found = local_tools::find_on_path();
    if found.is_empty() {
        println!("No known agent CLI found on PATH.");
    }
    for f in &found {
        let usable = match f.tool.engine {
            Some(_) => format!(
                "can generate; add with `aj models --add {}`",
                f.tool.command
            ),
            None => format!(
                "cannot generate yet ({})",
                f.tool
                    .gaps
                    .iter()
                    .map(|g| match g {
                        local_tools::Gap::Tools => "tools that can act stay on",
                        local_tools::Gap::Sessions => "every run leaves a session",
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        };
        println!(
            "{} ({})  {}  {}",
            f.tool.agent.product_name(),
            f.tool.command,
            f.path.display(),
            f.version.as_deref().unwrap_or("version unknown")
        );
        println!("    {usable}");
    }
    Ok(())
}

/// `aj models --add <tool>`: add a usable agent CLI found on PATH to `config.toml`.
pub(crate) fn cmd_add_tool(app: &App, key: &str) -> Result<()> {
    let Some(tool) = local_tools::tool(key) else {
        bail!(
            "unknown tool {key}; known: {}",
            local_tools::TOOLS
                .iter()
                .map(|t| t.command)
                .collect::<Vec<_>>()
                .join(", ")
        );
    };
    let Some(found) = local_tools::find_on_path()
        .into_iter()
        .find(|f| f.tool.command == tool.command)
    else {
        bail!("`{}` was not found on PATH", tool.command);
    };
    let mut config = app.load_config()?;
    let alias = config.add_local_tool(found.tool, &found.path)?;
    config.save(&app.data_dir.join("config.toml"))?;
    println!(
        "Model {alias:?} uses {}{}",
        found.path.display(),
        if config.default_model_name() == Some(alias.as_str()) {
            " (default)"
        } else {
            ""
        }
    );
    println!("Check it with `aj models --test --model {alias:?}`.");
    Ok(())
}
