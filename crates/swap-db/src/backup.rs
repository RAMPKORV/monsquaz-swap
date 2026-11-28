//! Backups: a consistent SQLite snapshot plus the files that it refers to.
//!
//! Layout under the backup root (`<host folder>/monsquaz-swap-backup`):
//!
//! ```text
//! snapshots/<id>/monsquaz.db      VACUUM INTO copy, rollback journal mode
//! snapshots/<id>/manifest.json    written last; its presence marks a backup complete
//! snapshots/.tmp-<id>/            work in progress; never listed, removed on the next run
//! blobs/<xx>/<sha256>             content-addressed, shared by every snapshot
//! writer-lock.json                single-writer heartbeat (see `writer`)
//! ```
//!
//! A snapshot directory appears under its final name only through one rename,
//! after the database, the blobs, and the manifest are on disk and synced. The
//! bot token is never in the database, so it is never in a backup.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::db::{Db, DbError};
use crate::migrations;
use crate::repos::AuditRepo;

/// The manifest format that this build writes and reads.
pub const FORMAT_VERSION: u32 = 1;
/// Transition and manual backups that retention keeps.
pub const DEFAULT_KEEP_TRANSITION: usize = 20;
/// Daily backups that retention keeps.
pub const DEFAULT_KEEP_DAILY: usize = 14;
/// The minimum time between two daily backups, in seconds.
pub const DAILY_INTERVAL_SECS: i64 = 24 * 60 * 60;
/// The folder that the app creates inside the folder that the host selects.
pub const ROOT_NAME: &str = "monsquaz-swap-backup";
/// The typed word that confirms a restore.
pub const RESTORE_CONFIRMATION: &str = "RESTORE";
/// The typed word that confirms a purge.
pub const PURGE_CONFIRMATION: &str = "PURGE";

const SNAPSHOTS: &str = "snapshots";
const BLOBS: &str = "blobs";
const MANIFEST: &str = "manifest.json";
const DB_FILE: &str = "monsquaz.db";
const TMP_PREFIX: &str = ".tmp-";
const PARTIAL_SUFFIX: &str = ".partial";

