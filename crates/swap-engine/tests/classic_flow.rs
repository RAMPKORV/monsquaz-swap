use swap_core::{AssignmentStatus, EventState, Membership, Role, TrackState};
use swap_db::{
    AssignmentRepo, Db, DeliveryRepo, EventRepo, FileRepo, ParticipantRepo, PlanRepo, SeenRepo,
    TrackRepo,
};
use swap_engine::{
    DeliveryPreparer, Effect, EngineError, EventSettings, JoinRequest, PassthroughPreparer,
    PreparedBytes, SubmitRequest, claim, create_event, dispatch, dispatch_pending, event_status,
    expire_offer, generate_schedule, join_with_seed, lock_roster, open_round, reveal, signup, skip,
    start_event, submit,
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

#[test]
fn the_classic_loop_reaches_a_reveal() {
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
    for (_, assignment_id, _) in &round_two {
        expire_offer(&fixture.db, EVENT, assignment_id, T4 + 86_401).unwrap();
    }

    let revealed = reveal(&fixture.db, EVENT, T6).unwrap();
    assert_eq!(revealed.tracks.len(), 4);
    for track in &revealed.tracks {
        assert_eq!(track.entries.len(), 1);
        assert_eq!(track.entries[0].status, AssignmentStatus::Submitted);
        assert_eq!(track.entries[0].round, 0);
    }
    let status = event_status(&fixture.db, EVENT).unwrap();
    assert_eq!(status.state, EventState::Reveal);
    assert_eq!(status.rounds, 3);
    assert_eq!(status.submitted_assignments, 4);
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
    assert_eq!(DeliveryRepo::count_pending(fixture.db.conn()).unwrap(), 4);

    let discord = FakeDiscord::new();
    let report = dispatch_pending(&fixture.db, &fixture.store, &discord, T5, 100)
        .await
        .unwrap();
    assert_eq!(report.sent, 4);
    assert_eq!(DeliveryRepo::count_pending(fixture.db.conn()).unwrap(), 0);
    assert_eq!(discord.sent_dms().len(), 4);

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
        assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 3);
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
    assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 3);
    let discord = FakeDiscord::new();
    let report = dispatch_pending(&db, &store, &discord, T5, 100)
        .await
        .unwrap();
    assert_eq!(report.sent, 3);
    assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 0);
    drop(db);

    let db = Db::open(&database).unwrap();
    let report = dispatch_pending(&db, &store, &discord, T6, 100)
        .await
        .unwrap();
    assert_eq!(report.sent, 0);
    assert_eq!(discord.sent_dms().len(), 3);
}

#[tokio::test]
async fn an_attachment_is_downloaded_stored_and_processed_once() {
    use std::sync::{Arc, Mutex};

    use swap_db::ProcessedAttachmentRepo;
    use swap_discord::{
        DiscordChannelId, DiscordUserId, DmAttachment, FakeAttachmentFetcher, InboundEvent,
        InboundHandler, MessageId,
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
        attachment: DmAttachment {
            id: "a-1".to_string(),
            filename: "edit.it".to_string(),
            size: edit.len() as u64,
            url: url.to_string(),
        },
    };
    service.handle(event.clone()).await.unwrap();

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

    service.handle(event).await.unwrap();
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
        DiscordUserId, FakeAttachmentFetcher, InboundEvent, InboundHandler, InteractionRef,
        SlashCommand,
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
        .handle(InboundEvent::SlashCommand {
            interaction: InteractionRef::new("i-1", "token"),
            user_id: DiscordUserId::new(user_id),
            guild_id: None,
            command: SlashCommand::Claim,
            options: Default::default(),
        })
        .await
        .unwrap();

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
    assert_eq!(service.backfill().await.unwrap(), 1);
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
async fn an_offer_holder_cannot_submit_before_a_claim() {
    use std::sync::{Arc, Mutex};

    use swap_discord::{
        DiscordChannelId, DiscordUserId, DmAttachment, FakeAttachmentFetcher, InboundEvent,
        InboundHandler, MessageId,
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
        .handle(InboundEvent::DmAttachment {
            user_id: DiscordUserId::new(user_id),
            channel_id: DiscordChannelId::new("dm-1"),
            message_id: MessageId::new("m-1"),
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
