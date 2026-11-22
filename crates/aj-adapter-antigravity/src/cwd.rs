//! Pure working-directory inference and internal-path classification for Antigravity transcripts.
//!
//! Antigravity logs carry no explicit top-level `cwd`; this module infers the likely workspace root
//! from explicit command working directories and file-tool paths while excluding internal artifact and temp dirs.

use std::cmp::Ordering;
use std::collections::HashMap;

/// True when `path` points inside an Antigravity artifact brain directory.
pub fn is_internal_brain_path(path: &str) -> bool {
    const BRAIN_DIRS: [&str; 2] = [
        "/.gemini/antigravity/brain",
        "/.gemini/antigravity-ide/brain",
    ];
    BRAIN_DIRS
        .iter()
        .any(|dir| path.ends_with(dir) || path.contains(&format!("{dir}/")))
}

/// True when `path` belongs to agent state, OS library folders, or temp dirs and must not be inferred as a workspace.
pub fn is_excluded_cwd_path(path: &str) -> bool {
    if !path.starts_with('/') {
        return true;
    }
    const CONTAINS: [&str; 4] = ["/.gemini", "/.claude", "/.codex", "/Library"];
    for marker in CONTAINS {
        if path.ends_with(marker) || path.contains(&format!("{marker}/")) {
            return true;
        }
    }
    const TEMP_PREFIXES: [&str; 4] = [
        "/tmp",
        "/private/tmp",
        "/var/folders",
        "/private/var/folders",
    ];
    TEMP_PREFIXES
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
}

#[derive(Default)]
pub struct CwdCandidates {
    explicit: Vec<String>,
    writes: Vec<String>,
    reads: Vec<String>,
    active_docs: Vec<String>,
}

impl CwdCandidates {
    pub fn add_explicit_cwd(&mut self, path: &str) {
        if let Some(norm) = normalize_dir(path) {
            self.explicit.push(norm);
        }
    }

    pub fn add_write_file(&mut self, path: &str) {
        if let Some(parent) = posix_parent(path).filter(|p| !is_excluded_cwd_path(p)) {
            self.writes.push(parent);
        }
    }

    pub fn add_read_file(&mut self, path: &str) {
        if let Some(parent) = posix_parent(path).filter(|p| !is_excluded_cwd_path(p)) {
            self.reads.push(parent);
        }
    }

    pub fn add_read_dir(&mut self, path: &str) {
        if let Some(norm) = normalize_dir(path) {
            self.reads.push(norm);
        }
    }

    pub fn add_search_path(&mut self, path: &str) {
        let Some(trimmed) = normalize_raw_dir(path) else {
            return;
        };
        let last_seg = trimmed.rsplit('/').next().unwrap_or("");
        let dir = if last_seg.contains('.') {
            posix_parent(trimmed).unwrap_or_else(|| "/".into())
        } else {
            trimmed.to_string()
        };
        if !is_excluded_cwd_path(&dir) {
            self.reads.push(dir);
        }
    }

    pub fn add_active_doc(&mut self, path: &str) {
        if let Some(parent) = posix_parent(path).filter(|p| !is_excluded_cwd_path(p)) {
            self.active_docs.push(parent);
        }
    }

    /// Infer the most likely working directory from collected candidates.
    pub fn infer(&self) -> Option<String> {
        if !self.explicit.is_empty() {
            return most_frequent(&self.explicit);
        }

        let pool: Vec<&str> = if !self.writes.is_empty() || !self.reads.is_empty() {
            self.writes
                .iter()
                .chain(self.reads.iter())
                .map(String::as_str)
                .collect()
        } else if !self.active_docs.is_empty() {
            self.active_docs.iter().map(String::as_str).collect()
        } else {
            return None;
        };

        let total = pool.len();
        let mut counts: HashMap<String, (usize, usize, usize)> = HashMap::new();
        for (seen_idx, dir) in pool.iter().enumerate() {
            let parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
            for k in 2..=parts.len() {
                if matches!(parts[0], "Users" | "home") && k < 3 {
                    continue;
                }
                let ancestor = format!("/{}", parts[..k].join("/"));
                counts
                    .entry(ancestor)
                    .and_modify(|(count, _, _)| *count += 1)
                    .or_insert((1, k, seen_idx));
            }
        }

        if counts.is_empty() {
            return pool.first().map(|s| (*s).to_string());
        }

        let thresh = (total * 4).div_ceil(5);
        let qualifying: Vec<(&String, &(usize, usize, usize))> = counts
            .iter()
            .filter(|(_, (count, _, _))| *count >= thresh)
            .collect();

        if !qualifying.is_empty() {
            return qualifying
                .into_iter()
                .max_by(|(pa, &(ca, da, ia)), (pb, &(cb, db, ib))| {
                    da.cmp(&db)
                        .then_with(|| ca.cmp(&cb))
                        .then_with(|| ib.cmp(&ia))
                        .then_with(|| pa.cmp(pb))
                })
                .map(|(p, _)| p.clone());
        }

        counts
            .into_iter()
            .max_by(|(pa, (ca, da, ia)), (pb, (cb, db, ib))| {
                ca.cmp(cb)
                    .then_with(|| da.cmp(db))
                    .then_with(|| ib.cmp(ia))
                    .then_with(|| pa.cmp(pb))
            })
            .map(|(p, _)| p)
    }
}