/// One process writes to a backup folder at a time.
static BACKUP_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Error)]
pub enum BackupError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("the manifest is not readable: {0}")]
    Json(#[from] serde_json::Error),
    #[error("file system error: {0}")]
    Io(#[from] io::Error),
    #[error("the backup id is not valid")]
    InvalidId,
    #[error("the backup does not exist")]
    NotFound,
    #[error("the folder path is not valid UTF-8")]
    NonUtf8Path,
    #[error("the backup contains a symbolic link at {0}")]
    Symlink(String),
    #[error("the backup format {0} is not supported")]
    UnsupportedFormat(u32),
    #[error("the backup schema version {found} is newer than supported {supported}")]
    SchemaTooNew { found: i64, supported: i64 },
    #[error("the database in the backup does not match the manifest hash")]
    DbHashMismatch,
    #[error("the database integrity check failed: {0}")]
    Integrity(String),
    #[error("the manifest does not match the database: {0}")]
    ManifestMismatch(String),
    #[error("file {0} is missing from the backup")]
    BlobMissing(String),
    #[error("file {0} does not match its hash")]
    BlobMismatch(String),
    #[error("file {0} is missing from the local file store")]
    SourceBlobMissing(String),
    #[error("the stored hash is not a SHA-256 hex string")]
    InvalidHash,
    #[error("type {0} to confirm")]
    Confirmation(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupKind {
    Transition,
    Daily,
    Manual,
}

impl BackupKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Transition => "transition",
            Self::Daily => "daily",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEvent {
    pub id: String,
    pub name: String,
    pub state: String,
}

/// The tip of the append-only data at snapshot time. A restore compares it
/// with the current database to detect a stale backup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerTip {
    pub audit_last_id: i64,
    pub audit_last_hash: Option<String>,
    pub seen_pairs: i64,
    pub deliveries: i64,
    pub outbox_last_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    pub app_version: String,
    pub kind: BackupKind,
    pub created_at: i64,
    pub created_at_utc: String,
    pub schema_version: i64,
    pub events: Vec<ManifestEvent>,
    pub db_sha256: String,
    pub db_size: u64,
    pub blobs: Vec<String>,
    pub ledger: LedgerTip,
}

/// Where a backup reads from and writes to.
#[derive(Debug, Clone, Copy)]
pub struct BackupTarget<'a> {
    /// The backup root. Use [`backup_root`] on the folder that the host selects.
    pub root: &'a Path,
    /// The local content-addressed file store (`xx/<sha256>` layout).
    pub blob_root: &'a Path,
    pub app_version: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    pub keep_transition: usize,
    pub keep_daily: usize,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            keep_transition: DEFAULT_KEEP_TRANSITION,
            keep_daily: DEFAULT_KEEP_DAILY,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupOutcome {
    pub id: String,
    pub manifest: Manifest,
    pub copied_blobs: usize,
    pub pruned: PruneReport,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PruneReport {
    pub removed_backups: usize,
    pub removed_blobs: usize,
    pub kept_backups: usize,
    /// False when an unreadable snapshot exists; then no blob is removed.
    pub swept: bool,
}

/// A snapshot directory. `manifest` is `None` when the manifest is missing or
/// unreadable; such a directory is never restored and never pruned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupEntry {
    pub id: String,
    pub manifest: Option<Manifest>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Staleness {
    /// The app has no current event data.
    NoCurrentData,
    /// The current data matches the backup.
    Same,
    /// The current data has entries that the backup does not have.
    BackupOlder { newer_audit_entries: i64 },
    /// The backup has entries that the current data does not have.
    BackupNewer,
    /// The current audit chain does not contain the backup's last entry.
    Diverged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub manifest: Manifest,
    /// The replaced database moved into `pre-restore/<stamp>/` in the data
    /// directory. `None` when there was no database.
    pub pre_restore_dir: Option<PathBuf>,
}

/// The backup root inside the folder that the host selects. A host can select
/// the backup root itself in the restore wizard.
pub fn backup_root(selected: &Path) -> PathBuf {
    if selected.file_name().and_then(|name| name.to_str()) == Some(ROOT_NAME) {
        selected.to_path_buf()
    } else {
        selected.join(ROOT_NAME)
    }
}

/// Take a snapshot, copy the referenced blobs, write the manifest, and apply
/// retention. The connection must not be inside a transaction.
pub fn create_backup(
    db: &Db,
    target: &BackupTarget<'_>,
    kind: BackupKind,
    now: i64,
    retention: Retention,
) -> Result<BackupOutcome, BackupError> {
    let _guard = BACKUP_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    ensure_dir(target.root)?;
    let snapshots = ensure_dir(&target.root.join(SNAPSHOTS))?;
    let blobs = ensure_dir(&target.root.join(BLOBS))?;
    remove_partial_snapshots(&snapshots)?;

    let stamp: String = db.conn().query_row(
        "SELECT strftime('%Y%m%dT%H%M%SZ', ?1, 'unixepoch'), \
                strftime('%Y-%m-%dT%H:%M:%SZ', ?1, 'unixepoch')",
        params![now],
        |row| {
            Ok(format!(
                "{}|{}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?
            ))
        },
    )?;
    let (compact, created_at_utc) = stamp.split_once('|').expect("two columns");
    let id = unique_id(&snapshots, compact, kind);
    let work = snapshots.join(format!("{TMP_PREFIX}{id}"));
    fs::create_dir(&work)?;

    let result = write_snapshot(db, target, &blobs, &work, kind, now, created_at_utc);
    let (manifest, copied_blobs) = match result {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_dir_all(&work);
            return Err(error);
        }
    };
    if let Err(error) = fs::rename(&work, snapshots.join(&id)) {
        let _ = fs::remove_dir_all(&work);
        return Err(error.into());
    }
    sync_dir(&snapshots);
    let pruned = prune_locked(target.root, retention)?;
    Ok(BackupOutcome {
        id,
        manifest,
        copied_blobs,
        pruned,
    })
}

fn write_snapshot(
    db: &Db,
    target: &BackupTarget<'_>,
    blobs: &Path,
    work: &Path,
    kind: BackupKind,
    now: i64,
    created_at_utc: &str,
) -> Result<(Manifest, usize), BackupError> {
    let db_path = work.join(DB_FILE);
    let db_text = db_path.to_str().ok_or(BackupError::NonUtf8Path)?;
    db.conn().execute("VACUUM INTO ?1", params![db_text])?;

    let (schema_version, events, referenced, ledger) = {
        let snapshot = Connection::open(&db_path)?;
        // A plain rollback journal keeps the backup a single file that a
        // read-only open never changes.
        let _mode: String =
            snapshot.query_row("PRAGMA journal_mode = DELETE", [], |row| row.get(0))?;
        integrity(&snapshot)?;
        (
            migrations::schema_version(&snapshot)?,
            list_events(&snapshot)?,
            referenced_blobs(&snapshot)?,
            ledger_tip(&snapshot)?,
        )
    };
    sync_file(&db_path)?;
    let (db_sha256, db_size) = hash_file(&db_path)?;

    let mut copied = 0;
    for hash in &referenced {
        let source = blob_path(target.blob_root, hash)?;
        if !source.is_file() {
            return Err(BackupError::SourceBlobMissing(short(hash)));
        }
        if copy_blob(&source, blobs, hash)? {
            copied += 1;
        }
    }

    let manifest = Manifest {
        format_version: FORMAT_VERSION,
        app_version: target.app_version.to_string(),
        kind,
        created_at: now,
        created_at_utc: created_at_utc.to_string(),
        schema_version,
        events,
        db_sha256,
        db_size,
        blobs: referenced.into_iter().collect(),
        ledger,
    };
    let manifest_path = work.join(MANIFEST);
    let mut file = File::create(&manifest_path)?;
    file.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
    file.sync_all()?;
    sync_dir(work);
    Ok((manifest, copied))
}

/// List snapshot directories, newest first. Work-in-progress directories,
/// symbolic links, and names that are not backup ids are skipped.
pub fn list_backups(root: &Path) -> Result<Vec<BackupEntry>, BackupError> {
    let snapshots = root.join(SNAPSHOTS);
    let mut entries = Vec::new();
    let read = match fs::read_dir(&snapshots) {
        Ok(read) => read,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(entries),
        Err(error) => return Err(error.into()),
    };
    for entry in read {
        let entry = entry?;
        let Some(id) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !is_backup_id(&id) || !fs::symlink_metadata(entry.path())?.is_dir() {
            continue;
        }
        let manifest = read_manifest(&entry.path()).ok();
        entries.push(BackupEntry { id, manifest });
    }
    entries.sort_by(|left, right| {
        let key = |entry: &BackupEntry| entry.manifest.as_ref().map(|m| m.created_at);
        key(right)
            .cmp(&key(left))
            .then_with(|| right.id.cmp(&left.id))
    });
    Ok(entries)
}

/// The time of the newest backup of `kind`, if any.
pub fn last_backup_at(root: &Path, kind: BackupKind) -> Result<Option<i64>, BackupError> {
    Ok(list_backups(root)?
        .into_iter()
        .filter_map(|entry| entry.manifest)
        .filter(|manifest| manifest.kind == kind)
        .map(|manifest| manifest.created_at)
        .max())
}

/// Check everything a restore depends on: the manifest, the database hash, the
/// SQLite integrity check, the manifest against the database, and every blob.
pub fn verify_backup(root: &Path, id: &str) -> Result<Manifest, BackupError> {
    let dir = snapshot_dir(root, id)?;
    let manifest = read_manifest(&dir)?;
    if manifest.format_version != FORMAT_VERSION {
        return Err(BackupError::UnsupportedFormat(manifest.format_version));
    }
    if manifest.schema_version > migrations::LATEST_VERSION {
        return Err(BackupError::SchemaTooNew {
            found: manifest.schema_version,
            supported: migrations::LATEST_VERSION,
        });
    }
    let db_path = dir.join(DB_FILE);
    refuse_symlink(&db_path, DB_FILE)?;
    let (sha, size) = hash_file(&db_path)?;
    if sha != manifest.db_sha256 || size != manifest.db_size {
        return Err(BackupError::DbHashMismatch);
    }
    {
        let snapshot = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        integrity(&snapshot)?;
        if migrations::schema_version(&snapshot)? != manifest.schema_version {
            return Err(BackupError::ManifestMismatch("schema version".to_string()));
        }
        if list_events(&snapshot)? != manifest.events {
            return Err(BackupError::ManifestMismatch("events".to_string()));
        }
        let referenced: Vec<String> = referenced_blobs(&snapshot)?.into_iter().collect();
        if referenced != manifest.blobs {
            return Err(BackupError::ManifestMismatch("file list".to_string()));
        }
    }
    let blobs = root.join(BLOBS);
    refuse_symlink(&blobs, BLOBS)?;
    for hash in &manifest.blobs {
        let path = blob_path(&blobs, hash)?;
        refuse_symlink(path.parent().expect("prefix dir"), &hash[..2])?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(BackupError::Symlink(format!("blobs/{}", short(hash))));
            }
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Err(BackupError::BlobMissing(short(hash))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(BackupError::BlobMissing(short(hash)));
            }
            Err(error) => return Err(error.into()),
        }
        if hash_file(&path)?.0 != *hash {
            return Err(BackupError::BlobMismatch(short(hash)));
        }
    }
    Ok(manifest)
}

