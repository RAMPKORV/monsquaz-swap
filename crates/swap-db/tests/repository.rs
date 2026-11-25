use swap_core::{AssignmentStatus, EventState, Membership, Readiness, Role, TrackState};
use swap_db::{
    AssignmentRecord, AssignmentRepo, Db, EventRecord, EventRepo, FileRecord, FileRepo,
    ParticipantRecord, ParticipantRepo, SeenRepo, TrackRecord, TrackRepo, migrations,
};

fn event() -> EventRecord {
    EventRecord {
        id: "evt-1".to_string(),
        name: "Test Swap".to_string(),
        state: EventState::Draft,
        seed: 42,
        settings_json: "{}".to_string(),
        created_at: 1_000,
        updated_at: 1_000,
    }
}

fn participant(
    event_id: &str,
    user_id: &str,
    role: Role,
    membership: Membership,
    readiness: Readiness,
) -> ParticipantRecord {
    ParticipantRecord {
        event_id: event_id.to_string(),
        user_id: user_id.to_string(),
        display_name: user_id.to_string(),
        role,
        membership,
        readiness,
        timezone: "Europe/Stockholm".to_string(),
        max_handoffs: 3,
        created_at: 1_000,
        updated_at: 1_000,
    }
}

fn track(event_id: &str, code: &str, owner_id: &str) -> TrackRecord {
    TrackRecord {
        event_id: event_id.to_string(),
        code: code.to_string(),
        owner_id: owner_id.to_string(),
        state: TrackState::Pool,
        created_at: 1_000,
    }
}

fn assignment(
    id: &str,
    event_id: &str,
    track_code: &str,
    participant_id: &str,
    status: AssignmentStatus,
) -> AssignmentRecord {
    AssignmentRecord {
        id: id.to_string(),
        event_id: event_id.to_string(),
        track_code: track_code.to_string(),
        participant_id: participant_id.to_string(),
        round: 1,
        status,
        offered_at: 1_000,
        claimed_at: None,
        due_at: None,
        submitted_at: None,
        created_at: 1_000,
        updated_at: 1_000,
    }
}

fn seed_event(db: &Db) {
    EventRepo::insert(db.conn(), &event()).unwrap();
    ParticipantRepo::upsert(
        db.conn(),
        &participant(
            "evt-1",
            "alice",
            Role::Participant,
            Membership::Active,
            Readiness::Ready,
        ),
    )
    .unwrap();
    TrackRepo::insert(db.conn(), &track("evt-1", "T01", "alice")).unwrap();
}

#[test]
fn migrations_apply_and_enable_foreign_keys() {
    let db = Db::open_in_memory().unwrap();
    assert_eq!(db.schema_version().unwrap(), migrations::LATEST_VERSION);
    let foreign_keys: i64 = db
        .conn()
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap();
    assert_eq!(foreign_keys, 1);
}

#[test]
fn migrations_are_idempotent() {
    let mut db = Db::open_in_memory().unwrap();
    migrations::migrate(db.conn_mut()).unwrap();
    assert_eq!(db.schema_version().unwrap(), migrations::LATEST_VERSION);
}

#[test]
fn an_event_round_trips() {
    let db = Db::open_in_memory().unwrap();
    EventRepo::insert(db.conn(), &event()).unwrap();
    let loaded = EventRepo::get(db.conn(), "evt-1").unwrap().unwrap();
    assert_eq!(loaded, event());
    assert!(EventRepo::set_state(db.conn(), "evt-1", EventState::Signup, 2_000).unwrap());
    let loaded = EventRepo::get(db.conn(), "evt-1").unwrap().unwrap();
    assert_eq!(loaded.state, EventState::Signup);
    assert_eq!(loaded.updated_at, 2_000);
}

