//! Backups before transitions and the restore drill (MSW-115).

use std::path::Path;
use std::sync::{Arc, Mutex};

use swap_core::EventState;
use swap_db::backup::{self, BackupKind, BackupTarget, RESTORE_CONFIRMATION, Retention, Staleness};
use swap_db::{Connection, Db, DbError, EventRepo, MetaRepo, PackRecord, PackRepo};
use swap_engine::{
    EngineError, EventSettings, JoinRequest, PassthroughPreparer, SubmitRequest, claim,
    create_event_with_secret, generate_schedule, join_with_seed, lock_roster, open_round,
    open_seed_round, reveal, start_event, submit,
};
use swap_files::BlobStore;

const EVENT: &str = "evt-1";
const SECRET: &str = "0000000000000000000000000000002a";
const T0: i64 = 1_800_000_000;

fn seed_bytes(tag: u8) -> Vec<u8> {
    let mut bytes = b"IMPM".to_vec();
    bytes.push(tag);
    bytes.resize(64, tag);
    bytes
}

fn draft_with_pack(db: &Db, store: &BlobStore) {
    create_event_with_secret(
        db,
        EVENT,
        "Drill Swap",
        42,
        &EventSettings::default(),
        Some(SECRET),
        T0,
    )
    .unwrap();
    MetaRepo::set(db.conn(), "host.current_event", EVENT).unwrap();
    let blob = store.store(b"IMPMdrill-pack").unwrap();
    PackRepo::insert(
        db.conn(),
        &PackRecord {
            id: "pack-1".to_string(),
            event_id: EVENT.to_string(),
            filename: "pack.zip".to_string(),
            sha256: blob.sha256.clone(),
            size: blob.size,
            storage_path: blob.relative_path.clone(),
            created_at: T0,
            locked_at: None,
        },
    )
    .unwrap();
}

fn join_all(db: &Db, store: &BlobStore, count: usize) {
    for index in 0..count {
        let user_id = format!("user-{index:02}");
        join_with_seed(
            db,
            store,
            &PassthroughPreparer,
            &JoinRequest {
                event_id: EVENT,
                user_id: &user_id,
                display_name: &format!("User {index}"),
                timezone: "Europe/Stockholm",
                max_handoffs: 20,
                filename: "seed.it",
                bytes: &seed_bytes(index as u8),
            },
            T0 + 10,
        )
        .unwrap();
    }
}

fn state(db: &Db) -> EventState {
    EventRepo::get(db.conn(), EVENT).unwrap().unwrap().state
}

#[test]
fn every_state_transition_runs_the_hook_before_the_change() {
    let dir = tempfile::tempdir().unwrap();
    let store = BlobStore::new(dir.path().join("files"));
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let record = seen.clone();
    let db = Db::open(dir.path().join("monsquaz.db"))
        .unwrap()
        .with_transition_hook(Box::new(move |db, event_id| {
            let state = EventRepo::get(db.conn(), event_id).unwrap().unwrap().state;
            record.lock().unwrap().push(format!("{state:?}"));
            Ok(())
        }));
    draft_with_pack(&db, &store);
    open_seed_round(&db, EVENT, T0 + 1_000_000, None, T0).unwrap();
    join_all(&db, &store, 3);
    lock_roster(&db, EVENT, T0 + 100).unwrap();
    generate_schedule(&db, EVENT, T0 + 110).unwrap();
    start_event(&db, EVENT, T0 + 120).unwrap();
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["Draft", "Signup", "ScheduleReview"]
    );
}

