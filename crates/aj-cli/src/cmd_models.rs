//! Models: list and test the configured aliases, find local agent CLIs and add one.

use aj_report::local_tools;
use anyhow::{bail, Result};
use clap::Args;

use crate::app::{clip, App};

#[derive(Args)]
pub(crate) struct ModelsArgs {
    /// Send a tiny request to each model (or the one given by --model) to check connectivity
    #[arg(long)]
    test: bool,
    /// Only list/test this model alias
    #[arg(long)]
    model: Option<String>,
    /// Look for known agent CLIs on PATH and say which can generate reports (runs only `--version`)
    #[arg(long, conflicts_with_all = ["test", "model"])]
    find: bool,
    /// Add a local agent CLI found on PATH as a provider and model (`claude` or `codex`)
    #[arg(long, value_name = "TOOL", conflicts_with_all = ["test", "model", "find"])]
    add: Option<String>,
}

pub(crate) fn cmd_models(app: &App, args: ModelsArgs) -> Result<()> {
    if args.find {
        return find_tools();
    }
    match args.add {
        Some(tool) => add_tool(app, &tool),
        None => list_models(app, args.test, args.model.as_deref()),
    }
}

/// `aj models`: the configured aliases, optionally tested with a tiny request.
fn list_models(app: &App, test: bool, only: Option<&str>) -> Result<()> {
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
fn find_tools() -> Result<()> {
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
fn add_tool(app: &App, key: &str) -> Result<()> {
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
