//! Async expiry protection, re-offer, claim reminder, and the zoned release
//! (MSW-084..MSW-087).

use swap_core::{AssignmentStatus, Readiness};
use swap_db::{
    AssignmentRepo, Db, DeliveryRepo, ExpiredPairRepo, ExpiryStateRepo, HostDecisionRepo, MetaRepo,
    PackRecord, PackRepo, ParticipantRepo, SeenRepo, TrackRepo,
};
use swap_engine::{
    Effect, EngineError, EventSettings, ExpiryProtection, JoinRequest, PassthroughPreparer,
    ReofferAfterExpiry, claim, create_event_with_secret, decide_expiry, dispatch_async_wave,
    dispatch_pending, join_with_seed, lock_roster, next_async_release, open_decisions,
    open_seed_round, set_participant_ready, skip, start_async_event, tick_deadlines,
};
use swap_files::BlobStore;

const EVENT: &str = "evt-async";
const SECRET: &str = "0000000000000000000000000000002b";
const T0: i64 = 1_000;
const T1: i64 = 2_000;
const T2: i64 = 2_100;
const T3: i64 = 2_200;
const DAY: i64 = 24 * 60 * 60;

struct Fixture {
    db: Db,
    store: BlobStore,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn new(people: usize, settings: EventSettings) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let db = Db::open(directory.path().join("swap.db")).unwrap();
        let store = BlobStore::new(directory.path().join("blobs"));
        create_event_with_secret(&db, EVENT, "Async Swap", 42, &settings, Some(SECRET), T0)
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
        for index in 0..people {
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
                    timezone: "Europe/Stockholm",
                    max_handoffs: 20,
                    filename: "seed.it",
                    bytes: &bytes,
                },
                T0,
            )
            .unwrap();
        }
        lock_roster(&db, EVENT, T1).unwrap();
        start_async_event(&db, EVENT, T2).unwrap();
        Self {
            db,
            store,
            _directory: directory,
        }
    }

    fn offers(&self, now: i64) -> Vec<(String, String, String)> {
        offers(&dispatch_async_wave(&self.db, EVENT, now).unwrap().effects)
    }

    fn expire_all(&self, now: i64) {
        tick_deadlines(&self.db, now).unwrap();
    }

    fn readiness(&self, user_id: &str) -> Readiness {
        ParticipantRepo::get(self.db.conn(), EVENT, user_id)
            .unwrap()
            .unwrap()
            .readiness
    }

    fn state(&self, user_id: &str) -> Option<swap_db::ExpiryStateRecord> {
        ExpiryStateRepo::get(self.db.conn(), EVENT, user_id).unwrap()
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

fn async_settings() -> EventSettings {
    EventSettings {
        mode: swap_engine::Mode::Async,
        handoff_policy: Some("Flexible".to_string()),
        ..EventSettings::default()
    }
}

// scenario: The queue skips a participant in cooldown and reports why
#[test]
fn the_default_preset_starts_a_cooldown_and_skips_the_queue() {
    let fixture = Fixture::new(3, async_settings());
    let first = fixture.offers(T3);
    assert_eq!(first.len(), 3);
    fixture.expire_all(T3 + DAY + 1);

    for (user_id, _, _) in &first {
        let state = fixture.state(user_id).expect("the expiry is recorded");
        assert_eq!(state.strikes, 1);
        assert_eq!(state.cooldown_until, Some(T3 + DAY + 1 + DAY));
        assert_eq!(fixture.readiness(user_id), Readiness::Ready);
    }
    let skipped = dispatch_async_wave(&fixture.db, EVENT, T3 + DAY + 2).unwrap();
    assert_eq!(skipped.offers, 0);
    assert_eq!(skipped.skips.len(), 3);
    assert!(
        skipped
            .skips
            .iter()
            .all(|skip| skip.reason.contains("cooldown"))
    );
}

// scenario: The second expiry pauses the participant and alerts the host
// scenario: The Ready gate prevents a loop
#[test]
fn the_second_expiry_pauses_and_alerts_the_host() {
    let fixture = Fixture::new(3, async_settings());
    let first = fixture.offers(T3);
    fixture.expire_all(T3 + DAY + 1);

    // The cooldown ends, and the queue offers again.
    let second_at = T3 + 2 * DAY + 2;
    let second = fixture.offers(second_at);
    assert_eq!(second.len(), 3);
    fixture.expire_all(second_at + DAY + 1);

    for (user_id, _, _) in &first {
        assert_eq!(fixture.readiness(user_id), Readiness::Paused);
        let state = fixture.state(user_id).unwrap();
        assert_eq!(state.strikes, 2);
        assert!(state.paused_at.is_some());
    }
    let alerts = DeliveryRepo::pending(fixture.db.conn(), 100).unwrap();
    assert!(
        alerts.iter().filter(|row| row.kind == "host_alert").count() >= 3,
        "every auto-pause queues one host alert"
    );
    let audit = swap_db::AuditRepo::list_for_event(fixture.db.conn(), EVENT).unwrap();
    assert!(
        audit
            .iter()
            .any(|entry| entry.action == "Participant auto-paused")
    );
}

// scenario: A resume resets the expiry strikes
#[test]
fn a_resume_resets_the_strikes_and_the_cooldown() {
    let fixture = Fixture::new(3, async_settings());
    let first = fixture.offers(T3);
    fixture.expire_all(T3 + DAY + 1);
    let person = &first[0].0;
    assert!(fixture.state(person).is_some());

    set_participant_ready(&fixture.db, EVENT, person, true, T3 + DAY + 2).unwrap();
    assert!(fixture.state(person).is_none());
    assert!(
        swap_db::AuditRepo::list_for_event(fixture.db.conn(), EVENT)
            .unwrap()
            .iter()
            .any(|entry| entry.action == "Expiry state reset")
    );
}

// scenario: The cooldown-only preset never pauses a participant
#[test]
fn cooldown_only_keeps_the_participant_ready() {
    let settings = EventSettings {
        expiry_protection: ExpiryProtection::CooldownOnly,
        ..async_settings()
    };
    let fixture = Fixture::new(3, settings);
    let first = fixture.offers(T3);
    fixture.expire_all(T3 + DAY + 1);
    let second_at = T3 + 2 * DAY + 2;
    fixture.offers(second_at);
    fixture.expire_all(second_at + DAY + 1);

    for (user_id, _, _) in &first {
        assert_eq!(fixture.readiness(user_id), Readiness::Ready);
        let state = fixture.state(user_id).unwrap();
        assert_eq!(state.strikes, 2);
        assert!(state.cooldown_until.is_some());
    }
}

// scenario: The host-review preset opens a decision without an automatic pause
#[test]
fn host_review_opens_a_decision_without_an_automatic_pause() {
    let settings = EventSettings {
        expiry_protection: ExpiryProtection::HostReview,
        ..async_settings()
    };
    let fixture = Fixture::new(3, settings);
    let first = fixture.offers(T3);
    fixture.expire_all(T3 + DAY + 1);

    let person = first[0].0.clone();
    assert_eq!(fixture.readiness(&person), Readiness::Ready);
    let state = fixture.state(&person).unwrap();
    assert_eq!(state.strikes, 1);
    assert!(state.cooldown_until.is_none());
    let decisions = open_decisions(&fixture.db, EVENT).unwrap();
    assert_eq!(decisions.len(), 3);
    assert!(decisions.iter().all(|decision| decision.kind == "expiry"));

    decide_expiry(&fixture.db, &decisions[0].id, "pause", T3 + DAY + 10).unwrap();
    let assignment = AssignmentRepo::get(
        fixture.db.conn(),
        decisions[0].assignment_id.as_deref().unwrap(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        fixture.readiness(&assignment.participant_id),
        Readiness::Paused
    );
    assert!(open_decisions(&fixture.db, EVENT).unwrap().len() == 2);
}

// scenario: The none preset has no automatic effect
#[test]
fn the_none_preset_has_no_automatic_effect() {
    let settings = EventSettings {
        expiry_protection: ExpiryProtection::None,
        ..async_settings()
    };
    let fixture = Fixture::new(3, settings);
    let first = fixture.offers(T3);
    fixture.expire_all(T3 + DAY + 1);

    for (user_id, _, _) in &first {
        assert_eq!(fixture.readiness(user_id), Readiness::Ready);
        let state = fixture.state(user_id).unwrap();
        assert_eq!(state.strikes, 1);
        assert!(state.cooldown_until.is_none());
    }
    let next = fixture.offers(T3 + DAY + 2);
    assert_eq!(next.len(), 3);
}

// scenario: An expired pair can return after the cooldown
#[test]
fn after_cooldown_reoffers_the_pair() {
    let settings = EventSettings {
        reoffer_after_expiry: ReofferAfterExpiry::AfterCooldown,
        expiry_protection: ExpiryProtection::None,
        ..async_settings()
    };
    let fixture = Fixture::new(2, settings);
    let first = fixture.offers(T3);
    assert_eq!(first.len(), 2);
    fixture.expire_all(T3 + DAY + 1);

    // No cooldown was set, so the pair is allowed at the next wave.
    let second = fixture.offers(T3 + DAY + 2);
    assert_eq!(
        second
            .iter()
            .map(|offer| (offer.0.clone(), offer.2.clone()))
            .collect::<Vec<_>>(),
        first
            .iter()
            .map(|offer| (offer.0.clone(), offer.2.clone()))
            .collect::<Vec<_>>()
    );
    let audit = swap_db::AuditRepo::list_for_event(fixture.db.conn(), EVENT).unwrap();
    assert!(audit.iter().any(|entry| {
        entry.action == "Async offer" && entry.detail_json.contains("re-offer after the cooldown")
    }));
}

// scenario: The last-resort rule uses the pair only when nobody else can take it
#[test]
fn last_resort_does_not_use_the_pair_when_someone_else_can_take_it() {
    let settings = EventSettings {
        reoffer_after_expiry: ReofferAfterExpiry::LastResort,
        expiry_protection: ExpiryProtection::None,
        ..async_settings()
    };
    let fixture = Fixture::new(3, settings);
    let first = fixture.offers(T3);
    assert_eq!(first.len(), 3);
    fixture.expire_all(T3 + DAY + 1);

    let wave = dispatch_async_wave(&fixture.db, EVENT, T3 + DAY + 2).unwrap();
    for (person, _, track) in &first {
        assert!(
            !offers(&wave.effects)
                .iter()
                .any(|offer| &offer.0 == person && &offer.2 == track),
            "another person can take the track"
        );
    }
}

// scenario: The last-resort rule uses the pair only when nobody else can take it
#[test]
fn last_resort_returns_the_pair_when_no_other_person_can() {
    let settings = EventSettings {
        reoffer_after_expiry: ReofferAfterExpiry::LastResort,
        expiry_protection: ExpiryProtection::None,
        ..async_settings()
    };
    let fixture = Fixture::new(2, settings);
    let first = fixture.offers(T3);
    assert_eq!(first.len(), 2);
    fixture.expire_all(T3 + DAY + 1);
    let second = fixture.offers(T3 + DAY + 2);
    assert_eq!(
        second
            .iter()
            .map(|offer| (offer.0.clone(), offer.2.clone()))
            .collect::<Vec<_>>(),
        first
            .iter()
            .map(|offer| (offer.0.clone(), offer.2.clone()))
            .collect::<Vec<_>>()
    );
}

// scenario: The host-review preset opens a decision without an automatic pause
#[test]
fn a_host_review_blocks_the_after_cooldown_reoffer_until_the_host_decides() {
    let settings = EventSettings {
        expiry_protection: ExpiryProtection::HostReview,
        reoffer_after_expiry: ReofferAfterExpiry::AfterCooldown,
        ..async_settings()
    };
    let fixture = Fixture::new(2, settings);
    let first = fixture.offers(T3);
    fixture.expire_all(T3 + DAY + 1);
    let (person, _, track) = first[0].clone();

    // The host has not decided, so the pair stays out even though no pair
    // cooldown exists.
    let blocked = fixture.offers(T3 + DAY + 2);
    assert!(
        !blocked
            .iter()
            .any(|offer| offer.0 == person && offer.2 == track)
    );
    let decisions = open_decisions(&fixture.db, EVENT).unwrap();
    let decision = decisions
        .iter()
        .find(|decision| decision.assignment_id.as_deref() == Some(first[0].1.as_str()))
        .unwrap();
    decide_expiry(&fixture.db, &decision.id, "ignore", T3 + DAY + 3).unwrap();
    let active = AssignmentRepo::list_active(fixture.db.conn(), EVENT).unwrap();
    for assignment in active {
        skip(&fixture.db, EVENT, &assignment.participant_id, T3 + DAY + 4).unwrap();
    }
    let allowed = fixture.offers(T3 + DAY + 5);
    assert!(
        allowed
            .iter()
            .any(|offer| offer.0 == person && offer.2 == track)
    );
}

// scenario: The host decides a same-track re-offer
#[test]
fn host_decides_reoffers_only_on_the_host_command() {
    let settings = EventSettings {
        reoffer_after_expiry: ReofferAfterExpiry::HostDecides,
        expiry_protection: ExpiryProtection::None,
        ..async_settings()
    };
    let fixture = Fixture::new(3, settings);
    let first = fixture.offers(T3);
    fixture.expire_all(T3 + DAY + 1);
    let (person, _, track) = first[0].clone();

    let decisions = open_decisions(&fixture.db, EVENT).unwrap();
    assert_eq!(decisions.len(), 3);
    let decision = decisions
        .iter()
        .find(|decision| decision.assignment_id.as_deref() == Some(first[0].1.as_str()))
        .expect("the expired offer opens a decision");
    decide_expiry(&fixture.db, &decision.id, "reoffer", T3 + DAY + 10).unwrap();
    let active = AssignmentRepo::list_active(fixture.db.conn(), EVENT).unwrap();
    assert!(active.iter().any(|assignment| {
        assignment.participant_id == person
            && assignment.track_code == track
            && assignment.status == AssignmentStatus::Offered
    }));
    let pending = DeliveryRepo::pending(fixture.db.conn(), 100).unwrap();
    assert!(pending.iter().any(|row| row.kind == "offer"));
}

// scenario: The default re-offer rule never returns an expired pair
#[test]
fn the_default_rule_never_returns_an_expired_pair() {
    let fixture = Fixture::new(2, async_settings());
    let first = fixture.offers(T3);
    fixture.expire_all(T3 + DAY + 1);
    let (person, _, track) = first[0].clone();
    let next = fixture.offers(T3 + 10 * DAY);
    assert!(
        !next
            .iter()
            .any(|offer| offer.0 == person && offer.2 == track),
        "the default is never"
    );
    assert!(
        ExpiredPairRepo::get(fixture.db.conn(), EVENT, &track, &person)
            .unwrap()
            .is_some()
    );
}

// scenario: The bot reminds the participant to claim
#[tokio::test]
async fn the_claim_reminder_fires_once_with_the_remaining_time() {
    let settings = EventSettings {
        claim_reminder_secs: Some(6 * 60 * 60),
        ..async_settings()
    };
    let fixture = Fixture::new(3, settings);
    fixture.offers(T3);
    let before = tick_deadlines(&fixture.db, T3 + 17 * 60 * 60).unwrap();
    assert_eq!(before.reminders, 0);
    let at = tick_deadlines(&fixture.db, T3 + 18 * 60 * 60 + 1).unwrap();
    assert_eq!(at.reminders, 3);
    let again = tick_deadlines(&fixture.db, T3 + 18 * 60 * 60 + 2).unwrap();
    assert_eq!(again.reminders, 0);

    let discord = swap_discord::FakeDiscord::new();
    dispatch_pending(
        &fixture.db,
        &fixture.store,
        &discord,
        T3 + 19 * 60 * 60,
        100,
    )
    .await
    .unwrap();
    let prompts: Vec<String> = discord
        .sent_dms()
        .into_iter()
        .map(|dm| dm.message.content)
        .filter(|content| content.contains("still unclaimed"))
        .collect();
    assert_eq!(prompts.len(), 3);
    assert!(prompts.iter().all(|content| {
        content.contains("5 hours and 59 minutes") || content.contains("6 hours")
    }));
    assert!(prompts.iter().all(|content| !content.contains("(due ")));
}

// scenario: A claim cancels the claim reminder
#[test]
fn a_claim_cancels_the_reminder_and_a_reoffer_gets_a_new_one() {
    let settings = EventSettings {
        claim_reminder_secs: Some(6 * 60 * 60),
        ..async_settings()
    };
    let fixture = Fixture::new(3, settings);
    let first = fixture.offers(T3);
    let (person, assignment, _) = first[0].clone();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment,
        &person,
        T3 + 60,
    )
    .unwrap();
    tick_deadlines(&fixture.db, T3 + 18 * 60 * 60 + 1).unwrap();
    assert!(!AssignmentRepo::claim_reminder_sent(fixture.db.conn(), &assignment).unwrap());
    let active = AssignmentRepo::list_active(fixture.db.conn(), EVENT).unwrap();
    assert!(
        active
            .iter()
            .filter(|row| row.status == AssignmentStatus::Offered)
            .all(|row| AssignmentRepo::claim_reminder_sent(fixture.db.conn(), &row.id).unwrap()),
        "the unclaimed offers get their one reminder"
    );
}

// scenario: A disabled claim reminder sends nothing
#[test]
fn a_disabled_claim_reminder_sends_nothing() {
    let settings = EventSettings {
        claim_reminder_secs: None,
        ..async_settings()
    };
    let fixture = Fixture::new(3, settings);
    fixture.offers(T3);
    let tick = tick_deadlines(&fixture.db, T3 + 23 * 60 * 60).unwrap();
    assert_eq!(tick.reminders, 0);
}

// scenario: The release time keeps its wall-clock time across a DST change
// scenario: The release uses the stored host zone
#[test]
fn the_release_uses_the_host_zone_in_winter_and_summer() {
    let settings = EventSettings {
        mode: swap_engine::Mode::Async,
        async_release_time: Some("18:00".to_string()),
        host_timezone: Some("Europe/Stockholm".to_string()),
        async_wave_offset_secs: 2 * 60 * 60,
        ..EventSettings::default()
    };
    let fixture = Fixture::new(2, settings);
    let winter = next_async_release(&fixture.db, EVENT, 1_768_435_200).unwrap();
    assert_eq!(winter.next_at, 1_768_496_400);
    let summer = next_async_release(&fixture.db, EVENT, 1_784_073_600).unwrap();
    assert_eq!(summer.next_at, 1_784_131_200);
}

// scenario: A release time in the spring-forward gap moves forward
#[test]
fn a_release_time_in_the_spring_forward_gap_moves_forward() {
    let settings = EventSettings {
        mode: swap_engine::Mode::Async,
        async_release_time: Some("02:30".to_string()),
        host_timezone: Some("Europe/Stockholm".to_string()),
        ..EventSettings::default()
    };
    let fixture = Fixture::new(2, settings);
    // 2026-03-29 00:00 UTC is before the gap. 02:30 does not exist and moves
    // to 03:30 local, which is 01:30 UTC.
    let release = next_async_release(&fixture.db, EVENT, 1_774_742_400).unwrap();
    assert_eq!(release.next_at, 1_774_747_800);
}

// scenario: A release time in the fall-back overlap uses the earlier offset
#[test]
fn a_release_time_in_the_fall_back_overlap_uses_the_earlier_offset() {
    let settings = EventSettings {
        mode: swap_engine::Mode::Async,
        async_release_time: Some("02:30".to_string()),
        host_timezone: Some("Europe/Stockholm".to_string()),
        ..EventSettings::default()
    };
    let fixture = Fixture::new(2, settings);
    // 2026-10-25 00:00 UTC. 02:30 happens twice; the earlier offset is CEST.
    let release = next_async_release(&fixture.db, EVENT, 1_792_886_400).unwrap();
    assert_eq!(release.next_at, 1_792_888_200);
}

// scenario: A daily window runs two dispatch waves
#[test]
fn a_daily_window_runs_two_dispatch_waves() {
    let settings = EventSettings {
        async_release_time: Some("18:00".to_string()),
        host_timezone: Some("UTC".to_string()),
        ..async_settings()
    };
    let fixture = Fixture::new(4, settings);
    let paused = user(3);
    set_participant_ready(&fixture.db, EVENT, &paused, false, T3).unwrap();
    // 2026-01-15 18:00 UTC.
    let first_at = 1_768_500_000;
    let first = swap_engine::dispatch_async_release(&fixture.db, EVENT, first_at).unwrap();
    assert_eq!(first.offers, 3);

    // One offer is claimed and skipped, so the track returns to the pool. The
    // paused person resumes and the second wave offers that track to them.
    let own = TrackRepo::list_for_event(fixture.db.conn(), EVENT)
        .unwrap()
        .into_iter()
        .find(|track| track.owner_id == paused)
        .unwrap()
        .code;
    let (person, assignment, _) = offers(&first.effects)
        .into_iter()
        .find(|offer| offer.2 != own)
        .expect("a track that the paused person does not own");
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment,
        &person,
        first_at + 60,
    )
    .unwrap();
    skip(&fixture.db, EVENT, &person, first_at + 120).unwrap();
    set_participant_ready(&fixture.db, EVENT, &paused, true, first_at + 180).unwrap();
    let second = swap_engine::dispatch_async_release(&fixture.db, EVENT, first_at + 7_200).unwrap();
    assert!(second.offers >= 1);
}

