//! Agent CLIs Lantai knows how to find on this machine, and whether each can generate reports.
//!
//! [`TOOLS`] is the table (pure data); [`find`] looks for each command on a search path and asks
//! for its version, which is the only IO here. Finding never runs a prompt, so it says nothing
//! about sign-in or quota. The bar a CLI must pass to become an engine is in `docs/SPEC.md`,
//! "Agent CLIs".

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use aj_ir::AgentId;
use serde::Serialize;

/// How a usable tool generates; each kind has an `Engine` implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEngine {
    ClaudeCli,
    CodexCli,
}

/// A part of the bar a tool misses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Gap {
    /// Tools that can act stay on.
    Tools,
    /// Every run leaves a session behind.
    Sessions,
}

/// One known agent CLI.
#[derive(Debug, Clone)]
pub struct ToolDescriptor {
    pub agent: AgentId,
    /// Executable name looked up on the search path.
    pub command: &'static str,
    pub version_args: &'static [&'static str],
    /// `Some` when the tool passes the bar.
    pub engine: Option<ToolEngine>,
    /// What it misses when `engine` is `None`.
    pub gaps: &'static [Gap],
}

/// Known agent CLIs, in display order.
pub const TOOLS: [ToolDescriptor; 4] = [
    ToolDescriptor {
        agent: AgentId::CLAUDE,
        command: "claude",
        version_args: &["--version"],
        engine: Some(ToolEngine::ClaudeCli),
        gaps: &[],
    },
    ToolDescriptor {
        agent: AgentId::CODEX,
        command: "codex",
        version_args: &["--version"],
        engine: Some(ToolEngine::CodexCli),
        gaps: &[],
    },
    ToolDescriptor {
        agent: AgentId::ANTIGRAVITY,
        command: "agy",
        version_args: &["--version"],
        engine: None,
        gaps: &[Gap::Tools, Gap::Sessions],
    },
    ToolDescriptor {
        agent: AgentId::OPENCODE,
        command: "opencode",
        version_args: &["--version"],
        engine: None,
        gaps: &[Gap::Sessions],
    },
];

/// The known tool with this agent identifier or command name.
pub fn tool(key: &str) -> Option<&'static ToolDescriptor> {
    TOOLS
        .iter()
        .find(|t| t.agent.as_str() == key || t.command == key)
}

/// A known tool present on this machine.
#[derive(Debug, Clone)]
pub struct FoundTool {
    pub tool: &'static ToolDescriptor,
    /// Absolute path of the executable (the first match on the search path).
    pub path: PathBuf,
    /// First line of its version output; `None` when it did not answer in time.
    pub version: Option<String>,
}

/// Where `command` would be looked for on `search_path` (a `PATH`-style list), in order.
/// Relative entries are skipped, like a login shell would never resolve them for us.
pub fn candidates(search_path: &OsStr, command: &str) -> Vec<PathBuf> {
    std::env::split_paths(search_path)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(command))
        .collect()
}

/// The first line of a version command's output, trimmed; `None` if there is none.
pub fn version_line(output: &[u8]) -> Option<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(80).collect())
}

/// Look for every known tool on `search_path` and ask each one found for its version.
pub fn find(search_path: &OsStr) -> Vec<FoundTool> {
    TOOLS
        .iter()
        .filter_map(|tool| {
            let path = candidates(search_path, tool.command)
                .into_iter()
                .find(|p| is_executable(p))?;
            let version = run_version(&path, tool.version_args);
            Some(FoundTool {
                tool,
                path,
                version,
            })
        })
        .collect()
}

/// Look on the process's own `PATH` (the app imports the login shell's at start).
pub fn find_on_path() -> Vec<FoundTool> {
    find(&std::env::var_os("PATH").unwrap_or_default())
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// Run `path args…` with no input and return the first line it prints, giving up after
/// [`VERSION_TIMEOUT`]. Runs in a temporary directory so no project configuration applies.
fn run_version(path: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(path);
    cmd.args(args).current_dir(std::env::temp_dir());
    let done = crate::process::run(&mut cmd, None, Some(VERSION_TIMEOUT), "the tool").ok()??;
    version_line(&done.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_consistent() {
        for t in &TOOLS {
            assert!(t.agent.registration().is_some(), "{}", t.agent);
            assert_eq!(t.engine.is_some(), t.gaps.is_empty(), "{}", t.agent);
        }
        assert_eq!(tool("agy").unwrap().agent, AgentId::ANTIGRAVITY);
        assert_eq!(tool("antigravity").unwrap().command, "agy");
        assert!(tool("cursor").is_none());
    }

    #[test]
    fn candidates_skip_relative_entries() {
        let path = std::env::join_paths(["/a/bin", "rel/bin", "", "/b"]).unwrap();
        assert_eq!(
            candidates(&path, "claude"),
            vec![PathBuf::from("/a/bin/claude"), PathBuf::from("/b/claude")]
        );
    }

    #[test]
    fn version_line_takes_first_non_empty_line() {
        assert_eq!(
            version_line(b"\n  codex-cli 0.1.0  \nmore\n").as_deref(),
            Some("codex-cli 0.1.0")
        );
        assert_eq!(version_line(b" \n"), None);
    }

    #[test]
    fn finds_executables_and_reads_versions() {
        let dir = std::env::temp_dir().join(format!("aj-tools-{}", std::process::id()));
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let script = bin.join("codex");
        std::fs::write(&script, "#!/bin/sh\necho 'codex-cli 9.9.9'\n").unwrap();
        // A file that is not executable does not count.
        std::fs::write(bin.join("opencode"), "#!/bin/sh\necho 1\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let found = find(bin.as_os_str());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].tool.agent, AgentId::CODEX);
        assert_eq!(found[0].path, script);
        assert_eq!(found[0].version.as_deref(), Some("codex-cli 9.9.9"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
