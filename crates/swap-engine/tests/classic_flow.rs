use swap_core::{AssignmentStatus, EventState, Membership, Role, TrackState, album_order};
use swap_db::{
    AssignmentRepo, AuditRepo, Db, DeadlineRepo, DeliveryRepo, DmCursorRepo, EventRepo, FileRepo,
    HostDecisionRepo, MetaRepo, ParticipantRepo, PlanRepo, SeenRepo, TrackRepo,
};
use swap_engine::{
    DeliveryPreparer, Effect, EngineError, EventSettings, JoinRequest, PassthroughPreparer,
    PreparedBytes, SubmitRequest, add_track_comment, claim, create_event, dispatch,
    dispatch_pending, event_status, expire_offer, generate_schedule, join_with_seed, lock_roster,
    open_round, redact_track_comment, reveal, reveal_package, reveal_with_channel, set_track_title,
    set_track_title_for_user, signup, skip, start_event, submit,
};
use swap_files::{BlobStore, FileError, FileKind, PrepareError, sha256_hex};

const EVENT: &str = "evt-1";
const T0: i64 = 1_000;
const T1: i64 = 2_000;
const T2: i64 = 2_100;
const T3: i64 = 2_200;
const T4: i64 = 2_300;
const T5: i64 = 2_400;
const T6: i64 = 2_500;
const T7: i64 = 2_600;

struct Fixture {
    db: Db,
    store: BlobStore,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new(seeded: usize, signups: usize) -> Self {
        Self::with_settings(seeded, signups, EventSettings::default())
    }

    fn with_settings(seeded: usize, signups: usize, settings: EventSettings) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = BlobStore::new(directory.path().join("blobs"));
        let db = Db::open_in_memory().unwrap();
        create_event(&db, EVENT, "Test Swap", 42, &settings, T0).unwrap();
        MetaRepo::set(db.conn(), "host.current_event", EVENT).unwrap();
        for index in 0..seeded {
            join(&db, &store, index);
        }
        for index in 0..signups {
            signup(
                &db,
                EVENT,
                &user(seeded + index),
                &format!("Signup {index}"),
                "Europe/Stockholm",
                3,
                T0,
            )
            .unwrap();
        }
        Self {
            db,
            store,
            _directory: directory,
        }
    }

    fn lock_and_schedule(&self) -> Vec<swap_db::PlanAssignmentRecord> {
        lock_roster(&self.db, EVENT, T1).unwrap();
        generate_schedule(&self.db, EVENT, T2).unwrap().assignments
    }

    fn start(&self) {
        start_event(&self.db, EVENT, T3).unwrap();
    }

    fn open(&self, round: u32) -> Vec<(String, String, String)> {
        let outcome = open_round(&self.db, EVENT, round, T4).unwrap();
        offers(&outcome.effects)
    }
}

fn user(index: usize) -> String {
    format!("user-{index:02}")
}

fn seed_bytes(tag: u8) -> Vec<u8> {
    let mut bytes = b"IMPM".to_vec();
    bytes.push(tag);
    bytes.resize(64, tag);
    bytes
}

fn xm_bytes() -> Vec<u8> {
    let mut bytes = b"Extended Module: ".to_vec();
    bytes.resize(64, 0);
    bytes
}

fn join(db: &Db, store: &BlobStore, index: usize) {
    let user_id = user(index);
    let bytes = seed_bytes(index as u8);
    let outcome = join_with_seed(
        db,
        store,
        &PassthroughPreparer,
        &JoinRequest {
            event_id: EVENT,
            user_id: &user_id,
            display_name: &format!("User {index}"),
            timezone: "Europe/Stockholm",
            max_handoffs: 3,
            filename: "seed.it",
            bytes: &bytes,
        },
        T0,
    )
    .unwrap();
    assert_eq!(outcome.effects.len(), 1);
}

fn offers(effects: &[Effect]) -> Vec<(String, String, String)> {
    effects
        .iter()
        .map(|effect| match effect {
            Effect::Offer {
                user_id,
                assignment_id,
                track_code,
                ..
            } => (user_id.clone(), assignment_id.clone(), track_code.clone()),
            other => panic!("expected an offer, got {other:?}"),
        })
        .collect()
}

struct ScrubbingPreparer;

impl DeliveryPreparer for ScrubbingPreparer {
    fn prepare(
        &self,
        bytes: &[u8],
        _kind: FileKind,
        _extension: &str,
    ) -> Result<PreparedBytes, PrepareError> {
        let delivery = b"scrubbed module".to_vec();
        Ok(PreparedBytes {
            delivery_sha256: sha256_hex(&delivery),
            original_sha256: sha256_hex(bytes),
            delivery,
            audio_verified: true,
            scrub_summary_json: "{\"song_name\":true}".to_string(),
        })
    }
}

struct RejectingPreparer;

impl DeliveryPreparer for RejectingPreparer {
    fn prepare(
        &self,
        _bytes: &[u8],
        _kind: FileKind,
        _extension: &str,
    ) -> Result<PreparedBytes, PrepareError> {
        Err(PrepareError::AudioChanged)
    }
}

#[test]
fn a_prepared_seed_keeps_the_original_and_delivers_the_scrub() {
    let directory = tempfile::tempdir().unwrap();
    let store = BlobStore::new(directory.path().join("blobs"));
    let db = Db::open_in_memory().unwrap();
    create_event(
        &db,
        EVENT,
        "Prepared Swap",
        42,
        &EventSettings::default(),
        T0,
    )
    .unwrap();
    for index in 0..2 {
        let user_id = user(index);
        let bytes = seed_bytes(index as u8);
        join_with_seed(
            &db,
            &store,
            &ScrubbingPreparer,
            &JoinRequest {
                event_id: EVENT,
                user_id: &user_id,
                display_name: &format!("User {index}"),
                timezone: "Europe/Stockholm",
                max_handoffs: 3,
                filename: "seed.it",
                bytes: &bytes,
            },
            T0,
        )
        .unwrap();
    }
    lock_roster(&db, EVENT, T1).unwrap();
    generate_schedule(&db, EVENT, T2).unwrap();
    start_event(&db, EVENT, T3).unwrap();
    let round = open_round(&db, EVENT, 0, T4).unwrap();
    let offers = offers(&round.effects);
    let (user_id, assignment_id, track_code) = &offers[0];
    let claimed = claim(&db, &store, EVENT, assignment_id, user_id, T5).unwrap();
    let file_bytes = match &claimed.effects[0] {
        Effect::TrackFile { bytes, .. } => bytes.clone(),
        other => panic!("expected a file, got {other:?}"),
    };
    assert_eq!(file_bytes, b"scrubbed module");

    let files = FileRepo::list_for_track(db.conn(), EVENT, track_code).unwrap();
    assert_eq!(files.len(), 1);
    let file = &files[0];
    assert_eq!(file.sha256, sha256_hex(b"scrubbed module"));
    assert!(file.audio_verified);
    assert!(file.original_sha256.is_some());
    let original_sha = file.original_sha256.clone().unwrap();
    assert_ne!(original_sha, file.sha256);
    let original = store.read(&original_sha).unwrap();
    assert_eq!(
        original,
        seed_bytes(if user_id == &user(0) { 1 } else { 0 })
    );
}

