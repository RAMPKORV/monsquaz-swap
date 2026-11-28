//! Fill-in behaviour: eligibility, the no-repeat guard at offer time, the
//! host-set due time, and the fill-in flows.

use swap_core::{AssignmentStatus, Membership, Readiness, Role};
use swap_db::{
    AssignmentRepo, Db, FillInChainRepo, HostDecisionRepo, MetaRepo, PackRecord, PackRepo,
    ParticipantRecord, ParticipantRepo, PlanAssignmentRecord, SeenRepo,
};
use swap_engine::{
    Effect, EngineError, EventSettings, FillInFlow, FillInTier, JoinRequest, LeaveActor,
    PassthroughPreparer, SubmitRequest, assign_fill_in, claim, create_event_with_secret,
    event_status, fill_in_candidates, generate_schedule, join_with_seed, leave_event, lock_roster,
    move_round_deadline, open_round, open_seed_round, set_track_title, skip, start_event,
    start_fill_in_list, submit, tick_deadlines,
};
use swap_files::BlobStore;

const EVENT: &str = "evt-1";
const SECRET: &str = "0000000000000000000000000000002a";
const T0: i64 = 1_000;
const T1: i64 = 2_000;
const T2: i64 = 2_100;
const T3: i64 = 2_200;
const T4: i64 = 2_300;
const T5: i64 = 2_400;

struct Fixture {
    db: Db,
    store: BlobStore,
    plan: Vec<PlanAssignmentRecord>,
    _directory: tempfile::TempDir,
}

impl Fixture {
    fn started(seeded: usize, settings: EventSettings) -> Self {
        Self::started_with_cap(seeded, settings, 20)
    }

    fn started_with_cap(seeded: usize, settings: EventSettings, cap: u32) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let db = Db::open(directory.path().join("swap.db")).unwrap();
        let store = BlobStore::new(directory.path().join("blobs"));
        seed_event(&db, &store, seeded, &settings, cap);
        lock_roster(&db, EVENT, T1).unwrap();
        let plan = generate_schedule(&db, EVENT, T2).unwrap().assignments;
        start_event(&db, EVENT, T3).unwrap();
        Self {
            db,
            store,
            plan,
            _directory: directory,
        }
    }

    fn holder(&self, round: u32, track_code: &str) -> String {
        self.plan
            .iter()
            .find(|row| row.round == round && row.track_code == track_code)
            .map(|row| row.participant_id.clone())
            .unwrap()
    }

    fn planned_for(&self, track_code: &str, user_id: &str) -> bool {
        self.plan
            .iter()
            .any(|row| row.track_code == track_code && row.participant_id == user_id)
    }

    /// Open round 0. The first holder skips before the claim, the next
    /// `skippers` holders skip too, and every other holder submits.
    /// Returns the stranded track and the round 0 offers.
    fn strand_round_zero(&self, skippers: usize) -> (String, Vec<(String, String, String)>) {
        let round_zero = offers(&open_round(&self.db, EVENT, 0, T4).unwrap().effects);
        for (index, (user_id, assignment_id, _)) in round_zero.iter().enumerate() {
            if index <= skippers {
                skip(&self.db, EVENT, user_id, T4 + 1).unwrap();
            } else {
                self.claim_and_submit(user_id, assignment_id, T4 + 1);
            }
        }
        (round_zero[0].2.clone(), round_zero)
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
}

fn seed_event(db: &Db, store: &BlobStore, seeded: usize, settings: &EventSettings, cap: u32) {
    create_event_with_secret(db, EVENT, "Fill Swap", 42, settings, Some(SECRET), T0).unwrap();
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
    open_seed_round(db, EVENT, T0 + 1_000_000, None, T0).unwrap();
    for index in 0..seeded {
        let user_id = user(index);
        let bytes = seed_bytes(index as u8);
        join_with_seed(
            db,
            store,
            &PassthroughPreparer,
            &JoinRequest {
                event_id: EVENT,
                user_id: &user_id,
                display_name: &format!("User {index}"),
                timezone: "UTC",
                max_handoffs: cap,
                filename: "seed.it",
                bytes: &bytes,
            },
            T0,
        )
        .unwrap();
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
fn a_round_open_skips_a_planned_pair_that_is_already_seen() {
    let fixture = Fixture::started(4, EventSettings::default());
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    for (user_id, assignment_id, _) in &round_zero {
        fixture.claim_and_submit(user_id, assignment_id, T4 + 1);
    }
    let planned = fixture
        .plan
        .iter()
        .find(|row| row.round == 1)
        .cloned()
        .unwrap();
    SeenRepo::record(
        fixture.db.conn(),
        EVENT,
        &planned.track_code,
        &planned.participant_id,
        "manual-probe",
        "claim",
        T4 + 5,
    )
    .unwrap();

    let outcome = open_round(&fixture.db, EVENT, 1, T4 + 10).unwrap();
    assert_eq!(outcome.offers, 3, "the round opens with one gap");
    let round_one: Vec<_> = AssignmentRepo::list_for_event(fixture.db.conn(), EVENT)
        .unwrap()
        .into_iter()
        .filter(|assignment| assignment.round == 1)
        .collect();
    assert_eq!(round_one.len(), 3);
    assert!(round_one.iter().all(
        |assignment| assignment.participant_id != planned.participant_id
            || assignment.track_code != planned.track_code
    ));
    let audit = swap_db::AuditRepo::list_for_event(fixture.db.conn(), EVENT).unwrap();
    assert!(audit.iter().any(|entry| {
        entry.action == "Planned turn skipped"
            && entry
                .detail_json
                .contains("the seen ledger already has this pair")
    }));
}

#[test]
fn a_claim_and_submit_record_the_pair_once_and_a_retry_is_idempotent() {
    let fixture = Fixture::started(3, EventSettings::default());
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (user_id, assignment_id, track_code) = round_zero[0].clone();
    fixture.claim_and_submit(&user_id, &assignment_id, T4 + 1);
    let retry = claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &assignment_id,
        &user_id,
        T4 + 3,
    );
    assert!(matches!(
        retry,
        Err(EngineError::WrongAssignmentState(
            AssignmentStatus::Submitted
        ))
    ));
    let seen: Vec<_> = SeenRepo::list_for_event(fixture.db.conn(), EVENT)
        .unwrap()
        .into_iter()
        .filter(|pair| pair.track_code == track_code && pair.participant_id == user_id)
        .collect();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].reason, "claim");
}

fn add_fill_in(db: &Db, user_id: &str) {
    add_fill_in_in(db, user_id, "UTC");
}