#[test]
fn a_participant_can_be_upserted() {
    let db = Db::open_in_memory().unwrap();
    seed_event(&db);
    let mut alice = ParticipantRepo::get(db.conn(), "evt-1", "alice")
        .unwrap()
        .unwrap();
    assert_eq!(alice.readiness, Readiness::Ready);
    alice.readiness = Readiness::Paused;
    alice.updated_at = 2_000;
    ParticipantRepo::upsert(db.conn(), &alice).unwrap();
    let alice = ParticipantRepo::get(db.conn(), "evt-1", "alice")
        .unwrap()
        .unwrap();
    assert_eq!(alice.readiness, Readiness::Paused);
    assert_eq!(
        ParticipantRepo::list_for_event(db.conn(), "evt-1")
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn seen_pairs_are_idempotent() {
    let db = Db::open_in_memory().unwrap();
    seed_event(&db);
    assert!(SeenRepo::record(db.conn(), "evt-1", "T01", "bob", "claim", 1_000).unwrap());
    assert!(!SeenRepo::record(db.conn(), "evt-1", "T01", "bob", "submit", 1_001).unwrap());
    assert!(SeenRepo::has(db.conn(), "evt-1", "T01", "bob").unwrap());
}

#[test]
fn only_one_active_assignment_per_track_and_person() {
    let db = Db::open_in_memory().unwrap();
    seed_event(&db);
    ParticipantRepo::upsert(
        db.conn(),
        &participant(
            "evt-1",
            "bob",
            Role::FillIn,
            Membership::Active,
            Readiness::Ready,
        ),
    )
    .unwrap();
    TrackRepo::insert(db.conn(), &track("evt-1", "T02", "bob")).unwrap();

    AssignmentRepo::insert(
        db.conn(),
        &assignment("a1", "evt-1", "T01", "alice", AssignmentStatus::Offered),
    )
    .unwrap();
    assert!(
        AssignmentRepo::insert(
            db.conn(),
            &assignment("a2", "evt-1", "T01", "bob", AssignmentStatus::Offered),
        )
        .is_err()
    );
    assert!(
        AssignmentRepo::insert(
            db.conn(),
            &assignment("a3", "evt-1", "T02", "alice", AssignmentStatus::Offered),
        )
        .is_err()
    );

    assert!(
        AssignmentRepo::update_status(db.conn(), "a1", AssignmentStatus::SkippedBeforeClaim, 2_000)
            .unwrap()
    );
    AssignmentRepo::insert(
        db.conn(),
        &assignment("a4", "evt-1", "T01", "alice", AssignmentStatus::Claimed),
    )
    .unwrap();
}

#[test]
fn foreign_keys_are_enforced() {
    let db = Db::open_in_memory().unwrap();
    assert!(TrackRepo::insert(db.conn(), &track("missing", "T01", "alice")).is_err());
}

#[test]
fn files_round_trip() {
    let db = Db::open_in_memory().unwrap();
    seed_event(&db);
    let file = FileRecord {
        id: "f1".to_string(),
        event_id: "evt-1".to_string(),
        track_code: "T01".to_string(),
        version: 1,
        holder_id: "alice".to_string(),
        sha256: "abc123".to_string(),
        size: 1_024,
        extension: "it".to_string(),
        storage_path: "Round1/R1.T01.alice.it".to_string(),
        original_sha256: Some("def456".to_string()),
        original_storage_path: Some("vault/def456".to_string()),
        audio_verified: true,
        scrub_summary_json: "{\"song_name\":true}".to_string(),
        created_at: 1_000,
    };
    FileRepo::insert(db.conn(), &file).unwrap();
    let files = FileRepo::list_for_track(db.conn(), "evt-1", "T01").unwrap();
    assert_eq!(files, vec![file]);
}

#[test]
fn a_file_database_survives_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("swap.db");
    {
        let db = Db::open(&path).unwrap();
        EventRepo::insert(db.conn(), &event()).unwrap();
    }
    let db = Db::open(&path).unwrap();
    let loaded = EventRepo::get(db.conn(), "evt-1").unwrap().unwrap();
    assert_eq!(loaded, event());
}