/// Compare a backup with the current database.
pub fn compare_with_current(
    manifest: &Manifest,
    current: &Connection,
) -> Result<Staleness, BackupError> {
    let tip = ledger_tip(current)?;
    let has_events: bool =
        current.query_row("SELECT EXISTS (SELECT 1 FROM events)", [], |row| row.get(0))?;
    if !has_events && tip.audit_last_id == 0 {
        return Ok(Staleness::NoCurrentData);
    }
    let backup = &manifest.ledger;
    if backup.audit_last_id > 0 {
        let hash: Option<String> = current
            .query_row(
                "SELECT hash FROM audit_log WHERE id = ?1",
                params![backup.audit_last_id],
                |row| row.get(0),
            )
            .optional()?;
        match hash {
            Some(hash) if Some(&hash) == backup.audit_last_hash.as_ref() => {}
            Some(_) => return Ok(Staleness::Diverged),
            None if tip.audit_last_id > backup.audit_last_id => return Ok(Staleness::Diverged),
            None => return Ok(Staleness::BackupNewer),
        }
    }
    let newer = current.query_row(
        "SELECT COUNT(*) FROM audit_log WHERE id > ?1",
        params![backup.audit_last_id],
        |row| row.get::<_, i64>(0),
    )?;
    if newer > 0 || tip.seen_pairs > backup.seen_pairs || tip.deliveries > backup.deliveries {
        return Ok(Staleness::BackupOlder {
            newer_audit_entries: newer,
        });
    }
    if tip == *backup {
        Ok(Staleness::Same)
    } else {
        Ok(Staleness::BackupNewer)
    }
}

