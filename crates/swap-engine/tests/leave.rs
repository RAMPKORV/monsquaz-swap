//! Leave the event with confirmation and host restore (D-051, MSW-109).

use std::sync::{Arc, Mutex};

use swap_core::{AssignmentStatus, Membership, TrackState};
use swap_db::{
    AssignmentRepo, AuditRepo, Db, DeliveryRepo, FileRepo, FillInChainRepo, MetaRepo, PackRecord,
    PackRepo, ParticipantRepo, PlanAssignmentRecord, PlanRepo, SeenRepo, TrackRepo,
};
use swap_discord::{
    Action, CustomId, DiscordUserId, FakeAttachmentFetcher, FakeDiscord, InboundEvent,
    InteractionRef, SlashCommand,
};
use swap_engine::{
    Effect, EngineError, EventSettings, FillInFlow, FixedClock, InboundService, JoinRequest,
    LEAVE_CONFIRM_SECS, LeaveActor, PassthroughPreparer, SubmitRequest, answer_restore, claim,
    create_event_with_secret, generate_schedule, join_with_seed, leave_event, lock_roster,
    open_round, open_seed_round, request_restore, start_event, submit, tick_deadlines,
};
use swap_files::BlobStore;

const EVENT: &str = "evt-1";
const SECRET: &str = "0000000000000000000000000000002a";
const T0: i64 = 1_000;
const T1: i64 = 2_000;
const T2: i64 = 2_100;
const T3: i64 = 2_200;
const T4: i64 = 2_300;

