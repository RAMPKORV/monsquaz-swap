use std::collections::HashSet;

use cucumber::{World as _, given, then, when};
use swap_core::{ParticipantId, Schedule, ScheduleError, SeenLedger, Touch, TrackCode, TrackSpec};

#[derive(Debug, Default, cucumber::World)]
struct SwapWorld {
    people: Vec<ParticipantId>,
    tracks: Vec<TrackSpec>,
    rounds: u32,
    schedule: Option<Schedule>,
    error: Option<ScheduleError>,
    ledger: SeenLedger,
}

fn person(value: &str) -> ParticipantId {
    ParticipantId::new(value)
}

fn track(value: &str) -> TrackCode {
    TrackCode::new(value)
}

fn roster(world: &mut SwapWorld, count: usize) {
    world.people = (0..count)
        .map(|index| person(&format!("P{index:02}")))
        .collect();
    world.tracks = world
        .people
        .iter()
        .enumerate()
        .map(|(index, owner)| TrackSpec {
            code: track(&format!("T{index:02}")),
            owner: owner.clone(),
        })
        .collect();
}

#[given("a locked roster of 8 participants with one seed track each")]
fn roster_eight(world: &mut SwapWorld) {
    roster(world, 8);
}

#[given("a locked roster of 4 participants with one seed track each")]
fn roster_four(world: &mut SwapWorld) {
    roster(world, 4);
}

#[given(regex = r"^a classic schedule with (\d+) swap rounds$")]
fn schedule_rounds(world: &mut SwapWorld, rounds: u32) {
    world.rounds = rounds;
}

#[when("the host generates the schedule")]
fn generate(world: &mut SwapWorld) {
    match Schedule::generate(&world.people, &world.tracks, world.rounds, 42) {
        Ok(schedule) => world.schedule = Some(schedule),
        Err(error) => world.error = Some(error),
    }
}

#[then("each round assigns one track to each active participant")]
fn each_round_has_one_track_per_person(world: &mut SwapWorld) {
    let schedule = world.schedule.as_ref().expect("schedule exists");
    for round in 0..schedule.rounds {
        let count = schedule
            .assignments
            .iter()
            .filter(|assignment| assignment.round == round)
            .count();
        assert_eq!(count, world.tracks.len(), "round {round} count");
    }
}

#[then("each round assigns each track to at most one participant")]
fn each_round_has_one_person_per_track(world: &mut SwapWorld) {
    let schedule = world.schedule.as_ref().expect("schedule exists");
    for round in 0..schedule.rounds {
        let mut seen_tracks = HashSet::new();
        for assignment in schedule
            .assignments
            .iter()
            .filter(|assignment| assignment.round == round)
        {
            assert!(
                seen_tracks.insert(assignment.track.clone()),
                "track reused in round {round}"
            );
        }
        assert_eq!(
            seen_tracks.len(),
            world.tracks.len(),
            "round {round} tracks"
        );
    }
}

#[then("no assignment gives a participant their own track")]
fn no_own_track(world: &mut SwapWorld) {
    let schedule = world.schedule.as_ref().expect("schedule exists");
    for assignment in &schedule.assignments {
        let spec = world
            .tracks
            .iter()
            .find(|spec| spec.code == assignment.track)
            .expect("track exists");
        assert_ne!(spec.owner, assignment.participant, "own track");
    }
}

#[then("no participant receives the same track twice")]
fn no_repeat_pair(world: &mut SwapWorld) {
    let schedule = world.schedule.as_ref().expect("schedule exists");
    let mut pairs = HashSet::new();
    for assignment in &schedule.assignments {
        assert!(
            pairs.insert((assignment.track.clone(), assignment.participant.clone())),
            "repeat pair"
        );
    }
}

#[then("the scheduler refuses the request")]
fn scheduler_refuses(world: &mut SwapWorld) {
    assert!(world.error.is_some(), "expected a schedule error");
    assert!(world.schedule.is_none(), "expected no schedule");
}

#[then("the error names the maximum of 3 rounds")]
fn error_names_max(world: &mut SwapWorld) {
    match world.error.as_ref() {
        Some(ScheduleError::TooManyRounds { max, .. }) => assert_eq!(*max, 3),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[given("an empty seen ledger")]
fn empty_ledger(world: &mut SwapWorld) {
    world.ledger = SeenLedger::new();
}

#[when("a participant claims a track")]
fn claim_track(world: &mut SwapWorld) {
    assert!(
        world
            .ledger
            .apply(&track("A"), &person("alice"), Touch::Claim)
    );
}

#[when("the participant submits the track")]
fn submit_track(world: &mut SwapWorld) {
    assert!(
        !world
            .ledger
            .apply(&track("A"), &person("alice"), Touch::Submit)
    );
}

#[when("another participant skips after claiming a track")]
fn skip_after_claim(world: &mut SwapWorld) {
    assert!(
        world
            .ledger
            .apply(&track("B"), &person("bob"), Touch::SkipAfterClaim)
    );
}

#[when("a third participant skips before claiming a track")]
fn skip_before_claim(world: &mut SwapWorld) {
    assert!(
        !world
            .ledger
            .apply(&track("C"), &person("carol"), Touch::SkipBeforeClaim)
    );
}

#[when("an offer to a fourth participant expires")]
fn offer_expires(world: &mut SwapWorld) {
    assert!(
        !world
            .ledger
            .apply(&track("D"), &person("dave"), Touch::OfferExpired)
    );
}

#[then("the claim pair is seen")]
fn claim_pair_seen(world: &mut SwapWorld) {
    assert!(world.ledger.has_seen(&track("A"), &person("alice")));
}

#[then("the submit pair is seen")]
fn submit_pair_seen(world: &mut SwapWorld) {
    assert!(world.ledger.has_seen(&track("A"), &person("alice")));
}

#[then("the skip-after-claim pair is seen")]
fn skip_after_pair_seen(world: &mut SwapWorld) {
    assert!(world.ledger.has_seen(&track("B"), &person("bob")));
}

#[then("the skip-before-claim pair is not seen")]
fn skip_before_pair_not_seen(world: &mut SwapWorld) {
    assert!(!world.ledger.has_seen(&track("C"), &person("carol")));
}

#[then("the expired-offer pair is not seen")]
fn expired_pair_not_seen(world: &mut SwapWorld) {
    assert!(!world.ledger.has_seen(&track("D"), &person("dave")));
}

#[tokio::main]
async fn main() {
    // `World::run` calls `run_and_exit`, so a failed step fails `cargo test`.
    SwapWorld::run("../../specs/features/core_invariants.feature").await;
}
