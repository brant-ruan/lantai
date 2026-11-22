//! Native pickers for folders and files (macOS `osascript` dialogs). Each runs on a blocking
//! thread and returns `None` when the user cancels. Prompts and paths go in as script
//! arguments, never into the script text.

use std::path::{Path, PathBuf};

/// The nearest folder that exists at or above `path`.
pub fn existing_dir(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|d| d.is_dir()).map(Path::to_path_buf)
}

pub async fn choose_folder(
    prompt: String,
    start: Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    run(
        "choose folder with prompt (item 1 of argv)",
        vec![prompt],
        start,
    )
    .await
}

/// A file name to save to (the dialog asks before replacing an existing file).
pub async fn choose_save_file(
    prompt: String,
    default_name: String,
    start: Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    run(
        "choose file name with prompt (item 1 of argv) default name (item 2 of argv)",
        vec![prompt, default_name],
        start,
    )
    .await
}

/// An existing file of the uniform type `uti` (for example `public.zip-archive`).
pub async fn choose_file(
    prompt: String,
    uti: String,
    start: Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    run(
        "choose file with prompt (item 1 of argv) of type {item 2 of argv}",
        vec![prompt, uti],
        start,
    )
    .await
}

async fn run(
    command: &'static str,
    args: Vec<String>,
    start: Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    tauri::async_runtime::spawn_blocking(move || run_blocking(command, args, start))
        .await
        .map_err(|e| format!("picker task failed: {e}"))?
}

#[cfg(target_os = "macos")]
fn run_blocking(
    command: &str,
    mut args: Vec<String>,
    start: Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    let mut command = command.to_string();
    if let Some(dir) = start.as_deref().and_then(existing_dir) {
        args.push(dir.to_string_lossy().into_owned());
        command.push_str(&format!(
            " default location (POSIX file (item {} of argv))",
            args.len()
        ));
    }
    let output = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(format!("on run argv\nPOSIX path of ({command})\nend run"))
        .args(&args)
        .output()
        .map_err(|e| format!("failed to launch the picker: {e}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return Ok(None);
    }
    let trimmed = if raw.len() > 1 {
        raw.trim_end_matches('/')
    } else {
        &raw
    };
    Ok(Some(PathBuf::from(trimmed)))
}

#[cfg(not(target_os = "macos"))]
fn run_blocking(
    _command: &str,
    _args: Vec<String>,
    _start: Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    Err("native pickers are currently supported on macOS only".to_string())
}