struct Fixture {
    db: Db,
    store: BlobStore,
    plan: Vec<PlanAssignmentRecord>,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn seeded(seeded: usize, settings: EventSettings) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let db = Db::open(directory.path().join("swap.db")).unwrap();
        let store = BlobStore::new(directory.path().join("blobs"));
        create_event_with_secret(&db, EVENT, "Leave Swap", 42, &settings, Some(SECRET), T0)
            .unwrap();
        MetaRepo::set(db.conn(), "host.current_event", EVENT).unwrap();
        let blob = store.store(b"IMPMtest-pack").unwrap();
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
        open_seed_round(&db, EVENT, T0 + 1_000_000, None, T0).unwrap();
        for index in 0..seeded {
            let user_id = user(index);
            let bytes = seed_bytes(index as u8);
            join_with_seed(
                &db,
                &store,
                &PassthroughPreparer,
                &JoinRequest {
                    event_id: EVENT,
                    user_id: &user_id,
                    display_name: &format!("User {index}"),
                    timezone: "UTC",
                    max_handoffs: 20,
                    filename: "seed.it",
                    bytes: &bytes,
                },
                T0,
            )
            .unwrap();
        }
        Self {
            db,
            store,
            plan: Vec::new(),
            _directory: directory,
        }
    }

    fn started(seeded: usize, settings: EventSettings) -> Self {
        let mut fixture = Self::seeded(seeded, settings);
        fixture.lock_and_start();
        fixture
    }

    fn lock_and_start(&mut self) {
        lock_roster(&self.db, EVENT, T1).unwrap();
        self.plan = generate_schedule(&self.db, EVENT, T2).unwrap().assignments;
        start_event(&self.db, EVENT, T3).unwrap();
    }

    fn claim_and_submit(&self, user_id: &str, assignment_id: &str, now: i64) {
        claim(&self.db, &self.store, EVENT, assignment_id, user_id, now).unwrap();
        let bytes = seed_bytes(200);
        submit(
            &self.db,
            &self.store,
            &PassthroughPreparer,
            &SubmitRequest {
                event_id: EVENT,
                user_id,
                filename: "edit.it",
                bytes: &bytes,
            },
            now + 1,
        )
        .unwrap();
    }

    fn membership(&self, user_id: &str) -> Membership {
        ParticipantRepo::get(self.db.conn(), EVENT, user_id)
            .unwrap()
            .unwrap()
            .membership
    }

    fn pending_for(&self, user_id: &str) -> Vec<swap_db::DeliveryRecord> {
        DeliveryRepo::pending(self.db.conn(), 1_000)
            .unwrap()
            .into_iter()
            .filter(|row| row.recipient_id.as_deref() == Some(user_id))
            .collect()
    }

    fn audit_actions(&self) -> Vec<(String, String)> {
        AuditRepo::list_for_event(self.db.conn(), EVENT)
            .unwrap()
            .into_iter()
            .map(|entry| (entry.actor, entry.action))
            .collect()
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

fn offers(effects: &[Effect]) -> Vec<(String, String, String)> {
    effects
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
        .collect()
}

#[test]
fn a_leave_skips_a_claimed_track_as_seen_and_returns_it_unchanged() {
    let fixture = Fixture::started(4, EventSettings::default());
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (leaver, assignment_id, track) = round_zero[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &leaver,
        T4 + 1,
    )
    .unwrap();
    let versions_before = FileRepo::list_for_track(fixture.db.conn(), EVENT, &track)
        .unwrap()
        .len();

    let outcome =
        leave_event(&fixture.db, EVENT, &leaver, LeaveActor::Participant, T4 + 2).unwrap();

    let skipped = outcome.skipped.expect("the held track is skipped");
    assert_eq!(skipped.status, AssignmentStatus::SkippedAfterClaim);
    let assignment = AssignmentRepo::get(fixture.db.conn(), &assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(assignment.status, AssignmentStatus::SkippedAfterClaim);
    assert!(SeenRepo::has(fixture.db.conn(), EVENT, &track, &leaver).unwrap());
    let seen_assignment: Option<String> = fixture
        .db
        .conn()
        .query_row(
            "SELECT assignment_id FROM seen_pairs
             WHERE event_id = ?1 AND track_code = ?2 AND participant_id = ?3",
            [EVENT, track.as_str(), leaver.as_str()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(seen_assignment.as_deref(), Some(assignment_id.as_str()));
    assert_eq!(
        TrackRepo::get(fixture.db.conn(), EVENT, &track)
            .unwrap()
            .unwrap()
            .state,
        TrackState::Pool
    );
    assert_eq!(
        FileRepo::list_for_track(fixture.db.conn(), EVENT, &track)
            .unwrap()
            .len(),
        versions_before,
        "the track returns unchanged"
    );
    assert_eq!(fixture.membership(&leaver), Membership::Left);
}

#[test]
fn a_leave_before_a_claim_is_not_seen() {
    let fixture = Fixture::started(4, EventSettings::default());
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (leaver, assignment_id, track) = round_zero[0].clone();

    leave_event(&fixture.db, EVENT, &leaver, LeaveActor::Participant, T4 + 2).unwrap();

    let assignment = AssignmentRepo::get(fixture.db.conn(), &assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(assignment.status, AssignmentStatus::SkippedBeforeClaim);
    assert!(!SeenRepo::has(fixture.db.conn(), EVENT, &track, &leaver).unwrap());
}

#[test]
fn a_leave_cancels_every_queued_dm_and_writes_an_audit_entry() {
    let fixture = Fixture::started(4, EventSettings::default());
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let leaver = round_zero[0].0.clone();
    assert!(
        !fixture.pending_for(&leaver).is_empty(),
        "the offer is queued"
    );

    let outcome =
        leave_event(&fixture.db, EVENT, &leaver, LeaveActor::Participant, T4 + 2).unwrap();

    assert!(outcome.cancelled_deliveries >= 1);
    assert!(fixture.pending_for(&leaver).is_empty());
    let actions = fixture.audit_actions();
    assert!(actions.contains(&("Participant".to_string(), "Participant left".to_string())));
    assert!(actions.contains(&(
        "Participant".to_string(),
        "Deliveries cancelled".to_string()
    )));
}

#[test]
fn no_dm_is_queued_for_a_leaver_after_the_leave() {
    let fixture = Fixture::started(4, EventSettings::default());
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let leaver = round_zero[0].0.clone();
    leave_event(&fixture.db, EVENT, &leaver, LeaveActor::Participant, T4 + 2).unwrap();
    for (user_id, assignment_id, _) in round_zero.iter().skip(1) {
        fixture.claim_and_submit(user_id, assignment_id, T4 + 3);
    }
    tick_deadlines(&fixture.db, T4 + 1_000_000).unwrap();
    open_round(&fixture.db, EVENT, 1, T4 + 1_000_001).unwrap();
    tick_deadlines(&fixture.db, T4 + 2_000_000).unwrap();

    assert!(fixture.pending_for(&leaver).is_empty());
}

#[test]
fn a_leave_starts_the_fill_in_flow_and_alerts_the_host() {
    let settings = EventSettings {
        fill_in_flow: FillInFlow::AutoChain,
        ..EventSettings::default()
    };
    let fixture = Fixture::started(5, settings);
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (leaver, _, track) = round_zero[0].clone();
    for (user_id, assignment_id, _) in round_zero.iter().skip(1) {
        fixture.claim_and_submit(user_id, assignment_id, T4 + 1);
    }

    leave_event(
        &fixture.db,
        EVENT,
        &leaver,
        LeaveActor::Participant,
        T4 + 10,
    )
    .unwrap();

    let chains = FillInChainRepo::list_for_event(fixture.db.conn(), EVENT).unwrap();
    assert!(
        chains.iter().any(|chain| chain.track_code == track),
        "the fill-in flow starts for the stranded slot"
    );
    let alerts: Vec<_> = DeliveryRepo::pending(fixture.db.conn(), 1_000)
        .unwrap()
        .into_iter()
        .filter(|row| row.kind == "host_alert")
        .collect();
    assert!(!alerts.is_empty(), "the host gets an alert");
}

#[test]
fn a_round_open_skips_the_planned_turn_of_a_leaver_as_a_gap() {
    let fixture = Fixture::started(4, EventSettings::default());
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    for (user_id, assignment_id, _) in &round_zero {
        fixture.claim_and_submit(user_id, assignment_id, T4 + 1);
    }
    let leaver = round_zero[0].0.clone();
    leave_event(&fixture.db, EVENT, &leaver, LeaveActor::Host, T4 + 5).unwrap();

    let round_one = offers(&open_round(&fixture.db, EVENT, 1, T4 + 10).unwrap().effects);

    assert_eq!(round_one.len(), 3);
    assert!(round_one.iter().all(|(user_id, _, _)| *user_id != leaver));
    let actions = fixture.audit_actions();
    assert!(actions.contains(&("Host".to_string(), "Participant left".to_string())));
    assert!(
        actions
            .iter()
            .any(|(_, action)| action == "Planned turn skipped")
    );
}

#[test]
fn a_second_leave_is_refused() {
    let fixture = Fixture::started(3, EventSettings::default());
    leave_event(&fixture.db, EVENT, &user(0), LeaveActor::Participant, T4).unwrap();
    assert!(matches!(
        leave_event(
            &fixture.db,
            EVENT,
            &user(0),
            LeaveActor::Participant,
            T4 + 1
        ),
        Err(EngineError::ParticipantLeft)
    ));
}

/// D-051 rule 4: a seed from a person who left before the roster lock keeps
/// circulating. The plan has one track more than people.
#[test]
fn a_seed_from_a_pre_lock_leaver_keeps_circulating() {
    let mut fixture = Fixture::seeded(5, EventSettings::default());
    let leaver = user(4);
    let orphan = TrackRepo::list_for_event(fixture.db.conn(), EVENT)
        .unwrap()
        .into_iter()
        .find(|track| track.owner_id == leaver)
        .unwrap()
        .code;
    leave_event(
        &fixture.db,
        EVENT,
        &leaver,
        LeaveActor::Participant,
        T0 + 10,
    )
    .unwrap();
    assert_eq!(fixture.membership(&leaver), Membership::Left);

    fixture.lock_and_start();

    assert!(
        fixture.plan.iter().any(|row| row.track_code == orphan),
        "the seed stays in the plan"
    );
    assert!(fixture.plan.iter().all(|row| row.participant_id != leaver));
    for round in 0..3 {
        let rows: Vec<_> = fixture
            .plan
            .iter()
            .filter(|row| row.round == round)
            .collect();
        assert_eq!(rows.len(), 4, "every person works in round {round}");
    }
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    assert_eq!(round_zero.len(), 4);
}

#[test]
fn a_restore_needs_the_persons_consent() {
    let fixture = Fixture::started(4, EventSettings::default());
    let leaver = user(0);
    leave_event(&fixture.db, EVENT, &leaver, LeaveActor::Participant, T4).unwrap();

    request_restore(&fixture.db, EVENT, &leaver, T4 + 1).unwrap();
    assert_eq!(fixture.membership(&leaver), Membership::Left);
    let pending = fixture.pending_for(&leaver);
    assert_eq!(pending.len(), 1, "the bot sends one restore DM");
    assert_eq!(pending[0].kind, "restore");
    assert!(matches!(
        request_restore(&fixture.db, EVENT, &leaver, T4 + 2),
        Err(EngineError::RestorePending)
    ));

    answer_restore(&fixture.db, EVENT, &leaver, false, T4 + 3).unwrap();
    assert_eq!(fixture.membership(&leaver), Membership::Left);
    assert!(matches!(
        answer_restore(&fixture.db, EVENT, &leaver, true, T4 + 4),
        Err(EngineError::NoPendingRestore)
    ));

    request_restore(&fixture.db, EVENT, &leaver, T4 + 5).unwrap();
    answer_restore(&fixture.db, EVENT, &leaver, true, T4 + 6).unwrap();
    assert_eq!(fixture.membership(&leaver), Membership::Active);
    let actions = fixture.audit_actions();
    assert!(actions.contains(&("Host".to_string(), "Restore requested".to_string())));
    assert!(actions.contains(&("Participant".to_string(), "Restore declined".to_string())));
    assert!(actions.contains(&(
        "Participant".to_string(),
        "Participant restored".to_string()
    )));
}

#[test]
fn a_restore_is_only_for_a_person_who_left() {
    let fixture = Fixture::started(3, EventSettings::default());
    assert!(matches!(
        request_restore(&fixture.db, EVENT, &user(0), T4),
        Err(EngineError::ParticipantNotLeft)
    ));
}

#[test]
fn a_restored_person_gets_planned_turns_from_the_next_round() {
    let fixture = Fixture::started(4, EventSettings::default());
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (leaver, assignment_id, _) = round_zero[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &leaver,
        T4 + 1,
    )
    .unwrap();
    leave_event(&fixture.db, EVENT, &leaver, LeaveActor::Participant, T4 + 2).unwrap();
    for (user_id, assignment_id, _) in round_zero.iter().skip(1) {
        fixture.claim_and_submit(user_id, assignment_id, T4 + 3);
    }
    request_restore(&fixture.db, EVENT, &leaver, T4 + 5).unwrap();
    answer_restore(&fixture.db, EVENT, &leaver, true, T4 + 6).unwrap();

    let round_one = offers(&open_round(&fixture.db, EVENT, 1, T4 + 10).unwrap().effects);

    assert!(round_one.iter().any(|(user_id, _, _)| *user_id == leaver));
    let skipped = AssignmentRepo::get(fixture.db.conn(), &assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        skipped.status,
        AssignmentStatus::SkippedAfterClaim,
        "an auto-skipped turn stays a skip"
    );
}

fn service(
    db: Db,
    store: BlobStore,
    discord: Arc<FakeDiscord>,
    now: i64,
) -> (InboundService, Arc<Mutex<Db>>) {
    let db = Arc::new(Mutex::new(db));
    let service = InboundService::new(
        db.clone(),
        store,
        Arc::new(PassthroughPreparer),
        discord,
        Arc::new(FakeAttachmentFetcher::new()),
        Arc::new(FixedClock(now)),
    );
    (service, db)
}

fn leave_command(user_id: &str) -> InboundEvent {
    InboundEvent::SlashCommand {
        interaction: InteractionRef::new("i-1", "token"),
        user_id: DiscordUserId::new(user_id),
        guild_id: None,
        command: SlashCommand::Leave,
        options: Default::default(),
    }
}

fn button(user_id: &str, custom_id: &str) -> InboundEvent {
    InboundEvent::Button {
        interaction: InteractionRef::new("i-2", "token"),
        user_id: DiscordUserId::new(user_id),
        guild_id: None,
        custom_id: CustomId::parse(custom_id).unwrap(),
    }
}

#[tokio::test]
async fn the_leave_command_shows_a_warning_and_changes_nothing() {
    let fixture = Fixture::started(3, EventSettings::default());
    let discord = Arc::new(FakeDiscord::new());
    let (service, db) = service(fixture.db, fixture.store, discord.clone(), T4);

    service.process(leave_command(&user(0))).await.unwrap();

    let replies = discord.ephemeral_replies();
    assert_eq!(replies.len(), 1);
    let reply = &replies[0].reply;
    assert!(reply.content.contains("returns unchanged"));
    assert!(reply.content.contains("Only the host"));
    let confirm = CustomId::parse(&reply.buttons[0].custom_id).unwrap();
    assert_eq!(confirm.action(), Action::ConfirmLeave);
    assert_eq!(confirm.event_id(), EVENT);
    assert!(discord.sent_dms().is_empty(), "the warning is not a DM");
    let db = db.lock().unwrap();
    assert_eq!(
        ParticipantRepo::get(db.conn(), EVENT, &user(0))
            .unwrap()
            .unwrap()
            .membership,
        Membership::Active
    );
}

#[tokio::test]
async fn the_leave_button_on_a_dm_shows_the_same_warning() {
    let fixture = Fixture::started(3, EventSettings::default());
    let discord = Arc::new(FakeDiscord::new());
    let (service, _db) = service(fixture.db, fixture.store, discord.clone(), T4);

    service
        .process(button(&user(1), "swap:evt-1:leave:"))
        .await
        .unwrap();

    let replies = discord.ephemeral_replies();
    assert_eq!(replies.len(), 1);
    assert!(
        replies[0]
            .reply
            .content
            .contains("A seed that you own stays")
    );
    assert!(discord.sent_dms().is_empty());
}

#[tokio::test]
async fn a_confirm_within_15_minutes_leaves_without_a_dm() {
    let fixture = Fixture::started(3, EventSettings::default());
    let discord = Arc::new(FakeDiscord::new());
    let issued = T4;
    let (service, db) = service(
        fixture.db,
        fixture.store,
        discord.clone(),
        issued + LEAVE_CONFIRM_SECS - 1,
    );

    service
        .process(button(&user(0), &format!("swap:evt-1:leave_ok:{issued}")))
        .await
        .unwrap();
    service.drain_pending().await.unwrap();

    {
        let db = db.lock().unwrap();
        assert_eq!(
            ParticipantRepo::get(db.conn(), EVENT, &user(0))
                .unwrap()
                .unwrap()
                .membership,
            Membership::Left
        );
    }
    assert_eq!(discord.ephemeral_replies().len(), 1);
    assert!(
        discord
            .sent_dms()
            .iter()
            .all(|dm| dm.user_id != DiscordUserId::new(user(0))),
        "the leaver gets no DM"
    );
    assert!(discord.sent_files().is_empty());
}

#[tokio::test]
async fn an_expired_confirmation_does_nothing_and_says_so() {
    let fixture = Fixture::started(3, EventSettings::default());
    let discord = Arc::new(FakeDiscord::new());
    let issued = T4;
    let (service, db) = service(
        fixture.db,
        fixture.store,
        discord.clone(),
        issued + LEAVE_CONFIRM_SECS + 1,
    );

    service
        .process(button(&user(0), &format!("swap:evt-1:leave_ok:{issued}")))
        .await
        .unwrap();

    let db = db.lock().unwrap();
    assert_eq!(
        ParticipantRepo::get(db.conn(), EVENT, &user(0))
            .unwrap()
            .unwrap()
            .membership,
        Membership::Active
    );
    let replies = discord.ephemeral_replies();
    assert_eq!(replies.len(), 1);
    assert!(replies[0].reply.content.contains("expired"));
}

#[tokio::test]
async fn a_leaver_gets_an_ephemeral_answer_and_no_dm_for_other_commands() {
    let fixture = Fixture::started(3, EventSettings::default());
    leave_event(&fixture.db, EVENT, &user(0), LeaveActor::Participant, T4).unwrap();
    let discord = Arc::new(FakeDiscord::new());
    let (service, _db) = service(fixture.db, fixture.store, discord.clone(), T4 + 5);

    service
        .process(InboundEvent::SlashCommand {
            interaction: InteractionRef::new("i-3", "token"),
            user_id: DiscordUserId::new(user(0)),
            guild_id: None,
            command: SlashCommand::Claim,
            options: Default::default(),
        })
        .await
        .unwrap();

    assert!(discord.sent_dms().is_empty());
    let replies = discord.ephemeral_replies();
    assert_eq!(replies.len(), 1);
    assert!(replies[0].reply.content.contains("left"));
}

#[tokio::test]
async fn an_offer_dm_and_a_track_file_carry_a_leave_button() {
    let fixture = Fixture::started(3, EventSettings::default());
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, assignment_id, _) = round_zero[0].clone();
    let discord = Arc::new(FakeDiscord::new());
    let store = fixture.store.clone();
    let (service, db) = service(fixture.db, fixture.store, discord.clone(), T4 + 1);
    service.drain_pending().await.unwrap();
    {
        let db = db.lock().unwrap();
        claim(&db, &store, EVENT, &assignment_id, &holder, T4 + 1).unwrap();
    }
    service.drain_pending().await.unwrap();

    let offer = discord
        .sent_dms()
        .into_iter()
        .find(|dm| {
            dm.user_id == DiscordUserId::new(holder.clone())
                && dm.message.content.contains("is available")
        })
        .unwrap();
    let leave = CustomId::parse(&offer.message.buttons[0].custom_id).unwrap();
    assert_eq!(leave.action(), Action::Leave);
    assert_eq!(leave.event_id(), EVENT);
    let file = discord.sent_files().pop().unwrap();
    assert_eq!(
        CustomId::parse(&file.file.buttons[0].custom_id)
            .unwrap()
            .action(),
        Action::Leave
    );
}

#[tokio::test]
async fn the_restore_dm_asks_for_consent_and_confirm_restores() {
    let fixture = Fixture::started(3, EventSettings::default());
    leave_event(&fixture.db, EVENT, &user(0), LeaveActor::Participant, T4).unwrap();
    request_restore(&fixture.db, EVENT, &user(0), T4 + 1).unwrap();
    let discord = Arc::new(FakeDiscord::new());
    let (service, db) = service(fixture.db, fixture.store, discord.clone(), T4 + 2);
    service.drain_pending().await.unwrap();

    let dms: Vec<_> = discord
        .sent_dms()
        .into_iter()
        .filter(|dm| dm.user_id == DiscordUserId::new(user(0)))
        .collect();
    assert_eq!(dms.len(), 1, "the leaver gets only the restore DM");
    assert!(
        dms[0]
            .message
            .content
            .contains("The host asks you to return to the event")
    );
    let actions: Vec<Action> = dms[0]
        .message
        .buttons
        .iter()
        .map(|button| CustomId::parse(&button.custom_id).unwrap().action())
        .collect();
    assert_eq!(
        actions,
        vec![Action::ConfirmRestore, Action::DeclineRestore]
    );

    service
        .process(button(&user(0), &dms[0].message.buttons[0].custom_id))
        .await
        .unwrap();
    let db = db.lock().unwrap();
    assert_eq!(
        ParticipantRepo::get(db.conn(), EVENT, &user(0))
            .unwrap()
            .unwrap()
            .membership,
        Membership::Active
    );
    assert_eq!(discord.ephemeral_replies().len(), 1);
}

#[test]
fn a_restore_keeps_a_paused_person_paused() {
    use swap_core::Readiness;

    let fixture = Fixture::started(3, EventSettings::default());
    {
        let mut participant = ParticipantRepo::get(fixture.db.conn(), EVENT, &user(0))
            .unwrap()
            .unwrap();
        participant.readiness = Readiness::Paused;
        ParticipantRepo::upsert(fixture.db.conn(), &participant).unwrap();
    }
    leave_event(&fixture.db, EVENT, &user(0), LeaveActor::Host, T4).unwrap();
    request_restore(&fixture.db, EVENT, &user(0), T4 + 1).unwrap();
    assert_eq!(
        answer_restore(&fixture.db, EVENT, &user(0), true, T4 + 2).unwrap(),
        swap_engine::RestoreAnswer::Restored
    );
    let participant = ParticipantRepo::get(fixture.db.conn(), EVENT, &user(0))
        .unwrap()
        .unwrap();
    assert_eq!(participant.membership, Membership::Active);
    assert_eq!(
        participant.readiness,
        Readiness::Paused,
        "a restore must not silently unpause the person"
    );
}

#[test]
fn a_restore_in_schedule_review_invalidates_the_draft_plan() {
    let mut fixture = Fixture::seeded(4, EventSettings::default());
    lock_roster(&fixture.db, EVENT, T1).unwrap();
    fixture.plan = generate_schedule(&fixture.db, EVENT, T2)
        .unwrap()
        .assignments;
    leave_event(&fixture.db, EVENT, &user(0), LeaveActor::Host, T2 + 1).unwrap();
    request_restore(&fixture.db, EVENT, &user(0), T2 + 2).unwrap();
    answer_restore(&fixture.db, EVENT, &user(0), true, T2 + 3).unwrap();

    assert!(
        PlanRepo::latest(fixture.db.conn(), EVENT)
            .unwrap()
            .is_none(),
        "the draft plan must be void"
    );
    assert!(
        fixture
            .audit_actions()
            .iter()
            .any(|(_, action)| { action == "Draft schedule invalidated" })
    );
    let pending = DeliveryRepo::pending(fixture.db.conn(), 100).unwrap();
    assert!(pending.iter().any(|row| {
        String::from_utf8_lossy(&row.payload).contains("Generate the schedule again")
    }));
    assert!(matches!(
        start_event(&fixture.db, EVENT, T3),
        Err(EngineError::NoPlan)
    ));
    let regenerated = generate_schedule(&fixture.db, EVENT, T3).unwrap();
    assert_eq!(
        regenerated.version, 2,
        "a new plan after a void must not reuse version 1"
    );
}

#[test]
fn a_restore_invitation_expires_after_seven_days() {
    let fixture = Fixture::started(3, EventSettings::default());
    leave_event(&fixture.db, EVENT, &user(0), LeaveActor::Host, T4).unwrap();
    request_restore(&fixture.db, EVENT, &user(0), T4 + 1).unwrap();
    let expired = answer_restore(
        &fixture.db,
        EVENT,
        &user(0),
        true,
        T4 + 1 + swap_engine::RESTORE_REQUEST_SECS + 1,
    );
    assert!(matches!(expired, Err(EngineError::NoPendingRestore)));
    assert_eq!(fixture.membership(&user(0)), Membership::Left);
    assert!(
        fixture
            .audit_actions()
            .iter()
            .any(|(_, action)| action == "Restore expired")
    );
    request_restore(&fixture.db, EVENT, &user(0), T4 + 1 + 8 * 86_400).unwrap();
    answer_restore(&fixture.db, EVENT, &user(0), true, T4 + 1 + 8 * 86_400).unwrap();
    assert_eq!(fixture.membership(&user(0)), Membership::Active);
}

#[test]
fn a_new_restore_request_replaces_an_expired_pending_invitation() {
    let fixture = Fixture::started(3, EventSettings::default());
    leave_event(&fixture.db, EVENT, &user(0), LeaveActor::Host, T4).unwrap();
    request_restore(&fixture.db, EVENT, &user(0), T4 + 1).unwrap();
    let fresh = T4 + 1 + swap_engine::RESTORE_REQUEST_SECS + 1;
    request_restore(&fixture.db, EVENT, &user(0), fresh)
        .expect("an expired pending invitation must not block a new one");
    let pending = swap_db::RestoreRepo::pending(fixture.db.conn(), EVENT, &user(0))
        .unwrap()
        .unwrap();
    assert_eq!(pending.requested_at, fresh);
    assert!(
        fixture
            .audit_actions()
            .iter()
            .any(|(_, action)| action == "Restore expired")
    );
}