fn add_fill_in_with_cap(db: &Db, user_id: &str, cap: u32) {
    ParticipantRepo::upsert(
        db.conn(),
        &ParticipantRecord {
            event_id: EVENT.to_string(),
            user_id: user_id.to_string(),
            display_name: format!("Volunteer {user_id}"),
            role: Role::FillIn,
            membership: Membership::Active,
            readiness: Readiness::Ready,
            timezone: "UTC".to_string(),
            max_handoffs: cap,
            created_at: T0,
            updated_at: T0,
        },
    )
    .unwrap();
}

fn add_fill_in_in(db: &Db, user_id: &str, timezone: &str) {
    ParticipantRepo::upsert(
        db.conn(),
        &ParticipantRecord {
            event_id: EVENT.to_string(),
            user_id: user_id.to_string(),
            display_name: format!("Volunteer {user_id}"),
            role: Role::FillIn,
            membership: Membership::Active,
            readiness: Readiness::Ready,
            timezone: timezone.to_string(),
            max_handoffs: 20,
            created_at: T0,
            updated_at: T0,
        },
    )
    .unwrap();
}

const DAY: i64 = 86_400;

#[test]
fn a_fill_in_after_a_missed_deadline_gets_the_fill_in_window() {
    let fixture = Fixture::started(4, EventSettings::default());
    add_fill_in(&fixture.db, "volunteer");
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (_, stranded_id, track) = round_zero[0].clone();
    for (user_id, assignment_id, _) in &round_zero[1..] {
        fixture.claim_and_submit(user_id, assignment_id, T4 + 1);
    }
    let deadline = T4 + DAY;
    tick_deadlines(&fixture.db, deadline).unwrap();
    assert_eq!(
        AssignmentRepo::get(fixture.db.conn(), &stranded_id)
            .unwrap()
            .unwrap()
            .status,
        AssignmentStatus::Expired
    );

    let now = deadline + 600;
    let outcome = assign_fill_in(&fixture.db, EVENT, &track, 2, "volunteer", None, now).unwrap();
    assert_eq!(outcome.due_at, now + DAY);
    let status = event_status(&fixture.db, EVENT).unwrap();
    assert_eq!(status.waiting_fill_ins.len(), 1);
    assert_eq!(status.waiting_fill_ins[0].track_code, track);
    assert_eq!(status.waiting_fill_ins[0].participant_id, "volunteer");
    assert_eq!(status.waiting_fill_ins[0].round_no, 2);
    assert_eq!(status.waiting_fill_ins[0].due_at, Some(now + DAY));

    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &outcome.assignment_id,
        "volunteer",
        now + 60,
    )
    .unwrap();
    let claimed = AssignmentRepo::get(fixture.db.conn(), &outcome.assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(claimed.due_at, Some(now + DAY));
    tick_deadlines(&fixture.db, now + 3_600).unwrap();
    assert_eq!(
        AssignmentRepo::get(fixture.db.conn(), &outcome.assignment_id)
            .unwrap()
            .unwrap()
            .status,
        AssignmentStatus::Claimed
    );
    assert!(matches!(
        open_round(&fixture.db, EVENT, 1, now + 3_600),
        Err(EngineError::RoundNotResolved { round: 0 })
    ));
}

// scenario: The host sets the fill-in due time
#[test]
fn the_host_sets_the_fill_in_due_time_and_a_past_time_is_refused() {
    let fixture = Fixture::started(4, EventSettings::default());
    add_fill_in(&fixture.db, "volunteer");
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (user_id, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &user_id, T4 + 1).unwrap();

    for past in [T4 + 10, T4 + 5] {
        assert!(matches!(
            assign_fill_in(
                &fixture.db,
                EVENT,
                &track,
                2,
                "volunteer",
                Some(past),
                T4 + 10
            ),
            Err(EngineError::FillInDueInPast)
        ));
    }
    assert!(matches!(
        assign_fill_in(
            &fixture.db,
            EVENT,
            &track,
            2,
            "volunteer",
            Some(T4 + 15 * DAY),
            T4 + 10
        ),
        Err(EngineError::FillInDueTooFar)
    ));
    let chosen = T4 + 3 * DAY;
    let outcome = assign_fill_in(
        &fixture.db,
        EVENT,
        &track,
        2,
        "volunteer",
        Some(chosen),
        T4 + 10,
    )
    .unwrap();
    assert_eq!(outcome.due_at, chosen);
    let offer = AssignmentRepo::get(fixture.db.conn(), &outcome.assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(offer.due_at, Some(chosen));
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &outcome.assignment_id,
        "volunteer",
        T4 + 20,
    )
    .unwrap();
    let claimed = AssignmentRepo::get(fixture.db.conn(), &outcome.assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(claimed.due_at, Some(chosen));
}

#[test]
fn the_default_fill_in_due_time_keeps_a_later_round_deadline() {
    let fixture = Fixture::started(
        4,
        EventSettings {
            fill_in_window_secs: 3_600,
            ..EventSettings::default()
        },
    );
    add_fill_in(&fixture.db, "volunteer");
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (user_id, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &user_id, T4 + 1).unwrap();
    let outcome =
        assign_fill_in(&fixture.db, EVENT, &track, 2, "volunteer", None, T4 + 10).unwrap();
    assert_eq!(outcome.due_at, T4 + DAY);
}

fn candidate_ids(fixture: &Fixture, track: &str) -> Vec<String> {
    fill_in_candidates(&fixture.db, EVENT, track, 2)
        .unwrap()
        .into_iter()
        .map(|candidate| candidate.participant_id)
        .collect()
}

fn group_settings() -> EventSettings {
    EventSettings {
        rounds: Some(3),
        group_mode: Some("Lookup".to_string()),
        group_count: Some(2),
        group_size: Some(5),
        ..EventSettings::default()
    }
}

fn group_flow(flow: FillInFlow) -> EventSettings {
    EventSettings {
        fill_in_flow: flow,
        ..group_settings()
    }
}

/// The seeded participants who submitted in round 0, do not own the track,
/// and have no plan row for the track.
fn expected_extra_turns(
    fixture: &Fixture,
    track: &str,
    round_zero: &[(String, String, String)],
    skippers: usize,
) -> Vec<String> {
    let mut expected: Vec<String> = round_zero
        .iter()
        .skip(skippers + 1)
        .map(|(user_id, _, _)| user_id.clone())
        .filter(|user_id| !fixture.planned_for(track, user_id))
        .filter(|user_id| {
            swap_db::TrackRepo::get(fixture.db.conn(), EVENT, track)
                .unwrap()
                .unwrap()
                .owner_id
                != *user_id
        })
        .collect();
    expected.sort();
    expected
}

// scenario: A participant planned for the track is refused
#[test]
fn a_participant_planned_for_the_track_later_is_refused() {
    let fixture = Fixture::started(4, EventSettings::default());
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (user_b, _, track_b) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &user_b, T4 + 1).unwrap();
    let user_a = fixture.holder(1, &track_b);
    let (_, assignment_a, _) = round_zero
        .iter()
        .find(|(user_id, _, _)| *user_id == user_a)
        .cloned()
        .unwrap();
    fixture.claim_and_submit(&user_a, &assignment_a, T4 + 2);

    assert!(!candidate_ids(&fixture, &track_b).contains(&user_a));
    let refused = assign_fill_in(&fixture.db, EVENT, &track_b, 2, &user_a, None, T4 + 10);
    assert!(
        matches!(refused, Err(EngineError::FillInNotEligible(_))),
        "got {refused:?}"
    );
    assert!(!SeenRepo::has(fixture.db.conn(), EVENT, &track_b, &user_a).unwrap());
}

