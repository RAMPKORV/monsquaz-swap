//! Backup, retention, verification, restore, purge, and the writer guard.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use swap_core::EventState;
use swap_db::backup::{
    self, BackupError, BackupKind, BackupTarget, PURGE_CONFIRMATION, RESTORE_CONFIRMATION,
    Retention,
};
use swap_db::writer::{self, STALE_AFTER_SECS, TAKEOVER_CONFIRMATION, WriterLock, WriterState};
use swap_db::{Db, EventRecord, EventRepo, PackRecord, PackRepo};

const T0: i64 = 1_800_000_000;
const ME: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct Fixture {
    _dir: tempfile::TempDir,
    data: PathBuf,
    blobs: PathBuf,
    root: PathBuf,
    db: Db,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let db = Db::open(data.join("monsquaz.db")).unwrap();
        Self {
            blobs: data.join("files"),
            root: backup::backup_root(&dir.path().join("usb")),
            data,
            db,
            _dir: dir,
        }
    }

    fn target(&self) -> BackupTarget<'_> {
        BackupTarget {
            root: &self.root,
            blob_root: &self.blobs,
            app_version: "0.1.1",
        }
    }

    /// Add an event whose pack refers to one blob. Returns the blob hash.
    fn add_event(&self, id: &str, bytes: &[u8]) -> String {
        let hash = hex::encode(Sha256::digest(bytes));
        let dir = self.blobs.join(&hash[..2]);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(&hash), bytes).unwrap();
        EventRepo::insert(
            self.db.conn(),
            &EventRecord {
                id: id.to_string(),
                name: format!("Swap {id}"),
                state: EventState::Draft,
                seed: 1,
                settings_json: "{}".to_string(),
                created_at: T0,
                updated_at: T0,
            },
        )
        .unwrap();
        PackRepo::insert(
            self.db.conn(),
            &PackRecord {
                id: format!("pack-{id}"),
                event_id: id.to_string(),
                filename: "pack.zip".to_string(),
                sha256: hash.clone(),
                size: bytes.len() as u64,
                storage_path: format!("{}/{hash}", &hash[..2]),
                created_at: T0,
                locked_at: None,
            },
        )
        .unwrap();
        hash
    }

    fn backup(&self, kind: BackupKind, now: i64) -> backup::BackupOutcome {
        self.backup_with(kind, now, Retention::default())
    }

    fn backup_with(
        &self,
        kind: BackupKind,
        now: i64,
        retention: Retention,
    ) -> backup::BackupOutcome {
        backup::create_backup(&self.db, &self.target(), kind, now, retention).unwrap()
    }

    fn backup_blob(&self, hash: &str) -> PathBuf {
        self.root.join("blobs").join(&hash[..2]).join(hash)
    }

    fn snapshot(&self, id: &str) -> PathBuf {
        self.root.join("snapshots").join(id)
    }
}

fn files_under(path: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(read) = fs::read_dir(path) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(files_under(&path));
            } else {
                out.push(path);
            }
        }
    }
    out
}

