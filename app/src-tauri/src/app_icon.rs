//! App icon choice: the bundled catalogue, the saved preference, and applying it.
//!
//! The chosen icon replaces the Dock icon of the running app and is also set as
//! the custom Finder icon of the `.app` bundle, so it still shows while the app
//! is closed. The catalogue images live in `app-icons/` and are compiled in.
//! Only macOS has a native implementation; elsewhere the choice is only stored.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The icon baked into the bundle (`icons/icon.icns`); choosing it removes any override.
pub const DEFAULT_ICON: &str = "bowl-navy";
pub const PREF_FILE: &str = "app-icon";

pub struct IconInfo {
    pub id: &'static str,
    pub label: &'static str,
    png: &'static [u8],
}

macro_rules! icon {
    ($id:literal, $label:literal) => {
        IconInfo {
            id: $id,
            label: $label,
            png: include_bytes!(concat!("../app-icons/", $id, ".png")),
        }
    };
}

pub static ICONS: &[IconInfo] = &[
    icon!("bowl-navy", "Sunrise"),
    icon!("branch-dark", "Branch · Night"),
    icon!("branch-cream", "Branch · Paper"),
    icon!("arc-gold", "Arcs · Gold"),
    icon!("arc-blue", "Arcs · Blue"),
    icon!("spiral-red", "Spiral"),
    icon!("wave-green", "Waves"),
];

pub fn find(id: &str) -> Option<&'static IconInfo> {
    ICONS.iter().find(|i| i.id == id)
}

/// The saved choice; the default when nothing (or something unknown) is saved.
pub fn load_pref(data_dir: &Path) -> &'static IconInfo {
    std::fs::read_to_string(data_dir.join(PREF_FILE))
        .ok()
        .and_then(|s| find(s.trim()))
        .or_else(|| find(DEFAULT_ICON))
        .expect("the default icon is in the catalogue")
}

pub fn save_pref(data_dir: &Path, icon: &IconInfo) -> Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let path = data_dir.join(PREF_FILE);
    std::fs::write(&path, icon.id).with_context(|| format!("failed to write {}", path.display()))
}

/// When to rewrite the bundle's Finder icon.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BundleSync {
    /// The user just picked an icon.
    Always,
    /// At startup: only when the bundle disagrees with the choice (e.g. after a rebuild).
    IfStale,
}

/// Apply `icon` to the Dock and, when running from a bundle, to the bundle itself.
///
/// The Dock always gets an explicit image (the default too): resetting it to
/// "no image" makes AppKit reload the bundle icon, which may still be the
/// previous custom icon at that moment.
pub fn apply(app: &tauri::AppHandle, icon: &'static IconInfo, sync: BundleSync) {
    let is_default = icon.id == DEFAULT_ICON;
    let bundle = bundle_path().filter(|b| match sync {
        BundleSync::Always => true,
        // A custom Finder icon is stored as an `Icon\r` file in the bundle root.
        BundleSync::IfStale => b.join("Icon\r").exists() == is_default,
    });
    // The default is the bundle's own icon, so the bundle needs no custom icon for it.
    let bundle_png = (!is_default).then_some(icon.png);
    let dock_png = icon.png;
    let _ = app.run_on_main_thread(move || {
        if let Some(bundle) = &bundle {
            native::set_bundle_icon(bundle, bundle_png);
            touch(bundle);
        }
        native::set_dock_icon(dock_png);
    });
}

/// Bump the bundle's modification time so Finder and the Dock drop their cached icon.
fn touch(path: &Path) {
    if let Ok(dir) = std::fs::File::open(path) {
        let _ = dir.set_modified(std::time::SystemTime::now());
    }
}

/// `/…/Lantai.app` when the executable runs from a bundle (not `tauri dev`).
fn bundle_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bundle = exe.parent()?.parent()?.parent()?;
    (bundle.extension()? == "app").then(|| bundle.to_path_buf())
}

#[cfg(target_os = "macos")]
mod native {
    use std::path::Path;

    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage, NSWorkspace, NSWorkspaceIconCreationOptions};
    use objc2_foundation::{NSData, NSString};

    fn image(png: &[u8]) -> Option<objc2::rc::Retained<NSImage>> {
        NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(png))
    }

    /// Replace the running app's Dock icon. Must run on the main thread.
    pub fn set_dock_icon(png: &[u8]) {
        let (Some(mtm), Some(image)) = (MainThreadMarker::new(), image(png)) else {
            return;
        };
        // SAFETY: a valid, non-nil image.
        unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
    }

    /// Set the bundle's custom Finder icon; `None` removes it.
    pub fn set_bundle_icon(bundle: &Path, png: Option<&[u8]>) {
        let Some(path) = bundle.to_str() else {
            return;
        };
        let image = png.and_then(image);
        NSWorkspace::sharedWorkspace().setIcon_forFile_options(
            image.as_deref(),
            &NSString::from_str(path),
            NSWorkspaceIconCreationOptions(0),
        );
    }
}

#[cfg(not(target_os = "macos"))]
mod native {
    pub fn set_dock_icon(_png: &[u8]) {}
    pub fn set_bundle_icon(_bundle: &std::path::Path, _png: Option<&[u8]>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_consistent() {
        assert!(find(DEFAULT_ICON).is_some());
        for icon in ICONS {
            assert!(icon.png.starts_with(b"\x89PNG"), "{} is not a PNG", icon.id);
        }
        let mut ids: Vec<_> = ICONS.iter().map(|i| i.id).collect();
        ids.dedup();
        assert_eq!(ids.len(), ICONS.len());
    }

    #[test]
    fn preference_round_trips_and_falls_back() {
        let dir = std::env::temp_dir().join(format!("aj-icon-pref-{}", std::process::id()));
        assert_eq!(load_pref(&dir).id, DEFAULT_ICON);
        save_pref(&dir, find("arc-gold").unwrap()).unwrap();
        assert_eq!(load_pref(&dir).id, "arc-gold");
        std::fs::write(dir.join(PREF_FILE), "gone").unwrap();
        assert_eq!(load_pref(&dir).id, DEFAULT_ICON);
        std::fs::remove_dir_all(&dir).ok();
    }
}
