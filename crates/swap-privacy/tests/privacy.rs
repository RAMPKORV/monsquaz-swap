use std::io::{Cursor, Read};

use swap_db::{
    AssignmentRepo, AuditRepo, Db, DeliveryRepo, DmCursorRepo, FileRepo, ParticipantRepo, PlanRepo,
    ProcessedAttachmentRepo, SeenRepo, TrackRepo,
};
use swap_engine::{
    Effect, EventSettings, JoinRequest, PassthroughPreparer, SubmitRequest, claim, create_event,
    expire_offer, generate_schedule, join_with_seed, lock_roster, open_round, reveal, start_event,
    submit,
};
use swap_files::BlobStore;
use swap_privacy::{PrivacyError, delete_participant, export_participant};

const EVENT: &str = "privacy-event";
const T0: i64 = 1_000;
const T1: i64 = 2_000;
const T2: i64 = 2_100;
const T3: i64 = 2_200;
const T4: i64 = 2_300;
const T5: i64 = 2_400;
const T6: i64 = 2_500;
const T7: i64 = 2_600;
const T8: i64 = 2_700;

struct Fixture {
    db: Db,
    store: BlobStore,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new(participants: usize) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = BlobStore::new(directory.path().join("blobs"));
        let db = Db::open_in_memory().unwrap();
        create_event(
            &db,
            EVENT,
            "Privacy Test",
            42,
            &EventSettings::default(),
            T0,
        )
        .unwrap();
        for index in 0..participants {
            join(&db, &store, index);
        }
        lock_roster(&db, EVENT, T1).unwrap();
        generate_schedule(&db, EVENT, T2).unwrap();
        start_event(&db, EVENT, T3).unwrap();
        Self {
            db,
            store,
            _directory: directory,
        }
    }

    fn open(&self, round: u32, now: i64) -> Vec<(String, String, String)> {
        let outcome = open_round(&self.db, EVENT, round, now).unwrap();
        outcome
            .effects
            .into_iter()
            .filter_map(|effect| match effect {
                Effect::Offer {
                    user_id,
                    assignment_id,
                    track_code,
                    ..
                } => Some((user_id, assignment_id, track_code)),
                _ => None,
            })
            .collect()
    }

    fn submit_all(&self, round: u32, now: i64) {
        for (index, (user_id, assignment_id, _track)) in
            self.open(round, now).into_iter().enumerate()
        {
            claim(
                &self.db,
                &self.store,
                EVENT,
                &assignment_id,
                &user_id,
                now + 10,
            )
            .unwrap();
            let bytes = seed_bytes(100 + round as usize * 10 + index);
            submit(
                &self.db,
                &self.store,
                &PassthroughPreparer,
                &SubmitRequest {
                    event_id: EVENT,
                    user_id: &user_id,
                    filename: "edit.it",
                    bytes: &bytes,
                },
                now + 20,
            )
            .unwrap();
        }
    }
}

fn join(db: &Db, store: &BlobStore, index: usize) {
    let user_id = user(index);
    let bytes = seed_bytes(index);
    join_with_seed(
        db,
        store,
        &PassthroughPreparer,
        &JoinRequest {
            event_id: EVENT,
            user_id: &user_id,
            display_name: &format!("Person {index}"),
            timezone: "Europe/Stockholm",
            max_handoffs: 3,
            filename: "seed.it",
            bytes: &bytes,
        },
        T0,
    )
    .unwrap();
}

fn user(index: usize) -> String {
    format!("user-{index:02}")
}

fn seed_bytes(salt: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; 128];
    bytes[..4].copy_from_slice(b"IMPM");
    for (index, byte) in bytes[4..].iter_mut().enumerate() {
        *byte = ((salt * 37 + index * 11) & 0xFF) as u8;
    }
    bytes
}

fn count(db: &Db, sql: &str, value: &str) -> i64 {
    db.conn().query_row(sql, [value], |row| row.get(0)).unwrap()
}