#[test]
fn a_backup_has_a_snapshot_a_manifest_and_the_referenced_blobs() {
    let fixture = Fixture::new();
    let hash = fixture.add_event("evt-1", b"pack bytes");
    let outcome = fixture.backup(BackupKind::Manual, T0);

    assert_eq!(outcome.copied_blobs, 1);
    assert_eq!(outcome.manifest.format_version, backup::FORMAT_VERSION);
    assert_eq!(outcome.manifest.app_version, "0.1.1");
    assert_eq!(outcome.manifest.created_at_utc, "2027-01-15T08:00:00Z");
    assert_eq!(outcome.manifest.blobs, vec![hash.clone()]);
    assert_eq!(outcome.manifest.events[0].state, "draft");
    assert_eq!(
        outcome.manifest.schema_version,
        swap_db::migrations::LATEST_VERSION
    );
    assert!(
        fixture
            .snapshot(&outcome.id)
            .join("manifest.json")
            .is_file()
    );
    assert_eq!(fs::read(fixture.backup_blob(&hash)).unwrap(), b"pack bytes");
    assert_eq!(
        backup::verify_backup(&fixture.root, &outcome.id).unwrap(),
        outcome.manifest
    );
    // The snapshot is one file in rollback journal mode.
    let names: Vec<_> = fs::read_dir(fixture.snapshot(&outcome.id))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn a_second_backup_does_not_copy_a_blob_again() {
    let fixture = Fixture::new();
    fixture.add_event("evt-1", b"pack bytes");
    fixture.backup(BackupKind::Manual, T0);
    fixture.add_event("evt-2", b"other pack");
    let second = fixture.backup(BackupKind::Manual, T0 + 1);
    assert_eq!(second.copied_blobs, 1);
    assert_eq!(second.manifest.blobs.len(), 2);
}

#[test]
fn two_backups_in_the_same_second_get_different_ids() {
    let fixture = Fixture::new();
    fixture.add_event("evt-1", b"pack bytes");
    let first = fixture.backup(BackupKind::Daily, T0);
    let second = fixture.backup(BackupKind::Daily, T0);
    assert_ne!(first.id, second.id);
    assert_eq!(backup::list_backups(&fixture.root).unwrap().len(), 2);
}

#[test]
fn a_missing_source_blob_fails_the_backup_without_a_visible_snapshot() {
    let fixture = Fixture::new();
    let hash = fixture.add_event("evt-1", b"pack bytes");
    fs::remove_file(fixture.blobs.join(&hash[..2]).join(&hash)).unwrap();
    let error = backup::create_backup(
        &fixture.db,
        &fixture.target(),
        BackupKind::Manual,
        T0,
        Retention::default(),
    )
    .unwrap_err();
    assert!(matches!(error, BackupError::SourceBlobMissing(_)));
    assert!(backup::list_backups(&fixture.root).unwrap().is_empty());
    let leftovers = fs::read_dir(fixture.root.join("snapshots"))
        .unwrap()
        .count();
    assert_eq!(leftovers, 0);
}

#[test]
fn a_partial_backup_is_not_listed_and_the_next_backup_removes_it() {
    let fixture = Fixture::new();
    fixture.add_event("evt-1", b"pack bytes");
    let partial = fixture
        .root
        .join("snapshots")
        .join(".tmp-20270115T080000Z-daily");
    fs::create_dir_all(&partial).unwrap();
    fs::write(partial.join("monsquaz.db"), b"half").unwrap();
    // A snapshot folder without a manifest is listed as incomplete.
    let incomplete = fixture
        .root
        .join("snapshots")
        .join("20270101T000000Z-daily");
    fs::create_dir_all(&incomplete).unwrap();

    let listed = backup::list_backups(&fixture.root).unwrap();
    assert_eq!(listed.len(), 1);
    assert!(listed[0].manifest.is_none());
    assert!(matches!(
        backup::verify_backup(&fixture.root, "20270101T000000Z-daily"),
        Err(BackupError::NotFound)
    ));

    let outcome = fixture.backup(BackupKind::Manual, T0);
    assert!(!partial.exists());
    // An incomplete snapshot stops the blob sweep.
    assert!(!outcome.pruned.swept);
    assert!(incomplete.exists());
}

#[test]
fn retention_keeps_the_newest_backups_of_each_kind_and_sweeps_blobs() {
    let fixture = Fixture::new();
    let old_hash = fixture.add_event("evt-old", b"old pack");
    fixture.backup(BackupKind::Transition, T0);
    // The old blob leaves the database before the next backups.
    fixture
        .db
        .conn()
        .execute("DELETE FROM events WHERE id = 'evt-old'", [])
        .unwrap();
    fixture.add_event("evt-1", b"pack bytes");
    for index in 1..=20 {
        fixture.backup(BackupKind::Transition, T0 + index);
    }
    for index in 0..15 {
        fixture.backup(BackupKind::Daily, T0 + 100 + index);
    }
    let kinds: Vec<_> = backup::list_backups(&fixture.root)
        .unwrap()
        .into_iter()
        .map(|entry| entry.manifest.unwrap())
        .collect();
    let transitions = kinds
        .iter()
        .filter(|m| m.kind == BackupKind::Transition)
        .count();
    let dailies = kinds.iter().filter(|m| m.kind == BackupKind::Daily).count();
    assert_eq!(transitions, 20);
    assert_eq!(dailies, 14);
    assert!(kinds.iter().all(|m| m.created_at != T0));
    assert!(kinds.iter().all(|m| m.created_at != T0 + 100));
    assert!(!fixture.backup_blob(&old_hash).exists());
}

#[test]
fn retention_never_removes_the_newest_complete_backup() {
    let fixture = Fixture::new();
    fixture.add_event("evt-1", b"pack bytes");
    let none = Retention {
        keep_transition: 0,
        keep_daily: 0,
    };
    fixture.backup_with(BackupKind::Daily, T0, none);
    let newest = fixture.backup_with(BackupKind::Transition, T0 + 1, none);
    let listed = backup::list_backups(&fixture.root).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, newest.id);
    backup::verify_backup(&fixture.root, &newest.id).unwrap();
}

