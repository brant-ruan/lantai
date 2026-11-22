//! Storage for provider API keys, kept apart from `config.toml` and the database.
//!
//! - macOS: the login Keychain (service `lantai`, account = provider name),
//!   driven through Apple's `/usr/bin/security`. Writes go through its stdin
//!   (`security -i`) with the key hex-encoded, so the key never appears in a
//!   process's arguments. Because the Keychain trusts the Apple-signed tool
//!   rather than our binary, rebuilt or unsigned app builds and the `aj` CLI read
//!   the same items without repeated access prompts.
//! - Elsewhere, or when `AJ_KEYSTORE=file`: `<data dir>/secrets.toml`, written
//!   with owner-only permissions (`[keys] <provider> = "..."`).
//!
//! Keys are never logged, never returned to the frontend, and never included in
//! error messages.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const SECRETS_FILE: &str = "secrets.toml";
pub const KEYCHAIN_SERVICE: &str = "lantai";
const SECURITY_BIN: &str = "/usr/bin/security";

#[derive(Debug, Clone)]
pub enum KeyStore {
    Keychain { service: String },
    File(PathBuf),
}

impl KeyStore {
    /// The platform's preferred store; `AJ_KEYSTORE=file` forces the file store.
    pub fn platform_default(data_dir: &Path) -> Self {
        let force_file = std::env::var("AJ_KEYSTORE").is_ok_and(|v| v == "file");
        if cfg!(target_os = "macos") && !force_file && Path::new(SECURITY_BIN).exists() {
            KeyStore::Keychain {
                service: KEYCHAIN_SERVICE.to_string(),
            }
        } else {
            KeyStore::File(data_dir.join(SECRETS_FILE))
        }
    }

    /// Human-readable location, for the settings page.
    pub fn describe(&self) -> String {
        match self {
            KeyStore::Keychain { service } => format!("macOS Keychain (service \"{service}\")"),
            KeyStore::File(path) => format!("{} (owner-only)", path.display()),
        }
    }

    pub fn has(&self, provider: &str) -> bool {
        match self {
            KeyStore::Keychain { service } => Command::new(SECURITY_BIN)
                .args(["find-generic-password", "-s", service, "-a", provider])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success()),
            KeyStore::File(path) => SecretsFile::load(path)
                .map(|f| f.get(provider).is_some())
                .unwrap_or(false),
        }
    }

    pub fn get(&self, provider: &str) -> Result<Option<String>> {
        match self {
            KeyStore::Keychain { service } => {
                let out = Command::new(SECURITY_BIN)
                    .args(["find-generic-password", "-s", service, "-a", provider, "-w"])
                    .stdin(Stdio::null())
                    .stderr(Stdio::null())
                    .output()
                    .context("failed to run the security tool")?;
                if !out.status.success() {
                    return Ok(None);
                }
                let key = String::from_utf8(out.stdout)
                    .context("the stored key is not valid UTF-8")?
                    .trim()
                    .to_string();
                Ok(Some(key).filter(|k| !k.is_empty()))
            }
            KeyStore::File(path) => Ok(SecretsFile::load(path)?.get(provider).map(str::to_string)),
        }
    }

    pub fn set(&self, provider: &str, key: &str) -> Result<()> {
        let key = key.trim();
        if key.is_empty() {
            return self.remove(provider);
        }
        if key.chars().any(char::is_control) {
            bail!("the API key contains control characters");
        }
        match self {
            KeyStore::Keychain { service } => {
                check_keychain_token(provider)?;
                let hex: String = key.bytes().map(|b| format!("{b:02x}")).collect();
                let script = format!(
                    "add-generic-password -U -s {service} -a {provider} -l \"Lantai: {provider}\" -X {hex}\n"
                );
                run_security_script(&script).context("failed to store the key in the Keychain")
            }
            KeyStore::File(path) => {
                let mut f = SecretsFile::load(path)?;
                f.keys.insert(provider.to_string(), key.to_string());
                f.save(path)
            }
        }
    }

    pub fn remove(&self, provider: &str) -> Result<()> {
        match self {
            KeyStore::Keychain { service } => {
                // Exits non-zero when there is no such item; that is fine.
                let _ = Command::new(SECURITY_BIN)
                    .args(["delete-generic-password", "-s", service, "-a", provider])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .context("failed to run the security tool")?;
                Ok(())
            }
            KeyStore::File(path) => {
                let mut f = SecretsFile::load(path)?;
                if f.keys.remove(provider).is_some() {
                    f.save(path)?;
                }
                Ok(())
            }
        }
    }

    pub fn rename(&self, from: &str, to: &str) -> Result<()> {
        if from == to {
            return Ok(());
        }
        if let Some(key) = self.get(from)? {
            self.set(to, &key)?;
            self.remove(from)?;
        }
        Ok(())
    }
}

/// Keychain scripts are parsed by `security -i`, so names must be plain tokens.
/// Provider names are already restricted to `[A-Za-z0-9_-]`; this is a backstop.
fn check_keychain_token(s: &str) -> Result<()> {
    if s.is_empty()
        || !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!("invalid provider name for the Keychain: {s}");
    }
    Ok(())
}

fn run_security_script(script: &str) -> Result<()> {
    let mut child = Command::new(SECURITY_BIN)
        .arg("-i")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to run the security tool")?;
    child
        .stdin
        .take()
        .context("no stdin")?
        .write_all(script.as_bytes())?;
    let out = child.wait_with_output()?;
    // `security -i` exits 0 even when a command fails; errors go to stderr.
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() || !stderr.trim().is_empty() {
        bail!("security: {}", stderr.trim());
    }
    Ok(())
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SecretsFile {
    #[serde(default)]
    keys: BTreeMap<String, String>,
}

impl SecretsFile {
    fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("failed to read {}", path.display())),
        }
    }

    fn get(&self, provider: &str) -> Option<&str> {
        self.keys
            .get(provider)
            .map(String::as_str)
            .filter(|k| !k.is_empty())
    }

    fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string(self).context("failed to serialize secrets")?;
        let tmp = path.with_extension("toml.tmp");
        write_private(&tmp, text.as_bytes())
            .with_context(|| format!("failed to write {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| format!("failed to replace {}", path.display()))
    }
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_round_trips_with_private_permissions() {
        let dir = std::env::temp_dir().join(format!("aj-keystore-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(SECRETS_FILE);
        let store = KeyStore::File(path.clone());
        assert!(!store.has("a"));
        store.set("a", " k1 \n").unwrap();
        store.rename("a", "b").unwrap();
        assert!(!store.has("a"));
        assert_eq!(store.get("b").unwrap().as_deref(), Some("k1"));
        assert!(store.set("b", "bad\u{7}key").is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        store.remove("b").unwrap();
        assert!(!store.has("b"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn keychain_names_must_be_plain_tokens() {
        assert!(check_keychain_token("gateway_v1-x").is_ok());
        assert!(check_keychain_token("a b").is_err());
        assert!(check_keychain_token("a\"b").is_err());
    }
}
