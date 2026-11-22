//! `aj search`: plain-text search over reports and session text.

use std::io::IsTerminal;

use aj_core::search::SnippetPart;
use aj_ir::AgentId;
use aj_report::search::{search, ReportRef, SearchLayer, SearchRequest};
use aj_store::SliceFilter;
use anyhow::{Context, Result};
use clap::Args;

use crate::app::{clip, App};

#[derive(Args)]
pub(crate) struct SearchArgs {
    /// Search terms; every term must occur. An argument containing spaces is one phrase
    #[arg(required = true)]
    terms: Vec<String>,
    /// all, reports, or sessions
    #[arg(long, default_value = "all")]
    layer: String,
    /// Only this project (its report sections and its sessions)
    #[arg(long)]
    project: Option<String>,
    /// Only sessions from this agent (e.g. claude, codex)
    #[arg(long)]
    agent: Option<String>,
    /// First local date, YYYY-MM-DD
    #[arg(long)]
    since: Option<String>,
    /// Last local date, YYYY-MM-DD
    #[arg(long)]
    until: Option<String>,
    /// Also search ignored sessions
    #[arg(long)]
    include_ignored: bool,
    /// Also search tool output summaries
    #[arg(long)]
    tool_output: bool,
    /// Most candidate turns to scan
    #[arg(long, default_value_t = aj_report::search::DEFAULT_TURN_LIMIT)]
    limit: usize,
}

/// Rebuild a query string, quoting arguments that contain whitespace.
fn query_of(terms: &[String]) -> String {
    terms
        .iter()
        .map(|t| {
            if t.chars().any(char::is_whitespace) {
                format!("\"{}\"", t.replace('"', ""))
            } else {
                t.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn render(parts: &[SnippetPart], color: bool) -> String {
    parts
        .iter()
        .map(|p| match (p.hit, color) {
            (true, true) => format!("\x1b[1;33m{}\x1b[0m", p.text),
            (true, false) => format!("**{}**", p.text),
            (false, _) => p.text.clone(),
        })
        .collect()
}

fn refs(list: &[ReportRef]) -> String {
    list.iter()
        .map(|r| format!("{} {}", r.kind, r.period_key))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn cmd_search(app: &App, args: SearchArgs) -> Result<()> {
    let project_id = match &args.project {
        Some(name) => Some(
            app.store
                .find_project(name)?
                .with_context(|| format!("no project named {name:?}"))?
                .id,
        ),
        None => None,
    };
    let agent = match &args.agent {
        Some(key) => Some(AgentId::parse(key)?),
        None => None,
    };
    let req = SearchRequest {
        query: query_of(&args.terms),
        layer: SearchLayer::parse(&args.layer)?,
        filter: SliceFilter {
            since: args.since,
            until: args.until,
            project_id,
            agent,
            include_ignored: args.include_ignored,
        },
        include_tool_output: args.tool_output,
        turn_limit: args.limit,
    };
    let started = std::time::Instant::now();
    let res = search(&app.store, &req)?;
    let elapsed = started.elapsed();
    let color = std::io::stdout().is_terminal();

    if req.layer != SearchLayer::Sessions {
        println!("Reports: {} sections", res.reports.len());
        for hit in &res.reports {
            let section = hit.section.as_deref().unwrap_or(if hit.human {
                "(day notes)"
            } else {
                "(header)"
            });
            let origin = if hit.human { " · note" } else { "" };
            println!(
                "  {} {} · {}{}",
                hit.report.kind, hit.report.period_key, section, origin
            );
            for s in &hit.snippets {
                println!("      {}", render(s, color));
            }
        }
        if res.reports_truncated {
            println!("  (more report sections not shown)");
        }
        println!();
    }

    if req.layer != SearchLayer::Reports {
        let turns: usize = res.sessions.slices.iter().map(|s| s.turns.len()).sum();
        println!(
            "Sessions: {} slices, {} turns",
            res.sessions.slices.len(),
            turns
        );
        for hit in &res.sessions.slices {
            let s = &hit.slice;
            println!(
                "  {}  S{}  {}  {}  {}{}",
                s.local_date,
                s.slice_id,
                s.agent.product_name(),
                s.project_name.as_deref().unwrap_or("Unassigned"),
                clip(s.title.as_deref().unwrap_or("(untitled)"), 60),
                if s.ignored { "  [ignored]" } else { "" }
            );
            for snip in &hit.title_snippets {
                println!(
                    "      {:<11} {}",
                    snip.field.as_str(),
                    render(&snip.parts, color)
                );
            }
            for turn in &hit.turns {
                for (i, snip) in turn.snippets.iter().enumerate() {
                    let label = if i == 0 {
                        format!("T{}", turn.turn_index)
                    } else {
                        String::new()
                    };
                    println!(
                        "    {:<5} {:<11} {}",
                        label,
                        snip.field.as_str(),
                        render(&snip.parts, color)
                    );
                }
                let cited = res.cited_by(s.slice_id, Some(turn.turn_index));
                if !cited.is_empty() {
                    println!("          cited by {}", refs(&cited));
                }
            }
        }
        if res.sessions.truncated {
            println!(
                "  (stopped after {} turns; narrow the search or raise --limit)",
                req.turn_limit
            );
        }
    }
    eprintln!("({} ms)", elapsed.as_millis());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_with_spaces_become_phrases() {
        assert_eq!(
            query_of(&["busy timeout".into(), "sqlite".into()]),
            "\"busy timeout\" sqlite"
        );
    }
}