#[test]
fn verification_refuses_a_changed_database() {
    let fixture = Fixture::new();
    fixture.add_event("evt-1", b"pack bytes");
    let outcome = fixture.backup(BackupKind::Manual, T0);
    let path = fixture.snapshot(&outcome.id).join("monsquaz.db");
    let mut bytes = fs::read(&path).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(&path, bytes).unwrap();
    assert!(matches!(
        backup::verify_backup(&fixture.root, &outcome.id),
        Err(BackupError::DbHashMismatch)
    ));
}

#[test]
fn verification_refuses_a_changed_or_missing_blob() {
    let fixture = Fixture::new();
    let hash = fixture.add_event("evt-1", b"pack bytes");
    let outcome = fixture.backup(BackupKind::Manual, T0);
    fs::write(fixture.backup_blob(&hash), b"tampered").unwrap();
    assert!(matches!(
        backup::verify_backup(&fixture.root, &outcome.id),
        Err(BackupError::BlobMismatch(_))
    ));
    fs::remove_file(fixture.backup_blob(&hash)).unwrap();
    assert!(matches!(
        backup::verify_backup(&fixture.root, &outcome.id),
        Err(BackupError::BlobMissing(_))
    ));
}

#[cfg(unix)]
#[test]
fn verification_refuses_a_symlinked_blob() {
    let fixture = Fixture::new();
    let hash = fixture.add_event("evt-1", b"pack bytes");
    let outcome = fixture.backup(BackupKind::Manual, T0);
    let blob = fixture.backup_blob(&hash);
    let elsewhere = fixture.data.join("elsewhere");
    fs::rename(&blob, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &blob).unwrap();
    assert!(matches!(
        backup::verify_backup(&fixture.root, &outcome.id),
        Err(BackupError::Symlink(_))
    ));
}

#[test]
fn verification_refuses_a_traversal_id() {
    let fixture = Fixture::new();
    for id in ["../data", "..", "20270115T080000Z-daily/../../x", "/etc"] {
        assert!(matches!(
            backup::verify_backup(&fixture.root, id),
            Err(BackupError::InvalidId)
        ));
    }
}

#[test]
fn a_restore_needs_the_typed_confirmation_and_keeps_the_current_data() {
    let fixture = Fixture::new();
    fixture.add_event("evt-1", b"pack bytes");
    let outcome = fixture.backup(BackupKind::Manual, T0);
    let before = fs::read(fixture.data.join("monsquaz.db")).unwrap();
    let error = backup::restore_backup(
        &fixture.root,
        &outcome.id,
        "restore please",
        &fixture.data,
        "monsquaz.db",
        &fixture.blobs,
        T0 + 1,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        BackupError::Confirmation(RESTORE_CONFIRMATION)
    ));
    assert_eq!(fs::read(fixture.data.join("monsquaz.db")).unwrap(), before);
}