#[test]
fn an_export_contains_only_the_participants_data() {
    let fixture = Fixture::new(3);
    fixture.submit_all(0, T4);
    let round_one = fixture.open(1, T7);
    for (_, assignment_id, _) in &round_one {
        expire_offer(&fixture.db, EVENT, assignment_id, T7 + 86_401).unwrap();
    }
    reveal(&fixture.db, EVENT, T8).unwrap();

    let export = export_participant(&fixture.db, &fixture.store, EVENT, &user(0), T8 + 1).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(export.zip)).unwrap();
    let mut manifest = String::new();
    zip.by_name("participant.json")
        .unwrap()
        .read_to_string(&mut manifest)
        .unwrap();
    assert!(manifest.contains("user-00"));
    assert!(manifest.contains("Person 0"));
    assert!(!manifest.contains("user-01"));
    assert!(!manifest.contains("user-02"));
    assert!(!manifest.contains("Person 1"));
    assert!(!manifest.contains("Person 2"));

    let value: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    let files = value["files"].as_array().unwrap();
    assert_eq!(files.len(), 2, "seed and one submission");
    for info in files {
        let path = info["pathInZip"].as_str().unwrap();
        let mut data = Vec::new();
        zip.by_name(path).unwrap().read_to_end(&mut data).unwrap();
        assert_eq!(
            swap_files::sha256_hex(&data),
            info["sha256"].as_str().unwrap()
        );
    }
    assert_eq!(value["seededTracks"].as_array().unwrap().len(), 1);
    assert_eq!(value["seenPairs"].as_array().unwrap().len(), 1);
}

#[test]
fn a_missing_participant_export_is_refused() {
    let fixture = Fixture::new(2);
    let error = export_participant(&fixture.db, &fixture.store, EVENT, "missing", T4).unwrap_err();
    assert!(matches!(error, PrivacyError::ParticipantMissing(_)));
}