#[test]
fn a_failed_preparation_does_not_store_a_seed() {
    let fixture = Fixture::new(1, 0);
    let user_id = user(1);
    let bytes = seed_bytes(9);
    let error = join_with_seed(
        &fixture.db,
        &fixture.store,
        &RejectingPreparer,
        &JoinRequest {
            event_id: EVENT,
            user_id: &user_id,
            display_name: "User 1",
            timezone: "Europe/Stockholm",
            max_handoffs: 3,
            filename: "seed.it",
            bytes: &bytes,
        },
        T0,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        EngineError::Prepare(PrepareError::AudioChanged)
    ));
    assert!(
        ParticipantRepo::get(fixture.db.conn(), EVENT, &user_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        TrackRepo::list_for_event(fixture.db.conn(), EVENT)
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn the_classic_loop_reaches_a_reveal() {
    let fixture = Fixture::new(4, 0);
    let lock = lock_roster(&fixture.db, EVENT, T1).unwrap();
    assert_eq!(lock.active, 4);
    assert_eq!(lock.fill_ins, 0);

    let schedule = generate_schedule(&fixture.db, EVENT, T2).unwrap();
    assert_eq!(schedule.version, 1);
    assert_eq!(schedule.rounds, 3);
    assert_eq!(schedule.assignments.len(), 12);
    assert_eq!(schedule.commitment.len(), 64);

    fixture.start();
    let round_zero = fixture.open(0);
    assert_eq!(round_zero.len(), 4);

    for (user_id, assignment_id, track_code) in &round_zero {
        let outcome = claim(
            &fixture.db,
            &fixture.store,
            EVENT,
            assignment_id,
            user_id,
            T5,
        )
        .unwrap();
        assert_eq!(outcome.filename, format!("{track_code}.it"));
        assert_eq!(outcome.effects.len(), 1);
        match &outcome.effects[0] {
            Effect::TrackFile {
                filename, bytes, ..
            } => {
                assert_eq!(filename, &format!("{track_code}.it"));
                assert_eq!(bytes.len(), 64);
                assert!(!filename.contains("user-"));
                assert!(!filename.contains("v1"));
            }
            other => panic!("expected a track file, got {other:?}"),
        }
        assert!(SeenRepo::has(fixture.db.conn(), EVENT, track_code, user_id).unwrap());
    }

    for (index, (user_id, _, _)) in round_zero.iter().enumerate() {
        let bytes = seed_bytes(100 + index as u8);
        let outcome = submit(
            &fixture.db,
            &fixture.store,
            &PassthroughPreparer,
            &SubmitRequest {
                event_id: EVENT,
                user_id,
                filename: "edit.it",
                bytes: &bytes,
            },
            T6,
        )
        .unwrap();
        assert_eq!(outcome.version, 2);
        assert!(matches!(outcome.effects[0], Effect::Dm { .. }));
    }

    let early = reveal(&fixture.db, EVENT, T6).unwrap_err();
    assert!(matches!(early, EngineError::PlanNotFinished { round: 1 }));

    let round_one = fixture.open(1);
    assert_eq!(round_one.len(), 4);
    for (_, assignment_id, _) in &round_one {
        expire_offer(&fixture.db, EVENT, assignment_id, T4 + 86_401).unwrap();
    }

    let round_two = fixture.open(2);
    assert_eq!(round_two.len(), 4);
    let blocked = reveal_with_channel(&fixture.db, EVENT, Some("reveal-channel"), T6).unwrap_err();
    assert!(matches!(blocked, EngineError::PendingAssignments { .. }));
    for (_, assignment_id, _) in &round_two {
        expire_offer(&fixture.db, EVENT, assignment_id, T4 + 86_401).unwrap();
    }

    let original_codes = TrackRepo::list_for_event(fixture.db.conn(), EVENT)
        .unwrap()
        .into_iter()
        .map(|track| track.code)
        .collect::<Vec<_>>();
    let expected_order = album_order(&original_codes, 42);
    let revealed = reveal_with_channel(&fixture.db, EVENT, Some("reveal-channel"), T6).unwrap();
    assert_eq!(revealed.tracks.len(), 4);
    assert_eq!(
        revealed
            .tracks
            .iter()
            .map(|track| track.track_code.clone())
            .collect::<Vec<_>>(),
        expected_order
    );
    assert!(!revealed.effects.is_empty());
    for track in &revealed.tracks {
        assert_eq!(track.entries.len(), 1);
        assert_eq!(track.entries[0].status, AssignmentStatus::Submitted);
        assert_eq!(track.entries[0].round, 0);
    }
    let status = event_status(&fixture.db, EVENT).unwrap();
    assert_eq!(status.state, EventState::Reveal);
    assert_eq!(status.rounds, 3);
    assert_eq!(status.submitted_assignments, 4);

    let discord = swap_discord::FakeDiscord::new();
    dispatch_pending(&fixture.db, &fixture.store, &discord, T7, 100)
        .await
        .unwrap();
    let messages = discord.channel_messages();
    assert!(messages.iter().any(|message| {
        message.message.content.contains("User 0")
            && message.message.content.contains("S")
            && !message.message.content.to_lowercase().contains("sha")
    }));
}

#[test]
fn only_a_claim_counts_as_seen_for_an_offer() {
    let fixture = Fixture::new(4, 0);
    fixture.lock_and_schedule();
    fixture.start();
    let round = fixture.open(0);
    assert_eq!(round.len(), 4);

    let (first_user, first_id, first_track) = &round[0];
    claim(&fixture.db, &fixture.store, EVENT, first_id, first_user, T5).unwrap();
    assert!(SeenRepo::has(fixture.db.conn(), EVENT, first_track, first_user).unwrap());
    let skipped = skip(&fixture.db, EVENT, first_user, T6).unwrap();
    assert_eq!(skipped.status, AssignmentStatus::SkippedAfterClaim);

    let (second_user, _, second_track) = &round[1];
    let skipped = skip(&fixture.db, EVENT, second_user, T6).unwrap();
    assert_eq!(skipped.status, AssignmentStatus::SkippedBeforeClaim);
    assert!(!SeenRepo::has(fixture.db.conn(), EVENT, second_track, second_user).unwrap());

    let (third_user, third_id, third_track) = &round[2];
    expire_offer(&fixture.db, EVENT, third_id, T4 + 86_401).unwrap();
    assert!(!SeenRepo::has(fixture.db.conn(), EVENT, third_track, third_user).unwrap());
}

#[test]
fn an_expiry_returns_the_track_and_blocks_a_late_claim() {
    let fixture = Fixture::new(2, 0);
    fixture.lock_and_schedule();
    fixture.start();
    let round = fixture.open(0);
    let (user_id, assignment_id, track_code) = &round[0];

    assert!(matches!(
        expire_offer(&fixture.db, EVENT, assignment_id, T4).unwrap_err(),
        EngineError::OfferNotDue
    ));
    expire_offer(&fixture.db, EVENT, assignment_id, T4 + 86_401).unwrap();
    let track = TrackRepo::get(fixture.db.conn(), EVENT, track_code)
        .unwrap()
        .unwrap();
    assert_eq!(track.state, TrackState::Pool);
    let late = claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        assignment_id,
        user_id,
        T4 + 86_402,
    )
    .unwrap_err();
    assert!(matches!(
        late,
        EngineError::WrongAssignmentState(AssignmentStatus::Expired)
    ));
}

#[test]
fn a_resubmission_keeps_every_version_and_a_format_change_is_refused() {
    let fixture = Fixture::new(3, 0);
    fixture.lock_and_schedule();
    fixture.start();
    let round = fixture.open(0);
    let (user_id, assignment_id, track_code) = &round[0];
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        assignment_id,
        user_id,
        T5,
    )
    .unwrap();

    let first = submit(
        &fixture.db,
        &fixture.store,
        &PassthroughPreparer,
        &SubmitRequest {
            event_id: EVENT,
            user_id,
            filename: "first.it",
            bytes: &seed_bytes(10),
        },
        T6,
    )
    .unwrap();
    assert_eq!(first.version, 2);
    let second = submit(
        &fixture.db,
        &fixture.store,
        &PassthroughPreparer,
        &SubmitRequest {
            event_id: EVENT,
            user_id,
            filename: "second.it",
            bytes: &seed_bytes(11),
        },
        T6 + 1,
    )
    .unwrap();
    assert_eq!(second.version, 3);

    let files = FileRepo::list_for_track(fixture.db.conn(), EVENT, track_code).unwrap();
    assert_eq!(files.len(), 3);
    assert_eq!(files[2].version, 3);
    assert_eq!(files[2].sha256, second.sha256);

    let change = submit(
        &fixture.db,
        &fixture.store,
        &PassthroughPreparer,
        &SubmitRequest {
            event_id: EVENT,
            user_id,
            filename: "changed.xm",
            bytes: &xm_bytes(),
        },
        T6 + 2,
    )
    .unwrap_err();
    assert!(matches!(
        change,
        EngineError::FormatChange { current, submitted }
            if current == "it" && submitted == "xm"
    ));

    let rejected = submit(
        &fixture.db,
        &fixture.store,
        &PassthroughPreparer,
        &SubmitRequest {
            event_id: EVENT,
            user_id,
            filename: "malware.exe",
            bytes: b"MZ",
        },
        T6 + 3,
    )
    .unwrap_err();
    assert!(matches!(
        rejected,
        EngineError::File(FileError::UnsupportedExtension(_))
    ));

    let (other_user, _, _) = &round[1];
    let none = submit(
        &fixture.db,
        &fixture.store,
        &PassthroughPreparer,
        &SubmitRequest {
            event_id: EVENT,
            user_id: other_user,
            filename: "edit.it",
            bytes: &seed_bytes(12),
        },
        T6 + 4,
    )
    .unwrap_err();
    assert!(matches!(none, EngineError::NoActiveAssignment));
}

#[test]
fn an_unseeded_signup_becomes_a_fill_in() {
    let fixture = Fixture::new(2, 1);
    let lock = lock_roster(&fixture.db, EVENT, T1).unwrap();
    assert_eq!(lock.active, 2);
    assert_eq!(lock.fill_ins, 1);

    let fill_in = ParticipantRepo::get(fixture.db.conn(), EVENT, &user(2))
        .unwrap()
        .unwrap();
    assert_eq!(fill_in.role, Role::FillIn);
    assert_eq!(fill_in.membership, Membership::Active);

    let assignments = generate_schedule(&fixture.db, EVENT, T2)
        .unwrap()
        .assignments;
    assert!(assignments.len() == 2);
    assert!(
        assignments
            .iter()
            .all(|assignment| assignment.participant_id != user(2))
    );
}

#[test]
fn a_second_seed_is_refused() {
    let fixture = Fixture::new(2, 0);
    let user_id = user(0);
    let error = join_with_seed(
        &fixture.db,
        &fixture.store,
        &PassthroughPreparer,
        &JoinRequest {
            event_id: EVENT,
            user_id: &user_id,
            display_name: "User 0",
            timezone: "Europe/Stockholm",
            max_handoffs: 3,
            filename: "seed.it",
            bytes: &seed_bytes(9),
        },
        T0,
    )
    .unwrap_err();
    assert!(matches!(error, EngineError::AlreadySeeded));
}

#[test]
fn too_many_rounds_are_refused() {
    let settings = EventSettings {
        rounds: Some(9),
        ..EventSettings::default()
    };
    let fixture = Fixture::with_settings(4, 0, settings);
    lock_roster(&fixture.db, EVENT, T1).unwrap();
    let error = generate_schedule(&fixture.db, EVENT, T2).unwrap_err();
    assert!(matches!(
        error,
        EngineError::Schedule(swap_core::ScheduleError::TooManyRounds {
            requested: 9,
            max: 3
        })
    ));
}

#[test]
fn a_replacement_draft_uses_a_new_seed() {
    let fixture = Fixture::new(8, 0);
    lock_roster(&fixture.db, EVENT, T1).unwrap();
    let first = generate_schedule(&fixture.db, EVENT, T2).unwrap();
    let second = generate_schedule(&fixture.db, EVENT, T3).unwrap();
    assert_eq!(first.version, 1);
    assert_eq!(second.version, 2);
    assert_ne!(first.assignments, second.assignments);
    assert_ne!(first.commitment, second.commitment);
}

