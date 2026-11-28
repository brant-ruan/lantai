//! Reports: report daily/weekly/monthly/history/show.

use std::path::PathBuf;

use aj_report::monthly::generate_monthly;
use aj_report::period::{Month, Week};
use aj_report::prompt_library::{KIND_DAILY, KIND_MONTHLY, KIND_WEEKLY};
use aj_report::versions::{display_text, shown_versions};
use aj_report::weekly::generate_weekly;
use aj_report::{generate_daily, ReportMode, ReportOptions, DEFAULT_CONCURRENCY};
use anyhow::{bail, Context, Result};
use chrono::Local;
use clap::Subcommand;

use crate::app::{resolve_date, App};

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
        #[arg(long, default_value_t = DEFAULT_CONCURRENCY)]
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
        #[arg(long, default_value_t = DEFAULT_CONCURRENCY)]
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
        #[arg(long, default_value_t = DEFAULT_CONCURRENCY)]
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
        #[arg(long, conflicts_with = "raw")]
        prompt: bool,
        /// Print the text as stored instead of as the app shows it (the generated title,
        /// project sections in the order they were written)
        #[arg(long)]
        raw: bool,
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
                RunArgs {
                    period: date,
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
                RunArgs {
                    period: week,
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
                RunArgs {
                    period: month,
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
            let project_id = project_id(app, project.as_deref())?;
            let versions = app.store.report_versions(kind, &period, project_id)?;
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
            raw,
        } => {
            let (kind, period) = resolve_period(app, date, week, month)?;
            let project_id = project_id(app, project.as_deref())?;
            let versions = shown_versions(&app.store, kind, &period, project_id)?;
            let chosen = match version {
                Some(n) => versions.into_iter().find(|v| v.version.number == n),
                None => versions.into_iter().find(|v| v.version.is_current),
            };
            let Some(v) = chosen else {
                bail!("no such report version for {period}; see `aj report history`");
            };
            if prompt {
                println!(
                    "{}",
                    v.version
                        .prompt_snapshot
                        .as_deref()
                        .unwrap_or("(no prompt recorded)")
                );
            } else if raw {
                println!("{}", v.version.markdown);
            } else {
                println!("{}", v.markdown);
            }
            Ok(())
        }
    }
}

/// The ID of the project named by `--project`, if one was given.
fn project_id(app: &App, project: Option<&str>) -> Result<Option<i64>> {
    Ok(project
        .map(|p| app.store.require_project(p))
        .transpose()?
        .map(|p| p.id))
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

fn cmd_report_monthly(
    app: &mut App,
    args: RunArgs<Month>,
    model: Option<String>,
    out: Option<PathBuf>,
) -> Result<()> {
    let (engine, opts) = prepare(app, args, model.as_deref())?;
    let started = std::time::Instant::now();
    let r = generate_monthly(&mut app.store, engine.as_ref(), &Local, &opts, &mut |msg| {
        eprintln!("{msg}")
    })?;
    if opts.dry_run {
        println!("{}", r.markdown);
        eprintln!(
            "\n[preview] {} weekly report(s) ({} slices, {} ignored); {} chars of requests. No model was called.",
            r.weeks_used, r.slices_used, r.slices_ignored, r.stage2_prompt_chars
        );
        return Ok(());
    }
    let shown = display_text(
        &app.store,
        KIND_MONTHLY,
        &opts.period.key(),
        &r.markdown,
        opts.language.tag,
    )?;
    println!("{shown}");
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
    write_out(out, &shown)
}

fn cmd_report_weekly(
    app: &mut App,
    args: RunArgs<Week>,
    model: Option<String>,
    out: Option<PathBuf>,
) -> Result<()> {
    let (engine, opts) = prepare(app, args, model.as_deref())?;
    let started = std::time::Instant::now();
    let r = generate_weekly(&mut app.store, engine.as_ref(), &Local, &opts, &mut |msg| {
        eprintln!("{msg}")
    })?;
    if opts.dry_run {
        println!("{}", r.markdown);
        eprintln!(
            "\n[preview] {} slices ({} ignored), {} with a cached digest; {} chars of requests. No model was called.",
            r.slices_used, r.slices_ignored, r.stage1.digests_cached, r.stage2_prompt_chars
        );
        return Ok(());
    }
    let shown = display_text(
        &app.store,
        KIND_WEEKLY,
        &opts.period.key(),
        &r.markdown,
        opts.language.tag,
    )?;
    println!("{shown}");
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
    write_out(out, &shown)
}

fn cmd_report_daily(
    app: &mut App,
    args: RunArgs<String>,
    model: Option<String>,
    out: Option<PathBuf>,
) -> Result<()> {
    let (engine, opts) = prepare(app, args, model.as_deref())?;
    let r = generate_daily(&mut app.store, engine.as_ref(), &Local, &opts, &mut |msg| {
        eprintln!("{msg}")
    })?;

    if opts.dry_run {
        println!("{}", r.markdown);
        eprintln!(
            "\n[preview] {} slices ({} ignored), {} with a cached digest; {} chars of material. No model was called.",
            r.slices_used, r.slices_ignored, r.stage1.digests_cached, r.stage2_prompt_chars
        );
        return Ok(());
    }
    let shown = display_text(
        &app.store,
        KIND_DAILY,
        &opts.period,
        &r.markdown,
        opts.language.tag,
    )?;
    println!("{shown}");
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
    write_out(out, &shown)
}

/// What the user asked for on the command line; budgets and the engine come from the config file.
struct RunArgs<P> {
    period: P,
    project: Option<String>,
    prompt: Option<String>,
    lang: Option<String>,
    concurrency: usize,
    dry_run: bool,
    update: bool,
}

/// The engine and options a report command runs with; names the engine unless it is a dry run.
fn prepare<P>(
    app: &App,
    args: RunArgs<P>,
    model: Option<&str>,
) -> Result<(Box<dyn aj_report::Engine>, ReportOptions<P>)> {
    let config = app.load_config()?;
    let engine = report_engine(&config, model, args.dry_run, app)?;
    let opts = ReportOptions {
        language: app.report_language(args.lang.as_deref())?,
        period: args.period,
        project: args.project,
        prompt: args.prompt,
        concurrency: args.concurrency,
        dry_run: args.dry_run,
        budgets: config.budgets(),
        cancel: None,
        mode: ReportMode::from_update(args.update),
        now: chrono::Utc::now(),
    };
    if !opts.dry_run {
        eprintln!("Engine: {}", engine.name());
    }
    Ok((engine, opts))
}

/// Writes the report to `--out` when given.
fn write_out(out: Option<PathBuf>, markdown: &str) -> Result<()> {
    if let Some(path) = out {
        std::fs::write(&path, markdown)
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
