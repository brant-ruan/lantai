//! Adapter interface and shared helpers.
//!
//! Each agent implements one [`SourceAdapter`], which discovers source files and translates raw records into IR events.
//! Adapters are stateless; cursors are persisted by the caller (the core layer).

pub mod text;

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use aj_ir::{AgentId, IrEvent};
use anyhow::{Context, Result};

/// A source file, corresponding to one session.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    /// Session ID from the source. Cursors are keyed by `(agent, source_session_id)`; the path is just an attribute.
    pub source_session_id: String,
    pub size: u64,
    /// Modification time (Unix seconds).
    pub mtime: i64,
    /// Content version given by sources that keep many sessions in one database file, where
    /// size, mtime and the file head say nothing about a single session. When set, the core layer
    /// compares only this value and reparses the whole session from offset 0 whenever it changes.
    /// File-based adapters leave it `None`.
    pub revision: Option<String>,
}

#[derive(Debug, Default)]
pub struct ParseOutput {
    pub events: Vec<IrEvent>,
    /// New byte offset: advanced only to the end of the last complete line.
    pub new_offset: u64,
    /// Number of lines that failed to parse.
    pub error_count: u64,
    /// Description of the most recent parse error.
    pub last_error: Option<String>,
    /// Number of lines skipped by the allowlist.
    pub skipped_lines: u64,
}

pub trait SourceAdapter {
    fn agent(&self) -> AgentId;

    /// Discover all source files.
    fn discover(&self) -> Result<Vec<SourceFile>>;

    /// Parse from `from_offset` to the end of the file. Must be fault tolerant: a bad line is counted, never fatal.
    fn parse(&self, file: &SourceFile, from_offset: u64) -> Result<ParseOutput>;

    /// Normalize a working directory to the project directory it belongs to (e.g. map an agent-created worktree back to its main repository).
    fn normalize_cwd(&self, cwd: &str) -> String {
        cwd.to_string()
    }

    /// Check which versions of the agent wrote the local data and whether this adapter can read
    /// them, so the settings page does not claim support for a format it has never seen.
    /// Adapters that do not check return [`Compatibility::Unchecked`].
    fn probe(&self) -> Result<SourceProbe> {
        Ok(SourceProbe::default())
    }
}

/// How well an adapter can read the data found on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compatibility {
    /// The adapter does not inspect versions.
    #[default]
    Unchecked,
    /// Every session found is in a format the adapter reads.
    Supported,
    /// Some sessions are in a format the adapter does not read yet.
    Partial,
    /// The local data is in a format the adapter does not read at all.
    Unsupported,
}

#[derive(Debug, Clone, Default)]
pub struct SourceProbe {
    pub compatibility: Compatibility,
    /// Agent versions seen in the local data, oldest first where the source tells.
    pub versions: Vec<String>,
    /// Why the data is not (fully) read, for `Partial` and `Unsupported`, or why probing failed.
    pub reason: Option<CompatReason>,
}

/// Structured so the shells can word it in the interface language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatReason {
    /// The data is in a newer storage format the adapter does not read yet (e.g. OpenCode 2.x).
    NewerFormat,
    /// Some sessions are only in the newer storage format.
    SessionsInNewerFormat(u64),
    /// Sessions were written by versions the adapter was not verified against.
    UntestedVersions(Vec<String>),
    /// The data does not have the expected structure.
    UnrecognizedLayout,
    /// Probing itself failed.
    ProbeFailed(String),
}

/// Fingerprint of the first bytes of a file, used to detect that the file was rewritten.
pub fn head_hash(path: &Path) -> Result<String> {
    let mut buf = [0u8; 512];
    let mut f = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let n = f.read(&mut buf)?;
    // FNV-1a: only needs to be stable and dependency-free, not collision resistant.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in &buf[..n] {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    Ok(format!("{h:016x}:{n}"))
}

/// Read lines starting at `from_offset`, calling `on_line` for every **complete** line.
///
/// A trailing partial line without a newline is not processed. The return value is the offset
/// just past the last complete line, so a file still being written resumes at the partial line next time.
pub fn read_complete_lines(
    path: &Path,
    from_offset: u64,
    mut on_line: impl FnMut(&[u8]),
) -> Result<u64> {
    let mut f = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    f.seek(SeekFrom::Start(from_offset))?;
    let mut reader = BufReader::with_capacity(1 << 20, f);
    let mut offset = from_offset;
    let mut line = Vec::new();
    loop {
        line.clear();
        let n = reader.read_until(b'\n', &mut line)?;
        if n == 0 || line.last() != Some(&b'\n') {
            break;
        }
        offset += n as u64;
        let trimmed = line.strip_suffix(b"\n").unwrap_or(&line);
        let trimmed = trimmed.strip_suffix(b"\r").unwrap_or(trimmed);
        if !trimmed.is_empty() {
            on_line(trimmed);
        }
    }
    Ok(offset)
}

