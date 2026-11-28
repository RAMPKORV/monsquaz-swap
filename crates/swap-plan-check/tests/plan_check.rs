//! Independent tests for the Rust checker and the corpus.
//!
//! The oracle in this file does not call the checker. It repeats the contract
//! with `HashSet` and explicit loops.

use std::collections::HashSet;

use swap_plan_check::corpus::{self, CaseKind};
use swap_plan_check::{
    Assignment, MAX_ASSIGNMENTS, MAX_PEOPLE, MAX_ROUNDS, MAX_TRACKS, NO_OWNER, PlanInput, Policy,
    Round, check_plan, verify_hall_certificate,
};

#[test]
fn corpus_has_at_least_one_thousand_plans() {
    let corpus = corpus::generate(1_200, 600);
    assert!(corpus.plans.len() >= 1_000);
    assert!(corpus.certs.len() >= 500);
}

#[test]
fn corpus_covers_every_required_case() {
    let corpus = corpus::generate(1_200, 600);
    let mut kinds = HashSet::new();
    let mut roster_sizes = HashSet::new();
    let mut policies = HashSet::new();
    for plan in &corpus.plans {
        kinds.insert(plan.kind);
        roster_sizes.insert(plan.plan.active_people_len);
        policies.insert(plan.plan.policy);
    }
    for kind in CaseKind::ALL {
        assert!(kinds.contains(&kind), "missing case kind: {}", kind.label());
    }
    assert!(roster_sizes.contains(&2));
    assert!(roster_sizes.contains(&12));
    assert!(policies.contains(&Policy::Strict));
    assert!(policies.contains(&Policy::Flexible));
}

#[test]
fn every_plan_verdict_matches_the_independent_oracle() {
    let corpus = corpus::generate(1_200, 600);
    for plan in &corpus.plans {
        let accepted = check_plan(&plan.plan).is_ok();
        let expected = oracle_accepts(&plan.plan);
        assert_eq!(
            accepted,
            expected,
            "case {} ({}) disagrees with the oracle",
            plan.id,
            plan.kind.label()
        );
    }
}

#[test]
fn corpus_reaches_every_typed_reject() {
    let corpus = corpus::generate(1_200, 600);
    let codes = corpus::reject_codes(&corpus);
    for code in 1..=14 {
        assert!(codes.contains(&code), "missing reject code {code}");
    }
}

#[test]
fn corpus_has_accepted_and_rejected_plans() {
    let corpus = corpus::generate(1_200, 600);
    let accepted = corpus
        .plans
        .iter()
        .filter(|plan| check_plan(&plan.plan).is_ok())
        .count();
    let rejected = corpus.plans.len() - accepted;
    assert!(accepted >= 100, "accepted plans: {accepted}");
    assert!(rejected >= 100, "rejected plans: {rejected}");
}

#[test]
fn corpus_has_accepted_and_rejected_certificates() {
    let corpus = corpus::generate(1_200, 600);
    let accepted = corpus
        .certs
        .iter()
        .filter(|cert| verify_hall_certificate(&corpus.plans[cert.plan_id].plan, &cert.cert))
        .count();
    let rejected = corpus.certs.len() - accepted;
    assert!(accepted >= 50, "accepted certificates: {accepted}");
    assert!(rejected >= 50, "rejected certificates: {rejected}");
}

