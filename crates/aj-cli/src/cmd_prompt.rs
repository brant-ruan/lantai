//! Report prompts: prompt list/show/new/edit/rename/delete/default.
//!
//! Prompts belong to a report language; every command works on the prompts of the
//! configured report language unless `--lang` names another.

use std::path::{Path, PathBuf};
use std::process::Command;

use aj_report::lang::Language;
use aj_report::prompt_library::{self, KINDS};
use aj_store::PromptRow;
use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};

use crate::app::App;

#[derive(Args)]
pub(crate) struct KindArg {
    /// Report kind the prompt is for
    #[arg(long, default_value = "daily", value_parser = KINDS)]
    kind: String,
    /// Report language the prompt is for; defaults to `[report] language`
    #[arg(long)]
    lang: Option<String>,
}

#[derive(Subcommand)]
pub(crate) enum PromptCmd {
    /// List the prompts of a report language (* marks the default of each kind)
    List {
        /// Only this report kind
        #[arg(long, value_parser = KINDS)]
        kind: Option<String>,
        /// Report language; defaults to `[report] language`
        #[arg(long)]
        lang: Option<String>,
        /// List the prompts of every language
        #[arg(long)]
        all: bool,
    },
    /// Print a prompt
    Show {
        name: String,
        #[command(flatten)]
        kind: KindArg,
        /// Also print the fixed rules the program appends
        #[arg(long)]
        full: bool,
    },
    /// Create a prompt, by copying an existing one or reading a file
    New {
        name: String,
        #[command(flatten)]
        kind: KindArg,
        /// Copy the body of this prompt (defaults to the current default prompt)
        #[arg(long, conflicts_with = "file")]
        from: Option<String>,
        /// Read the body from this file
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Edit a prompt in $EDITOR, or replace its body with a file
    Edit {
        name: String,
        #[command(flatten)]
        kind: KindArg,
        /// Replace the body with the contents of this file
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Rename a prompt
    Rename {
        name: String,
        new_name: String,
        #[command(flatten)]
        kind: KindArg,
    },
    /// Delete a prompt
    Delete {
        name: String,
        #[command(flatten)]
        kind: KindArg,
    },
    /// Make a prompt the default for its kind in the report language
    Default {
        name: String,
        #[command(flatten)]
        kind: KindArg,
    },
}

pub(crate) fn cmd_prompt(app: &mut App, cmd: PromptCmd) -> Result<()> {
    match cmd {
        PromptCmd::List { kind, lang, all } => {
            let lang = app.report_language(lang.as_deref())?;
            let prompts = if all {
                app.store.list_prompts(kind.as_deref(), None)?
            } else {
                prompt_library::offered(&app.store, kind.as_deref(), lang)?
            };
            let defaults = prompt_library::default_ids(&app.store, lang)?;
            println!("Report language: {}", lang.tag);
            for p in prompts {
                println!(
                    "{} {:<8} {:<8} {:<20} {:>5} chars  {}",
                    if defaults.contains(&p.id) { "*" } else { " " },
                    p.kind,
                    p.language,
                    p.name,
                    p.body.chars().count(),
                    if p.is_builtin { "built-in" } else { "" }
                );
            }
            Ok(())
        }
        PromptCmd::Show { name, kind, full } => {
            let lang = app.report_language(kind.lang.as_deref())?;
            let p = find(app, &kind, &name)?;
            if full {
                let instructions = prompt_library::assemble(&p.body, &Default::default(), lang);
                println!("{}", prompt_library::with_language(&instructions, lang));
            } else {
                println!("{}", p.body.trim_end());
            }
            Ok(())
        }
        PromptCmd::New {
            name,
            kind,
            from,
            file,
        } => {
            let lang = app.report_language(kind.lang.as_deref())?;
            let body = match file {
                Some(path) => read_body(&path)?,
                None => {
                    prompt_library::resolve(&app.store, &kind.kind, lang, from.as_deref())?.body
                }
            };
            app.store
                .create_prompt(&kind.kind, lang.tag, &name, &body)?;
            println!(
                "Created {} {} prompt \"{}\"",
                lang.tag,
                kind.kind,
                name.trim()
            );
            println!("Edit it with `aj prompt edit \"{}\"`.", name.trim());
            Ok(())
        }
        PromptCmd::Edit { name, kind, file } => {
            let p = find(app, &kind, &name)?;
            if p.is_builtin {
                bail!(
                    "\"{}\" is built in and cannot be changed; copy it first with `aj prompt new <name> --from \"{}\"`",
                    p.name,
                    p.name
                );
            }
            let body = match file {
                Some(path) => read_body(&path)?,
                None => edit_in_editor(&app.data_dir, &p.body)?,
            };
            if body.trim() == p.body.trim() {
                println!("No changes.");
                return Ok(());
            }
            app.store.update_prompt_body(p.id, &body)?;
            println!("Updated \"{}\"", p.name);
            Ok(())
        }
        PromptCmd::Rename {
            name,
            new_name,
            kind,
        } => {
            let p = find(app, &kind, &name)?;
            app.store.rename_prompt(p.id, &new_name)?;
            println!("Renamed: {} → {}", p.name, new_name.trim());
            Ok(())
        }
        PromptCmd::Delete { name, kind } => {
            let p = find(app, &kind, &name)?;
            app.store.delete_prompt(p.id)?;
            println!("Deleted \"{}\"", p.name);
            Ok(())
        }
        PromptCmd::Default { name, kind } => {
            let p = find(app, &kind, &name)?;
            let lang = app.report_language(kind.lang.as_deref())?;
            app.store.set_default_prompt(p.id, lang.tag)?;
            println!(
                "\"{}\" is now the default {} prompt for {} reports",
                p.name, p.kind, lang.tag
            );
            Ok(())
        }
    }
}

fn find(app: &App, kind: &KindArg, name: &str) -> Result<PromptRow> {
    let lang: &Language = app.report_language(kind.lang.as_deref())?;
    prompt_library::resolve(&app.store, &kind.kind, lang, Some(name))
}

fn read_body(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

/// Open the body in the user's editor and return what they saved.
fn edit_in_editor(data_dir: &Path, body: &str) -> Result<String> {
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".into());
    // The variable may carry arguments, e.g. `code -w`.
    let mut parts = editor.split_whitespace();
    let Some(program) = parts.next() else {
        bail!("$EDITOR is empty; set it, or pass --file");
    };

    let path = data_dir.join("prompt-edit.md");
    std::fs::write(&path, body).with_context(|| format!("failed to write {}", path.display()))?;
    let status = Command::new(program)
        .args(parts)
        .arg(&path)
        .status()
        .with_context(|| format!("failed to start the editor `{editor}`"))?;
    let edited = read_body(&path);
    std::fs::remove_file(&path).ok();
    if !status.success() {
        bail!("the editor exited with an error; nothing was saved");
    }
    edited
}
