//! Imports the user's login-shell environment at startup.
//!
//! An app launched from Finder or the Dock inherits only launchd's minimal
//! environment: `PATH` lacks user tool directories (so engines such as the
//! `claude` binary are not found) and variables exported from shell profiles
//! (such as API keys named by `api_key_env`) are missing. This module asks the
//! login shell for its environment once and fills in what the process lacks.

use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MARKER: &str = "__AJ_SHELL_ENV_BEGIN__";
const TIMEOUT: Duration = Duration::from_secs(5);

/// Variables that describe the shell session itself and must not be copied.
const SKIP: &[&str] = &["_", "PWD", "OLDPWD", "SHLVL", "TERM_SESSION_ID"];

/// Merges the login shell's environment into this process.
///
/// `PATH` is replaced by the shell's value; every other variable is only added
/// when absent, so values set explicitly at launch keep precedence. Failures
/// are reported on stderr and otherwise ignored.
///
/// Must be called before any other thread is spawned.
pub fn import_login_shell_env() {
    if !cfg!(target_os = "macos") {
        return;
    }
    match read_login_shell_env() {
        Ok(vars) => {
            for (key, value) in vars {
                if SKIP.contains(&key.as_str()) {
                    continue;
                }
                if key == "PATH" || std::env::var_os(&key).is_none() {
                    std::env::set_var(key, value);
                }
            }
        }
        Err(err) => eprintln!("warning: could not import login shell environment: {err}"),
    }
}

fn read_login_shell_env() -> Result<HashMap<String, String>, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut child = Command::new(&shell)
        .arg("-ilc")
        .arg(format!("printf '%s' {MARKER}; /usr/bin/env -0"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("spawn {shell}: {e}"))?;

    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{shell} did not finish within {TIMEOUT:?}"));
            }
            Err(e) => return Err(format!("wait for {shell}: {e}")),
        }
    }

    let mut out = Vec::new();
    child
        .stdout
        .take()
        .ok_or("no stdout")?
        .read_to_end(&mut out)
        .map_err(|e| format!("read shell output: {e}"))?;
    Ok(parse_env_output(&String::from_utf8_lossy(&out)))
}

/// Parses `env -0` output that follows [`MARKER`]; anything the shell printed
/// before the marker (profile banners and the like) is discarded.
fn parse_env_output(raw: &str) -> HashMap<String, String> {
    let Some(start) = raw.find(MARKER) else {
        return HashMap::new();
    };
    raw[start + MARKER.len()..]
        .split('\0')
        .filter_map(|entry| entry.split_once('='))
        .filter(|(key, _)| !key.is_empty())
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_after_marker_and_keeps_multiline_values() {
        let raw = format!("banner\n{MARKER}A=1\0B=x\ny\0=bad\0C=a=b\0");
        let vars = parse_env_output(&raw);
        assert_eq!(vars.get("A").map(String::as_str), Some("1"));
        assert_eq!(vars.get("B").map(String::as_str), Some("x\ny"));
        assert_eq!(vars.get("C").map(String::as_str), Some("a=b"));
        assert_eq!(vars.len(), 3);
    }

    #[test]
    fn missing_marker_yields_nothing() {
        assert!(parse_env_output("A=1\0").is_empty());
    }
}