/// Verify a backup, then install it as `<data_dir>/<db_name>` and copy its
/// blobs into `blob_root`. The replaced database files move to
/// `<data_dir>/pre-restore/<stamp>/`. The caller must stop every other
/// connection to the database first.
pub fn restore_backup(
    root: &Path,
    id: &str,
    confirmation: &str,
    data_dir: &Path,
    db_name: &str,
    blob_root: &Path,
    now: i64,
) -> Result<RestoreReport, BackupError> {
    if confirmation.trim() != RESTORE_CONFIRMATION {
        return Err(BackupError::Confirmation(RESTORE_CONFIRMATION));
    }
    let manifest = verify_backup(root, id)?;
    let source = snapshot_dir(root, id)?.join(DB_FILE);
    fs::create_dir_all(data_dir)?;
    let staged = data_dir.join(format!("{db_name}.restore-tmp"));
    let _ = fs::remove_file(&staged);
    let staged_result = (|| -> Result<(), BackupError> {
        fs::copy(&source, &staged)?;
        sync_file(&staged)?;
        if hash_file(&staged)?.0 != manifest.db_sha256 {
            return Err(BackupError::DbHashMismatch);
        }
        let blobs = root.join(BLOBS);
        fs::create_dir_all(blob_root)?;
        for hash in &manifest.blobs {
            copy_blob(&blob_path(&blobs, hash)?, blob_root, hash)?;
        }
        Ok(())
    })();
    if let Err(error) = staged_result {
        let _ = fs::remove_file(&staged);
        return Err(error);
    }

    let target = data_dir.join(db_name);
    let companions = [
        db_name.to_string(),
        format!("{db_name}-wal"),
        format!("{db_name}-shm"),
    ];
    let pre_restore_dir = if target.exists() {
        let parent = data_dir.join("pre-restore");
        fs::create_dir_all(&parent)?;
        let mut dir = parent.join(now.to_string());
        let mut counter = 2;
        while dir.exists() {
            dir = parent.join(format!("{now}-{counter}"));
            counter += 1;
        }
        fs::create_dir(&dir)?;
        for name in &companions {
            let path = data_dir.join(name);
            if path.exists() {
                fs::rename(&path, dir.join(name))?;
            }
        }
        Some(dir)
    } else {
        None
    };
    if let Err(error) = fs::rename(&staged, &target) {
        if let Some(dir) = &pre_restore_dir {
            for name in &companions {
                let _ = fs::rename(dir.join(name), data_dir.join(name));
            }
        }
        return Err(error.into());
    }
    sync_dir(data_dir);

    let db = Db::open(&target)?;
    let detail = serde_json::json!({
        "backupId": id,
        "kind": manifest.kind.as_str(),
        "createdAt": manifest.created_at,
    })
    .to_string();
    for event in &manifest.events {
        AuditRepo::insert(
            db.conn(),
            &event.id,
            now,
            "Host",
            "Backup restored",
            None,
            &detail,
        )?;
    }
    Ok(RestoreReport {
        manifest,
        pre_restore_dir,
    })
}

