//! The `aj` command-line tool that ships inside the app (docs/SPEC.md, "Distribution"):
//! installing it links `~/.local/bin/aj` to the bundled binary, which needs no
//! administrator rights. An existing file there that is not our link is never replaced.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// Name of the bundled binary, next to the app's own executable.
const BINARY: &str = "aj";

/// Where the link goes, relative to the home directory.
const LINK_DIR: &str = ".local/bin";

/// What is at the link location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkState {
    /// Nothing there.
    Missing,
    /// A link to this app's binary.
    Installed,
    /// A link to another copy of Lantai (for example after the app moved) or a dangling one.
    StaleLink,
    /// A file or a link we did not make; left alone.
    Other,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CliToolStatus {
    /// Whether this build carries the binary (development builds may not).
    pub bundled: bool,
    /// Whether the app runs from a temporary, randomized location macOS uses for
    /// downloaded apps that were not moved yet; a link there would break.
    pub translocated: bool,
    pub state: LinkState,
    /// The link location, with the home directory written as `~`.
    pub link_path: String,
    /// Whether the link's folder is on the PATH of the login shell.
    pub on_path: bool,
}

/// Decide what the link location holds. `link_target` is where an existing symbolic link
/// points; `is_file` tells a regular file (or folder) from nothing at all.
fn classify(link_target: Option<&Path>, is_file: bool, bundled: &Path) -> LinkState {
    match link_target {
        Some(target) if target == bundled => LinkState::Installed,
        Some(target) if is_lantai_binary(target) || !target.exists() => LinkState::StaleLink,
        Some(_) => LinkState::Other,
        None if is_file => LinkState::Other,
        None => LinkState::Missing,
    }
}

/// A path to the `aj` inside some copy of the app.
fn is_lantai_binary(path: &Path) -> bool {
    path.ends_with(Path::new("Contents/MacOS").join(BINARY))
        && path
            .ancestors()
            .any(|p| p.extension().is_some_and(|e| e == "app"))
}

fn bundled_binary() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot locate the app: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "cannot locate the app's folder".to_string())?;
    Ok(dir.join(BINARY))
}

fn link_path() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "cannot find the home folder".to_string())?;
    Ok(home.join(LINK_DIR).join(BINARY))
}

fn status() -> Result<CliToolStatus, String> {
    let bundled = bundled_binary()?;
    let link = link_path()?;
    let link_target = std::fs::read_link(&link).ok();
    let is_file = link_target.is_none() && link.symlink_metadata().is_ok();
    let link_dir = link.parent().map(Path::to_path_buf).unwrap_or_default();
    Ok(CliToolStatus {
        bundled: bundled.is_file(),
        translocated: bundled.to_string_lossy().contains("/AppTranslocation/"),
        state: classify(link_target.as_deref(), is_file, &bundled),
        link_path: format!("~/{LINK_DIR}/{BINARY}"),
        on_path: std::env::var_os("PATH")
            .is_some_and(|p| std::env::split_paths(&p).any(|d| d == link_dir)),
    })
}

#[tauri::command(async)]
pub fn cli_tool_status() -> Result<CliToolStatus, String> {
    status()
}

#[tauri::command(async)]
pub fn install_cli_tool() -> Result<CliToolStatus, String> {
    let current = status()?;
    if !current.bundled {
        return Err("this build of Lantai does not include the command-line tool".into());
    }
    if current.translocated {
        return Err("move Lantai to the Applications folder and open it from there first".into());
    }
    let link = link_path()?;
    match current.state {
        LinkState::Installed => return Ok(current),
        LinkState::Other => {
            return Err(format!(
                "{} already exists and was not made by Lantai; remove it first",
                current.link_path
            ))
        }
        LinkState::StaleLink => {
            std::fs::remove_file(&link).map_err(|e| format!("cannot replace the old link: {e}"))?
        }
        LinkState::Missing => {}
    }
    if let Some(dir) = link.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("cannot create ~/{LINK_DIR}: {e}"))?;
    }
    std::os::unix::fs::symlink(bundled_binary()?, &link)
        .map_err(|e| format!("cannot create the link: {e}"))?;
    status()
}

#[tauri::command(async)]
pub fn uninstall_cli_tool() -> Result<CliToolStatus, String> {
    let current = status()?;
    if matches!(current.state, LinkState::Installed | LinkState::StaleLink) {
        std::fs::remove_file(link_path()?).map_err(|e| format!("cannot remove the link: {e}"))?;
    }
    status()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLED: &str = "/Applications/Lantai.app/Contents/MacOS/aj";

    #[test]
    fn classifies_the_link_location() {
        let bundled = Path::new(BUNDLED);
        assert_eq!(classify(None, false, bundled), LinkState::Missing);
        assert_eq!(classify(None, true, bundled), LinkState::Other);
        assert_eq!(
            classify(Some(bundled), false, bundled),
            LinkState::Installed
        );
        assert_eq!(
            classify(
                Some(Path::new("/Users/u/Downloads/Lantai.app/Contents/MacOS/aj")),
                false,
                bundled
            ),
            LinkState::StaleLink
        );
        assert_eq!(
            classify(Some(Path::new("/nonexistent/aj-elsewhere")), false, bundled),
            LinkState::StaleLink
        );
        // An existing link to something else is someone's own tool.
        assert_eq!(
            classify(Some(Path::new("/bin/ls")), false, bundled),
            LinkState::Other
        );
    }

    #[test]
    fn recognizes_lantai_binaries_only_inside_an_app() {
        assert!(is_lantai_binary(Path::new(BUNDLED)));
        assert!(!is_lantai_binary(Path::new("/usr/local/bin/aj")));
        assert!(!is_lantai_binary(Path::new("/opt/Contents/MacOS/aj")));
    }
}