// scenario: A participant who receives every track is never a candidate
// scenario: No eligible fill-in exists
#[test]
fn a_participant_who_sees_every_track_is_never_a_candidate() {
    let fixture = Fixture::started(4, EventSettings::default());
    let (track, _) = fixture.strand_round_zero(0);
    assert!(candidate_ids(&fixture, &track).is_empty());
}

// scenario: A seeded participant can take one extra turn
// scenario: A fill-in rescue is credited
#[test]
fn a_group_participant_who_submitted_is_an_extra_turn_candidate() {
    let fixture = Fixture::started(10, group_settings());
    let (track, round_zero) = fixture.strand_round_zero(0);
    let expected = expected_extra_turns(&fixture, &track, &round_zero, 0);
    assert!(
        !expected.is_empty(),
        "the group plan must leave someone out"
    );
    let candidates = fill_in_candidates(&fixture.db, EVENT, &track, 2).unwrap();
    let mut ids: Vec<String> = candidates
        .iter()
        .map(|candidate| candidate.participant_id.clone())
        .collect();
    ids.sort();
    assert_eq!(ids, expected);
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.tier == FillInTier::ExtraTurn)
    );
    let chosen = &candidates[0].participant_id;
    let outcome = assign_fill_in(&fixture.db, EVENT, &track, 2, chosen, None, T4 + 10).unwrap();
    fixture.claim_and_submit(chosen, &outcome.assignment_id, T4 + 20);
    let package = swap_engine::reveal_package(&fixture.db, EVENT).unwrap();
    let credits = &package
        .album
        .iter()
        .find(|entry| entry.track_code == track)
        .unwrap()
        .credits;
    assert!(
        credits
            .iter()
            .any(|credit| credit.participant_id == *chosen && credit.fill_in),
        "the extra turn must be credited as a fill-in: {credits:?}"
    );
    assert!(
        credits
            .iter()
            .filter(|credit| credit.participant_id != *chosen)
            .all(|credit| !credit.fill_in)
    );
}

// scenario: A participant who skipped is not eligible
#[test]
fn a_participant_who_skipped_is_not_eligible() {
    let fixture = Fixture::started(10, group_settings());
    let (track, round_zero) = fixture.strand_round_zero(4);
    let skipped: Vec<String> = round_zero[1..5]
        .iter()
        .map(|(user_id, _, _)| user_id.clone())
        .filter(|user_id| !fixture.planned_for(&track, user_id))
        .collect();
    assert!(!skipped.is_empty(), "a skipper must lack a plan row");
    let ids = candidate_ids(&fixture, &track);
    for user_id in &skipped {
        assert!(!ids.contains(user_id));
        let refused = assign_fill_in(&fixture.db, EVENT, &track, 2, user_id, None, T4 + 10);
        assert!(matches!(refused, Err(EngineError::FillInNotEligible(_))));
    }
}

// scenario: The fill-in pool has two tiers in classic mode
#[test]
fn an_extra_turn_is_hidden_while_a_volunteer_is_eligible() {
    let fixture = Fixture::started(10, group_settings());
    add_fill_in(&fixture.db, "volunteer");
    let (track, round_zero) = fixture.strand_round_zero(0);
    let extra = expected_extra_turns(&fixture, &track, &round_zero, 0);
    assert!(!extra.is_empty());
    let candidates = fill_in_candidates(&fixture.db, EVENT, &track, 2).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].participant_id, "volunteer");
    assert_eq!(candidates[0].tier, FillInTier::Volunteer);
    let refused = assign_fill_in(&fixture.db, EVENT, &track, 2, &extra[0], None, T4 + 10);
    assert!(matches!(refused, Err(EngineError::FillInNotEligible(_))));
}

// scenario: A fill-in has never seen the track
#[test]
fn a_volunteer_never_receives_the_same_track_twice() {
    let fixture = Fixture::started(4, EventSettings::default());
    add_fill_in(&fixture.db, "volunteer");
    let (track, _) = fixture.strand_round_zero(0);
    let outcome =
        assign_fill_in(&fixture.db, EVENT, &track, 2, "volunteer", None, T4 + 10).unwrap();
    fixture.claim_and_submit("volunteer", &outcome.assignment_id, T4 + 20);
    let round_one = offers(&open_round(&fixture.db, EVENT, 1, T4 + 100).unwrap().effects);
    let (user_id, _, _) = round_one
        .iter()
        .find(|(_, _, code)| *code == track)
        .cloned()
        .unwrap();
    skip(&fixture.db, EVENT, &user_id, T4 + 101).unwrap();
    let round_no = 3;
    assert!(
        fill_in_candidates(&fixture.db, EVENT, &track, round_no)
            .unwrap()
            .iter()
            .all(|candidate| candidate.participant_id != "volunteer")
    );
    assert!(matches!(
        assign_fill_in(
            &fixture.db,
            EVENT,
            &track,
            round_no,
            "volunteer",
            None,
            T4 + 110
        ),
        Err(EngineError::FillInNotEligible(_))
    ));
}