/// Apply retention: keep the newest `keep_transition` transition and manual
/// backups, the newest `keep_daily` daily backups, and always the newest
/// complete backup. Then remove every blob that no kept manifest refers to.
pub fn prune(root: &Path, retention: Retention) -> Result<PruneReport, BackupError> {
    let _guard = BACKUP_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    prune_locked(root, retention)
}

fn prune_locked(root: &Path, retention: Retention) -> Result<PruneReport, BackupError> {
    let entries = list_backups(root)?;
    let snapshots = root.join(SNAPSHOTS);
    let mut report = PruneReport::default();
    let mut unreadable = false;
    let mut transition = 0;
    let mut daily = 0;
    let mut newest_kept = false;
    let mut marked = BTreeSet::new();
    for entry in entries {
        let Some(manifest) = entry.manifest else {
            unreadable = true;
            report.kept_backups += 1;
            continue;
        };
        let keep = match manifest.kind {
            BackupKind::Daily => {
                daily += 1;
                daily <= retention.keep_daily
            }
            BackupKind::Transition | BackupKind::Manual => {
                transition += 1;
                transition <= retention.keep_transition
            }
        } || !newest_kept;
        newest_kept = true;
        if keep {
            report.kept_backups += 1;
            marked.extend(manifest.blobs);
        } else {
            // Hide the snapshot with one rename, then delete it. A failure in
            // the middle leaves a work directory, not a broken backup.
            let hidden = snapshots.join(format!("{TMP_PREFIX}{}", entry.id));
            fs::rename(snapshots.join(&entry.id), &hidden)?;
            fs::remove_dir_all(&hidden)?;
            report.removed_backups += 1;
        }
    }
    if !unreadable {
        report.removed_blobs = sweep_blobs(&root.join(BLOBS), &marked)?;
        report.swept = true;
    }
    Ok(report)
}

/// Remove every snapshot and every blob from the backup root. The writer lock
/// and any other file in the folder stay.
pub fn purge(root: &Path, confirmation: &str) -> Result<PruneReport, BackupError> {
    if confirmation.trim() != PURGE_CONFIRMATION {
        return Err(BackupError::Confirmation(PURGE_CONFIRMATION));
    }
    let _guard = BACKUP_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut report = PruneReport {
        swept: true,
        ..PruneReport::default()
    };
    report.removed_backups = list_backups(root)?.len();
    for name in [SNAPSHOTS, BLOBS] {
        let path = root.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(BackupError::Symlink(name.to_string()));
            }
            Ok(_) => {
                if name == BLOBS {
                    report.removed_blobs = sweep_blobs(&path, &BTreeSet::new())?;
                }
                fs::remove_dir_all(&path)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(report)
}

fn sweep_blobs(blobs: &Path, marked: &BTreeSet<String>) -> Result<usize, BackupError> {
    let mut removed = 0;
    let read = match fs::read_dir(blobs) {
        Ok(read) => read,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    for prefix in read {
        let prefix = prefix?;
        if !fs::symlink_metadata(prefix.path())?.is_dir() {
            continue;
        }
        for blob in fs::read_dir(prefix.path())? {
            let blob = blob?;
            let metadata = fs::symlink_metadata(blob.path())?;
            if !metadata.is_file() {
                continue;
            }
            let name = blob.file_name();
            let keep = name.to_str().is_some_and(|name| marked.contains(name));
            if !keep {
                fs::remove_file(blob.path())?;
                removed += 1;
            }
        }
    }
    Ok(removed)
}

fn snapshot_dir(root: &Path, id: &str) -> Result<PathBuf, BackupError> {
    if !is_backup_id(id) {
        return Err(BackupError::InvalidId);
    }
    let snapshots = root.join(SNAPSHOTS);
    refuse_symlink(&snapshots, SNAPSHOTS)?;
    let dir = snapshots.join(id);
    match fs::symlink_metadata(&dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(BackupError::Symlink(format!("{SNAPSHOTS}/{id}")))
        }
        Ok(metadata) if metadata.is_dir() => Ok(dir),
        Ok(_) => Err(BackupError::NotFound),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Err(BackupError::NotFound),
        Err(error) => Err(error.into()),
    }
}

fn read_manifest(dir: &Path) -> Result<Manifest, BackupError> {
    let path = dir.join(MANIFEST);
    refuse_symlink(&path, MANIFEST)?;
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(BackupError::NotFound);
        }
        Err(error) => return Err(error.into()),
    };
    Ok(serde_json::from_slice(&bytes)?)
}