#[test]
fn a_restore_of_a_damaged_backup_keeps_the_current_data() {
    let fixture = Fixture::new();
    let hash = fixture.add_event("evt-1", b"pack bytes");
    let outcome = fixture.backup(BackupKind::Manual, T0);
    fs::write(fixture.backup_blob(&hash), b"tampered").unwrap();
    let before = fs::read(fixture.data.join("monsquaz.db")).unwrap();
    assert!(
        backup::restore_backup(
            &fixture.root,
            &outcome.id,
            RESTORE_CONFIRMATION,
            &fixture.data,
            "monsquaz.db",
            &fixture.blobs,
            T0 + 1,
        )
        .is_err()
    );
    assert_eq!(fs::read(fixture.data.join("monsquaz.db")).unwrap(), before);
    assert!(!fixture.data.join("pre-restore").exists());
}

#[test]
fn a_restore_moves_the_replaced_database_aside_and_records_an_audit_entry() {
    let fixture = Fixture::new();
    fixture.add_event("evt-1", b"pack bytes");
    let outcome = fixture.backup(BackupKind::Manual, T0);
    fixture.add_event("evt-2", b"newer pack");
    let report = backup::restore_backup(
        &fixture.root,
        &outcome.id,
        RESTORE_CONFIRMATION,
        &fixture.data,
        "monsquaz.db",
        &fixture.blobs,
        T0 + 5,
    )
    .unwrap();
    let kept = report.pre_restore_dir.unwrap();
    assert!(kept.join("monsquaz.db").is_file());
    let restored = Db::open(fixture.data.join("monsquaz.db")).unwrap();
    assert!(EventRepo::get(restored.conn(), "evt-2").unwrap().is_none());
    let action: String = restored
        .conn()
        .query_row(
            "SELECT action FROM audit_log WHERE event_id = 'evt-1' ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(action, "Backup restored");
    let previous = Db::open(kept.join("monsquaz.db")).unwrap();
    assert!(EventRepo::get(previous.conn(), "evt-2").unwrap().is_some());
}

#[test]
fn purge_needs_the_typed_confirmation_and_removes_only_backup_data() {
    let fixture = Fixture::new();
    fixture.add_event("evt-1", b"pack bytes");
    fixture.backup(BackupKind::Manual, T0);
    fs::write(fixture.root.join("notes.txt"), b"host file").unwrap();
    assert!(matches!(
        backup::purge(&fixture.root, "purge"),
        Err(BackupError::Confirmation(PURGE_CONFIRMATION))
    ));
    let report = backup::purge(&fixture.root, PURGE_CONFIRMATION).unwrap();
    assert_eq!(report.removed_backups, 1);
    assert_eq!(report.removed_blobs, 1);
    assert!(backup::list_backups(&fixture.root).unwrap().is_empty());
    assert!(!fixture.root.join("blobs").exists());
    assert!(fixture.root.join("notes.txt").is_file());
}

#[test]
fn the_daily_timer_finds_the_last_daily_backup() {
    let fixture = Fixture::new();
    fixture.add_event("evt-1", b"pack bytes");
    assert_eq!(
        backup::last_backup_at(&fixture.root, BackupKind::Daily).unwrap(),
        None
    );
    fixture.backup(BackupKind::Daily, T0);
    fixture.backup(BackupKind::Manual, T0 + 50);
    assert_eq!(
        backup::last_backup_at(&fixture.root, BackupKind::Daily).unwrap(),
        Some(T0)
    );
}

#[test]
fn a_backup_holds_no_planted_secret_from_outside_the_database() {
    let fixture = Fixture::new();
    fixture.add_event("evt-1", b"pack bytes");
    // Split so that the canary is not a secret-shaped literal in the source.
    let token = concat!(
        "MTIzNDU2",
        "Nzg5MDEy",
        "MzQ1Njc4",
        ".planted.",
        "backup-token-canary",
    );
    fs::write(fixture.data.join("host-settings.json"), token).unwrap();
    fs::write(
        fixture.data.join("app.log"),
        format!("Authorization: Bot {token}"),
    )
    .unwrap();
    fixture.backup(BackupKind::Manual, T0);
    for path in files_under(&fixture.root) {
        let bytes = fs::read(&path).unwrap();
        assert!(
            !bytes
                .windows(token.len())
                .any(|window| window == token.as_bytes()),
            "{} holds the token",
            path.display()
        );
    }
}

#[test]
fn a_restored_copy_with_a_fresh_foreign_heartbeat_refuses_until_takeover() {
    let fixture = Fixture::new();
    fixture.add_event("evt-1", b"pack bytes");
    // Machine OTHER runs the event and writes a heartbeat into the database.
    assert_eq!(
        writer::heartbeat(fixture.db.conn(), Some(&fixture.root), OTHER, T0).unwrap(),
        WriterState::Free
    );
    let outcome = fixture.backup(BackupKind::Manual, T0 + 10);
    backup::restore_backup(
        &fixture.root,
        &outcome.id,
        RESTORE_CONFIRMATION,
        &fixture.data,
        "monsquaz.db",
        &fixture.blobs,
        T0 + 20,
    )
    .unwrap();
    let restored = Db::open(fixture.data.join("monsquaz.db")).unwrap();

    // This machine (ME) starts within the stale window: the guard refuses, even
    // without the folder copy.
    let state = writer::check(restored.conn(), None, ME, T0 + 60).unwrap();
    assert!(
        matches!(state, WriterState::Foreign(WriterLock { ref machine_id, .. }) if machine_id == OTHER)
    );
    // A heartbeat does not overwrite a fresh foreign lock.
    assert!(matches!(
        writer::heartbeat(restored.conn(), Some(&fixture.root), ME, T0 + 60).unwrap(),
        WriterState::Foreign(_)
    ));
    assert!(matches!(
        writer::take_over(
            restored.conn(),
            Some(&fixture.root),
            ME,
            T0 + 61,
            "take over"
        ),
        Err(writer::WriterError::Confirmation)
    ));
    writer::take_over(
        restored.conn(),
        Some(&fixture.root),
        ME,
        T0 + 61,
        TAKEOVER_CONFIRMATION,
    )
    .unwrap();
    assert_eq!(
        writer::check(restored.conn(), Some(&fixture.root), ME, T0 + 62).unwrap(),
        WriterState::Own
    );
    let action: String = restored
        .conn()
        .query_row(
            "SELECT action FROM audit_log ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(action, "Writer takeover");

    // The other machine now sees a fresh foreign lock in the shared folder and
    // must stop at its next heartbeat.
    assert!(matches!(
        writer::heartbeat(fixture.db.conn(), Some(&fixture.root), OTHER, T0 + 70).unwrap(),
        WriterState::Foreign(_)
    ));
}

#[test]
fn a_stale_foreign_heartbeat_does_not_block() {
    let fixture = Fixture::new();
    writer::heartbeat(fixture.db.conn(), Some(&fixture.root), OTHER, T0).unwrap();
    assert!(matches!(
        writer::check(
            fixture.db.conn(),
            Some(&fixture.root),
            ME,
            T0 + STALE_AFTER_SECS - 1
        )
        .unwrap(),
        WriterState::Foreign(_)
    ));
    assert_eq!(
        writer::heartbeat(
            fixture.db.conn(),
            Some(&fixture.root),
            ME,
            T0 + STALE_AFTER_SECS
        )
        .unwrap(),
        WriterState::Free
    );
    assert_eq!(
        writer::check(
            fixture.db.conn(),
            Some(&fixture.root),
            ME,
            T0 + STALE_AFTER_SECS
        )
        .unwrap(),
        WriterState::Own
    );
}