#[test]
fn an_extra_turn_does_not_use_the_handoff_cap() {
    let settings = EventSettings {
        rounds: Some(3),
        handoff_policy: Some("Flexible".to_string()),
        ..EventSettings::default()
    };
    let fixture = Fixture::started_with_cap(5, settings, 2);
    let (first_track, round_zero) = fixture.strand_round_zero(0);
    let extra = expected_extra_turns(&fixture, &first_track, &round_zero, 0);
    assert!(!extra.is_empty(), "a capped plan must leave someone out");
    let helper = extra[0].clone();
    let outcome =
        assign_fill_in(&fixture.db, EVENT, &first_track, 2, &helper, None, T4 + 10).unwrap();
    fixture.claim_and_submit(&helper, &outcome.assignment_id, T4 + 20);

    // The helper keeps the planned round 1 turn and finishes it. The helper
    // has now seen 3 tracks with a cap of 2: the extra turn is not counted.
    let round_one = offers(&open_round(&fixture.db, EVENT, 1, T4 + 100).unwrap().effects);
    let owner_of = |code: &str| {
        swap_db::TrackRepo::get(fixture.db.conn(), EVENT, code)
            .unwrap()
            .unwrap()
            .owner_id
    };
    let second_track = round_one
        .iter()
        .map(|(_, _, code)| code.clone())
        .find(|code| {
            *code != first_track && owner_of(code) != helper && !fixture.planned_for(code, &helper)
        })
        .expect("a second unplanned track exists");
    for (user_id, assignment_id, code) in &round_one {
        if *code == second_track {
            skip(&fixture.db, EVENT, user_id, T4 + 101).unwrap();
        } else {
            fixture.claim_and_submit(user_id, assignment_id, T4 + 110);
        }
    }
    assert_eq!(
        SeenRepo::list_for_participant(fixture.db.conn(), EVENT, &helper)
            .unwrap()
            .len(),
        3
    );
    let candidates = fill_in_candidates(&fixture.db, EVENT, &second_track, 3).unwrap();
    assert!(
        candidates
            .iter()
            .any(|candidate| candidate.participant_id == helper),
        "the extra turn must not use the cap: {candidates:?}"
    );
}

// ---- MSW-083: fill-in flows -------------------------------------------------

/// 09:38 in Tokyo at T4, outside the default quiet hours (22:00 to 08:00).
const AWAKE: &str = "Asia/Tokyo";

fn flow(flow: FillInFlow) -> EventSettings {
    EventSettings {
        fill_in_flow: flow,
        ..EventSettings::default()
    }
}

/// The seeded participants use UTC in the fixture. T4 is 00:38 UTC, inside
/// the default quiet hours. A chain test moves them to an awake timezone.
fn wake_everyone(fixture: &Fixture) {
    for mut participant in ParticipantRepo::list_for_event(fixture.db.conn(), EVENT).unwrap() {
        participant.timezone = AWAKE.to_string();
        ParticipantRepo::upsert(fixture.db.conn(), &participant).unwrap();
    }
}

/// The open fill-in offer for one track, as (participant, assignment id).
fn fill_in_offer(db: &Db, track: &str) -> Option<(String, String)> {
    AssignmentRepo::list_active(db.conn(), EVENT)
        .unwrap()
        .into_iter()
        .find(|assignment| {
            assignment.track_code == track && assignment.status == AssignmentStatus::Offered
        })
        .map(|assignment| (assignment.participant_id, assignment.id))
}

fn fill_in_decisions(db: &Db) -> usize {
    HostDecisionRepo::list_open(db.conn(), EVENT)
        .unwrap()
        .iter()
        .filter(|decision| decision.kind == "fill_in")
        .count()
}

fn volunteers(fixture: &Fixture, count: usize) -> Vec<String> {
    let ids: Vec<String> = (0..count)
        .map(|index| format!("volunteer-{index}"))
        .collect();
    for id in &ids {
        add_fill_in_in(&fixture.db, id, AWAKE);
    }
    ids
}

#[test]
fn candidates_rank_by_fewest_edits() {
    let fixture = Fixture::started(4, EventSettings::default());
    volunteers(&fixture, 2);
    let (first, round_zero) = fixture.strand_round_zero(1);
    let second = round_zero[1].2.clone();
    let ranked = fill_in_candidates(&fixture.db, EVENT, &first, 2).unwrap();
    let busy = ranked[0].participant_id.clone();
    let outcome = assign_fill_in(&fixture.db, EVENT, &first, 2, &busy, None, T4 + 10).unwrap();
    fixture.claim_and_submit(&busy, &outcome.assignment_id, T4 + 20);
    let ranked = fill_in_candidates(&fixture.db, EVENT, &second, 2).unwrap();
    assert_eq!(ranked.len(), 2);
    assert_eq!(ranked[0].edits, 0);
    assert_eq!(ranked[1].participant_id, busy);
    assert_eq!(ranked[1].edits, 1);
}

// scenario: The automatic chain offers the slot to one person at a time
#[test]
fn an_auto_chain_offers_three_people_in_rank_order_then_asks_the_host() {
    let fixture = Fixture::started(4, flow(FillInFlow::AutoChain));
    volunteers(&fixture, 4);
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    for (user_id, assignment_id, _) in &round_zero[1..] {
        fixture.claim_and_submit(user_id, assignment_id, T4 + 1);
    }
    let ranked: Vec<String> = {
        skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
        let (first, _) = fill_in_offer(&fixture.db, &track).expect("the chain offers at once");
        let mut rest: Vec<String> = fill_in_candidates(&fixture.db, EVENT, &track, 2)
            .unwrap()
            .into_iter()
            .map(|candidate| candidate.participant_id)
            .collect();
        rest.insert(0, first);
        rest
    };

    // First person declines before the claim.
    skip(&fixture.db, EVENT, &ranked[0], T4 + 2).unwrap();
    let (second, second_id) = fill_in_offer(&fixture.db, &track).unwrap();
    assert_eq!(second, ranked[1]);
    let offer = AssignmentRepo::get(fixture.db.conn(), &second_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        offer.due_at,
        Some(T4 + 2 + 86_400),
        "each offer gets the fill-in window"
    );

    // Second person lets the offer expire.
    tick_deadlines(&fixture.db, T4 + 2 + 86_400).unwrap();
    let (third, _) = fill_in_offer(&fixture.db, &track).unwrap();
    assert_eq!(third, ranked[2]);
    assert_eq!(fill_in_decisions(&fixture.db), 0);

    // Third person declines: the chain stops after 3 offers.
    skip(&fixture.db, EVENT, &third, T4 + 86_500).unwrap();
    assert!(fill_in_offer(&fixture.db, &track).is_none());
    assert_eq!(fill_in_decisions(&fixture.db), 1);
    let chains = FillInChainRepo::list_for_event(fixture.db.conn(), EVENT).unwrap();
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].status, "exhausted");
    assert_eq!(chains[0].offers, 3);
    assert!(!chains[0].tried.contains(&ranked[3]));
}