#[test]
fn deletion_replaces_direct_identifiers_and_keeps_a_hash_audit() {
    let fixture = Fixture::new(3);
    fixture.submit_all(0, T4);
    let round_one = fixture.open(1, T7);
    for (_, assignment_id, _) in &round_one {
        expire_offer(&fixture.db, EVENT, assignment_id, T7 + 86_401).unwrap();
    }
    reveal(&fixture.db, EVENT, T8).unwrap();

    let report = delete_participant(&fixture.db, EVENT, &user(0), T8 + 10).unwrap();
    assert!(report.tombstone_id.starts_with("deleted-"));
    assert_eq!(report.seeded_tracks, 1);
    assert_eq!(report.assignments_kept, 2);
    assert_eq!(report.seen_pairs, 1);
    assert_eq!(report.files_kept, 2);

    assert!(
        ParticipantRepo::get(fixture.db.conn(), EVENT, &user(0))
            .unwrap()
            .is_none()
    );
    let tombstone = ParticipantRepo::get(fixture.db.conn(), EVENT, &report.tombstone_id)
        .unwrap()
        .unwrap();
    assert_eq!(tombstone.display_name, "Removed participant");
    assert_eq!(tombstone.timezone, "UTC");

    for (sql, value) in [
        (
            "SELECT COUNT(*) FROM participants WHERE user_id = ?1",
            user(0).as_str(),
        ),
        (
            "SELECT COUNT(*) FROM tracks WHERE owner_id = ?1",
            user(0).as_str(),
        ),
        (
            "SELECT COUNT(*) FROM assignments WHERE participant_id = ?1",
            user(0).as_str(),
        ),
        (
            "SELECT COUNT(*) FROM seen_pairs WHERE participant_id = ?1",
            user(0).as_str(),
        ),
        (
            "SELECT COUNT(*) FROM files WHERE holder_id = ?1",
            user(0).as_str(),
        ),
        (
            "SELECT COUNT(*) FROM plan_assignments WHERE participant_id = ?1",
            user(0).as_str(),
        ),
    ] {
        assert_eq!(count(&fixture.db, sql, value), 0, "{sql}");
    }

    assert_eq!(
        TrackRepo::list_for_event(fixture.db.conn(), EVENT)
            .unwrap()
            .iter()
            .filter(|track| track.owner_id == report.tombstone_id)
            .count(),
        1
    );
    assert_eq!(
        FileRepo::list_for_holder(fixture.db.conn(), EVENT, &report.tombstone_id)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        SeenRepo::list_for_participant(fixture.db.conn(), EVENT, &report.tombstone_id)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        AuditRepo::last_hash(fixture.db.conn(), EVENT).unwrap(),
        Some(report.audit_hash.clone())
    );
    let subject: String = fixture
        .db
        .conn()
        .query_row(
            "SELECT subject FROM audit_log WHERE event_id = ?1 ORDER BY id DESC LIMIT 1",
            [EVENT],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(subject, report.tombstone_id);
    let detail: String = fixture
        .db
        .conn()
        .query_row(
            "SELECT detail_json FROM audit_log WHERE event_id = ?1 ORDER BY id DESC LIMIT 1",
            [EVENT],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!detail.contains(&user(0)));
}

#[test]
fn the_audit_chain_links_entries() {
    let fixture = Fixture::new(2);
    let first = AuditRepo::insert(
        fixture.db.conn(),
        EVENT,
        T0,
        "host",
        "test.first",
        None,
        "{\"n\":1}",
    )
    .unwrap();
    let second = AuditRepo::insert(
        fixture.db.conn(),
        EVENT,
        T0 + 1,
        "host",
        "test.second",
        None,
        "{\"n\":2}",
    )
    .unwrap();
    assert_ne!(first, second);
    let previous: Option<String> = fixture
        .db
        .conn()
        .query_row(
            "SELECT prev_hash FROM audit_log WHERE event_id = ?1 ORDER BY id DESC LIMIT 1",
            [EVENT],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(previous, Some(first));
    assert_eq!(
        AuditRepo::last_hash(fixture.db.conn(), EVENT).unwrap(),
        Some(second)
    );
}

#[test]
fn deletion_is_refused_while_an_assignment_is_active() {
    let fixture = Fixture::new(3);
    let round = fixture.open(0, T4);
    let target = &round[0].0;
    let error = delete_participant(&fixture.db, EVENT, target, T5).unwrap_err();
    assert!(matches!(error, PrivacyError::ActiveAssignment));
    assert!(
        ParticipantRepo::get(fixture.db.conn(), EVENT, target)
            .unwrap()
            .is_some()
    );
    assert!(
        AuditRepo::last_hash(fixture.db.conn(), EVENT)
            .unwrap()
            .is_none()
    );
}

#[test]
fn deletion_removes_future_plan_rows_but_keeps_past_credits() {
    let fixture = Fixture::new(3);
    fixture.submit_all(0, T4);
    let report = delete_participant(&fixture.db, EVENT, &user(0), T6).unwrap();
    assert_eq!(report.plan_assignments_removed, 1);
    assert_eq!(report.assignments_kept, 1);

    let plan = PlanRepo::latest(fixture.db.conn(), EVENT).unwrap().unwrap();
    let rows = PlanRepo::list_assignments(fixture.db.conn(), EVENT, plan.version).unwrap();
    assert_eq!(rows.len(), 5, "six plan rows minus one future row");
    assert!(rows.iter().all(|row| row.participant_id != user(0)));
    assert_eq!(
        AssignmentRepo::list_for_event(fixture.db.conn(), EVENT)
            .unwrap()
            .iter()
            .filter(|assignment| assignment.participant_id == report.tombstone_id)
            .count(),
        1
    );
}

#[test]
fn deletion_removes_delivery_state_and_inbound_cursors() {
    let fixture = Fixture::new(3);
    fixture.submit_all(0, T4);
    let target = user(0);
    DeliveryRepo::enqueue(
        fixture.db.conn(),
        "delivery-1",
        EVENT,
        Some(&target),
        "dm",
        b"payload",
        T5,
    )
    .unwrap();
    DmCursorRepo::upsert(fixture.db.conn(), &target, "dm", Some("m-1"), T5).unwrap();
    ProcessedAttachmentRepo::mark(fixture.db.conn(), &target, "m-1", "a-1", T5).unwrap();

    delete_participant(&fixture.db, EVENT, &target, T6).unwrap();

    assert!(
        DeliveryRepo::get(fixture.db.conn(), "delivery-1")
            .unwrap()
            .is_none()
    );
    assert!(
        DmCursorRepo::get(fixture.db.conn(), &target)
            .unwrap()
            .is_none()
    );
    assert!(
        !ProcessedAttachmentRepo::is_processed(fixture.db.conn(), &target, "m-1", "a-1").unwrap()
    );
}