/// A backup id is `<UTC stamp>-<kind>` with an optional `-<n>` suffix. Only
/// ASCII letters, digits, and `-` are allowed, so an id cannot leave the
/// snapshots folder.
pub fn is_backup_id(id: &str) -> bool {
    let valid_chars = id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-');
    let Some((stamp, rest)) = id.split_once('-') else {
        return false;
    };
    let kind = rest.split('-').next().unwrap_or("");
    valid_chars
        && id.len() <= 64
        && stamp.len() == 16
        && stamp.as_bytes()[8] == b'T'
        && stamp.ends_with('Z')
        && matches!(kind, "transition" | "daily" | "manual")
}

fn unique_id(snapshots: &Path, compact: &str, kind: BackupKind) -> String {
    let base = format!("{compact}-{}", kind.as_str());
    let mut id = base.clone();
    let mut counter = 2;
    while snapshots.join(&id).exists() || snapshots.join(format!("{TMP_PREFIX}{id}")).exists() {
        id = format!("{base}-{counter}");
        counter += 1;
    }
    id
}

fn remove_partial_snapshots(snapshots: &Path) -> Result<(), BackupError> {
    for entry in fs::read_dir(snapshots)? {
        let entry = entry?;
        let name = entry.file_name();
        if name
            .to_str()
            .is_some_and(|name| name.starts_with(TMP_PREFIX))
            && fs::symlink_metadata(entry.path())?.is_dir()
        {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

fn ensure_dir(path: &Path) -> Result<PathBuf, BackupError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(BackupError::Symlink(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("folder")
                .to_string(),
        )),
        Ok(metadata) if metadata.is_dir() => Ok(path.to_path_buf()),
        Ok(_) => Err(BackupError::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "a file blocks the backup folder",
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            Ok(path.to_path_buf())
        }
        Err(error) => Err(error.into()),
    }
}

fn refuse_symlink(path: &Path, label: &str) -> Result<(), BackupError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(BackupError::Symlink(label.to_string()))
        }
        _ => Ok(()),
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn blob_path(root: &Path, hash: &str) -> Result<PathBuf, BackupError> {
    if !is_sha256(hash) {
        return Err(BackupError::InvalidHash);
    }
    Ok(root.join(&hash[..2]).join(hash))
}

fn short(hash: &str) -> String {
    hash.chars().take(12).collect()
}

/// Copy one blob into a content-addressed folder. Returns `false` when a file
/// of the same size is already there. The copy is hashed while it is written,
/// and a mismatch is refused.
fn copy_blob(source: &Path, dest_root: &Path, hash: &str) -> Result<bool, BackupError> {
    let dest = blob_path(dest_root, hash)?;
    let prefix = dest.parent().expect("prefix dir");
    ensure_dir(prefix)?;
    let source_len = fs::metadata(source)?.len();
    match fs::symlink_metadata(&dest) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(BackupError::Symlink(format!("blobs/{}", short(hash))));
        }
        Ok(metadata) if metadata.is_file() && metadata.len() == source_len => return Ok(false),
        _ => {}
    }
    let partial = prefix.join(format!("{hash}{PARTIAL_SUFFIX}"));
    let result = (|| -> Result<(), BackupError> {
        let mut input = File::open(source)?;
        let mut output = File::create(&partial)?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
            output.write_all(&buffer[..read])?;
        }
        output.sync_all()?;
        if hex::encode(hasher.finalize()) != hash {
            return Err(BackupError::BlobMismatch(short(hash)));
        }
        fs::rename(&partial, &dest)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result.map(|()| true)
}