#[test]
fn the_first_claim_fills_the_chain() {
    let fixture = Fixture::started(4, flow(FillInFlow::AutoChain));
    volunteers(&fixture, 2);
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
    let (user_id, offer_id) = fill_in_offer(&fixture.db, &track).unwrap();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &offer_id,
        &user_id,
        T4 + 5,
    )
    .unwrap();
    let chains = FillInChainRepo::list_for_event(fixture.db.conn(), EVENT).unwrap();
    assert_eq!(chains[0].status, "filled");
    assert_eq!(fill_in_decisions(&fixture.db), 0);
}

// scenario: The host orders a fill-in list
#[test]
fn a_host_list_skips_an_ineligible_person_and_works_down_the_list() {
    let fixture = Fixture::started(4, flow(FillInFlow::HostList));
    let people = volunteers(&fixture, 2);
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    let owner = swap_db::TrackRepo::get(fixture.db.conn(), EVENT, &track)
        .unwrap()
        .unwrap()
        .owner_id;
    skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
    assert!(fill_in_offer(&fixture.db, &track).is_none());
    assert_eq!(
        fill_in_decisions(&fixture.db),
        1,
        "the host must order a list"
    );

    let list = vec![owner.clone(), people[1].clone(), people[0].clone()];
    let outcome = start_fill_in_list(&fixture.db, EVENT, &track, 2, &list, T4 + 10).unwrap();
    assert_eq!(outcome.offered_to.as_deref(), Some(people[1].as_str()));
    assert_eq!(fill_in_decisions(&fixture.db), 0);

    skip(&fixture.db, EVENT, &people[1], T4 + 11).unwrap();
    let (next, _) = fill_in_offer(&fixture.db, &track).unwrap();
    assert_eq!(next, people[0]);
    skip(&fixture.db, EVENT, &people[0], T4 + 12).unwrap();
    assert!(fill_in_offer(&fixture.db, &track).is_none());
    assert_eq!(fill_in_decisions(&fixture.db), 1);
    let exhausted = start_fill_in_list(
        &fixture.db,
        EVENT,
        &track,
        2,
        std::slice::from_ref(&owner),
        T4 + 13,
    )
    .unwrap();
    assert_eq!(exhausted.status, "exhausted");
    assert!(
        exhausted.decision_opened,
        "a tracked cell opens a host decision when the list is exhausted"
    );
    assert!(
        AssignmentRepo::list_for_event(fixture.db.conn(), EVENT)
            .unwrap()
            .iter()
            .all(|assignment| assignment.participant_id != owner || assignment.track_code != track)
    );

    assert!(matches!(
        start_fill_in_list(&fixture.db, EVENT, &track, 2, &[], T4 + 13),
        Err(EngineError::FillInListEmpty)
    ));
    let duplicate = vec!["a".to_string(), "a".to_string()];
    assert!(matches!(
        start_fill_in_list(&fixture.db, EVENT, &track, 2, &duplicate, T4 + 13),
        Err(EngineError::FillInListDuplicate)
    ));
}

// scenario: The host appoints a fill-in by default
#[test]
fn a_declined_host_appointment_opens_a_host_decision() {
    let fixture = Fixture::started(4, EventSettings::default());
    let people = volunteers(&fixture, 2);
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
    assert_eq!(
        fill_in_decisions(&fixture.db),
        0,
        "a plain skip keeps today's behaviour"
    );
    assign_fill_in(&fixture.db, EVENT, &track, 2, &people[0], None, T4 + 2).unwrap();
    skip(&fixture.db, EVENT, &people[0], T4 + 3).unwrap();
    assert_eq!(fill_in_decisions(&fixture.db), 1);
    assert!(fill_in_offer(&fixture.db, &track).is_none());
    assign_fill_in(&fixture.db, EVENT, &track, 2, &people[1], None, T4 + 4).unwrap();
    assert_eq!(fill_in_decisions(&fixture.db), 0);
}

// scenario: A fill-in chain respects quiet hours
#[test]
fn a_chain_waits_for_quiet_hours_to_end() {
    let fixture = Fixture::started(4, flow(FillInFlow::AutoChain));
    add_fill_in(&fixture.db, "sleeper");
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
    assert!(
        fill_in_offer(&fixture.db, &track).is_none(),
        "00:38 UTC is quiet"
    );
    let chains = FillInChainRepo::list_active(fixture.db.conn(), EVENT).unwrap();
    assert_eq!(chains.len(), 1);
    assert!(chains[0].current_assignment_id.is_none());

    tick_deadlines(&fixture.db, T4 + 3_600).unwrap();
    assert!(fill_in_offer(&fixture.db, &track).is_none());
    let morning = T4 + 8 * 3_600;
    tick_deadlines(&fixture.db, morning).unwrap();
    let (user_id, offer_id) = fill_in_offer(&fixture.db, &track).unwrap();
    assert_eq!(user_id, "sleeper");
    let offer = AssignmentRepo::get(fixture.db.conn(), &offer_id)
        .unwrap()
        .unwrap();
    assert_eq!(offer.offered_at, morning);
}

// scenario: A fill-in chain never moves a round
#[test]
fn opening_the_next_round_stops_a_waiting_chain() {
    let fixture = Fixture::started(4, flow(FillInFlow::AutoChain));
    add_fill_in(&fixture.db, "sleeper");
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    for (index, (user_id, assignment_id, _)) in round_zero.iter().enumerate() {
        if index == 0 {
            skip(&fixture.db, EVENT, user_id, T4 + 1).unwrap();
        } else {
            fixture.claim_and_submit(user_id, assignment_id, T4 + 1);
        }
    }
    assert_eq!(
        FillInChainRepo::list_active(fixture.db.conn(), EVENT)
            .unwrap()
            .len(),
        1
    );
    open_round(&fixture.db, EVENT, 1, T4 + 100).unwrap();
    assert!(
        FillInChainRepo::list_active(fixture.db.conn(), EVENT)
            .unwrap()
            .is_empty()
    );
    tick_deadlines(&fixture.db, T4 + 8 * 3_600).unwrap();
    assert!(
        AssignmentRepo::list_active(fixture.db.conn(), EVENT)
            .unwrap()
            .iter()
            .all(|assignment| assignment.participant_id != "sleeper")
    );
}

