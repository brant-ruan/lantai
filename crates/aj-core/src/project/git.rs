//! Probes Git information from a working directory. Read-only on the file system; never runs the `git` command.

use std::path::{Path, PathBuf};

/// Walk up from the working directory to the repository root. A worktree maps back to its main repository.
pub fn find_git_root(cwd: &Path) -> Option<PathBuf> {
    for dir in cwd.ancestors() {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            return Some(dir.to_path_buf());
        }
        if dot_git.is_file() {
            // In a worktree, .git is a file containing `gitdir: /repo/.git/worktrees/name`
            let content = std::fs::read_to_string(&dot_git).ok()?;
            let gitdir = content.trim().strip_prefix("gitdir:")?.trim();
            return Some(match gitdir.find("/.git/") {
                Some(pos) => PathBuf::from(&gitdir[..pos]),
                None => dir.to_path_buf(),
            });
        }
    }
    None
}

/// Read the URL of the repository's `origin`, or of the first remote when there is no origin.
///
/// Returns the normalized URL. URLs in `.git/config` may embed an access token
/// (`https://user:token@host/…`); the raw value must never leave this function.
pub fn read_remote(git_root: &Path) -> Option<String> {
    let config = std::fs::read_to_string(git_root.join(".git").join("config")).ok()?;
    parse_remote(&config).map(|url| normalize_remote(&url))
}

/// Normalize a remote URL to `host/owner/repo`, so that the ssh and https forms are equivalent.
pub fn normalize_remote(url: &str) -> String {
    let mut s = url.trim().to_lowercase();
    // A query or fragment can carry a token (`?access_token=…`) and never names the repository.
    if let Some(pos) = s.find(['?', '#']) {
        s.truncate(pos);
    }
    if let Some(pos) = s.find("://") {
        s = s[pos + 3..].to_string();
    }
    if let Some(pos) = s.find('@') {
        // Strip only user info that precedes the host name
        if !s[..pos].contains('/') {
            s = s[pos + 1..].to_string();
        }
    }
    // scp form: host:owner/repo
    if let Some(pos) = s.find(':') {
        let after = &s[pos + 1..];
        let is_port = after
            .split('/')
            .next()
            .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
        if !is_port {
            s.replace_range(pos..pos + 1, "/");
        }
    }
    s.trim_end_matches('/').trim_end_matches(".git").to_string()
}

fn parse_remote(config: &str) -> Option<String> {
    let mut current: Option<String> = None;
    let mut first: Option<String> = None;
    for line in config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            current = line
                .strip_prefix("[remote \"")
                .and_then(|rest| rest.strip_suffix("\"]"))
                .map(str::to_string);
            continue;
        }
        let Some(name) = &current else { continue };
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "url" {
            continue;
        }
        let url = value.trim().to_string();
        if name == "origin" {
            return Some(url);
        }
        first.get_or_insert(url);
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_root_for_subdirectory_and_worktree() {
        let base = std::env::temp_dir().join(format!("aj-core-git-{}", std::process::id()));
        let repo = base.join("myrepo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src/deep")).unwrap();
        assert_eq!(find_git_root(&repo.join("src/deep")), Some(repo.clone()));

        let wt = base.join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {}/.git/worktrees/wt\n", repo.display()),
        )
        .unwrap();
        assert_eq!(find_git_root(&wt), Some(repo.clone()));

        std::fs::write(
            repo.join(".git/config"),
            "[core]\n\tbare = false\n[remote \"upstream\"]\n\turl = https://example.com/up.git\n[remote \"origin\"]\n\turl = git@example.com:me/repo.git\n\tfetch = +refs/heads/*\n",
        )
        .unwrap();
        assert_eq!(read_remote(&repo).as_deref(), Some("example.com/me/repo"));
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn remote_forms_are_equivalent() {
        for url in [
            "git@github.com:Owner/Repo.git",
            "https://github.com/owner/repo",
            "https://user@github.com/owner/repo.git/",
            "ssh://git@github.com/owner/repo.git",
        ] {
            assert_eq!(normalize_remote(url), "github.com/owner/repo", "{url}");
        }
        assert_eq!(
            normalize_remote("ssh://git@host:2222/o/r.git"),
            "host:2222/o/r"
        );
    }

    #[test]
    fn credentials_in_remote_are_dropped() {
        for url in [
            "https://someone:ghp_exampletoken1234567890@github.com/owner/repo",
            "https://ghp_exampletoken1234567890@github.com/owner/repo.git",
            "https://oauth2:token-value@github.com/owner/repo",
            "https://github.com/owner/repo.git?access_token=ghp_exampletoken1234567890",
            "https://github.com/owner/repo#token=abc@def",
        ] {
            let out = normalize_remote(url);
            assert_eq!(out, "github.com/owner/repo", "{url}");
        }
    }

    #[test]
    fn falls_back_to_first_remote() {
        assert_eq!(
            parse_remote("[remote \"upstream\"]\n url = a\n[branch \"main\"]\n url = ignored\n")
                .as_deref(),
            Some("a")
        );
        assert_eq!(parse_remote("[core]\n x = 1\n"), None);
    }
}