fn hash_file(path: &Path) -> Result<(String, u64), BackupError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size += read as u64;
        hasher.update(&buffer[..read]);
    }
    Ok((hex::encode(hasher.finalize()), size))
}

fn sync_file(path: &Path) -> Result<(), BackupError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

/// Best effort: some platforms cannot open a directory to sync it.
fn sync_dir(path: &Path) {
    if let Ok(dir) = File::open(path) {
        let _ = dir.sync_all();
    }
}

fn integrity(conn: &Connection) -> Result<(), BackupError> {
    let mut statement = conn.prepare("PRAGMA integrity_check")?;
    let rows: Vec<String> = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    if rows == ["ok"] {
        Ok(())
    } else {
        Err(BackupError::Integrity(rows.join("; ")))
    }
}

fn list_events(conn: &Connection) -> Result<Vec<ManifestEvent>, BackupError> {
    let mut statement = conn.prepare("SELECT id, name, state FROM events ORDER BY id")?;
    let rows = statement
        .query_map([], |row| {
            Ok(ManifestEvent {
                id: row.get(0)?,
                name: row.get(1)?,
                state: row.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Every blob hash that the database refers to.
pub fn referenced_blobs(conn: &Connection) -> Result<BTreeSet<String>, BackupError> {
    let mut statement = conn.prepare(
        "SELECT sha256 FROM files
         UNION SELECT original_sha256 FROM files WHERE original_sha256 IS NOT NULL
         UNION SELECT sha256 FROM packs
         UNION SELECT sha256 FROM late_seeds
         UNION SELECT original_sha256 FROM late_seeds",
    )?;
    let hashes: BTreeSet<String> = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<_, _>>()?;
    if hashes.iter().any(|hash| !is_sha256(hash)) {
        return Err(BackupError::InvalidHash);
    }
    Ok(hashes)
}

fn ledger_tip(conn: &Connection) -> Result<LedgerTip, BackupError> {
    let (audit_last_id, audit_last_hash) = conn
        .query_row(
            "SELECT id, hash FROM audit_log ORDER BY id DESC LIMIT 1",
            [],
            |row| Ok((row.get::<_, i64>(0)?, Some(row.get::<_, String>(1)?))),
        )
        .optional()?
        .unwrap_or((0, None));
    let seen_pairs = conn.query_row("SELECT COUNT(*) FROM seen_pairs", [], |row| row.get(0))?;
    let deliveries = conn.query_row("SELECT COUNT(*) FROM deliveries", [], |row| row.get(0))?;
    let outbox_last_hash = conn
        .query_row(
            "SELECT id, status, updated_at FROM deliveries ORDER BY rowid DESC LIMIT 1",
            [],
            |row| {
                Ok(format!(
                    "{}|{}|{}",
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?
                ))
            },
        )
        .optional()?
        .map(|row| hex::encode(Sha256::digest(row.as_bytes())));
    Ok(LedgerTip {
        audit_last_id,
        audit_last_hash,
        seen_pairs,
        deliveries,
        outbox_last_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_ids_cannot_leave_the_folder() {
        assert!(is_backup_id("20261128T120000Z-daily"));
        assert!(is_backup_id("20261128T120000Z-transition-2"));
        assert!(!is_backup_id("../20261128T120000Z-daily"));
        assert!(!is_backup_id("20261128T120000Z-daily/.."));
        assert!(!is_backup_id(".tmp-20261128T120000Z-daily"));
        assert!(!is_backup_id("20261128T120000Z-other"));
        assert!(!is_backup_id(""));
    }

    #[test]
    fn the_backup_root_is_a_subfolder_unless_selected_directly() {
        let selected = Path::new("/media/usb");
        assert_eq!(backup_root(selected), selected.join(ROOT_NAME));
        let direct = selected.join(ROOT_NAME);
        assert_eq!(backup_root(&direct), direct);
    }
}