// scenario: A fill-in chain survives a restart
#[test]
fn a_chain_resumes_after_a_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("restart.db");
    let store = BlobStore::new(directory.path().join("blobs"));
    let (track, first) = {
        let db = Db::open(&path).unwrap();
        seed_event(&db, &store, 4, &flow(FillInFlow::AutoChain), 20);
        lock_roster(&db, EVENT, T1).unwrap();
        generate_schedule(&db, EVENT, T2).unwrap();
        start_event(&db, EVENT, T3).unwrap();
        for index in 0..3 {
            add_fill_in_in(&db, &format!("volunteer-{index}"), AWAKE);
        }
        let round_zero = offers(&open_round(&db, EVENT, 0, T4).unwrap().effects);
        let (holder, _, track) = round_zero[0].clone();
        skip(&db, EVENT, &holder, T4 + 1).unwrap();
        let (first, _) = fill_in_offer(&db, &track).unwrap();
        (track, first)
    };

    let db = Db::open(&path).unwrap();
    let chain = FillInChainRepo::list_active(db.conn(), EVENT).unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].tried.last(), Some(&first));
    tick_deadlines(&db, T4 + 1 + 86_400).unwrap();
    let (second, _) = fill_in_offer(&db, &track).unwrap();
    assert_ne!(second, first);
    let chain = FillInChainRepo::list_active(db.conn(), EVENT).unwrap();
    assert_eq!(chain[0].offers, 2);
}

// ---- D-063, D-064, D-069, D-070, and MSW-084 -------------------------------

// scenario: A participant cannot take a second extra turn in the same round
#[test]
fn a_person_cannot_take_two_extra_turns_in_the_same_round() {
    let fixture = Fixture::started(10, group_settings());
    let (first_track, round_zero) = fixture.strand_round_zero(1);
    let second_track = round_zero[1].2.clone();
    let candidates = fill_in_candidates(&fixture.db, EVENT, &first_track, 2).unwrap();
    let helper = candidates[0].participant_id.clone();
    let outcome =
        assign_fill_in(&fixture.db, EVENT, &first_track, 2, &helper, None, T4 + 10).unwrap();
    fixture.claim_and_submit(&helper, &outcome.assignment_id, T4 + 20);

    assert!(
        fill_in_candidates(&fixture.db, EVENT, &second_track, 2)
            .unwrap()
            .iter()
            .all(|candidate| candidate.participant_id != helper),
        "the person already took an extra turn in this round"
    );
    let refused = assign_fill_in(&fixture.db, EVENT, &second_track, 2, &helper, None, T4 + 30);
    assert!(
        matches!(refused, Err(EngineError::FillInNotEligible(_))),
        "got {refused:?}"
    );
}

#[test]
fn an_auto_chain_does_not_offer_a_second_extra_turn() {
    let fixture = Fixture::started(10, group_flow(FillInFlow::AutoChain));
    wake_everyone(&fixture);
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, first_track) = round_zero[0].clone();
    let (second_holder, _, second_track) = round_zero[1].clone();
    for (user_id, assignment_id, code) in &round_zero[1..] {
        if *code != second_track {
            fixture.claim_and_submit(user_id, assignment_id, T4 + 1);
        }
    }
    skip(&fixture.db, EVENT, &holder, T4 + 2).unwrap();
    let (helper, offer_id) = fill_in_offer(&fixture.db, &first_track).unwrap();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &offer_id,
        &helper,
        T4 + 5,
    )
    .unwrap();
    let bytes = seed_bytes(200);
    submit(
        &fixture.db,
        &fixture.store,
        &PassthroughPreparer,
        &SubmitRequest {
            event_id: EVENT,
            user_id: &helper,
            filename: "edit.it",
            bytes: &bytes,
        },
        T4 + 6,
    )
    .unwrap();

    skip(&fixture.db, EVENT, &second_holder, T4 + 7).unwrap();
    let (next, _) = fill_in_offer(&fixture.db, &second_track).unwrap();
    assert_ne!(next, helper);
}

// scenario: The automatic chain moves from tier 1 to tier 2
#[test]
fn an_auto_chain_falls_to_tier_two_after_tier_one_declines() {
    let fixture = Fixture::started(10, group_flow(FillInFlow::AutoChain));
    wake_everyone(&fixture);
    let people = volunteers(&fixture, 2);
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    for (user_id, assignment_id, _) in &round_zero[1..] {
        fixture.claim_and_submit(user_id, assignment_id, T4 + 1);
    }
    skip(&fixture.db, EVENT, &holder, T4 + 2).unwrap();
    let (first, _) = fill_in_offer(&fixture.db, &track).unwrap();
    assert_eq!(first, people[0], "tier 1 comes first");
    skip(&fixture.db, EVENT, &first, T4 + 3).unwrap();
    let (second, _) = fill_in_offer(&fixture.db, &track).unwrap();
    assert_eq!(second, people[1], "the second tier-1 person is next");
    skip(&fixture.db, EVENT, &second, T4 + 4).unwrap();

    let (third, _) = fill_in_offer(&fixture.db, &track).unwrap();
    assert!(
        !people.contains(&third),
        "the third offer must be a tier-2 extra turn, got {third}"
    );
    assert_eq!(fill_in_decisions(&fixture.db), 0);

    skip(&fixture.db, EVENT, &third, T4 + 5).unwrap();
    assert!(fill_in_offer(&fixture.db, &track).is_none());
    assert_eq!(
        fill_in_decisions(&fixture.db),
        1,
        "both tiers are exhausted"
    );
}

#[test]
fn a_host_list_accepts_a_long_ordered_list() {
    let fixture = Fixture::started(10, flow(FillInFlow::HostList));
    let people = volunteers(&fixture, 8);
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
    let outcome = start_fill_in_list(&fixture.db, EVENT, &track, 2, &people, T4 + 2).unwrap();
    assert_eq!(outcome.offered_to.as_deref(), Some(people[0].as_str()));
    let (_, offer_id) = fill_in_offer(&fixture.db, &track).unwrap();
    let offer = AssignmentRepo::get(fixture.db.conn(), &offer_id)
        .unwrap()
        .unwrap();
    assert_eq!(offer.claim_deadline_at, Some(T4 + 2 + 6 * 3_600));
    assert_eq!(offer.due_at, Some(T4 + 2 + DAY));
    let chain = FillInChainRepo::list_active(fixture.db.conn(), EVENT)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(chain.host_list.len(), 8);
}

