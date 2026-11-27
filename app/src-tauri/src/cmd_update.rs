//! Update checks (docs/SPEC.md, "Distribution"): the automatic-check setting, a check against
//! GitHub Releases, and opening a release page. Nothing is downloaded or installed.

use aj_update::UpdateCheck;
use serde::Serialize;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// The running app's version (the workspace version).
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsDto {
    pub check_updates: bool,
    pub current_version: String,
}

#[tauri::command(async)]
pub fn update_settings(state: tauri::State<'_, AppState>) -> CmdResult<UpdateSettingsDto> {
    let cfg = state.load_config()?;
    Ok(UpdateSettingsDto {
        check_updates: cfg.check_updates(),
        current_version: CURRENT_VERSION.to_string(),
    })
}

#[tauri::command(async)]
pub fn set_check_updates(state: tauri::State<'_, AppState>, on: bool) -> CmdResult<()> {
    state
        .modify_config(|c| {
            c.set_check_updates(on);
            Ok(())
        })
        .map(|_| ())
        .map_err(CmdError::from)
}

#[tauri::command(async)]
pub fn check_for_updates() -> CmdResult<UpdateCheck> {
    aj_update::check(CURRENT_VERSION).map_err(CmdError::from)
}

/// Open a release page from a check result; only pages of Lantai's own releases open here.
#[tauri::command(async)]
pub fn open_release_page(url: String) -> CmdResult<()> {
    if !is_release_page(&url) {
        return Err(format!("not a Lantai release page: {url}").into());
    }
    crate::cmd_links::open_in_browser(&url)
}

fn is_release_page(url: &str) -> bool {
    let prefix = format!("https://github.com/{}/releases/", aj_update::REPOSITORY);
    url.strip_prefix(&prefix).is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '-' | '_'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_own_release_pages_open() {
        assert!(is_release_page(
            "https://github.com/brant-ruan/lantai/releases/tag/v0.2.0"
        ));
        assert!(!is_release_page(
            "https://github.com/brant-ruan/lantai/releases/"
        ));
        assert!(!is_release_page(
            "https://github.com/someone/else/releases/tag/v1"
        ));
        assert!(!is_release_page(
            "https://github.com/brant-ruan/lantai/releases/tag/v1 -a Calculator"
        ));
        assert!(!is_release_page("file:///etc/passwd"));
    }
}
