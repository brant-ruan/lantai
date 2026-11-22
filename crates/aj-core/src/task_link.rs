//! Same-task signals between slices of one project and day. Pure logic.
//!
//! Two slices are probably the same task when they work in the same Git root on the same
//! branch and changed some of the same files. Timing is reported as context only: on real
//! data, parallel sessions on one branch overlap in time constantly, so time alone links
//! unrelated work. The signal does not depend on the agent.

use std::collections::BTreeSet;

use chrono::{DateTime, Duration, Utc};

/// Slices at most this far apart are reported as adjacent.
pub const ADJACENT_MINUTES: i64 = 30;

/// What the signal needs to know about one slice.
#[derive(Debug, Clone)]
pub struct SliceFacts<'a> {
    pub slice_id: i64,
    pub git_root: Option<&'a str>,
    pub branch: Option<&'a str>,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    /// Modified files, already made comparable with [`comparable_files`].
    pub files: &'a BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskLink {
    /// The earlier slice of the pair (by start time, then ID).
    pub a: i64,
    pub b: i64,
    pub branch: String,
    pub shared_files: usize,
    /// Minutes between the two slices; 0 when they overlap.
    pub gap_minutes: i64,
}

impl TaskLink {
    pub fn overlapping(&self) -> bool {
        self.gap_minutes == 0
    }

    pub fn adjacent(&self) -> bool {
        self.gap_minutes <= ADJACENT_MINUTES
    }
}

/// Paths relative to the first root that contains them, so that the same file edited from
/// different checkouts (a worktree, another agent's working directory) compares equal.
pub fn comparable_files(files: &[String], roots: &[&str]) -> BTreeSet<String> {
    files
        .iter()
        .map(|f| {
            roots
                .iter()
                .filter(|r| !r.is_empty())
                .find_map(|r| {
                    f.strip_prefix(r.trim_end_matches('/'))
                        .and_then(|rest| rest.strip_prefix('/'))
                })
                .unwrap_or(f)
                .to_string()
        })
        .collect()
}

/// Every pair of slices that is probably the same task, ordered by the earlier slice.
pub fn link_slices(slices: &[SliceFacts]) -> Vec<TaskLink> {
    let mut sorted: Vec<&SliceFacts> = slices.iter().collect();
    sorted.sort_by_key(|s| (s.start, s.slice_id));
    let mut out = Vec::new();
    for (i, a) in sorted.iter().enumerate() {
        for b in &sorted[i + 1..] {
            if let Some(link) = link(a, b) {
                out.push(link);
            }
        }
    }
    out
}

fn non_empty(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

/// `a` starts no later than `b`.
fn link(a: &SliceFacts, b: &SliceFacts) -> Option<TaskLink> {
    let root = non_empty(a.git_root)?;
    let branch = non_empty(a.branch)?;
    if non_empty(b.git_root) != Some(root) || non_empty(b.branch) != Some(branch) {
        return None;
    }
    let shared_files = a.files.intersection(b.files).count();
    let gap = (b.start - a.end).max(Duration::zero());
    let gap_minutes = gap.num_minutes();
    if shared_files == 0 {
        return None;
    }
    Some(TaskLink {
        a: a.slice_id,
        b: b.slice_id,
        branch: branch.to_string(),
        shared_files,
        gap_minutes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(h: u32, m: u32) -> DateTime<Utc> {
        format!("2026-11-17T{h:02}:{m:02}:00Z").parse().unwrap()
    }

    fn files(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    fn facts<'a>(
        id: i64,
        branch: &'a str,
        span: (DateTime<Utc>, DateTime<Utc>),
        files: &'a BTreeSet<String>,
    ) -> SliceFacts<'a> {
        SliceFacts {
            slice_id: id,
            git_root: Some("/repo"),
            branch: Some(branch),
            start: span.0,
            end: span.1,
            files,
        }
    }

    #[test]
    fn worktree_and_main_checkout_paths_compare_equal() {
        let a = comparable_files(&["/repo/src/a.rs".into()], &["/repo"]);
        let b = comparable_files(
            &["/repo/.claude/worktrees/x/src/a.rs".into(), "/tmp/o".into()],
            &["/repo/.claude/worktrees/x/", "/repo"],
        );
        assert_eq!(a, files(&["src/a.rs"]));
        assert_eq!(b, files(&["/tmp/o", "src/a.rs"]));
    }

    #[test]
    fn links_only_slices_sharing_files_on_the_same_branch() {
        let shared = files(&["src/a.rs", "src/b.rs"]);
        let other = files(&["src/a.rs"]);
        let none = files(&[]);
        let slices = [
            // Shared file, far apart in time.
            facts(1, "main", (t(9, 0), t(10, 0)), &shared),
            facts(2, "main", (t(15, 0), t(16, 0)), &other),
            // Shares a file with 1 and 2 and starts 20 minutes after slice 2.
            facts(3, "main", (t(16, 20), t(17, 0)), &other),
            // Overlaps slice 3 but shares no file.
            facts(5, "main", (t(16, 30), t(17, 0)), &none),
            // Overlaps slice 3 but on another branch.
            facts(4, "feature", (t(16, 30), t(17, 0)), &shared),
        ];
        let links = link_slices(&slices);
        let pairs: Vec<(i64, i64, usize, i64)> = links
            .iter()
            .map(|l| (l.a, l.b, l.shared_files, l.gap_minutes))
            .collect();
        assert_eq!(pairs, vec![(1, 2, 1, 300), (1, 3, 1, 380), (2, 3, 1, 20)]);
        assert!(!links[0].adjacent() && links[2].adjacent());
    }

    #[test]
    fn overlap_counts_as_zero_gap_and_missing_git_info_never_links() {
        let f = files(&["x"]);
        let a = facts(1, "main", (t(9, 0), t(11, 0)), &f);
        let b = facts(2, "main", (t(10, 0), t(10, 30)), &f);
        let links = link_slices(&[b.clone(), a.clone()]);
        assert_eq!(links.len(), 1);
        assert_eq!((links[0].a, links[0].b), (1, 2));
        assert!(links[0].overlapping());

        let mut no_branch = b.clone();
        no_branch.branch = Some("");
        assert!(link_slices(&[a.clone(), no_branch]).is_empty());
        let mut no_root = b;
        no_root.git_root = None;
        assert!(link_slices(&[a, no_root]).is_empty());
    }
}