// scenario: A chain offer has a separate claim and edit window
#[test]
fn a_chain_offer_has_a_claim_window_and_a_separate_edit_due() {
    let fixture = Fixture::started(4, flow(FillInFlow::AutoChain));
    add_fill_in_in(&fixture.db, "volunteer", AWAKE);
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
    let (_, offer_id) = fill_in_offer(&fixture.db, &track).unwrap();
    let offer = AssignmentRepo::get(fixture.db.conn(), &offer_id)
        .unwrap()
        .unwrap();
    assert_eq!(offer.offered_at, T4 + 1);
    assert_eq!(offer.claim_deadline_at, Some(T4 + 1 + 6 * 3_600));
    assert_eq!(offer.due_at, Some(T4 + 1 + DAY));

    let claimed = claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &offer_id,
        "volunteer",
        T4 + 1 + 2 * 3_600,
    )
    .unwrap();
    let after = AssignmentRepo::get(fixture.db.conn(), &offer_id)
        .unwrap()
        .unwrap();
    assert_eq!(after.claimed_at, Some(T4 + 1 + 2 * 3_600));
    assert_eq!(
        after.due_at,
        Some(T4 + 1 + 2 * 3_600 + DAY),
        "the edit window starts at the claim"
    );
    assert_eq!(claimed.assignment_id, offer_id);
}

#[test]
fn a_chain_offer_expires_at_the_claim_window() {
    let fixture = Fixture::started(4, flow(FillInFlow::AutoChain));
    volunteers(&fixture, 2);
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
    let (first, offer_id) = fill_in_offer(&fixture.db, &track).unwrap();
    let claim_deadline = T4 + 1 + 6 * 3_600;
    tick_deadlines(&fixture.db, claim_deadline).unwrap();
    assert_eq!(
        AssignmentRepo::get(fixture.db.conn(), &offer_id)
            .unwrap()
            .unwrap()
            .status,
        AssignmentStatus::Expired
    );
    let (second, _) = fill_in_offer(&fixture.db, &track).unwrap();
    assert_ne!(second, first);
}

#[test]
fn a_host_due_time_before_the_round_deadline_is_kept_on_claim() {
    let fixture = Fixture::started(4, EventSettings::default());
    add_fill_in(&fixture.db, "volunteer");
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
    let round_deadline = swap_db::RoundRepo::get(fixture.db.conn(), EVENT, 2)
        .unwrap()
        .unwrap()
        .deadline_at
        .unwrap();
    let chosen = round_deadline - 3_600;
    let outcome = assign_fill_in(
        &fixture.db,
        EVENT,
        &track,
        2,
        "volunteer",
        Some(chosen),
        T4 + 10,
    )
    .unwrap();
    assert_eq!(outcome.due_at, chosen);
    let offer = AssignmentRepo::get(fixture.db.conn(), &outcome.assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(offer.claim_deadline_at, Some(chosen));
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &outcome.assignment_id,
        "volunteer",
        T4 + 20,
    )
    .unwrap();
    assert_eq!(
        AssignmentRepo::get(fixture.db.conn(), &outcome.assignment_id)
            .unwrap()
            .unwrap()
            .due_at,
        Some(chosen),
        "the claim must not extend a host due time to the round deadline"
    );
}

// scenario: A fill-in is refused for a cell that is not stranded
#[test]
fn a_fill_in_is_refused_for_a_cell_that_is_not_stranded() {
    let fixture = Fixture::started(4, EventSettings::default());
    add_fill_in(&fixture.db, "volunteer");
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (user_id, assignment_id, track) = round_zero[0].clone();
    fixture.claim_and_submit(&user_id, &assignment_id, T4 + 1);
    let refused = assign_fill_in(&fixture.db, EVENT, &track, 2, "volunteer", None, T4 + 10);
    assert!(
        matches!(refused, Err(EngineError::FillInNotEligible(_))),
        "got {refused:?}"
    );
    let list = start_fill_in_list(
        &fixture.db,
        EVENT,
        &track,
        2,
        &["volunteer".to_string()],
        T4 + 11,
    );
    assert!(matches!(list, Err(EngineError::FillInNotEligible(_))));
}

#[test]
fn a_round_deadline_move_does_not_change_a_fill_in_due_time() {
    let fixture = Fixture::started(4, EventSettings::default());
    add_fill_in(&fixture.db, "volunteer");
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
    let round_deadline = swap_db::RoundRepo::get(fixture.db.conn(), EVENT, 2)
        .unwrap()
        .unwrap()
        .deadline_at
        .unwrap();
    let chosen = round_deadline + 2 * 3_600;
    let outcome = assign_fill_in(
        &fixture.db,
        EVENT,
        &track,
        2,
        "volunteer",
        Some(chosen),
        T4 + 10,
    )
    .unwrap();
    move_round_deadline(
        &fixture.db,
        EVENT,
        2,
        round_deadline + 6 * 3_600,
        "test move",
        T4 + 20,
    )
    .unwrap();
    let moved = AssignmentRepo::get(fixture.db.conn(), &outcome.assignment_id)
        .unwrap()
        .unwrap();
    assert_eq!(moved.due_at, Some(chosen));
    assert_eq!(moved.claim_deadline_at, Some(chosen));
}

#[test]
fn a_reoffer_after_a_pre_claim_skip_can_be_claimed() {
    let fixture = Fixture::started(4, EventSettings::default());
    add_fill_in(&fixture.db, "volunteer");
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
    let first = assign_fill_in(&fixture.db, EVENT, &track, 2, "volunteer", None, T4 + 2).unwrap();
    skip(&fixture.db, EVENT, "volunteer", T4 + 3).unwrap();
    let second = assign_fill_in(&fixture.db, EVENT, &track, 2, "volunteer", None, T4 + 4).unwrap();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &second.assignment_id,
        "volunteer",
        T4 + 5,
    )
    .expect("a pre-claim skip must not block a later claim");
    assert_ne!(first.assignment_id, second.assignment_id);
}

#[test]
fn a_reoffer_after_an_expired_fill_in_offer_can_be_claimed() {
    let fixture = Fixture::started(4, EventSettings::default());
    add_fill_in(&fixture.db, "volunteer");
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
    let first = assign_fill_in(
        &fixture.db,
        EVENT,
        &track,
        2,
        "volunteer",
        Some(T4 + 100),
        T4 + 2,
    )
    .unwrap();
    tick_deadlines(&fixture.db, T4 + 100).unwrap();
    assert_eq!(
        AssignmentRepo::get(fixture.db.conn(), &first.assignment_id)
            .unwrap()
            .unwrap()
            .status,
        AssignmentStatus::Expired
    );
    let second =
        assign_fill_in(&fixture.db, EVENT, &track, 2, "volunteer", None, T4 + 101).unwrap();
    claim(
        &fixture.db,
        &fixture.store,
        EVENT,
        &second.assignment_id,
        "volunteer",
        T4 + 102,
    )
    .expect("an expired offer must not block a later claim");
}