// scenario: A changed host zone moves the next release
#[test]
fn a_changed_host_zone_moves_the_next_release() {
    let stockholm = EventSettings {
        mode: swap_engine::Mode::Async,
        async_release_time: Some("18:00".to_string()),
        host_timezone: Some("Europe/Stockholm".to_string()),
        ..EventSettings::default()
    };
    let fixture = Fixture::new(2, stockholm);
    let before = next_async_release(&fixture.db, EVENT, 1_768_435_200).unwrap();

    let new_york = EventSettings {
        mode: swap_engine::Mode::Async,
        async_release_time: Some("18:00".to_string()),
        host_timezone: Some("America/New_York".to_string()),
        ..EventSettings::default()
    };
    let other = Fixture::new(2, new_york);
    let after = next_async_release(&other.db, EVENT, 1_768_435_200).unwrap();
    assert_ne!(before.next_at, after.next_at);
}

// scenario: An expiry returns the track without a seen mark
#[test]
fn an_expired_pair_is_not_seen() {
    let fixture = Fixture::new(2, async_settings());
    let first = fixture.offers(T3);
    fixture.expire_all(T3 + DAY + 1);
    let (person, _, track) = first[0].clone();
    assert!(!SeenRepo::has(fixture.db.conn(), EVENT, &track, &person).unwrap());
    let decision = HostDecisionRepo::list_open(fixture.db.conn(), EVENT).unwrap();
    assert!(decision.is_empty());
}

// scenario: A host re-offer is refused under the never rule
#[test]
fn a_host_reoffer_is_refused_under_the_never_rule() {
    let settings = EventSettings {
        expiry_protection: ExpiryProtection::HostReview,
        reoffer_after_expiry: ReofferAfterExpiry::Never,
        ..async_settings()
    };
    let fixture = Fixture::new(2, settings);
    fixture.offers(T3);
    fixture.expire_all(T3 + DAY + 1);
    let decisions = open_decisions(&fixture.db, EVENT).unwrap();
    let refused =
        decide_expiry(&fixture.db, &decisions[0].id, "reoffer", T3 + DAY + 3).unwrap_err();
    assert!(matches!(refused, EngineError::ReofferNotAllowed));
}