#[test]
fn a_failed_backup_refuses_the_transition() {
    let dir = tempfile::tempdir().unwrap();
    let store = BlobStore::new(dir.path().join("files"));
    let db = Db::open(dir.path().join("monsquaz.db")).unwrap();
    draft_with_pack(&db, &store);
    open_seed_round(&db, EVENT, T0 + 1_000_000, None, T0).unwrap();
    join_all(&db, &store, 3);
    let db = db.with_transition_hook(Box::new(|_, _| Err("disk full".to_string())));
    let error = lock_roster(&db, EVENT, T0 + 100).unwrap_err();
    assert!(matches!(error, EngineError::Db(DbError::BackupFailed(_))));
    assert_eq!(state(&db), EventState::Signup);
}

fn hook(root: &Path, blob_root: &Path) -> swap_db::TransitionHook {
    let root = root.to_path_buf();
    let blob_root = blob_root.to_path_buf();
    Box::new(move |db, _event| {
        let now = EventRepo::get(db.conn(), EVENT)
            .map_err(|error| error.to_string())?
            .map_or(T0, |event| event.updated_at + 1);
        backup::create_backup(
            db,
            &BackupTarget {
                root: &root,
                blob_root: &blob_root,
                app_version: "test",
            },
            BackupKind::Transition,
            now,
            Retention::default(),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
    })
}

#[test]
fn the_backup_before_a_transition_holds_the_state_before_the_change() {
    let dir = tempfile::tempdir().unwrap();
    let blob_root = dir.path().join("files");
    let root = dir.path().join("backups");
    let store = BlobStore::new(&blob_root);
    let db = Db::open(dir.path().join("monsquaz.db")).unwrap();
    draft_with_pack(&db, &store);
    open_seed_round(&db, EVENT, T0 + 1_000_000, None, T0).unwrap();
    join_all(&db, &store, 3);
    let db = db.with_transition_hook(hook(&root, &blob_root));
    lock_roster(&db, EVENT, T0 + 100).unwrap();

    let backups = backup::list_backups(&root).unwrap();
    assert_eq!(backups.len(), 1);
    let manifest = backup::verify_backup(&root, &backups[0].id).unwrap();
    assert_eq!(manifest.kind, BackupKind::Transition);
    assert_eq!(manifest.events.len(), 1);
    assert_eq!(manifest.events[0].state, "signup");
    // One pack plus three seeds.
    assert_eq!(manifest.blobs.len(), 4);
    assert_eq!(state(&db), EventState::ScheduleReview);
}

/// Every row of every table, in a stable order, without the restore audit rows.
fn dump(conn: &Connection) -> Vec<String> {
    let mut tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    tables.retain(|name| name != "sqlite_sequence");
    let mut rows = Vec::new();
    for table in tables {
        let filter = if table == "audit_log" {
            " WHERE action != 'Backup restored'"
        } else {
            ""
        };
        let mut statement = conn
            .prepare(&format!("SELECT * FROM {table}{filter} ORDER BY rowid"))
            .unwrap();
        let columns = statement.column_count();
        let mut query = statement.query([]).unwrap();
        while let Some(row) = query.next().unwrap() {
            let values: Vec<String> = (0..columns)
                .map(|index| format!("{:?}", row.get_ref(index).unwrap()))
                .collect();
            rows.push(format!("{table}: {}", values.join(" | ")));
        }
    }
    rows
}

#[test]
fn restore_drill_brings_back_the_seen_ledger_files_and_state() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let blob_root = data.join("files");
    let root = backup::backup_root(&dir.path().join("usb"));
    let store = BlobStore::new(&blob_root);
    std::fs::create_dir_all(&data).unwrap();
    let db = Db::open(data.join("monsquaz.db")).unwrap();

    // Run an event through two rounds with claims and submissions.
    draft_with_pack(&db, &store);
    open_seed_round(&db, EVENT, T0 + 1_000_000, None, T0).unwrap();
    join_all(&db, &store, 4);
    lock_roster(&db, EVENT, T0 + 100).unwrap();
    generate_schedule(&db, EVENT, T0 + 110).unwrap();
    start_event(&db, EVENT, T0 + 120).unwrap();
    for round in 0..2 {
        let opened = open_round(&db, EVENT, round, T0 + 200 + i64::from(round) * 100).unwrap();
        for effect in &opened.effects {
            let swap_engine::Effect::Offer {
                user_id,
                assignment_id,
                ..
            } = effect
            else {
                continue;
            };
            let at = T0 + 210 + i64::from(round) * 100;
            claim(&db, &store, EVENT, assignment_id, user_id, at).unwrap();
            submit(
                &db,
                &store,
                &PassthroughPreparer,
                &SubmitRequest {
                    event_id: EVENT,
                    user_id,
                    filename: "edit.it",
                    bytes: &seed_bytes(100 + round as u8 * 10 + user_id.len() as u8),
                },
                at + 5,
            )
            .unwrap();
        }
    }
    assert_eq!(state(&db), EventState::Active);

    let before = dump(db.conn());
    let outcome = backup::create_backup(
        &db,
        &BackupTarget {
            root: &root,
            blob_root: &blob_root,
            app_version: "test",
        },
        BackupKind::Manual,
        T0 + 1_000,
        Retention::default(),
    )
    .unwrap();
    let blobs: Vec<(String, Vec<u8>)> = outcome
        .manifest
        .blobs
        .iter()
        .map(|hash| (hash.clone(), store.read(hash).unwrap()))
        .collect();
    assert!(blobs.len() > 5);
    drop(db);

    // Wipe the app data folder.
    std::fs::remove_dir_all(&data).unwrap();

    let report = backup::restore_backup(
        &root,
        &outcome.id,
        RESTORE_CONFIRMATION,
        &data,
        "monsquaz.db",
        &blob_root,
        T0 + 2_000,
    )
    .unwrap();
    assert!(report.pre_restore_dir.is_none());

    let restored = Db::open(data.join("monsquaz.db")).unwrap();
    assert_eq!(dump(restored.conn()), before);
    assert_eq!(state(&restored), EventState::Active);
    for (hash, bytes) in &blobs {
        assert_eq!(&BlobStore::new(&blob_root).read(hash).unwrap(), bytes);
    }
    let restore_audits: i64 = restored
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'Backup restored'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(restore_audits, 1);

    // The restored event continues: the last round opens and the reveal runs.
    let last = open_round(&restored, EVENT, 2, T0 + 3_000).unwrap();
    for effect in &last.effects {
        if let swap_engine::Effect::Offer { assignment_id, .. } = effect {
            swap_engine::expire_offer(&restored, EVENT, assignment_id, T0 + 10_000_000).unwrap();
        }
    }
    reveal(&restored, EVENT, T0 + 10_000_001).unwrap();
    assert_eq!(state(&restored), EventState::Reveal);
}

#[test]
fn a_backup_from_before_new_activity_is_reported_as_older() {
    let dir = tempfile::tempdir().unwrap();
    let blob_root = dir.path().join("files");
    let root = dir.path().join("backups");
    let store = BlobStore::new(&blob_root);
    let db = Db::open(dir.path().join("monsquaz.db")).unwrap();
    draft_with_pack(&db, &store);
    let target = BackupTarget {
        root: &root,
        blob_root: &blob_root,
        app_version: "test",
    };
    let first =
        backup::create_backup(&db, &target, BackupKind::Manual, T0, Retention::default()).unwrap();
    assert_eq!(
        backup::compare_with_current(&first.manifest, db.conn()).unwrap(),
        Staleness::Same
    );
    open_seed_round(&db, EVENT, T0 + 1_000_000, None, T0 + 1).unwrap();
    assert!(matches!(
        backup::compare_with_current(&first.manifest, db.conn()).unwrap(),
        Staleness::BackupOlder {
            newer_audit_entries: 1
        }
    ));
    let empty = Db::open_in_memory().unwrap();
    assert_eq!(
        backup::compare_with_current(&first.manifest, empty.conn()).unwrap(),
        Staleness::NoCurrentData
    );
}