/// Read every line of a zstd-compressed file, which may hold several concatenated frames.
///
/// A compressed stream cannot be resumed at a byte offset, so the whole file is read each time.
/// A file still being written can end inside a frame: decoding stops quietly there, the partial
/// line is dropped, and the return value is `true`. A complete stream returns `false`, and its last
/// line is processed even without a trailing newline.
pub fn read_zstd_lines(path: &Path, mut on_line: impl FnMut(&[u8])) -> Result<bool> {
    let f = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let decoder = zstd::stream::read::Decoder::new(f)
        .with_context(|| format!("failed to decompress {}", path.display()))?;
    let mut reader = BufReader::with_capacity(1 << 20, decoder);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => return Ok(false),
            Ok(_) => {
                let trimmed = line.trim_ascii_end();
                if !trimmed.is_empty() {
                    on_line(trimmed);
                }
            }
            Err(_) => return Ok(true),
        }
    }
}

/// Extract the top-level `"type"` field without deserializing the line, for allowlist pre-filtering.
///
/// Only a leading `{"type":"..."` is trusted, so multi-megabyte objects are never scanned.
/// Returns `None` otherwise, in which case the caller should fall back to a full parse.
pub fn peek_type(line: &[u8]) -> Option<&str> {
    const WINDOW: usize = 4096;
    let find = |hay: &[u8]| -> Option<(usize, usize)> {
        let needle = b"\"type\":\"";
        let pos = hay.windows(needle.len()).position(|w| w == needle)?;
        let start = pos + needle.len();
        let len = hay[start..].iter().position(|b| *b == b'"')?;
        Some((start, start + len))
    };
    let head = &line[..line.len().min(WINDOW)];
    // A leading {"type":"..." is always a top-level field.
    if head.starts_with(b"{\"type\":\"") {
        let (s, e) = find(head)?;
        return std::str::from_utf8(&head[s..e]).ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn partial_last_line_is_left_for_next_time() {
        let dir = std::env::temp_dir().join(format!("aj-adapter-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.jsonl");
        let mut f = File::create(&p).unwrap();
        f.write_all(b"{\"a\":1}\n{\"b\":2}\n{\"c\":").unwrap();
        drop(f);

        let mut seen = vec![];
        let off = read_complete_lines(&p, 0, |l| seen.push(l.to_vec())).unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(off, 16);

        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(b"3}\n").unwrap();
        drop(f);
        let mut seen = vec![];
        let off2 = read_complete_lines(&p, off, |l| seen.push(l.to_vec())).unwrap();
        assert_eq!(seen, vec![b"{\"c\":3}".to_vec()]);
        assert_eq!(off2, 24);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn zstd_lines_span_frames_and_stop_at_a_partial_frame() {
        let dir = std::env::temp_dir().join(format!("aj-adapter-zstd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.jsonl.zstd");
        let mut bytes = zstd::encode_all(&b"{\"a\":1}\n"[..], 3).unwrap();
        bytes.extend(zstd::encode_all(&b"{\"b\":2}\n{\"c\":3}"[..], 3).unwrap());
        std::fs::write(&p, &bytes).unwrap();

        let mut seen = vec![];
        let truncated = read_zstd_lines(&p, |l| seen.push(l.to_vec())).unwrap();
        assert!(!truncated);
        assert_eq!(seen.len(), 3, "the last line counts without a newline");

        // A third frame that is still being written: its partial line is dropped.
        let third = zstd::encode_all(&b"{\"d\":4}\n{\"e\":"[..], 3).unwrap();
        bytes.extend(&third[..third.len() - 4]);
        std::fs::write(&p, &bytes).unwrap();
        let mut seen = vec![];
        let truncated = read_zstd_lines(&p, |l| seen.push(l.to_vec())).unwrap();
        assert!(truncated);
        assert!(seen.len() >= 3 && seen.iter().all(|l| l.ends_with(b"}")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn peek_type_only_trusts_leading_field() {
        assert_eq!(
            peek_type(br#"{"type":"api-request","x":1}"#),
            Some("api-request")
        );
        assert_eq!(peek_type(br#"{"parentUuid":null,"type":"user"}"#), None);
    }
}
