//! Opens web pages in the system browser: Lantai's own pages, whose URLs are fixed here, and
//! links people write in their notes, which must be plain http(s) addresses so that nothing
//! else (files, other URL schemes, command-line flags) can be opened through this path.

use crate::error::CmdResult;
use serde::Deserialize;

/// Where a sponsor can support Lantai.
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SponsorChannel {
    GithubSponsors,
    Afdian,
}

impl SponsorChannel {
    fn url(self) -> &'static str {
        match self {
            Self::GithubSponsors => "https://github.com/sponsors/brant-ruan",
            Self::Afdian => "https://afdian.com/a/bonan",
        }
    }
}

pub(crate) fn open_in_browser(url: &str) -> CmdResult<()> {
    let status = std::process::Command::new("/usr/bin/open")
        .arg(url)
        .status()
        .map_err(|e| format!("failed to open {url}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("failed to open {url}: {status}").into())
    }
}

#[tauri::command(async)]
pub fn open_sponsor_page(channel: SponsorChannel) -> CmdResult<()> {
    open_in_browser(channel.url())
}

/// A link written in a note: an http or https URL without whitespace.
fn is_web_link(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() > "https://".len()
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[tauri::command(async)]
pub fn open_note_link(url: String) -> CmdResult<()> {
    if !is_web_link(&url) {
        return Err(format!("not a web link: {url}").into());
    }
    open_in_browser(&url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_web_links_open() {
        assert!(is_web_link("https://example.com/a?b=c"));
        assert!(is_web_link("http://localhost:1420"));
        assert!(!is_web_link("file:///etc/passwd"));
        assert!(!is_web_link("-a Calculator"));
        assert!(!is_web_link("https://"));
        assert!(!is_web_link("https://example.com/a b"));
    }
}
