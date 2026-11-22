//! Antigravity session adapter.
//!
//! All knowledge about the Antigravity transcript format lives in this crate only.
//! Format findings come from inspecting AGY 2.0 and Antigravity IDE transcripts in 2026-11;
//! see `docs/ADAPTERS.md`.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use aj_adapter::{read_complete_lines, ParseOutput, SourceAdapter, SourceFile};
use aj_ir::{AgentKind, IrEvent};
use anyhow::{Context, Result};

mod cwd;
mod parse;

use parse::{step_type, Parser, TOOL_RESULT_STEP_TYPES};

const FULL_TRANSCRIPT: &str = "transcript_full.jsonl";
const COMPACT_TRANSCRIPT: &str = "transcript.jsonl";

pub struct AntigravityAdapter {
    /// Session brain roots, normally `~/.gemini/antigravity/brain` and `~/.gemini/antigravity-ide/brain`.
    roots: Vec<PathBuf>,
}

impl AntigravityAdapter {
    pub fn new(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }
}

impl SourceAdapter for AntigravityAdapter {
    fn agent(&self) -> AgentKind {
        AgentKind::Antigravity
    }

    fn discover(&self) -> Result<Vec<SourceFile>> {
        let mut by_id: HashMap<String, SourceFile> = HashMap::new();
        for root in &self.roots {
            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };
            for entry in entries.flatten() {
                let dir = entry.path();
                let Ok(dir_meta) = entry.metadata() else {
                    continue;
                };
                if !dir_meta.is_dir() {
                    continue;
                }
                let Some(conv_id) = dir.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if conv_id.starts_with('.') {
                    continue;
                }
                let Some(file) = discover_conversation(&dir, conv_id) else {
                    continue;
                };
                match by_id.get(&file.source_session_id) {
                    Some(existing) if existing.mtime > file.mtime => {}
                    Some(existing)
                        if existing.mtime == file.mtime
                            && is_full_transcript(&existing.path)
                            && !is_full_transcript(&file.path) => {}
                    _ => {
                        by_id.insert(file.source_session_id.clone(), file);
                    }
                }
            }
        }
        let mut out: Vec<SourceFile> = by_id.into_values().collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn parse(&self, file: &SourceFile, from_offset: u64) -> Result<ParseOutput> {
        let mut out = ParseOutput::default();
        let compact = file.path.file_name().and_then(|n| n.to_str()) == Some(COMPACT_TRANSCRIPT);
        let mut parser = Parser::new(compact);

        if from_offset > 0 {
            resume_pending_tools(&file.path, from_offset, &mut parser)?;
        }

        out.new_offset = read_complete_lines(&file.path, from_offset, |line| {
            parser.line(line, &mut out);
        })?;

        if !out.events.is_empty() {
            if let Some(cwd) = parser.inferred_cwd() {
                out.events.insert(
                    0,
                    IrEvent::SessionMeta {
                        cwd: Some(cwd),
                        git_branch: None,
                        agent_version: None,
                        parent_session_id: None,
                    },
                );
            }
        }
        Ok(out)
    }
}

fn is_full_transcript(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some(FULL_TRANSCRIPT)
}

fn discover_conversation(conv_dir: &Path, conv_id: &str) -> Option<SourceFile> {
    let logs = conv_dir.join(".system_generated").join("logs");
    let full = logs.join(FULL_TRANSCRIPT);
    let compact = logs.join(COMPACT_TRANSCRIPT);
    let path = if full.is_file() {
        full
    } else if compact.is_file() {
        compact
    } else {
        return None;
    };
    let meta = std::fs::metadata(&path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Some(SourceFile {
        path,
        source_session_id: conv_id.to_string(),
        size: meta.len(),
        mtime,
        revision: None,
    })
}

/// Scan `0..from_offset` to find the last `PLANNER_RESPONSE` and how many of its tool results were already consumed.
fn resume_pending_tools(path: &Path, from_offset: u64, parser: &mut Parser) -> Result<()> {
    let f = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = BufReader::with_capacity(1 << 20, f.take(from_offset));
    let mut last_planner: Option<Vec<u8>> = None;
    let mut results_after = 0usize;
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 || line.last() != Some(&b'\n') {
            break;
        }
        let trimmed = line.strip_suffix(b"\n").unwrap_or(&line);
        let trimmed = trimmed.strip_suffix(b"\r").unwrap_or(trimmed);
        if trimmed.is_empty() {
            continue;
        }
        match step_type(trimmed).as_deref() {
            Some("USER_INPUT") => {
                last_planner = None;
                results_after = 0;
            }
            Some("PLANNER_RESPONSE") => {
                last_planner = Some(trimmed.to_vec());
                results_after = 0;
            }
            Some(ty) if TOOL_RESULT_STEP_TYPES.contains(&ty) && last_planner.is_some() => {
                results_after += 1;
            }
            _ => {}
        }
    }
    if let Some(planner) = last_planner {
        parser.seed_pending_from_planner(&planner, results_after);
    }
    Ok(())
}
