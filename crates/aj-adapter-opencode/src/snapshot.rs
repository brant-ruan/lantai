//! Private copy of the OpenCode database, so the source directory is never written.
//!
//! Opening a WAL database with SQLite, even read-only, creates or touches its `-shm` / `-wal`
//! files, and `immutable=1` misses commits that are still in the WAL. So the main file and its WAL
//! are copied into a cache directory owned by Lantai and only the copy is opened.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{bail, Context, Result};

pub(crate) const DB_FILE: &str = "opencode.db";
const WAL_SUFFIX: &str = "-wal";
const SHM_SUFFIX: &str = "-shm";
/// Records which source state the cached copy was taken from.
const STAMP_FILE: &str = "source.stamp";
const ATTEMPTS: usize = 3;

/// Size and modification time of a file, or `None` when it does not exist.
type Stamp = Option<(u64, SystemTime)>;

fn stamp(path: &Path) -> Stamp {
    let meta = fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

fn stamp_text(db: Stamp, wal: Stamp) -> String {
    let one = |s: Stamp| match s {
        Some((len, t)) => {
            let nanos = t
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            format!("{len}@{nanos}")
        }
        None => "-".to_string(),
    };
    format!("{} {}", one(db), one(wal))
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Copy `<source_dir>/opencode.db` (and its WAL) into `cache_dir` and return the copy's path.
///
/// Returns `None` when there is no database. An unchanged source reuses the previous copy. When
/// the source changes while it is being copied (OpenCode writing or checkpointing), the copy is
/// retried and finally given up with an error, to be tried again on the next scan.
pub(crate) fn take(source_dir: &Path, cache_dir: &Path) -> Result<Option<PathBuf>> {
    let src_db = source_dir.join(DB_FILE);
    let src_wal = with_suffix(&src_db, WAL_SUFFIX);
    if stamp(&src_db).is_none() {
        return Ok(None);
    }
    fs::create_dir_all(cache_dir)
        .with_context(|| format!("failed to create {}", cache_dir.display()))?;
    let dst_db = cache_dir.join(DB_FILE);
    let dst_wal = with_suffix(&dst_db, WAL_SUFFIX);
    let stamp_path = cache_dir.join(STAMP_FILE);

    for attempt in 0..ATTEMPTS {
        let before = stamp_text(stamp(&src_db), stamp(&src_wal));
        if dst_db.exists() && fs::read_to_string(&stamp_path).ok().as_deref() == Some(&before) {
            return Ok(Some(dst_db));
        }
        let _ = fs::remove_file(&stamp_path);
        for f in [&dst_wal, &with_suffix(&dst_db, SHM_SUFFIX)] {
            let _ = fs::remove_file(f);
        }
        fs::copy(&src_db, &dst_db)
            .with_context(|| format!("failed to copy {}", src_db.display()))?;
        if src_wal.exists() {
            fs::copy(&src_wal, &dst_wal)
                .with_context(|| format!("failed to copy {}", src_wal.display()))?;
        }
        let after = stamp_text(stamp(&src_db), stamp(&src_wal));
        if after == before {
            fs::write(&stamp_path, before)?;
            return Ok(Some(dst_db));
        }
        if attempt + 1 < ATTEMPTS {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    bail!(
        "{} kept changing while it was copied; will retry on the next scan",
        src_db.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aj-oc-snap-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_database_is_not_an_error() {
        let dir = temp("missing");
        assert!(take(&dir.join("src"), &dir.join("cache"))
            .unwrap()
            .is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn copies_db_and_wal_but_never_touches_the_source() {
        let dir = temp("copy");
        let src = dir.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join(DB_FILE), b"db").unwrap();
        fs::write(src.join("opencode.db-wal"), b"wal").unwrap();
        fs::write(src.join("opencode.db-shm"), b"shm").unwrap();
        let before: Vec<_> = ["opencode.db", "opencode.db-wal", "opencode.db-shm"]
            .iter()
            .map(|f| stamp(&src.join(f)))
            .collect();

        let cache = dir.join("cache");
        let copy = take(&src, &cache).unwrap().unwrap();
        assert_eq!(fs::read(&copy).unwrap(), b"db");
        assert_eq!(fs::read(cache.join("opencode.db-wal")).unwrap(), b"wal");
        assert!(!cache.join("opencode.db-shm").exists());

        // Unchanged source: the copy is reused, even if it was modified since.
        fs::write(&copy, b"opened").unwrap();
        take(&src, &cache).unwrap();
        assert_eq!(fs::read(&copy).unwrap(), b"opened");

        let after: Vec<_> = ["opencode.db", "opencode.db-wal", "opencode.db-shm"]
            .iter()
            .map(|f| stamp(&src.join(f)))
            .collect();
        assert_eq!(before, after);
        fs::remove_dir_all(&dir).ok();
    }
}
