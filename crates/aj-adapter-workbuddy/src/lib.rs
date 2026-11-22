//! WorkBuddy session adapter.
//!
//! All knowledge about the WorkBuddy session format lives in this crate only.
//! Format findings come from inspecting WorkBuddy 5.11 desktop sessions in 2026-11;
//! see `docs/ADAPTERS.md`.
//!
//! Sessions live at `<root>/<encoded cwd>/<session id>.jsonl` and are only ever appended to,
//! so they resume at a byte offset. Subagent logs under `<session id>/subagents/` are not read.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use aj_adapter::{read_complete_lines, ParseOutput, SourceAdapter, SourceFile};
use aj_ir::AgentId;
use anyhow::Result;

mod parse;

use parse::Parser;

pub struct WorkBuddyAdapter {
    /// Session root, normally `~/.workbuddy/projects`.
    root: PathBuf,
}

impl WorkBuddyAdapter {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl SourceAdapter for WorkBuddyAdapter {
    fn agent(&self) -> AgentId {
        AgentId::WORKBUDDY
    }

    fn discover(&self) -> Result<Vec<SourceFile>> {
        let mut out = Vec::new();
        let Ok(projects) = std::fs::read_dir(&self.root) else {
            return Ok(out);
        };
        for project in projects.flatten() {
            if !project.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            // Only `<session id>.jsonl` directly under a project directory; subagent logs and
            // rendered tool outputs live in a `<session id>/` directory next to it.
            let Ok(files) = std::fs::read_dir(project.path()) else {
                continue;
            };
            out.extend(files.flatten().filter_map(|f| session_file(&f.path())));
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn parse(&self, file: &SourceFile, from_offset: u64) -> Result<ParseOutput> {
        let mut out = ParseOutput::default();
        let mut parser = Parser::default();
        out.new_offset =
            read_complete_lines(&file.path, from_offset, |line| parser.line(line, &mut out))?;
        Ok(out)
    }
}

/// A session file: `<session id>.jsonl`. The session ID is the file stem.
fn session_file(path: &Path) -> Option<SourceFile> {
    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
        return None;
    }
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let id = path.file_stem()?.to_str()?.to_string();
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Some(SourceFile {
        path: path.to_path_buf(),
        source_session_id: id,
        size: meta.len(),
        mtime,
        revision: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_skips_subagents_and_side_files() {
        let dir = std::env::temp_dir().join(format!("aj-wb-discover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let project = dir.join("Users-u-pjts-app");
        let session_dir = project.join("s1");
        std::fs::create_dir_all(session_dir.join("subagents")).unwrap();
        std::fs::create_dir_all(session_dir.join("tool-results")).unwrap();
        for p in [
            project.join("s1.jsonl"),
            project.join("s1.file-rollback.ndjson"),
            session_dir.join("subagents/agent-1.jsonl"),
            session_dir.join("tool-results/call-render.txt"),
            dir.join("stray.jsonl"),
        ] {
            std::fs::write(p, b"").unwrap();
        }
        let found = WorkBuddyAdapter::new(dir.clone()).discover().unwrap();
        let ids: Vec<_> = found.iter().map(|f| f.source_session_id.as_str()).collect();
        assert_eq!(ids, ["s1"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