#[test]
fn a_committed_plan_is_stored_and_the_state_advances() {
    let fixture = Fixture::new(4, 0);
    fixture.lock_and_schedule();
    let plan = PlanRepo::latest(fixture.db.conn(), EVENT).unwrap().unwrap();
    assert_eq!(plan.version, 1);
    assert_eq!(plan.rounds, 3);
    let assignments = PlanRepo::list_assignments(fixture.db.conn(), EVENT, plan.version).unwrap();
    assert_eq!(assignments.len(), 12);

    let status = event_status(&fixture.db, EVENT).unwrap();
    assert_eq!(status.state, EventState::ScheduleReview);
    assert!(status.committed);
    assert_eq!(status.rounds, 3);

    fixture.start();
    assert_eq!(
        event_status(&fixture.db, EVENT).unwrap().state,
        EventState::Active
    );
}

#[tokio::test]
async fn offers_and_claims_cross_the_discord_port() {
    use swap_discord::FakeDiscord;

    let fixture = Fixture::new(3, 0);
    fixture.lock_and_schedule();
    fixture.start();
    let discord = FakeDiscord::new();
    let round = open_round(&fixture.db, EVENT, 0, T4).unwrap();
    let results = dispatch(&discord, &round.effects).await;
    assert_eq!(results.len(), 3);
    assert!(results.iter().all(Result::is_ok));
    assert_eq!(discord.sent_dms().len(), 3);

    let (user_id, assignment_id, track_code) = offers(&round.effects)[0].clone();
    let claimed = claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        T5,
    )
    .unwrap();
    let results = dispatch(&discord, &claimed.effects).await;
    assert!(results[0].is_ok());
    let files = discord.sent_files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].user_id.0, user_id);
    assert_eq!(files[0].file.filename, format!("{track_code}.it"));
}

#[test]
fn rounds_open_in_order_and_only_once() {
    let fixture = Fixture::new(3, 0);
    fixture.lock_and_schedule();
    fixture.start();

    let out_of_order = open_round(&fixture.db, EVENT, 1, T4).unwrap_err();
    assert!(matches!(
        out_of_order,
        EngineError::RoundOutOfOrder {
            expected: 0,
            requested: 1
        }
    ));

    fixture.open(0);
    let duplicate = open_round(&fixture.db, EVENT, 0, T4).unwrap_err();
    assert!(matches!(
        duplicate,
        EngineError::RoundAlreadyOpened { round: 0 }
    ));
}

#[test]
fn an_assignment_is_visible_without_an_owner_or_title() {
    let fixture = Fixture::new(3, 0);
    let assignments = fixture.lock_and_schedule();
    fixture.start();
    let round = fixture.open(0);
    let assignment = AssignmentRepo::get(fixture.db.conn(), &round[0].1)
        .unwrap()
        .unwrap();
    assert_eq!(assignment.round, 0);
    assert_eq!(assignment.status, AssignmentStatus::Offered);
    assert!(!assignments.is_empty());
}

#[tokio::test]
async fn the_outbox_delivers_offers_and_claimed_files() {
    use swap_discord::FakeDiscord;

    let fixture = Fixture::new(4, 0);
    fixture.lock_and_schedule();
    fixture.start();
    let offers = fixture.open(0);
    assert_eq!(DeliveryRepo::count_pending(fixture.db.conn()).unwrap(), 8);

    let discord = FakeDiscord::new();
    let report = dispatch_pending(&fixture.db, &fixture.store, &discord, T5, 100)
        .await
        .unwrap();
    assert_eq!(report.sent, 8);
    assert_eq!(DeliveryRepo::count_pending(fixture.db.conn()).unwrap(), 0);
    assert_eq!(discord.sent_dms().len(), 8);

    let (user_id, assignment_id, _) = offers[0].clone();
    let claimed = claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        T4,
    )
    .unwrap();
    let expected_bytes = match &claimed.effects[0] {
        Effect::TrackFile { bytes, .. } => bytes.clone(),
        other => panic!("expected a file, got {other:?}"),
    };
    assert_eq!(DeliveryRepo::count_pending(fixture.db.conn()).unwrap(), 1);
    let report = dispatch_pending(&fixture.db, &fixture.store, &discord, T5, 100)
        .await
        .unwrap();
    assert_eq!(report.sent, 1);
    assert_eq!(discord.sent_files().len(), 1);
    assert_eq!(discord.sent_files()[0].file.bytes, expected_bytes);
}

#[tokio::test]
async fn a_restart_preserves_the_event_and_pending_outbox() {
    use swap_discord::FakeDiscord;

    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("event.db");
    let store = BlobStore::new(directory.path().join("blobs"));
    {
        let db = Db::open(&database).unwrap();
        create_event(&db, EVENT, "Test Swap", 42, &EventSettings::default(), T0).unwrap();
        for index in 0..3 {
            join(&db, &store, index);
        }
        lock_roster(&db, EVENT, T1).unwrap();
        generate_schedule(&db, EVENT, T2).unwrap();
        start_event(&db, EVENT, T3).unwrap();
        open_round(&db, EVENT, 0, T4).unwrap();
        assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 6);
        assert_eq!(
            EventRepo::get(db.conn(), EVENT).unwrap().unwrap().state,
            EventState::Active
        );
    }

    let db = Db::open(&database).unwrap();
    let integrity: String = db
        .conn()
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
    assert_eq!(
        EventRepo::get(db.conn(), EVENT).unwrap().unwrap().state,
        EventState::Active
    );
    assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 6);
    let discord = FakeDiscord::new();
    let report = dispatch_pending(&db, &store, &discord, T5, 100)
        .await
        .unwrap();
    assert_eq!(report.sent, 6);
    assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 0);
    drop(db);

    let db = Db::open(&database).unwrap();
    let report = dispatch_pending(&db, &store, &discord, T6, 100)
        .await
        .unwrap();
    assert_eq!(report.sent, 0);
    assert_eq!(discord.sent_dms().len(), 6);
}