fn normalize_raw_dir(path: &str) -> Option<&str> {
    if !path.starts_with('/') {
        return None;
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        Some("/")
    } else {
        Some(trimmed)
    }
}

fn normalize_dir(path: &str) -> Option<String> {
    let dir = normalize_raw_dir(path)?;
    (!is_excluded_cwd_path(dir)).then(|| dir.to_string())
}

fn posix_parent(path: &str) -> Option<String> {
    let trimmed = normalize_raw_dir(path)?;
    if trimmed == "/" {
        return None;
    }
    match trimmed.rfind('/') {
        Some(0) => Some("/".into()),
        Some(pos) => Some(trimmed[..pos].to_string()),
        None => None,
    }
}

fn most_frequent(items: &[String]) -> Option<String> {
    let mut counts: HashMap<&str, (usize, usize)> = HashMap::new();
    for (idx, item) in items.iter().enumerate() {
        counts
            .entry(item.as_str())
            .and_modify(|(c, _)| *c += 1)
            .or_insert((1, idx));
    }
    counts
        .into_iter()
        .max_by(|(a, (ca, ia)), (b, (cb, ib))| match ca.cmp(cb) {
            Ordering::Equal => ib.cmp(ia).then_with(|| a.cmp(b)),
            ord => ord,
        })
        .map(|(s, _)| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_cwd_takes_priority_and_uses_most_frequent() {
        let mut c = CwdCandidates::default();
        c.add_write_file("/Users/alice/other/src/lib.rs");
        c.add_explicit_cwd("/Users/alice/app");
        c.add_explicit_cwd("/Users/alice/other");
        c.add_explicit_cwd("/Users/alice/app/");
        assert_eq!(c.infer().as_deref(), Some("/Users/alice/app"));
    }

    #[test]
    fn infers_repo_root_from_multi_subdir_paths() {
        let mut c = CwdCandidates::default();
        c.add_read_file("/Users/alice/pjts/app/src/main.rs");
        c.add_write_file("/Users/alice/pjts/app/crates/core/lib.rs");
        c.add_read_dir("/Users/alice/pjts/app/tests");
        c.add_search_path("/Users/alice/pjts/app/docs/spec.md");
        assert_eq!(c.infer().as_deref(), Some("/Users/alice/pjts/app"));
    }

    #[test]
    fn eighty_percent_threshold_resists_stray_reads() {
        let mut c = CwdCandidates::default();
        for i in 0..8 {
            c.add_read_file(&format!("/Users/alice/pjts/app/src/mod_{i}.rs"));
        }
        c.add_read_file("/Users/alice/Downloads/notes.txt");
        assert_eq!(c.infer().as_deref(), Some("/Users/alice/pjts/app/src"));
    }

    #[test]
    fn excludes_brain_tmp_and_library_paths() {
        assert!(is_internal_brain_path(
            "/Users/alice/.gemini/antigravity/brain/conv-1/plan.md"
        ));
        assert!(is_internal_brain_path(
            "/Users/alice/.gemini/antigravity-ide/brain"
        ));
        assert!(!is_internal_brain_path("/Users/alice/pjts/app/brain/a.rs"));

        let mut c = CwdCandidates::default();
        c.add_write_file("/Users/alice/.gemini/antigravity/brain/conv-1/plan.md");
        c.add_read_file("/tmp/scratch.txt");
        c.add_read_file("/private/tmp/out.log");
        c.add_read_file("/Users/alice/Library/Logs/app.log");
        c.add_read_file("/Users/alice/.codex/sessions/foo.jsonl");
        assert_eq!(c.infer(), None);
    }

    #[test]
    fn rejects_shallow_home_dirs_when_inferring_ancestors() {
        let mut c = CwdCandidates::default();
        c.add_read_file("/Users/alice/proj_a/a.rs");
        c.add_read_file("/Users/alice/proj_a/b.rs");
        c.add_read_file("/Users/alice/proj_b/c.rs");
        // `/Users/alice` has count 3, but is skipped because depth < 3 under `/Users`.
        assert_eq!(c.infer().as_deref(), Some("/Users/alice/proj_a"));
    }

    #[test]
    fn falls_back_to_active_document_when_no_tool_paths() {
        let mut c = CwdCandidates::default();
        c.add_active_doc("/Users/alice/pjts/notes/todo.md");
        assert_eq!(c.infer().as_deref(), Some("/Users/alice/pjts/notes"));
    }

    #[test]
    fn returns_none_when_empty() {
        assert_eq!(CwdCandidates::default().infer(), None);
    }
}
