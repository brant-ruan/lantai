//! Backup archives of Lantai's own data: a consistent copy of the database and the
//! configuration files, packed into one zip file, and restoring them from such a file
//! (docs/SPEC.md, "Backup and restore"). Secrets are never part of an archive; API keys stay in the
//! keychain. Upgrading an older database goes through here too, so it is always backed up first
//! (docs/SPEC.md, "Upgrading the database"). The caller names the files ([`DataFiles`]); this crate
//! knows nothing else about them.

use std::fs::File;
use std::io::{self, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use aj_store::{
    database_state, file_schema_version, snapshot_file, DatabaseState, NotReady, Store,
    SCHEMA_VERSION,
};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// The archive layout this build writes and reads.
pub const FORMAT: u32 = 1;
const MANIFEST: &str = "manifest.json";
/// Where the data replaced by a restore is kept, inside the data directory.
pub const BACKUPS_DIR: &str = "backups";

/// What of a data directory an archive holds.
#[derive(Debug, Clone, Copy)]
pub struct DataFiles<'a> {
    pub dir: &'a Path,
    /// The database's file name.
    pub database: &'a str,
    /// Other files (configuration), stored and restored as they are when present.
    pub files: &'a [&'a str],
}

impl DataFiles<'_> {
    fn database_path(&self) -> PathBuf {
        self.dir.join(self.database)
    }
}

/// `manifest.json` at the root of an archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    /// The Lantai version that wrote the archive.
    pub app_version: String,
    pub schema_version: i64,
    /// RFC 3339, UTC.
    pub created_at: String,
    pub database: String,
    /// The other files in the archive.
    pub files: Vec<String>,
}

/// The result of a restore.
#[derive(Debug, Clone)]
pub struct Restored {
    pub manifest: Manifest,
    /// The folder holding the data the restore replaced.
    pub previous: PathBuf,
}

/// The result of an upgrade.
#[derive(Debug, Clone)]
pub struct Upgraded {
    pub from: i64,
    pub to: i64,
    /// The archive written before migrating.
    pub backup: PathBuf,
}

