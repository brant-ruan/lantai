//! DeepSeek Harness session adapter.
//!
//! All knowledge about the DeepSeek Harness session format lives in this crate only.
//! Format findings come from inspecting DeepSeek Harness desktop sessions (format v4) in 2026-11;
//! see `docs/ADAPTERS.md`.
//!
//! Sessions live at `<root>/<encoded cwd>/<session id>/session.v<N>.jsonl.zstd`: a zstd stream of
//! frames appended while the session runs, so it is always read whole.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use aj_adapter::{read_zstd_lines, ParseOutput, SourceAdapter, SourceFile};
use aj_ir::AgentKind;
use anyhow::Result;

mod parse;

use parse::Parser;

const FILE_PREFIX: &str = "session.v";
const FILE_SUFFIX: &str = ".jsonl.zstd";

pub struct DshAdapter {
    /// Session root, normally `~/.dsh/sessions`.
    root: PathBuf,
}

impl DshAdapter {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

impl SourceAdapter for DshAdapter {
    fn agent(&self) -> AgentKind {
        AgentKind::Dsh
    }

    fn discover(&self) -> Result<Vec<SourceFile>> {
        let mut out = Vec::new();
        for project in subdirs(&self.root) {
            for session in subdirs(&project) {
                if let Some(file) = session_file(&session) {
                    out.push(file);
                }
            }
        }
        out.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(out)
    }

    fn parse(&self, file: &SourceFile, _from_offset: u64) -> Result<ParseOutput> {
        // A compressed stream cannot be resumed at a byte offset: always parse it whole.
        // Ingestion deduplicates by source event ID, so this is idempotent. A session still
        // running usually ends inside a frame; the lines before it are kept.
        let mut out = ParseOutput::default();
        let mut parser = Parser::default();
        read_zstd_lines(&file.path, |line| parser.line(line, &mut out))?;
        out.new_offset = file.size;
        Ok(out)
    }
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with('.'))
        })
        .collect()
}

/// The session file of one session directory: the highest format version present.
/// The session ID is the directory name.
fn session_file(dir: &Path) -> Option<SourceFile> {
    let id = dir.file_name()?.to_str()?.to_string();
    let mut best: Option<(u32, PathBuf, std::fs::Metadata)> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let Some(version) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(format_version)
        else {
            continue;
        };
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || best.as_ref().is_some_and(|(v, _, _)| *v >= version) {
            continue;
        }
        best = Some((version, path, meta));
    }
    let (_, path, meta) = best?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Some(SourceFile {
        path,
        source_session_id: id,
        size: meta.len(),
        mtime,
        revision: None,
    })
}

/// Format version from a session file name: `session.v<N>.jsonl.zstd`.
fn format_version(name: &str) -> Option<u32> {
    name.strip_prefix(FILE_PREFIX)?
        .strip_suffix(FILE_SUFFIX)?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_format_versions_from_file_names() {
        assert_eq!(format_version("session.v4.jsonl.zstd"), Some(4));
        assert_eq!(format_version("session.v12.jsonl.zstd"), Some(12));
        assert_eq!(format_version("session.lock"), None);
        assert_eq!(format_version("session.v4.jsonl"), None);
    }

    #[test]
    fn discovery_takes_one_file_per_session_with_the_highest_version() {
        let dir = std::env::temp_dir().join(format!("aj-dsh-discover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let a = dir.join("--Users-u-a--/session-1");
        let b = dir.join("--Users-u-a--/2222");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        for p in [
            a.join("session.v3.jsonl.zstd"),
            a.join("session.v4.jsonl.zstd"),
            a.join("session.lock"),
            b.join("session.lock"),
        ] {
            std::fs::write(p, b"").unwrap();
        }
        let found = DshAdapter::new(dir.clone()).discover().unwrap();
        assert_eq!(
            found.len(),
            1,
            "a directory without a session file is not a session"
        );
        assert_eq!(found[0].source_session_id, "session-1");
        assert!(found[0].path.ends_with("session.v4.jsonl.zstd"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
