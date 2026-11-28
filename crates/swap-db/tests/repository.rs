use swap_core::{AssignmentStatus, EventState, Membership, Readiness, Role, TrackState};
use swap_db::{
    AssignmentRecord, AssignmentRepo, Connection, Db, DbError, EventRecord, EventRepo, FileRecord,
    FileRepo, ParticipantRecord, ParticipantRepo, RestoreRepo, SeenRepo, TrackRecord, TrackRepo,
    migrations,
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
        extension_secs: 0,
        claim_deadline_at: None,
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
fn migration_seven_upgrades_a_version_six_database() {
    let mut connection = Connection::open_in_memory().unwrap();
    for (index, sql) in migrations::MIGRATIONS.iter().take(6).enumerate() {
        connection.execute_batch(sql).unwrap();
        connection
            .pragma_update(None, "user_version", index as i64 + 1)
            .unwrap();
    }
    connection
        .execute(
            "INSERT INTO events (id, name, state, seed, settings_json, created_at, updated_at)
             VALUES ('evt-1', 'Old Swap', 'draft', 42, '{}', 1, 1)",
            [],
        )
        .unwrap();

    migrations::migrate(&mut connection).unwrap();

    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, migrations::LATEST_VERSION);
    let name: String = connection
        .query_row("SELECT name FROM events WHERE id = 'evt-1'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(name, "Old Swap");
    for table in [
        "assignment_deadlines",
        "host_decisions",
        "track_titles",
        "track_comments",
    ] {
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "{table} is missing");
    }
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
fn a_seen_pair_retry_for_the_same_assignment_is_idempotent() {
    let db = Db::open_in_memory().unwrap();
    seed_event(&db);
    assert!(SeenRepo::record(db.conn(), "evt-1", "T01", "bob", "a1", "claim", 1_000).unwrap());
    assert!(!SeenRepo::record(db.conn(), "evt-1", "T01", "bob", "a1", "submit", 1_001).unwrap());
    assert!(SeenRepo::has(db.conn(), "evt-1", "T01", "bob").unwrap());
    let pairs = SeenRepo::list_for_event(db.conn(), "evt-1").unwrap();
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].reason, "claim");
}

#[test]
fn a_seen_pair_from_a_second_assignment_fails_loudly() {
    let db = Db::open_in_memory().unwrap();
    seed_event(&db);
    SeenRepo::record(db.conn(), "evt-1", "T01", "bob", "a1", "claim", 1_000).unwrap();
    let repeat = SeenRepo::record(db.conn(), "evt-1", "T01", "bob", "a2", "claim", 1_001);
    assert!(
        matches!(&repeat, Err(DbError::SeenPairRepeat { track, participant })
            if track == "T01" && participant == "bob"),
        "got {repeat:?}"
    );
}

#[test]
fn raw_sql_cannot_insert_a_duplicate_seen_pair() {
    let db = Db::open_in_memory().unwrap();
    seed_event(&db);
    let insert = "INSERT INTO seen_pairs
        (event_id, track_code, participant_id, reason, first_seen_at, assignment_id)
        VALUES ('evt-1', 'T01', 'bob', 'claim', 1, ?1)";
    db.conn().execute(insert, ["a1"]).unwrap();
    assert!(db.conn().execute(insert, ["a2"]).is_err());
}

#[test]
fn raw_sql_cannot_assign_an_owner_their_own_track() {
    let db = Db::open_in_memory().unwrap();
    seed_event(&db);
    let own = db.conn().execute(
        "INSERT INTO assignments (id, event_id, track_code, participant_id, round, status,
            offered_at, created_at, updated_at)
         VALUES ('a1', 'evt-1', 'T01', 'alice', 0, 'submitted', 1, 1, 1)",
        [],
    );
    assert!(own.unwrap_err().to_string().contains("own track"));
    ParticipantRepo::upsert(
        db.conn(),
        &participant(
            "evt-1",
            "bob",
            Role::Participant,
            Membership::Active,
            Readiness::Ready,
        ),
    )
    .unwrap();
    AssignmentRepo::insert(
        db.conn(),
        &assignment("a2", "evt-1", "T01", "bob", AssignmentStatus::Offered),
    )
    .unwrap();
    let moved = db.conn().execute(
        "UPDATE assignments SET participant_id = 'alice' WHERE id = 'a2'",
        [],
    );
    assert!(moved.unwrap_err().to_string().contains("own track"));
}

#[test]
fn raw_sql_cannot_plan_an_owner_their_own_track() {
    let db = Db::open_in_memory().unwrap();
    seed_event(&db);
    db.conn()
        .execute(
            "INSERT INTO plan_versions (event_id, version, seed, rounds, settings_json,
                commitment, committed_at)
             VALUES ('evt-1', 1, 1, 1, '{}', 'c', 1)",
            [],
        )
        .unwrap();
    let own = db.conn().execute(
        "INSERT INTO plan_assignments (event_id, plan_version, round, track_code, participant_id)
         VALUES ('evt-1', 1, 0, 'T01', 'alice')",
        [],
    );
    assert!(own.unwrap_err().to_string().contains("own track"));
}