fn oracle_accepts(input: &PlanInput) -> bool {
    if input.active_people_len > MAX_PEOPLE
        || input.active_tracks_len > MAX_TRACKS
        || input.rounds_len > MAX_ROUNDS
        || input.rounds.iter().any(|round| round.len > MAX_ASSIGNMENTS)
    {
        return false;
    }
    let people = &input.active_people[..input.active_people_len];
    let tracks = &input.active_tracks[..input.active_tracks_len];
    if people
        .iter()
        .any(|participant| *participant as usize >= MAX_PEOPLE)
    {
        return false;
    }
    if tracks.iter().any(|track| *track as usize >= MAX_TRACKS) {
        return false;
    }
    if people.len() != people.iter().copied().collect::<HashSet<_>>().len() {
        return false;
    }
    if tracks.len() != tracks.iter().copied().collect::<HashSet<_>>().len() {
        return false;
    }
    for &track in tracks {
        if track as usize >= MAX_TRACKS || input.owners[track as usize] == NO_OWNER {
            return false;
        }
    }

    let mut seen: HashSet<(u16, u16)> = HashSet::new();
    for round in &input.rounds[..input.rounds_len] {
        let mut round_people = HashSet::new();
        let mut round_tracks = HashSet::new();
        for assignment in &round.assignments[..round.len] {
            if !people.contains(&assignment.participant) {
                return false;
            }
            if !tracks.contains(&assignment.track) {
                return false;
            }
            if input.owners[assignment.track as usize] == NO_OWNER {
                return false;
            }
            if input.owners[assignment.track as usize] == assignment.participant {
                return false;
            }
            if !round_people.insert(assignment.participant) {
                return false;
            }
            if !round_tracks.insert(assignment.track) {
                return false;
            }
            if !seen.insert((assignment.participant, assignment.track)) {
                return false;
            }
        }
        if input.policy == Policy::Strict {
            if round_people.len() != people.len()
                || !people.iter().all(|person| round_people.contains(person))
            {
                return false;
            }
            if round_tracks.len() != tracks.len()
                || !tracks.iter().all(|track| round_tracks.contains(track))
            {
                return false;
            }
        }
    }
    true
}

#[test]
fn empty_plan_is_vacuously_valid() {
    let input = PlanInput::default();
    assert_eq!(check_plan(&input), Ok(()));
}

#[test]
fn arbitrary_input_never_panics() {
    let mut seed = 0x1234_5678_9ABC_DEF0u64;
    for _ in 0..2_000 {
        seed ^= seed >> 12;
        seed ^= seed << 25;
        seed ^= seed >> 27;
        let mut input = PlanInput {
            active_people_len: (seed as usize) % (MAX_PEOPLE + 4),
            active_tracks_len: ((seed >> 8) as usize) % (MAX_TRACKS + 4),
            rounds_len: ((seed >> 16) as usize) % (MAX_ROUNDS + 3),
            ..PlanInput::default()
        };
        for (index, value) in input.active_people.iter_mut().enumerate() {
            *value = ((seed >> (index % 16)) as u16).wrapping_add(index as u16);
        }
        for (index, value) in input.active_tracks.iter_mut().enumerate() {
            *value = ((seed >> (index % 16)) as u16).wrapping_add(index as u16);
        }
        for (index, value) in input.owners.iter_mut().enumerate() {
            *value = if index % 3 == 0 {
                NO_OWNER
            } else {
                (seed >> (index % 16)) as u16
            };
        }
        for round in &mut input.rounds {
            round.len = ((seed >> 24) as usize) % (MAX_ASSIGNMENTS + 3);
            for (index, assignment) in round.assignments.iter_mut().enumerate() {
                assignment.participant = ((seed >> (index % 16)) as u16).wrapping_add(1);
                assignment.track = ((seed >> ((index + 4) % 16)) as u16).wrapping_add(2);
            }
        }
        let _ = check_plan(&input);
        let cert = swap_plan_check::HallCertificate {
            round: (seed as usize) % (MAX_ROUNDS + 3),
            tracks: input.active_tracks,
            tracks_len: (seed as usize) % (MAX_TRACKS + 3),
            neighbors: input.active_people,
            neighbors_len: ((seed >> 8) as usize) % (MAX_PEOPLE + 3),
        };
        let _ = verify_hall_certificate(&input, &cert);
    }
}

#[test]
fn round_default_is_empty() {
    let round = Round::default();
    assert_eq!(round.len, 0);
    assert_eq!(round.assignments[0], Assignment::default());
}