/// Write an archive of `data` to `dest` (replaced if it exists). The database is copied at
/// whatever schema it is, and the manifest says which.
pub fn export(
    data: &DataFiles,
    dest: &Path,
    app_version: &str,
    now: DateTime<Utc>,
) -> Result<Manifest> {
    let work = WorkDir::create(data.dir, "export", now)?;
    let snapshot = work.path.join(data.database);
    snapshot_file(&data.database_path(), &snapshot)?;
    let schema_version = file_schema_version(&snapshot)?;

    let files: Vec<&str> = data
        .files
        .iter()
        .copied()
        .filter(|f| data.dir.join(f).is_file())
        .collect();
    let manifest = Manifest {
        format: FORMAT,
        app_version: app_version.to_string(),
        schema_version,
        created_at: now.to_rfc3339_opts(SecondsFormat::Secs, true),
        database: data.database.to_string(),
        files: files.iter().map(|f| f.to_string()).collect(),
    };

    // Written next to the destination first, so a failure never leaves half an archive.
    let partial = with_suffix(dest, ".partial");
    let result = (|| -> Result<()> {
        let out = File::create(&partial)
            .with_context(|| format!("failed to create {}", partial.display()))?;
        let mut zip = ZipWriter::new(BufWriter::new(out));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file(MANIFEST, options)?;
        serde_json::to_writer_pretty(&mut zip, &manifest)?;
        add_file(&mut zip, data.database, &snapshot, options)?;
        for f in &files {
            add_file(&mut zip, f, &data.dir.join(f), options)?;
        }
        zip.finish()?.flush()?;
        std::fs::rename(&partial, dest)
            .with_context(|| format!("failed to write {}", dest.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    result?;
    Ok(manifest)
}

/// The file name of the archive written before upgrading from schema `from`.
pub fn upgrade_backup_name(from: i64, now: DateTime<Utc>) -> String {
    format!(
        "lantai-before-upgrade-schema-{from}-{}.zip",
        now.format("%Y%m%d-%H%M%S")
    )
}

/// Back `data` up into `backup_dir` (created if missing), then migrate its database to this
/// build's schema. Only an outdated database is upgraded; any other state is refused unchanged.
/// When a migration step fails, the error names the backup, which is kept.
pub fn upgrade(
    data: &DataFiles,
    backup_dir: &Path,
    app_version: &str,
    now: DateTime<Utc>,
) -> Result<Upgraded> {
    let database = data.database_path();
    let (from, to) = match database_state(&database)? {
        DatabaseState::Outdated { from, to } => (from, to),
        DatabaseState::Ready => bail!("the database is already up to date"),
        state => return Err(NotReady(state).into()),
    };
    std::fs::create_dir_all(backup_dir)
        .with_context(|| format!("failed to create {}", backup_dir.display()))?;
    let backup = backup_dir.join(upgrade_backup_name(from, now));
    export(data, &backup, app_version, now)
        .context("the backup before upgrading failed; nothing was changed")?;
    Store::migrate(&database).with_context(|| {
        format!(
            "the upgrade failed; the backup made before it is at {}",
            backup.display()
        )
    })?;
    Ok(Upgraded { from, to, backup })
}

/// The manifest of an archive, checked to be one this build can restore.
pub fn read_manifest(archive: &Path) -> Result<Manifest> {
    let mut zip = open_archive(archive)?;
    let manifest: Manifest = {
        let entry = zip
            .by_name(MANIFEST)
            .context("this is not a Lantai backup (it has no manifest.json)")?;
        serde_json::from_reader(entry).context("the backup's manifest.json is not valid")?
    };
    if manifest.format > FORMAT {
        bail!(
            "the backup was made by a newer Lantai ({}, archive format {}); update Lantai to restore it",
            manifest.app_version,
            manifest.format
        );
    }
    if manifest.schema_version > SCHEMA_VERSION {
        bail!(
            "the backup's database is at schema {} but this Lantai knows {SCHEMA_VERSION}; update Lantai to restore it",
            manifest.schema_version
        );
    }
    Ok(manifest)
}

/// Replace the database and the configuration files of `data` with the archive's, bringing an
/// older database to this build's schema. What is replaced is first copied to `backups/before-restore-<time>/` in the data directory. Files of
/// `data` that the archive does not hold are left as they are; files in the archive that
/// `data` does not name are ignored.
pub fn restore(data: &DataFiles, archive: &Path, now: DateTime<Utc>) -> Result<Restored> {
    let manifest = read_manifest(archive)?;
    let mut zip = open_archive(archive)?;
    let work = WorkDir::create(data.dir, "restore", now)?;

    // Bring the archived database to this build's schema and check it, away from the live one.
    let incoming = work.path.join(data.database);
    extract(&mut zip, &manifest.database, &incoming)?;
    let version = file_schema_version(&incoming)?;
    if version > SCHEMA_VERSION {
        bail!(
            "the backup's database is at schema {version} but this Lantai knows {SCHEMA_VERSION}"
        );
    }
    {
        Store::migrate(&incoming).context("the backup's database cannot be upgraded")?;
        let store = Store::open(&incoming).context("the backup's database cannot be opened")?;
        store.check_integrity()?;
    }
    let files: Vec<&str> = data
        .files
        .iter()
        .copied()
        .filter(|f| manifest.files.iter().any(|m| m == f))
        .collect();
    for f in &files {
        extract(&mut zip, f, &work.path.join(f))?;
    }

    // Keep what is about to be replaced.
    let previous = data
        .dir
        .join(BACKUPS_DIR)
        .join(format!("before-restore-{}", now.format("%Y%m%d-%H%M%S")));
    std::fs::create_dir_all(&previous)
        .with_context(|| format!("failed to create {}", previous.display()))?;
    let mut live = Store::open(&data.database_path())?;
    live.snapshot_to(&previous.join(data.database))?;
    for f in &files {
        let current = data.dir.join(f);
        if current.is_file() {
            std::fs::copy(&current, previous.join(f))
                .with_context(|| format!("failed to keep a copy of {}", current.display()))?;
        }
    }

    live.replace_with(&incoming)?;
    for f in &files {
        let target = data.dir.join(f);
        let staged = with_suffix(&target, ".restoring");
        std::fs::copy(work.path.join(f), &staged)
            .and_then(|_| std::fs::rename(&staged, &target))
            .with_context(|| format!("failed to restore {}", target.display()))?;
    }
    Ok(Restored { manifest, previous })
}

fn open_archive(path: &Path) -> Result<ZipArchive<BufReader<File>>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    ZipArchive::new(BufReader::new(file)).context("this is not a zip archive")
}

fn add_file<W: Write + io::Seek>(
    zip: &mut ZipWriter<W>,
    name: &str,
    path: &Path,
    options: SimpleFileOptions,
) -> Result<()> {
    let mut src = File::open(path).with_context(|| format!("failed to read {}", path.display()))?;
    let large = src.metadata()?.len() > u64::from(u32::MAX);
    zip.start_file(name, options.large_file(large))?;
    io::copy(&mut src, zip)?;
    Ok(())
}

/// Copy the entry `name` to `dest`. Entries are looked up by the names Lantai expects, never
/// by the paths an archive lists, so nothing is written outside `dest`.
fn extract<R: io::Read + io::Seek>(zip: &mut ZipArchive<R>, name: &str, dest: &Path) -> Result<()> {
    let mut entry = zip
        .by_name(name)
        .with_context(|| format!("the backup has no {name}"))?;
    let mut out =
        File::create(dest).with_context(|| format!("failed to create {}", dest.display()))?;
    io::copy(&mut entry, &mut out).with_context(|| format!("failed to unpack {name}"))?;
    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

/// A scratch folder in the data directory (same file system as the database), removed when
/// dropped.
struct WorkDir {
    path: PathBuf,
}

impl WorkDir {
    fn create(data_dir: &Path, what: &str, now: DateTime<Utc>) -> Result<Self> {
        let path = data_dir.join(format!(
            ".{what}-{}-{}",
            now.format("%Y%m%d-%H%M%S"),
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: &[&str] = &["config.toml", "app-icon"];

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aj-backup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn data(dir: &Path) -> DataFiles<'_> {
        DataFiles {
            dir,
            database: "journal.sqlite",
            files: FILES,
        }
    }

    fn projects(dir: &Path) -> Vec<String> {
        Store::open(&dir.join("journal.sqlite"))
            .unwrap()
            .projects()
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect()
    }

    #[test]
    fn export_then_restore_brings_back_database_and_config() {
        let root = temp_dir("round-trip");
        let dir = root.join("data");
        std::fs::create_dir_all(&dir).unwrap();
        Store::open(&dir.join("journal.sqlite"))
            .unwrap()
            .create_project("kept", None)
            .unwrap();
        std::fs::write(dir.join("config.toml"), "default_model = \"a\"\n").unwrap();
        let now = Utc::now();
        let archive = root.join("backup.zip");
        let manifest = export(&data(&dir), &archive, "0.1.0", now).unwrap();
        assert_eq!(manifest.files, vec!["config.toml".to_string()]);
        assert_eq!(read_manifest(&archive).unwrap(), manifest);

        // Later changes are undone by the restore and kept aside.
        Store::open(&dir.join("journal.sqlite"))
            .unwrap()
            .create_project("later", None)
            .unwrap();
        std::fs::write(dir.join("config.toml"), "default_model = \"b\"\n").unwrap();
        std::fs::write(dir.join("app-icon"), "classic").unwrap();
        let restored = restore(&data(&dir), &archive, now).unwrap();

        assert_eq!(projects(&dir), vec!["kept".to_string()]);
        assert_eq!(
            std::fs::read_to_string(dir.join("config.toml")).unwrap(),
            "default_model = \"a\"\n"
        );
        // Not in the archive, so left alone.
        assert_eq!(
            std::fs::read_to_string(dir.join("app-icon")).unwrap(),
            "classic"
        );
        assert_eq!(
            std::fs::read_to_string(restored.previous.join("config.toml")).unwrap(),
            "default_model = \"b\"\n"
        );
        assert_eq!(projects(&restored.previous), vec!["kept", "later"]);
        // No scratch folders are left behind.
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(leftovers.is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A database at schema 8 with one project, as an older Lantai left it.
    fn old_database(dir: &Path) {
        let conn = rusqlite::Connection::open(dir.join("journal.sqlite")).unwrap();
        conn.execute_batch(&format!(
            "{}\nPRAGMA user_version = 8;\n\
             INSERT INTO logical_projects (id, name, created_at) VALUES (1, 'old', 't');",
            include_str!("../../aj-store/tests/fixtures/schema_v8.sql")
        ))
        .unwrap();
    }

    #[test]
    fn upgrade_backs_up_then_migrates_and_the_backup_restores() {
        let root = temp_dir("upgrade");
        let dir = root.join("data");
        std::fs::create_dir_all(&dir).unwrap();
        old_database(&dir);
        std::fs::write(dir.join("config.toml"), "default_model = \"a\"\n").unwrap();
        let db = dir.join("journal.sqlite");
        assert!(Store::open(&db).is_err(), "opening never migrates");

        let backups = dir.join(BACKUPS_DIR);
        let now = Utc::now();
        let up = upgrade(&data(&dir), &backups, "0.3.0", now).unwrap();
        assert_eq!((up.from, up.to), (8, SCHEMA_VERSION));
        assert_eq!(up.backup, backups.join(upgrade_backup_name(8, now)));
        let manifest = read_manifest(&up.backup).unwrap();
        assert_eq!(manifest.schema_version, 8);
        assert_eq!(manifest.files, vec!["config.toml".to_string()]);
        assert_eq!(projects(&dir), vec!["old".to_string()]);
        let err = upgrade(&data(&dir), &backups, "0.3.0", now).unwrap_err();
        assert!(err.to_string().contains("up to date"), "{err}");

        // The old-schema backup restores into this build, migrated on the way.
        Store::open(&db)
            .unwrap()
            .create_project("later", None)
            .unwrap();
        restore(&data(&dir), &up.backup, now).unwrap();
        assert_eq!(projects(&dir), vec!["old".to_string()]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn upgrade_refuses_a_newer_database_without_a_backup() {
        let root = temp_dir("upgrade-newer");
        let db = root.join("journal.sqlite");
        rusqlite::Connection::open(&db)
            .unwrap()
            .execute_batch(&format!(
                "CREATE TABLE t (x); PRAGMA user_version = {};",
                SCHEMA_VERSION + 1
            ))
            .unwrap();
        let backups = root.join(BACKUPS_DIR);
        let err = upgrade(&data(&root), &backups, "0.3.0", Utc::now()).unwrap_err();
        assert!(err.to_string().contains("update Lantai"), "{err}");
        assert!(!backups.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn refuses_archives_it_cannot_restore() {
        let root = temp_dir("refuse");
        let not_zip = root.join("notes.zip");
        std::fs::write(&not_zip, "hello").unwrap();
        assert!(read_manifest(&not_zip).is_err());

        let newer = root.join("newer.zip");
        let mut zip = ZipWriter::new(File::create(&newer).unwrap());
        zip.start_file(MANIFEST, SimpleFileOptions::default())
            .unwrap();
        let manifest = Manifest {
            format: FORMAT,
            app_version: "9.0.0".into(),
            schema_version: SCHEMA_VERSION + 1,
            created_at: "2026-11-22T00:00:00Z".into(),
            database: "journal.sqlite".into(),
            files: vec![],
        };
        serde_json::to_writer(&mut zip, &manifest).unwrap();
        zip.finish().unwrap();
        let err = read_manifest(&newer).unwrap_err().to_string();
        assert!(err.contains("update Lantai"), "{err}");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