#[test]
fn raw_sql_cannot_create_a_second_active_holder() {
    let db = Db::open_in_memory().unwrap();
    seed_event(&db);
    for user in ["bob", "carol"] {
        ParticipantRepo::upsert(
            db.conn(),
            &participant(
                "evt-1",
                user,
                Role::Participant,
                Membership::Active,
                Readiness::Ready,
            ),
        )
        .unwrap();
    }
    TrackRepo::insert(db.conn(), &track("evt-1", "T02", "carol")).unwrap();
    let insert = "INSERT INTO assignments (id, event_id, track_code, participant_id, round,
            status, offered_at, created_at, updated_at)
         VALUES (?1, 'evt-1', ?2, ?3, 0, ?4, 1, 1, 1)";
    for status in ["offered", "claimed", "overdue"] {
        db.conn().execute("DELETE FROM assignments", []).unwrap();
        db.conn()
            .execute(insert, ["a1", "T01", "bob", status])
            .unwrap();
        assert!(
            db.conn()
                .execute(insert, ["a2", "T01", "carol", "offered"])
                .is_err(),
            "a second active holder of one track was accepted ({status})"
        );
        assert!(
            db.conn()
                .execute(insert, ["a3", "T02", "bob", "claimed"])
                .is_err(),
            "a second active track for one person was accepted ({status})"
        );
    }
    db.conn().execute("DELETE FROM assignments", []).unwrap();
    for (index, status) in ["submitted", "expired", "skipped_before_claim"]
        .iter()
        .enumerate()
    {
        db.conn()
            .execute(insert, [&format!("done-{index}"), "T01", "bob", status])
            .unwrap();
    }
    db.conn()
        .execute(insert, ["a4", "T01", "bob", "offered"])
        .unwrap();
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
    TrackRepo::insert(db.conn(), &track("evt-1", "T03", "carol")).unwrap();

    AssignmentRepo::insert(
        db.conn(),
        &assignment("a1", "evt-1", "T03", "alice", AssignmentStatus::Offered),
    )
    .unwrap();
    assert!(
        AssignmentRepo::insert(
            db.conn(),
            &assignment("a2", "evt-1", "T03", "bob", AssignmentStatus::Offered),
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
        &assignment("a4", "evt-1", "T03", "alice", AssignmentStatus::Claimed),
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

#[test]
fn migration_eleven_links_existing_seen_pairs_to_their_claim() {
    let mut connection = Connection::open_in_memory().unwrap();
    for (index, sql) in migrations::MIGRATIONS.iter().take(10).enumerate() {
        connection.execute_batch(sql).unwrap();
        connection
            .pragma_update(None, "user_version", index as i64 + 1)
            .unwrap();
    }
    connection
        .execute_batch(
            "INSERT INTO events (id, name, state, seed, settings_json, created_at, updated_at)
               VALUES ('evt-1', 'Old Swap', 'active', 42, '{}', 1, 1);
             INSERT INTO participants (event_id, user_id, display_name, role, membership,
               readiness, timezone, created_at, updated_at)
               VALUES ('evt-1', 'bob', 'Bob', 'participant', 'active', 'ready', 'UTC', 1, 1);
             INSERT INTO tracks (event_id, code, owner_id, state, created_at)
               VALUES ('evt-1', 'T01', 'alice', 'submitted', 1);
             INSERT INTO assignments (id, event_id, track_code, participant_id, round, status,
               offered_at, claimed_at, created_at, updated_at)
               VALUES ('a-old', 'evt-1', 'T01', 'bob', 0, 'submitted', 1, 2, 1, 3);
             INSERT INTO seen_pairs (event_id, track_code, participant_id, reason, first_seen_at)
               VALUES ('evt-1', 'T01', 'bob', 'claim', 2);",
        )
        .unwrap();

    migrations::migrate(&mut connection).unwrap();

    assert!(!SeenRepo::record(&connection, "evt-1", "T01", "bob", "a-old", "submit", 4).unwrap());
    assert!(SeenRepo::record(&connection, "evt-1", "T01", "bob", "a-new", "claim", 5).is_err());
}

#[test]
fn the_latest_schema_keeps_a_separate_claim_deadline() {
    let db = Db::open_in_memory().unwrap();
    seed_event(&db);
    ParticipantRepo::upsert(
        db.conn(),
        &participant(
            "evt-1",
            "bob",
            Role::Participant,
            Membership::Active,
            Readiness::Ready,
        ),
    )
    .unwrap();
    let mut row = assignment("a1", "evt-1", "T01", "bob", AssignmentStatus::Offered);
    row.claim_deadline_at = Some(1_100);
    AssignmentRepo::insert(db.conn(), &row).unwrap();
    let loaded = AssignmentRepo::get(db.conn(), "a1").unwrap().unwrap();
    assert_eq!(loaded.claim_deadline_at, Some(1_100));
    assert_eq!(loaded.due_at, None);
}

#[test]
fn a_restore_can_be_asked_again_in_the_same_second() {
    let db = Db::open_in_memory().unwrap();
    seed_event(&db);
    RestoreRepo::insert(db.conn(), "evt-1", "alice", 1_000).unwrap();
    assert!(RestoreRepo::answer(db.conn(), "evt-1", "alice", "declined", 1_001).unwrap());
    RestoreRepo::insert(db.conn(), "evt-1", "alice", 1_000).unwrap();
    assert!(
        RestoreRepo::pending(db.conn(), "evt-1", "alice")
            .unwrap()
            .is_some()
    );
}