#[test]
fn a_host_list_offers_tier_one_before_a_tier_two_entry() {
    let fixture = Fixture::started(10, group_flow(FillInFlow::HostList));
    let people = volunteers(&fixture, 1);
    let (track, round_zero) = fixture.strand_round_zero(0);
    let extra = expected_extra_turns(&fixture, &track, &round_zero, 0);
    let tier_two = extra
        .first()
        .expect("the group plan must leave an extra-turn candidate")
        .clone();
    let list = vec![tier_two.clone(), people[0].clone()];
    let outcome = start_fill_in_list(&fixture.db, EVENT, &track, 2, &list, T4 + 10).unwrap();
    assert_eq!(
        outcome.offered_to.as_deref(),
        Some(people[0].as_str()),
        "the host order cannot put tier 2 before an eligible tier-1 person"
    );
}

#[test]
fn an_auto_chain_with_one_volunteer_falls_to_tier_two() {
    let fixture = Fixture::started(10, group_flow(FillInFlow::AutoChain));
    wake_everyone(&fixture);
    let people = volunteers(&fixture, 1);
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    for (user_id, assignment_id, _) in &round_zero[1..] {
        fixture.claim_and_submit(user_id, assignment_id, T4 + 1);
    }
    skip(&fixture.db, EVENT, &holder, T4 + 2).unwrap();
    let (first, _) = fill_in_offer(&fixture.db, &track).unwrap();
    assert_eq!(first, people[0]);
    skip(&fixture.db, EVENT, &first, T4 + 3).unwrap();
    let (second, _) = fill_in_offer(&fixture.db, &track).unwrap();
    assert_ne!(second, people[0]);
    assert!(
        !second.starts_with("volunteer"),
        "the next offer must be a tier-2 extra turn"
    );
    assert_eq!(fill_in_decisions(&fixture.db), 0);
}

#[test]
fn an_exhausted_auto_chain_hands_over_to_a_host_appointment() {
    let fixture = Fixture::started(4, flow(FillInFlow::AutoChain));
    let people = volunteers(&fixture, 4);
    let round_zero = offers(&open_round(&fixture.db, EVENT, 0, T4).unwrap().effects);
    let (holder, _, track) = round_zero[0].clone();
    skip(&fixture.db, EVENT, &holder, T4 + 1).unwrap();
    let mut offered_people = Vec::new();
    for index in 0..3 {
        let (offered, _) = fill_in_offer(&fixture.db, &track).unwrap();
        offered_people.push(offered.clone());
        skip(&fixture.db, EVENT, &offered, T4 + 2 + index).unwrap();
    }
    assert_eq!(fill_in_decisions(&fixture.db), 1, "the chain is exhausted");
    let target = people
        .iter()
        .find(|person| !offered_people.contains(person))
        .expect("one volunteer was not tried")
        .clone();
    let outcome = assign_fill_in(&fixture.db, EVENT, &track, 2, &target, None, T4 + 10).unwrap();
    assert_eq!(fill_in_decisions(&fixture.db), 0);
    let (offered, _) = fill_in_offer(&fixture.db, &track).unwrap();
    assert_eq!(offered, target);
    assert_eq!(outcome.due_at, T4 + 10 + 86_400);
}

#[test]
fn a_fill_in_list_exhausts_a_gap_cell_without_a_decision() {
    let fixture = Fixture::started(4, flow(FillInFlow::HostList));
    let planned = fixture
        .plan
        .iter()
        .find(|row| row.round == 0)
        .cloned()
        .unwrap();
    leave_event(
        &fixture.db,
        EVENT,
        &planned.participant_id,
        LeaveActor::Host,
        T4,
    )
    .unwrap();
    let outcome = open_round(&fixture.db, EVENT, 0, T4 + 1).unwrap();
    assert_eq!(outcome.offers, 3, "the leaver's cell is a gap");
    let owner = swap_db::TrackRepo::get(fixture.db.conn(), EVENT, &planned.track_code)
        .unwrap()
        .unwrap()
        .owner_id;
    let list =
        start_fill_in_list(&fixture.db, EVENT, &planned.track_code, 2, &[owner], T4 + 2).unwrap();
    assert_eq!(list.status, "exhausted");
    assert!(
        !list.decision_opened,
        "a gap cell with no assignment row has no decision"
    );
}

// scenario: A dedicated fill-in edit counts toward the shared budget
#[test]
fn a_dedicated_fill_in_respects_the_handoff_cap() {
    let fixture = Fixture::started(4, EventSettings::default());
    add_fill_in_with_cap(&fixture.db, "volunteer", 1);
    let (first_track, round_zero) = fixture.strand_round_zero(1);
    let second_track = round_zero[1].2.clone();
    let outcome = assign_fill_in(
        &fixture.db,
        EVENT,
        &first_track,
        2,
        "volunteer",
        None,
        T4 + 10,
    )
    .unwrap();
    fixture.claim_and_submit("volunteer", &outcome.assignment_id, T4 + 20);
    assert!(
        fill_in_candidates(&fixture.db, EVENT, &second_track, 2)
            .unwrap()
            .iter()
            .all(|candidate| candidate.participant_id != "volunteer")
    );
    assert!(matches!(
        assign_fill_in(
            &fixture.db,
            EVENT,
            &second_track,
            2,
            "volunteer",
            None,
            T4 + 30
        ),
        Err(EngineError::FillInNotEligible(_))
    ));
}

// scenario: A final fill-in can name the track
#[test]
fn a_final_fill_in_can_name_the_track() {
    let fixture = Fixture::started(4, EventSettings::default());
    for round in 0..2 {
        let round_offers = offers(
            &open_round(&fixture.db, EVENT, round, T4 + round as i64 * 100)
                .unwrap()
                .effects,
        );
        for (user_id, assignment_id, _) in &round_offers {
            fixture.claim_and_submit(user_id, assignment_id, T4 + round as i64 * 100 + 1);
        }
    }
    let final_offers = offers(&open_round(&fixture.db, EVENT, 2, T5).unwrap().effects);
    let (holder, _, track) = final_offers[0].clone();
    skip(&fixture.db, EVENT, &holder, T5 + 1).unwrap();
    for (user_id, assignment_id, _) in &final_offers[1..] {
        fixture.claim_and_submit(user_id, assignment_id, T5 + 2);
    }
    add_fill_in(&fixture.db, "volunteer");
    let outcome =
        assign_fill_in(&fixture.db, EVENT, &track, 4, "volunteer", None, T5 + 10).unwrap();
    fixture.claim_and_submit("volunteer", &outcome.assignment_id, T5 + 20);
    let title = set_track_title(
        &fixture.db,
        EVENT,
        &track,
        "Final Name",
        "volunteer",
        T5 + 30,
    )
    .expect("the final fill-in must name the track");
    assert_eq!(title.title, "Final Name");
}