#[tokio::test]
async fn an_attachment_is_downloaded_stored_and_processed_once() {
    use std::sync::{Arc, Mutex};

    use swap_db::ProcessedAttachmentRepo;
    use swap_discord::{
        DiscordChannelId, DiscordUserId, DmAttachment, FakeAttachmentFetcher, InboundEvent,
        MessageId,
    };
    use swap_engine::{FixedClock, InboundService};

    let directory = tempfile::tempdir().unwrap();
    let store = BlobStore::new(directory.path().join("blobs"));
    let db = Db::open_in_memory().unwrap();
    create_event(&db, EVENT, "Test Swap", 42, &EventSettings::default(), T0).unwrap();
    for index in 0..3 {
        join(&db, &store, index);
    }
    lock_roster(&db, EVENT, T1).unwrap();
    generate_schedule(&db, EVENT, T2).unwrap();
    start_event(&db, EVENT, T3).unwrap();
    let offers = open_round(&db, EVENT, 0, T4)
        .unwrap()
        .effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Offer {
                user_id,
                assignment_id,
                track_code,
                ..
            } => Some((user_id.clone(), assignment_id.clone(), track_code.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();
    let (user_id, assignment_id, track_code) = offers[0].clone();
    claim(&db, &store, EVENT, &assignment_id, &user_id, T4).unwrap();

    let discord = Arc::new(swap_discord::FakeDiscord::new());
    let fetcher = Arc::new(FakeAttachmentFetcher::new());
    let edit = seed_bytes(99);
    let url = "https://cdn.discordapp.com/attachments/1/2/edit.it";
    fetcher.add(url, &edit);
    let db = Arc::new(Mutex::new(db));
    let service = InboundService::new(
        db.clone(),
        store.clone(),
        Arc::new(PassthroughPreparer),
        discord.clone(),
        fetcher,
        Arc::new(FixedClock(T5)),
    );
    let event = InboundEvent::DmAttachment {
        user_id: DiscordUserId::new(user_id.clone()),
        channel_id: DiscordChannelId::new("dm-1"),
        message_id: MessageId::new("m-1"),
        author_name: None,
        attachment: DmAttachment {
            id: "a-1".to_string(),
            filename: "edit.it".to_string(),
            size: edit.len() as u64,
            url: url.to_string(),
        },
    };
    service.process(event.clone()).await.unwrap();
    service.drain_pending().await.unwrap();

    let versions = {
        let db = db.lock().unwrap();
        FileRepo::list_for_track(db.conn(), EVENT, &track_code).unwrap()
    };
    assert_eq!(versions.len(), 2);
    assert!(
        discord
            .sent_dms()
            .iter()
            .any(|sent| { sent.message.content.contains("Received version 2") })
    );
    {
        let db = db.lock().unwrap();
        assert!(ProcessedAttachmentRepo::is_processed(db.conn(), &user_id, "m-1", "a-1").unwrap());
    }

    service.process(event).await.unwrap();
    let versions = {
        let db = db.lock().unwrap();
        FileRepo::list_for_track(db.conn(), EVENT, &track_code).unwrap()
    };
    assert_eq!(versions.len(), 2);
}

#[tokio::test]
async fn a_claim_command_delivers_the_claimed_file_from_the_outbox() {
    use std::sync::{Arc, Mutex};

    use swap_discord::{
        DiscordUserId, FakeAttachmentFetcher, InboundEvent, InteractionRef, SlashCommand,
    };
    use swap_engine::{FixedClock, InboundService};

    let directory = tempfile::tempdir().unwrap();
    let store = BlobStore::new(directory.path().join("blobs"));
    let db = Db::open_in_memory().unwrap();
    create_event(&db, EVENT, "Test Swap", 42, &EventSettings::default(), T0).unwrap();
    for index in 0..3 {
        join(&db, &store, index);
    }
    lock_roster(&db, EVENT, T1).unwrap();
    generate_schedule(&db, EVENT, T2).unwrap();
    start_event(&db, EVENT, T3).unwrap();
    let offers = open_round(&db, EVENT, 0, T4).unwrap();
    let user_id = match &offers.effects[0] {
        Effect::Offer { user_id, .. } => user_id.clone(),
        other => panic!("expected an offer, got {other:?}"),
    };

    let discord = Arc::new(swap_discord::FakeDiscord::new());
    let service = InboundService::new(
        Arc::new(Mutex::new(db)),
        store,
        Arc::new(PassthroughPreparer),
        discord.clone(),
        Arc::new(FakeAttachmentFetcher::new()),
        Arc::new(FixedClock(T5)),
    );
    service
        .process(InboundEvent::SlashCommand {
            interaction: InteractionRef::new("i-1", "token"),
            user_id: DiscordUserId::new(user_id),
            guild_id: None,
            command: SlashCommand::Claim,
            options: Default::default(),
        })
        .await
        .unwrap();
    service.drain_pending().await.unwrap();

    assert_eq!(discord.sent_files().len(), 1);
}

#[tokio::test]
async fn backfill_recovers_a_dm_attachment_after_an_outage() {
    use std::sync::{Arc, Mutex};

    use swap_db::DmCursorRepo;
    use swap_discord::{DiscordUserId, DmAttachment, FakeAttachmentFetcher, InboundDm, MessageId};
    use swap_engine::{FixedClock, InboundService};

    let directory = tempfile::tempdir().unwrap();
    let store = BlobStore::new(directory.path().join("blobs"));
    let db = Db::open_in_memory().unwrap();
    create_event(&db, EVENT, "Test Swap", 42, &EventSettings::default(), T0).unwrap();
    for index in 0..3 {
        join(&db, &store, index);
    }
    lock_roster(&db, EVENT, T1).unwrap();
    generate_schedule(&db, EVENT, T2).unwrap();
    start_event(&db, EVENT, T3).unwrap();
    let offers = open_round(&db, EVENT, 0, T4).unwrap();
    let (user_id, assignment_id, track_code) = match &offers.effects[0] {
        Effect::Offer {
            user_id,
            assignment_id,
            track_code,
            ..
        } => (user_id.clone(), assignment_id.clone(), track_code.clone()),
        other => panic!("expected an offer, got {other:?}"),
    };
    claim(&db, &store, EVENT, &assignment_id, &user_id, T4).unwrap();
    DmCursorRepo::upsert(db.conn(), &user_id, "dm-1", None, T4).unwrap();

    let discord = Arc::new(swap_discord::FakeDiscord::new());
    let edit = seed_bytes(77);
    let url = "https://cdn.discordapp.com/attachments/1/2/backfill.it";
    let fetcher = Arc::new(FakeAttachmentFetcher::new());
    fetcher.add(url, &edit);
    discord.push_inbound(
        DiscordUserId::new(user_id.clone()),
        InboundDm {
            message_id: MessageId::new("m-backfill"),
            content: String::new(),
            attachments: vec![DmAttachment {
                id: "a-backfill".to_string(),
                filename: "backfill.it".to_string(),
                size: edit.len() as u64,
                url: url.to_string(),
            }],
        },
    );
    let db = Arc::new(Mutex::new(db));
    let service = InboundService::new(
        db.clone(),
        store.clone(),
        Arc::new(PassthroughPreparer),
        discord.clone(),
        fetcher,
        Arc::new(FixedClock(T5)),
    );
    assert_eq!(service.backfill().await.unwrap().handled, 1);
    let versions = {
        let db = db.lock().unwrap();
        FileRepo::list_for_track(db.conn(), EVENT, &track_code).unwrap()
    };
    assert_eq!(versions.len(), 2);
    let cursor = {
        let db = db.lock().unwrap();
        DmCursorRepo::get(db.conn(), &user_id).unwrap().unwrap()
    };
    assert_eq!(cursor.last_seen_message_id.as_deref(), Some("m-backfill"));
}

#[tokio::test]
async fn backfill_ignores_an_attachment_from_before_the_claim() {
    use std::sync::{Arc, Mutex};

    use swap_db::ProcessedAttachmentRepo;
    use swap_discord::{DiscordUserId, DmAttachment, FakeAttachmentFetcher, InboundDm, MessageId};
    use swap_engine::{FixedClock, InboundService};

    let fixture = Fixture::new(3, 0);
    fixture.lock_and_schedule();
    fixture.start();
    let offers = fixture.open(0);
    let (user_id, assignment_id, track_code) = offers[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        T4,
    )
    .unwrap();
    let claimed_at = 1_700_000_000_i64;
    fixture
        .db
        .conn()
        .execute(
            "UPDATE assignments SET claimed_at = ?1 WHERE id = ?2",
            (claimed_at, &assignment_id),
        )
        .unwrap();
    DmCursorRepo::upsert(fixture.db.conn(), &user_id, "dm-1", None, T4).unwrap();
    let old_message = ((((claimed_at - 3_600) * 1000) - 1_420_070_400_000) as u64) << 22;

    let discord = Arc::new(swap_discord::FakeDiscord::new());
    let url = "https://cdn.discordapp.com/attachments/1/2/old.it";
    let fetcher = Arc::new(FakeAttachmentFetcher::new());
    fetcher.add(url, &seed_bytes(88));
    discord.push_inbound(
        DiscordUserId::new(user_id.clone()),
        InboundDm {
            message_id: MessageId::new(old_message.to_string()),
            content: String::new(),
            attachments: vec![DmAttachment {
                id: "a-old".to_string(),
                filename: "old.it".to_string(),
                size: seed_bytes(88).len() as u64,
                url: url.to_string(),
            }],
        },
    );
    let db = Arc::new(Mutex::new(fixture.db));
    let service = InboundService::new(
        db.clone(),
        fixture.store.clone(),
        Arc::new(PassthroughPreparer),
        discord,
        fetcher,
        Arc::new(FixedClock(T5)),
    );
    service.backfill().await.unwrap();
    let versions = {
        let db = db.lock().unwrap();
        FileRepo::list_for_track(db.conn(), EVENT, &track_code).unwrap()
    };
    assert_eq!(versions.len(), 1);
    assert!(
        ProcessedAttachmentRepo::is_processed(
            db.lock().unwrap().conn(),
            &user_id,
            &old_message.to_string(),
            "a-old"
        )
        .unwrap()
    );
}

#[tokio::test]
async fn a_failed_backfill_is_retried_for_the_same_participant() {
    use std::sync::{Arc, Mutex};

    use swap_discord::{
        DiscordError, DiscordUserId, DmAttachment, FakeAttachmentFetcher, InboundDm, MessageId,
    };
    use swap_engine::{FixedClock, InboundService};

    let fixture = Fixture::new(3, 0);
    fixture.lock_and_schedule();
    fixture.start();
    let offers = fixture.open(0);
    let (user_id, assignment_id, track_code) = offers[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        T4,
    )
    .unwrap();
    DmCursorRepo::upsert(fixture.db.conn(), &user_id, "dm-1", None, T4).unwrap();

    let discord = Arc::new(swap_discord::FakeDiscord::new());
    let url = "https://cdn.discordapp.com/attachments/1/2/retry.it";
    let fetcher = Arc::new(FakeAttachmentFetcher::new());
    fetcher.add(url, &seed_bytes(93));
    discord.push_inbound(
        DiscordUserId::new(user_id.clone()),
        InboundDm {
            message_id: MessageId::new("m-retry"),
            content: String::new(),
            attachments: vec![DmAttachment {
                id: "a-retry".to_string(),
                filename: "retry.it".to_string(),
                size: seed_bytes(93).len() as u64,
                url: url.to_string(),
            }],
        },
    );
    let db = Arc::new(Mutex::new(fixture.db));
    let service = InboundService::new(
        db.clone(),
        fixture.store.clone(),
        Arc::new(PassthroughPreparer),
        discord.clone(),
        fetcher,
        Arc::new(FixedClock(T5)),
    );
    service.mark_backfill_pending();
    discord.fail_next_fetch(DiscordError::Transport("offline".to_string()));
    let report = service.backfill().await.unwrap();
    assert_eq!(report.failed, 1);
    assert_eq!(report.handled, 0);
    assert_eq!(
        FileRepo::list_for_track(db.lock().unwrap().conn(), EVENT, &track_code)
            .unwrap()
            .len(),
        1
    );
    assert!(
        AuditRepo::list_for_event(db.lock().unwrap().conn(), EVENT)
            .unwrap()
            .iter()
            .any(|entry| entry.action == "backfill.failed")
    );

    let report = service.retry_backfill().await.unwrap();
    assert_eq!(report.failed, 0);
    assert_eq!(report.handled, 1);
    assert_eq!(
        FileRepo::list_for_track(db.lock().unwrap().conn(), EVENT, &track_code)
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn a_signup_seed_attachment_creates_a_seeded_participant() {
    use std::sync::{Arc, Mutex};

    use swap_discord::{
        DiscordChannelId, DiscordUserId, DmAttachment, FakeAttachmentFetcher, InboundEvent,
        MessageId,
    };
    use swap_engine::{FixedClock, InboundService};

    let fixture = Fixture::new(0, 0);
    let discord = Arc::new(swap_discord::FakeDiscord::new());
    let url = "https://cdn.discordapp.com/attachments/1/2/seed.it";
    let fetcher = Arc::new(FakeAttachmentFetcher::new());
    fetcher.add(url, &seed_bytes(7));
    let db = Arc::new(Mutex::new(fixture.db));
    let service = InboundService::new(
        db.clone(),
        fixture.store.clone(),
        Arc::new(PassthroughPreparer),
        discord.clone(),
        fetcher,
        Arc::new(FixedClock(T5)),
    );
    service
        .process(InboundEvent::DmAttachment {
            user_id: DiscordUserId::new("user-new"),
            channel_id: DiscordChannelId::new("dm-1"),
            message_id: MessageId::new("m-seed"),
            author_name: Some("Ari".to_string()),
            attachment: DmAttachment {
                id: "a-seed".to_string(),
                filename: "seed.it".to_string(),
                size: seed_bytes(7).len() as u64,
                url: url.to_string(),
            },
        })
        .await
        .unwrap();

    let participant = {
        let db = db.lock().unwrap();
        ParticipantRepo::get(db.conn(), EVENT, "user-new")
            .unwrap()
            .unwrap()
    };
    assert_eq!(participant.membership, Membership::Seeded);
    assert_eq!(participant.display_name, "Ari");
    let tracks = {
        let db = db.lock().unwrap();
        TrackRepo::list_for_event(db.conn(), EVENT).unwrap()
    };
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].owner_id, "user-new");
    let cursor = {
        let db = db.lock().unwrap();
        DmCursorRepo::get(db.conn(), "user-new").unwrap().unwrap()
    };
    assert_eq!(cursor.last_seen_message_id.as_deref(), Some("m-seed"));

    service.drain_pending().await.unwrap();
    assert!(
        discord
            .sent_dms()
            .iter()
            .any(|dm| dm.message.content.contains("Seed received"))
    );
}

#[tokio::test]
async fn an_active_assignment_wins_over_a_new_signup_event() {
    use std::sync::{Arc, Mutex};

    use swap_discord::{
        DiscordChannelId, DiscordUserId, DmAttachment, FakeAttachmentFetcher, InboundEvent,
        InteractionRef, MessageId, SlashCommand,
    };
    use swap_engine::{FixedClock, InboundService};

    let fixture = Fixture::new(3, 0);
    fixture.lock_and_schedule();
    fixture.start();
    let offers = fixture.open(0);
    let (user_id, assignment_id, track_code) = offers[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        T4,
    )
    .unwrap();
    create_event(
        &fixture.db,
        "evt-2",
        "Next Swap",
        7,
        &EventSettings::default(),
        T5,
    )
    .unwrap();
    MetaRepo::set(fixture.db.conn(), "host.current_event", "evt-2").unwrap();

    let discord = Arc::new(swap_discord::FakeDiscord::new());
    let url = "https://cdn.discordapp.com/attachments/1/2/edit.it";
    let fetcher = Arc::new(FakeAttachmentFetcher::new());
    fetcher.add(url, &seed_bytes(99));
    let db = Arc::new(Mutex::new(fixture.db));
    let service = InboundService::new(
        db.clone(),
        fixture.store.clone(),
        Arc::new(PassthroughPreparer),
        discord.clone(),
        fetcher,
        Arc::new(FixedClock(T5)),
    );
    service
        .process(InboundEvent::DmAttachment {
            user_id: DiscordUserId::new(user_id.clone()),
            channel_id: DiscordChannelId::new("dm-1"),
            message_id: MessageId::new("m-edit"),
            author_name: None,
            attachment: DmAttachment {
                id: "a-edit".to_string(),
                filename: "edit.it".to_string(),
                size: seed_bytes(99).len() as u64,
                url: url.to_string(),
            },
        })
        .await
        .unwrap();
    let active_versions = {
        let db = db.lock().unwrap();
        FileRepo::list_for_track(db.conn(), EVENT, &track_code).unwrap()
    };
    assert_eq!(active_versions.len(), 2);
    let next_tracks = {
        let db = db.lock().unwrap();
        TrackRepo::list_for_event(db.conn(), "evt-2").unwrap()
    };
    assert!(next_tracks.is_empty());
    service
        .process(InboundEvent::SlashCommand {
            interaction: InteractionRef::new("i-join", "token"),
            user_id: DiscordUserId::new(user_id.clone()),
            guild_id: None,
            command: SlashCommand::Join,
            options: Default::default(),
        })
        .await
        .unwrap();
    assert!(
        ParticipantRepo::get(db.lock().unwrap().conn(), "evt-2", &user_id)
            .unwrap()
            .is_none()
    );
    assert!(
        discord
            .sent_dms()
            .iter()
            .any(|dm| dm.message.content.contains("already in an active event"))
    );
}

#[tokio::test]
async fn a_join_is_refused_while_another_event_is_not_started() {
    use std::sync::{Arc, Mutex};

    use swap_discord::{
        DiscordUserId, FakeAttachmentFetcher, InboundEvent, InteractionRef, SlashCommand,
    };
    use swap_engine::{FixedClock, InboundService};

    let fixture = Fixture::new(3, 0);
    fixture.lock_and_schedule();
    create_event(
        &fixture.db,
        "evt-2",
        "Next Swap",
        7,
        &EventSettings::default(),
        T5,
    )
    .unwrap();
    MetaRepo::set(fixture.db.conn(), "host.current_event", "evt-2").unwrap();
    let discord = Arc::new(swap_discord::FakeDiscord::new());
    let db = Arc::new(Mutex::new(fixture.db));
    let service = InboundService::new(
        db.clone(),
        fixture.store.clone(),
        Arc::new(PassthroughPreparer),
        discord.clone(),
        Arc::new(FakeAttachmentFetcher::new()),
        Arc::new(FixedClock(T5)),
    );
    service
        .process(InboundEvent::SlashCommand {
            interaction: InteractionRef::new("i-join", "token"),
            user_id: DiscordUserId::new(user(0)),
            guild_id: None,
            command: SlashCommand::Join,
            options: Default::default(),
        })
        .await
        .unwrap();
    assert!(
        ParticipantRepo::get(db.lock().unwrap().conn(), "evt-2", &user(0))
            .unwrap()
            .is_none()
    );
    assert!(
        discord
            .sent_dms()
            .iter()
            .any(|dm| dm.message.content.contains("has not started"))
    );
}

#[tokio::test]
async fn the_deadline_tick_is_idempotent_and_marks_overdue_without_moving_the_track() {
    use swap_engine::{DeadlineReport, tick_deadlines};

    let base = T4;
    let settings = EventSettings {
        round_window_secs: 100,
        ..EventSettings::default()
    };
    let fixture = Fixture::with_settings(3, 0, settings);
    fixture
        .db
        .conn()
        .execute("UPDATE participants SET timezone = 'Asia/Tokyo'", [])
        .unwrap();
    fixture.lock_and_schedule();
    fixture.start();
    let offers = fixture.open(0);
    let (user_id, assignment_id, track_code) = offers[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        base,
    )
    .unwrap();
    let discord = swap_discord::FakeDiscord::new();

    let early: DeadlineReport = tick_deadlines(&fixture.db, base + 40).unwrap();
    assert_eq!(early.reminders, 0);
    assert_eq!(early.overdue, 0);

    let half = tick_deadlines(&fixture.db, base + 50).unwrap();
    assert_eq!(half.reminders, 1);
    dispatch_pending(&fixture.db, &fixture.store, &discord, base + 50, 100)
        .await
        .unwrap();
    let sent_before = discord.sent_dms().len();
    assert!(
        discord
            .sent_dms()
            .iter()
            .any(|dm| dm.message.content.contains(&track_code))
    );
    assert_eq!(tick_deadlines(&fixture.db, base + 51).unwrap().reminders, 0);
    assert_eq!(discord.sent_dms().len(), sent_before);

    assert_eq!(tick_deadlines(&fixture.db, base + 80).unwrap().reminders, 1);
    assert_eq!(tick_deadlines(&fixture.db, base + 95).unwrap().reminders, 1);
    let overdue = tick_deadlines(&fixture.db, base + 100).unwrap();
    assert_eq!(overdue.overdue, 1);
    let assignment = AssignmentRepo::get(fixture.db.conn(), &assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(assignment.status, AssignmentStatus::Overdue);
    let track = TrackRepo::get(fixture.db.conn(), EVENT, &track_code)
        .unwrap()
        .unwrap();
    assert_eq!(track.state, TrackState::InCustody);
    assert!(
        !fixture
            .db
            .conn()
            .query_row::<i64, _, _>(
                "SELECT COUNT(*) FROM host_decisions WHERE assignment_id = ?1 AND status = 'open'",
                [&assignment_id],
                |row| row.get(0),
            )
            .unwrap()
            .eq(&0)
    );
}

#[test]
fn quiet_hours_delay_a_reminder_without_moving_the_deadline() {
    use swap_engine::tick_deadlines;

    let base = 25_130_i64;
    let settings = EventSettings {
        round_window_secs: 100,
        ..EventSettings::default()
    };
    let fixture = Fixture::with_settings(2, 0, settings);
    fixture.lock_and_schedule();
    fixture.start();
    let offers = fixture.open(0);
    let (user_id, assignment_id, _) = offers[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        base,
    )
    .unwrap();
    let due = AssignmentRepo::get(fixture.db.conn(), &assignment_id)
        .unwrap()
        .unwrap()
        .due_at
        .unwrap();
    assert_eq!(tick_deadlines(&fixture.db, base + 50).unwrap().reminders, 0);
    assert_eq!(tick_deadlines(&fixture.db, base + 70).unwrap().reminders, 1);
    let after = AssignmentRepo::get(fixture.db.conn(), &assignment_id)
        .unwrap()
        .unwrap()
        .due_at
        .unwrap();
    assert_eq!(after, due);
}

#[test]
fn quiet_hours_can_be_turned_off() {
    use swap_engine::tick_deadlines;

    let base = 25_130_i64;
    let settings = EventSettings {
        round_window_secs: 100,
        quiet_hours_start: 8,
        quiet_hours_end: 8,
        ..EventSettings::default()
    };
    let fixture = Fixture::with_settings(2, 0, settings);
    fixture.lock_and_schedule();
    fixture.start();
    let offers = fixture.open(0);
    let (user_id, assignment_id, _) = offers[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        base,
    )
    .unwrap();
    assert_eq!(tick_deadlines(&fixture.db, base + 50).unwrap().reminders, 1);
}

#[test]
fn reminders_do_not_burst_after_quiet_hours() {
    use swap_engine::tick_deadlines;

    let base = 25_130_i64;
    let settings = EventSettings {
        round_window_secs: 100,
        ..EventSettings::default()
    };
    let fixture = Fixture::with_settings(2, 0, settings);
    fixture.lock_and_schedule();
    fixture.start();
    let offers = fixture.open(0);
    let (user_id, assignment_id, _) = offers[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        base,
    )
    .unwrap();
    assert_eq!(tick_deadlines(&fixture.db, base + 20).unwrap().reminders, 0);
    assert_eq!(tick_deadlines(&fixture.db, base + 50).unwrap().reminders, 0);
    assert_eq!(tick_deadlines(&fixture.db, base + 99).unwrap().reminders, 1);
    assert_eq!(tick_deadlines(&fixture.db, base + 99).unwrap().reminders, 0);
}

#[test]
fn a_manual_edit_swaps_two_assignments_and_validates_the_plan() {
    use swap_engine::edit_plan_assignment;

    let fixture = Fixture::with_settings(
        5,
        0,
        EventSettings {
            rounds: Some(2),
            ..EventSettings::default()
        },
    );
    let schedule = fixture.lock_and_schedule();
    let before = PlanRepo::latest(fixture.db.conn(), EVENT).unwrap().unwrap();
    let target = schedule
        .iter()
        .find(|assignment| assignment.round == 0)
        .unwrap()
        .clone();
    let owner = TrackRepo::get(fixture.db.conn(), EVENT, &target.track_code)
        .unwrap()
        .unwrap()
        .owner_id;
    let candidate = schedule
        .iter()
        .find(|assignment| {
            assignment.round == 0
                && assignment.participant_id != target.participant_id
                && assignment.participant_id != owner
                && !schedule.iter().any(|other| {
                    other.participant_id == assignment.participant_id
                        && other.track_code == target.track_code
                })
        })
        .unwrap()
        .clone();
    let candidate_old_track = candidate.track_code.clone();
    let displaced = target.participant_id.clone();

    assert!(matches!(
        edit_plan_assignment(&fixture.db, EVENT, 0, &target.track_code, &owner, T4),
        Err(EngineError::PlanOwnTrack { .. })
    ));
    assert!(matches!(
        edit_plan_assignment(
            &fixture.db,
            EVENT,
            0,
            &target.track_code,
            &target.participant_id,
            T4
        ),
        Err(EngineError::PlanNoChange)
    ));
    let target_track = &target.track_code;
    let holders = schedule
        .iter()
        .filter(|assignment| &assignment.track_code == target_track)
        .collect::<Vec<_>>();
    let repeat_candidate = holders[0];
    let repeat_target = holders[1];
    assert!(matches!(
        edit_plan_assignment(
            &fixture.db,
            EVENT,
            repeat_target.round,
            target_track,
            &repeat_candidate.participant_id,
            T4
        ),
        Err(EngineError::PlanRepeat { .. })
    ));

    let outcome = edit_plan_assignment(
        &fixture.db,
        EVENT,
        0,
        &target.track_code,
        &candidate.participant_id,
        T4,
    )
    .unwrap();
    assert_eq!(outcome.version, 2);
    assert_ne!(outcome.commitment, before.commitment);
    let after = PlanRepo::list_assignments(fixture.db.conn(), EVENT, 2).unwrap();
    assert_eq!(after.len(), schedule.len());
    assert_eq!(
        after
            .iter()
            .find(|assignment| {
                assignment.round == 0 && assignment.track_code == target.track_code
            })
            .unwrap()
            .participant_id,
        candidate.participant_id
    );
    assert_eq!(
        after
            .iter()
            .find(|assignment| {
                assignment.round == 0 && assignment.track_code == candidate_old_track
            })
            .unwrap()
            .participant_id,
        displaced
    );
    let mut seen = std::collections::BTreeSet::new();
    for assignment in &after {
        assert!(seen.insert((
            assignment.participant_id.clone(),
            assignment.track_code.clone()
        )));
        assert_ne!(
            TrackRepo::get(fixture.db.conn(), EVENT, &assignment.track_code)
                .unwrap()
                .unwrap()
                .owner_id,
            assignment.participant_id
        );
    }
}

#[test]
fn a_complete_schedule_has_no_valid_single_cell_swap() {
    use swap_engine::edit_plan_assignment;

    let fixture = Fixture::new(4, 0);
    let schedule = fixture.lock_and_schedule();
    let target = schedule
        .iter()
        .find(|assignment| assignment.round == 0)
        .unwrap()
        .clone();
    let candidates = schedule
        .iter()
        .filter(|assignment| {
            assignment.round == 0 && assignment.participant_id != target.participant_id
        })
        .collect::<Vec<_>>();
    assert!(!candidates.is_empty());
    for candidate in candidates {
        let result = edit_plan_assignment(
            &fixture.db,
            EVENT,
            0,
            &target.track_code,
            &candidate.participant_id,
            T4,
        );
        assert!(
            result.is_err(),
            "a complete schedule must reject every single-cell swap"
        );
    }
    assert_eq!(
        PlanRepo::latest(fixture.db.conn(), EVENT)
            .unwrap()
            .unwrap()
            .version,
        1
    );
}

#[test]
fn the_first_small_extension_is_automatic_and_the_third_is_refused() {
    use swap_engine::{ExtensionStatus, decide_extension, open_decisions, request_extension};

    let settings = EventSettings {
        round_window_secs: 100,
        ..EventSettings::default()
    };
    let fixture = Fixture::with_settings(2, 0, settings);
    fixture.lock_and_schedule();
    fixture.start();
    let offers = fixture.open(0);
    let (user_id, assignment_id, track_code) = offers[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        T4,
    )
    .unwrap();
    let original_due = AssignmentRepo::get(fixture.db.conn(), &assignment_id)
        .unwrap()
        .unwrap()
        .due_at
        .unwrap();

    assert!(matches!(
        request_extension(&fixture.db, EVENT, &user_id, 2, T4 + 5),
        Err(EngineError::InvalidExtensionHours)
    ));
    let first = request_extension(&fixture.db, EVENT, &user_id, 6, T4 + 10).unwrap();
    assert_eq!(first.status, ExtensionStatus::Approved);
    assert_eq!(first.due_at, Some(original_due + 6 * 3_600));
    let assignment = AssignmentRepo::get(fixture.db.conn(), &assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(assignment.status, AssignmentStatus::Claimed);
    assert_eq!(
        TrackRepo::get(fixture.db.conn(), EVENT, &track_code)
            .unwrap()
            .unwrap()
            .state,
        TrackState::InCustody
    );

    let second = request_extension(&fixture.db, EVENT, &user_id, 6, T4 + 20).unwrap();
    assert_eq!(second.status, ExtensionStatus::PendingHost);
    let decisions = open_decisions(&fixture.db, EVENT).unwrap();
    assert_eq!(decisions.len(), 1);
    let decision_id = decisions[0].id.clone();
    let approved =
        decide_extension(&fixture.db, &decision_id, true, "extra time", T4 + 30).unwrap();
    assert_eq!(approved.status, ExtensionStatus::Approved);

    let third = request_extension(&fixture.db, EVENT, &user_id, 6, T4 + 40).unwrap();
    assert_eq!(third.status, ExtensionStatus::Refused);
    assert_eq!(
        TrackRepo::get(fixture.db.conn(), EVENT, &track_code)
            .unwrap()
            .unwrap()
            .state,
        TrackState::InCustody
    );
}

#[test]
fn the_reveal_package_carries_titles_comments_and_credits() {
    let fixture = Fixture::new(3, 0);
    fixture.lock_and_schedule();
    fixture.start();
    let round_zero = fixture.open(0);
    let (user_id, assignment_id, track_code) = round_zero[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        T5,
    )
    .unwrap();
    let bytes = seed_bytes(101);
    submit(
        &fixture.db,
        &fixture.store,
        &PassthroughPreparer,
        &SubmitRequest {
            event_id: EVENT,
            user_id: &user_id,
            filename: "edit.it",
            bytes: &bytes,
        },
        T6,
    )
    .unwrap();

    let other_user = &round_zero[1].0;
    assert!(matches!(
        set_track_title(&fixture.db, EVENT, &track_code, "Stolen", other_user, T6),
        Err(EngineError::NotTitleHolder)
    ));
    set_track_title(
        &fixture.db,
        EVENT,
        &track_code,
        "  Midnight Draft  ",
        "host",
        T6,
    )
    .unwrap();
    let owner = TrackRepo::get(fixture.db.conn(), EVENT, &track_code)
        .unwrap()
        .unwrap()
        .owner_id;
    let outsider = round_zero
        .iter()
        .map(|(candidate, _, _)| candidate)
        .find(|candidate| **candidate != user_id && **candidate != owner)
        .unwrap();
    assert!(matches!(
        add_track_comment(
            &fixture.db,
            EVENT,
            &track_code,
            outsider,
            "comment",
            "Not my track",
            T6,
        ),
        Err(EngineError::NotTrackContributor)
    ));
    add_track_comment(
        &fixture.db,
        EVENT,
        &track_code,
        &user_id,
        "comment",
        "  A great bassline.  ",
        T6,
    )
    .unwrap();
    let redacted = add_track_comment(
        &fixture.db,
        EVENT,
        &track_code,
        &user_id,
        "questionnaire",
        "I used the seed melody.",
        T6,
    )
    .unwrap();
    redact_track_comment(&fixture.db, &redacted.id, T6).unwrap();

    let package = reveal_package(&fixture.db, EVENT).unwrap();
    assert!(!package.published);
    assert_eq!(package.event_state, EventState::Active);
    assert_eq!(package.album.len(), 3);
    assert_eq!(
        package
            .album
            .iter()
            .map(|track| track.position)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    let codes = package
        .album
        .iter()
        .map(|track| track.track_code.clone())
        .collect::<Vec<_>>();
    assert_eq!(codes, album_order(&codes, 42));
    let track = package
        .album
        .iter()
        .find(|track| track.track_code == track_code)
        .unwrap();
    assert_eq!(track.title.as_deref(), Some("Midnight Draft"));
    assert_eq!(
        track
            .credits
            .iter()
            .filter(|entry| entry.status == AssignmentStatus::Submitted)
            .count(),
        1
    );
    assert_eq!(track.comments.len(), 1);
    assert_eq!(track.comments[0].body, "A great bassline.");
    assert_eq!(track.comments[0].author_name, "User 0");
    assert!(track.versions.iter().any(|version| {
        version.version == 2
            && version.final_version
            && version.holder_id == user_id
            && version.playable
    }));
    assert!(track.versions.iter().any(|version| {
        version.version == 1 && version.role == swap_engine::RevealVersionRole::Seed
    }));
}

#[test]
fn the_final_holder_is_asked_for_a_title_once() {
    let fixture = Fixture::new(3, 0);
    fixture.lock_and_schedule();
    fixture.start();
    let first_round = fixture.open(0);
    for (index, (user_id, assignment_id, _)) in first_round.iter().enumerate() {
        claim(
            &fixture.db,
            &fixture.store,
            EVENT,
            assignment_id,
            user_id,
            T5,
        )
        .unwrap();
        let bytes = seed_bytes(110 + index as u8);
        submit(
            &fixture.db,
            &fixture.store,
            &PassthroughPreparer,
            &SubmitRequest {
                event_id: EVENT,
                user_id,
                filename: "edit.it",
                bytes: &bytes,
            },
            T6,
        )
        .unwrap();
    }
    let final_offers = fixture.open(1);
    let (user_id, assignment_id, track_code) = final_offers[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        T5,
    )
    .unwrap();
    let bytes = seed_bytes(200);
    let first = submit(
        &fixture.db,
        &fixture.store,
        &PassthroughPreparer,
        &SubmitRequest {
            event_id: EVENT,
            user_id: &user_id,
            filename: "edit.it",
            bytes: &bytes,
        },
        T6,
    )
    .unwrap();
    assert_eq!(first.effects.len(), 2);
    assert!(first.effects.iter().any(|effect| {
        matches!(effect, Effect::Dm { content, .. } if content.contains("/swap title"))
    }));
    assert!(first.effects.iter().any(|effect| {
        matches!(effect, Effect::Dm { content, .. } if content.contains(&track_code))
    }));
    assert!(set_track_title_for_user(&fixture.db, EVENT, &user_id, "Final Name", T7).is_ok());
    let early_user = &first_round[0].0;
    assert!(matches!(
        set_track_title_for_user(&fixture.db, EVENT, early_user, "Too Early", T7),
        Err(EngineError::NotTitleHolder)
    ));

    let second_bytes = seed_bytes(201);
    let second = submit(
        &fixture.db,
        &fixture.store,
        &PassthroughPreparer,
        &SubmitRequest {
            event_id: EVENT,
            user_id: &user_id,
            filename: "edit.it",
            bytes: &second_bytes,
        },
        T7,
    )
    .unwrap();
    assert_eq!(second.effects.len(), 1);
}

#[test]
fn a_missed_deadline_decision_records_seen_and_returns_the_track() {
    use swap_engine::{decide_deadline, open_decisions, tick_deadlines};

    let settings = EventSettings {
        round_window_secs: 100,
        ..EventSettings::default()
    };
    let fixture = Fixture::with_settings(2, 0, settings);
    fixture.lock_and_schedule();
    fixture.start();
    let offers = fixture.open(0);
    let (user_id, assignment_id, track_code) = offers[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        T4,
    )
    .unwrap();
    let due = AssignmentRepo::get(fixture.db.conn(), &assignment_id)
        .unwrap()
        .unwrap()
        .due_at
        .unwrap();
    tick_deadlines(&fixture.db, due).unwrap();
    assert_eq!(open_decisions(&fixture.db, EVENT).unwrap().len(), 1);

    decide_deadline(
        &fixture.db,
        &assignment_id,
        "missed",
        0,
        "no submission",
        due + 1,
    )
    .unwrap();
    let assignment = AssignmentRepo::get(fixture.db.conn(), &assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(assignment.status, AssignmentStatus::SkippedAfterClaim);
    assert_eq!(
        TrackRepo::get(fixture.db.conn(), EVENT, &track_code)
            .unwrap()
            .unwrap()
            .state,
        TrackState::Pool
    );
    assert!(SeenRepo::has(fixture.db.conn(), EVENT, &track_code, &user_id).unwrap());
    assert!(open_decisions(&fixture.db, EVENT).unwrap().is_empty());
}

#[test]
fn deadline_flags_and_decisions_survive_a_restart() {
    use swap_engine::tick_deadlines;

    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("deadlines.db");
    let store = BlobStore::new(directory.path().join("blobs"));
    let (user_id, assignment_id) = {
        let db = Db::open(&database).unwrap();
        create_event(&db, EVENT, "Test Swap", 42, &EventSettings::default(), T0).unwrap();
        MetaRepo::set(db.conn(), "host.current_event", EVENT).unwrap();
        for index in 0..2 {
            join(&db, &store, index);
        }
        lock_roster(&db, EVENT, T1).unwrap();
        generate_schedule(&db, EVENT, T2).unwrap();
        start_event(&db, EVENT, T3).unwrap();
        let offers = offers(&open_round(&db, EVENT, 0, T4).unwrap().effects);
        let (user_id, assignment_id, _) = offers[0].clone();
        claim(&db, &store, EVENT, &assignment_id, &user_id, T4).unwrap();
        tick_deadlines(&db, T4 + 43_200).unwrap();
        let deadline = DeadlineRepo::get(db.conn(), &assignment_id)
            .unwrap()
            .unwrap();
        assert!(deadline.reminded_50);
        tick_deadlines(&db, T4 + 86_400).unwrap();
        assert_eq!(
            HostDecisionRepo::list_open(db.conn(), EVENT).unwrap().len(),
            1
        );
        (user_id, assignment_id)
    };

    let db = Db::open(&database).unwrap();
    let report = tick_deadlines(&db, T4 + 86_401).unwrap();
    assert_eq!(report.reminders, 0);
    let deadline = DeadlineRepo::get(db.conn(), &assignment_id)
        .unwrap()
        .unwrap();
    assert!(deadline.reminded_50);
    assert_eq!(
        HostDecisionRepo::list_open(db.conn(), EVENT).unwrap().len(),
        1
    );
    assert_eq!(
        AssignmentRepo::get(db.conn(), &assignment_id)
            .unwrap()
            .unwrap()
            .participant_id,
        user_id
    );
}

#[test]
fn a_signup_after_the_roster_lock_is_refused() {
    let fixture = Fixture::new(2, 0);
    lock_roster(&fixture.db, EVENT, T1).unwrap();
    let error = signup(&fixture.db, EVENT, "late-user", "Late", "UTC", 3, T2).unwrap_err();
    assert!(matches!(error, EngineError::WrongEventState { .. }));
    assert!(
        ParticipantRepo::get(fixture.db.conn(), EVENT, "late-user")
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn stopping_queues_releases_events_waiting_for_backfill() {
    use std::sync::{Arc, Mutex};

    use swap_discord::{
        DiscordChannelId, DiscordError, DiscordUserId, DmAttachment, FakeAttachmentFetcher,
        InboundEvent, InboundHandler, MessageId,
    };
    use swap_engine::{FixedClock, InboundService};

    let fixture = Fixture::new(1, 0);
    DmCursorRepo::upsert(fixture.db.conn(), &user(0), "dm", None, T4).unwrap();
    let discord = Arc::new(swap_discord::FakeDiscord::new());
    let db = Arc::new(Mutex::new(fixture.db));
    let service = InboundService::new(
        db,
        fixture.store.clone(),
        Arc::new(PassthroughPreparer),
        discord.clone(),
        Arc::new(FakeAttachmentFetcher::new()),
        Arc::new(FixedClock(T5)),
    );
    service.mark_backfill_pending();
    discord.fail_next_fetch(DiscordError::Transport("offline".to_string()));
    let report = service.backfill().await.unwrap();
    assert_eq!(report.failed, 1);
    service
        .handle(InboundEvent::DmAttachment {
            user_id: DiscordUserId::new(user(0)),
            channel_id: DiscordChannelId::new("dm"),
            message_id: MessageId::new("m-wait"),
            author_name: None,
            attachment: DmAttachment {
                id: "a-wait".to_string(),
                filename: "wait.it".to_string(),
                size: 4,
                url: "https://cdn.discordapp.com/attachments/1/2/wait.it".to_string(),
            },
        })
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), service.stop_queues())
        .await
        .expect("stop_queues must not wait for a pending backfill");
}

#[tokio::test]
async fn a_join_command_creates_a_signed_up_participant() {
    use std::sync::{Arc, Mutex};

    use swap_discord::{
        DiscordUserId, FakeAttachmentFetcher, InboundEvent, InteractionRef, SlashCommand,
    };
    use swap_engine::{FixedClock, InboundService};

    let fixture = Fixture::new(0, 0);
    let discord = Arc::new(swap_discord::FakeDiscord::new());
    let db = Arc::new(Mutex::new(fixture.db));
    let service = InboundService::new(
        db.clone(),
        fixture.store.clone(),
        Arc::new(PassthroughPreparer),
        discord.clone(),
        Arc::new(FakeAttachmentFetcher::new()),
        Arc::new(FixedClock(T5)),
    );
    service
        .process(InboundEvent::SlashCommand {
            interaction: InteractionRef::new("i-join", "token"),
            user_id: DiscordUserId::new("user-join"),
            guild_id: None,
            command: SlashCommand::Join,
            options: [
                ("timezone".to_string(), "Europe/Stockholm".to_string()),
                ("max_handoffs".to_string(), "6".to_string()),
            ]
            .into_iter()
            .collect(),
        })
        .await
        .unwrap();
    let participant = {
        let db = db.lock().unwrap();
        ParticipantRepo::get(db.conn(), EVENT, "user-join")
            .unwrap()
            .unwrap()
    };
    assert_eq!(participant.membership, Membership::SignedUp);
    assert_eq!(participant.timezone, "Europe/Stockholm");
    assert_eq!(participant.max_handoffs, 6);
    let cursor = {
        let db = db.lock().unwrap();
        DmCursorRepo::get(db.conn(), "user-join").unwrap().unwrap()
    };
    assert!(cursor.last_seen_message_id.is_some());
    service
        .process(InboundEvent::SlashCommand {
            interaction: InteractionRef::new("i-join-2", "token"),
            user_id: DiscordUserId::new("user-join"),
            guild_id: None,
            command: SlashCommand::Join,
            options: Default::default(),
        })
        .await
        .unwrap();
    let participant = {
        let db = db.lock().unwrap();
        ParticipantRepo::get(db.conn(), EVENT, "user-join")
            .unwrap()
            .unwrap()
    };
    assert_eq!(participant.timezone, "Europe/Stockholm");
    assert_eq!(participant.max_handoffs, 6);
    assert!(
        discord
            .sent_dms()
            .iter()
            .any(|dm| dm.message.content.contains("signed up"))
    );
}

#[tokio::test]
async fn queued_attachments_keep_their_message_order() {
    use std::sync::{Arc, Mutex};

    use swap_discord::{
        DiscordChannelId, DiscordUserId, DmAttachment, FakeAttachmentFetcher, InboundEvent,
        InboundHandler, MessageId,
    };
    use swap_engine::{FixedClock, InboundService};

    let fixture = Fixture::new(3, 0);
    fixture.lock_and_schedule();
    fixture.start();
    let offers = fixture.open(0);
    let (user_id, assignment_id, track_code) = offers[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        T4,
    )
    .unwrap();

    let discord = Arc::new(swap_discord::FakeDiscord::new());
    let fetcher = Arc::new(FakeAttachmentFetcher::new());
    let first = seed_bytes(91);
    let second = seed_bytes(92);
    let first_url = "https://cdn.discordapp.com/attachments/1/2/first.it";
    let second_url = "https://cdn.discordapp.com/attachments/1/2/second.it";
    fetcher.add(first_url, &first);
    fetcher.add(second_url, &second);
    let db = Arc::new(Mutex::new(fixture.db));
    let service = InboundService::new(
        db.clone(),
        fixture.store.clone(),
        Arc::new(PassthroughPreparer),
        discord,
        fetcher,
        Arc::new(FixedClock(T5)),
    );
    for (message_id, url, bytes) in [("m-1", first_url, &first), ("m-2", second_url, &second)] {
        service
            .handle(InboundEvent::DmAttachment {
                user_id: DiscordUserId::new(user_id.clone()),
                channel_id: DiscordChannelId::new("dm-1"),
                message_id: MessageId::new(message_id),
                author_name: None,
                attachment: DmAttachment {
                    id: format!("a-{message_id}"),
                    filename: "edit.it".to_string(),
                    size: bytes.len() as u64,
                    url: url.to_string(),
                },
            })
            .await
            .unwrap();
    }
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let count = {
                let db = db.lock().unwrap();
                FileRepo::list_for_track(db.conn(), EVENT, &track_code)
                    .unwrap()
                    .len()
            };
            if count >= 3 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let versions = {
        let db = db.lock().unwrap();
        FileRepo::list_for_track(db.conn(), EVENT, &track_code).unwrap()
    };
    assert_eq!(versions.len(), 3);
    assert_eq!(versions[2].sha256, sha256_hex(&second));
}

#[tokio::test]
async fn stopped_queues_reject_new_events() {
    use std::sync::{Arc, Mutex};

    use swap_discord::{
        DiscordChannelId, DiscordUserId, DmAttachment, FakeAttachmentFetcher, InboundEvent,
        InboundHandler, MessageId,
    };
    use swap_engine::{FixedClock, InboundService};

    let directory = tempfile::tempdir().unwrap();
    let db = Arc::new(Mutex::new(Db::open_in_memory().unwrap()));
    let service = InboundService::new(
        db,
        BlobStore::new(directory.path().join("blobs")),
        Arc::new(PassthroughPreparer),
        Arc::new(swap_discord::FakeDiscord::new()),
        Arc::new(FakeAttachmentFetcher::new()),
        Arc::new(FixedClock(T5)),
    );
    service.stop_queues().await;
    let error = service
        .handle(InboundEvent::DmAttachment {
            user_id: DiscordUserId::new("u-1"),
            channel_id: DiscordChannelId::new("dm-1"),
            message_id: MessageId::new("m-1"),
            author_name: None,
            attachment: DmAttachment {
                id: "a-1".to_string(),
                filename: "late.it".to_string(),
                size: 4,
                url: "https://cdn.discordapp.com/attachments/1/2/late.it".to_string(),
            },
        })
        .await;
    assert!(error.is_err());
    service.stop_queues().await;
}

#[tokio::test]
async fn an_offer_holder_cannot_submit_before_a_claim() {
    use std::sync::{Arc, Mutex};

    use swap_discord::{
        DiscordChannelId, DiscordUserId, DmAttachment, FakeAttachmentFetcher, InboundEvent,
        MessageId,
    };
    use swap_engine::{FixedClock, InboundService};

    let directory = tempfile::tempdir().unwrap();
    let store = BlobStore::new(directory.path().join("blobs"));
    let db = Db::open_in_memory().unwrap();
    create_event(&db, EVENT, "Test Swap", 42, &EventSettings::default(), T0).unwrap();
    for index in 0..3 {
        join(&db, &store, index);
    }
    lock_roster(&db, EVENT, T1).unwrap();
    generate_schedule(&db, EVENT, T2).unwrap();
    start_event(&db, EVENT, T3).unwrap();
    let offers = open_round(&db, EVENT, 0, T4).unwrap();
    let user_id = match &offers.effects[0] {
        Effect::Offer { user_id, .. } => user_id.clone(),
        other => panic!("expected an offer, got {other:?}"),
    };
    let discord = Arc::new(swap_discord::FakeDiscord::new());
    let service = InboundService::new(
        Arc::new(Mutex::new(db)),
        store,
        Arc::new(PassthroughPreparer),
        discord.clone(),
        Arc::new(FakeAttachmentFetcher::new()),
        Arc::new(FixedClock(T5)),
    );
    service
        .process(InboundEvent::DmAttachment {
            user_id: DiscordUserId::new(user_id),
            channel_id: DiscordChannelId::new("dm-1"),
            message_id: MessageId::new("m-1"),
            author_name: None,
            attachment: DmAttachment {
                id: "a-1".to_string(),
                filename: "edit.it".to_string(),
                size: 10,
                url: "https://cdn.discordapp.com/attachments/1/2/edit.it".to_string(),
            },
        })
        .await
        .unwrap();
    assert_eq!(discord.sent_files().len(), 0);
    assert!(
        discord
            .sent_dms()
            .iter()
            .any(|sent| sent.message.content.contains("no active track"))
    );
}

#[test]
fn an_attachment_source_is_marked_in_the_submit_transaction() {
    let fixture = Fixture::new(3, 0);
    let assignments = fixture.lock_and_schedule();
    fixture.start();
    let offers = fixture.open(0);
    let (user_id, assignment_id, track_code) = offers[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        T4,
    )
    .unwrap();
    let bytes = seed_bytes(55);
    let request = SubmitRequest {
        event_id: EVENT,
        user_id: &user_id,
        filename: "edit.it",
        bytes: &bytes,
    };
    let source = swap_engine::SubmissionSource {
        message_id: "m-1",
        attachment_id: "a-1",
    };
    swap_engine::submit_attachment(
        &fixture.db,
        &fixture.store,
        &PassthroughPreparer,
        &request,
        source,
        T5,
    )
    .unwrap();
    let error = swap_engine::submit_attachment(
        &fixture.db,
        &fixture.store,
        &PassthroughPreparer,
        &request,
        source,
        T5,
    )
    .unwrap_err();
    assert!(matches!(error, EngineError::AttachmentAlreadyProcessed));
    let versions = FileRepo::list_for_track(fixture.db.conn(), EVENT, &track_code).unwrap();
    assert_eq!(versions.len(), 2);
    assert!(!assignments.is_empty());
}
