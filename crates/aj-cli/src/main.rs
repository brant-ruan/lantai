//! `aj`: the command-line entry point of Lantai.

use std::path::PathBuf;

use aj_report::{AppConfig, KeyStore};
use aj_store::Store;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod app;
mod cmd_browse;
mod cmd_project;
mod cmd_prompt;
mod cmd_report;
mod cmd_search;

use app::{find_session, resolve_date, title, App};
use cmd_browse::*;
use cmd_project::{AssignArgs, ProjectCmd, RuleCmd};
use cmd_prompt::PromptCmd;
use cmd_report::{cmd_add_tool, cmd_find_tools, cmd_models, ReportKind};
use cmd_search::SearchArgs;

#[derive(Parser)]
#[command(
    name = "aj",
    version,
    about = "Turn local coding-agent sessions into a work journal"
)]
struct Cli {
    /// Data directory (defaults to lantai under the system application data directory; AJ_DATA_DIR also sets it)
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scan sessions incrementally and ingest them
    Scan {
        /// Backfill range on first run: only session files modified within this many days
        #[arg(long, default_value_t = 90)]
        since: i64,
        /// Do not limit the backfill range
        #[arg(long)]
        all: bool,
    },
    /// Recompute the daily slices of every stored session (after the day boundary changed)
    Reslice,
    /// List recent days that have activity
    Days {
        #[arg(long, default_value_t = 14)]
        limit: u32,
    },
    /// List the session slices of a day, grouped by project
    Sessions {
        /// Date as YYYY-MM-DD; defaults to today
        #[arg(long)]
        date: Option<String>,
    },
    /// Replay a session or slice (a session ID prefix, or S<slice number>)
    Show {
        target: String,
        /// Show every agent reply instead of only the conclusion of each turn
        #[arg(long)]
        full: bool,
    },
    /// Search reports and session text (every term must occur)
    Search(SearchArgs),
    /// Show the dehydrated text sent to the model and the cached digest
    Digest {
        /// Slice reference, e.g. S12
        slice: String,
    },
    /// Attach a one-line note to a session (highest priority when generating reports); omit the text to clear it
    Note {
        session: String,
        text: Option<String>,
    },
    /// Exclude a session from reports
    Ignore { session: String },
    /// Include a previously ignored session again
    Include { session: String },
    /// Manage projects
    Project {
        #[command(subcommand)]
        cmd: ProjectCmd,
    },
    /// Manage assignment rules
    Rule {
        #[command(subcommand)]
        cmd: RuleCmd,
    },
    /// Assign sessions to a project (manual; rules never change it afterwards)
    Assign(AssignArgs),
    /// Remove a manual assignment and let the rules decide again
    Unassign {
        #[arg(required = true)]
        targets: Vec<String>,
    },
    /// Accept assignment suggestions
    Accept {
        /// Session ID prefixes; without any, use --date or --all
        targets: Vec<String>,
        /// Accept the suggestions of every session on this day
        #[arg(long)]
        date: Option<String>,
        /// Only accept suggestions pointing to this project
        #[arg(long)]
        project: Option<String>,
        /// Accept every suggestion
        #[arg(long)]
        all: bool,
    },
    /// Manage report prompts
    Prompt {
        #[command(subcommand)]
        cmd: PromptCmd,
    },
    /// Generate and inspect reports
    Report {
        #[command(subcommand)]
        kind: ReportKind,
    },
    /// Export sessions in the IR exchange format: a header line per session, then its events (parsed straight from the source files)
    Export {
        /// Only export sessions whose ID starts with this
        #[arg(long)]
        session: Option<String>,
    },
    /// List the configured model aliases and their providers; find and add local agent CLIs
    Models {
        /// Send a tiny request to each model (or the one given by --model) to check connectivity
        #[arg(long)]
        test: bool,
        /// Only list/test this model alias
        #[arg(long)]
        model: Option<String>,
        /// Look for known agent CLIs on PATH and say which can generate reports (runs only `--version`)
        #[arg(long, conflicts_with_all = ["test", "model"])]
        find: bool,
        /// Add a local agent CLI found on PATH as a provider and model (e.g. `claude`)
        #[arg(long, value_name = "TOOL", conflicts_with_all = ["test", "model", "find"])]
        add: Option<String>,
    },
    /// Show the parsing health of each source
    Doctor,
}

fn main() -> Result<()> {
    // Exit quietly instead of panicking when a pipe such as `head` closes our output early.
    #[cfg(unix)]
    // SAFETY: restores the default SIGPIPE disposition before any thread is created.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    let data_dir = match cli
        .data_dir
        .or_else(|| std::env::var_os("AJ_DATA_DIR").map(PathBuf::from))
    {
        Some(d) => d,
        None => dirs::data_dir()
            .context("cannot determine the application data directory")?
            .join(aj_core::APP_DATA_DIR_NAME),
    };
    let mut store = Store::open(&data_dir.join("journal.sqlite"))?;
    aj_report::prompt_library::sync_builtins(&mut store)?;
    let cfg = AppConfig::load(
        &data_dir.join("config.toml"),
        KeyStore::platform_default(&data_dir),
    )?
    .core_config();
    let mut app = App {
        store,
        cfg,
        data_dir,
    };

    match cli.command {
        Cmd::Scan { since, all } => cmd_scan(&mut app, (!all).then_some(since)),
        Cmd::Reslice => cmd_reslice(&mut app),
        Cmd::Days { limit } => cmd_days(&app, limit),
        Cmd::Sessions { date } => cmd_sessions(&app, &resolve_date(&app, date)),
        Cmd::Show { target, full } => cmd_show(&app, &target, full),
        Cmd::Digest { slice } => cmd_digest(&app, &slice),
        Cmd::Search(args) => cmd_search::cmd_search(&app, args),
        Cmd::Note { session, text } => {
            let s = find_session(&app, &session)?;
            app.store.set_session_note(s.id, text.as_deref())?;
            println!(
                "Note {}: {}",
                if text.is_some() { "set" } else { "cleared" },
                title(&s)
            );
            Ok(())
        }
        Cmd::Ignore { session } => set_ignored(&app, &session, true),
        Cmd::Include { session } => set_ignored(&app, &session, false),
        Cmd::Report { kind } => cmd_report::cmd_report(&mut app, kind),
        Cmd::Prompt { cmd } => cmd_prompt::cmd_prompt(&mut app, cmd),
        Cmd::Project { cmd } => cmd_project::cmd_project(&mut app, cmd),
        Cmd::Rule { cmd } => cmd_project::cmd_rule(&mut app, cmd),
        Cmd::Assign(args) => cmd_project::cmd_assign(&app, args),
        Cmd::Unassign { targets } => cmd_project::cmd_unassign(&app, &targets),
        Cmd::Accept {
            targets,
            date,
            project,
            all,
        } => cmd_project::cmd_accept(&app, &targets, date.as_deref(), project.as_deref(), all),
        Cmd::Models { find: true, .. } => cmd_find_tools(),
        Cmd::Models {
            add: Some(tool), ..
        } => cmd_add_tool(&app, &tool),
        Cmd::Models { test, model, .. } => cmd_models(&app, test, model.as_deref()),
        Cmd::Export { session } => cmd_export(&app, session.as_deref()),
        Cmd::Doctor => cmd_doctor(&app),
    }
}
