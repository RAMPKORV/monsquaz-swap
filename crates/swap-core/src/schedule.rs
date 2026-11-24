use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ids::{ParticipantId, TrackCode};

const MAX_ATTEMPTS: u32 = 128;
const SPLITMIX_GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

/// One seeded track and its owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackSpec {
    pub code: TrackCode,
    pub owner: ParticipantId,
}

/// One track handoff in a classic round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledAssignment {
    pub round: u32,
    pub track: TrackCode,
    pub participant: ParticipantId,
}

/// A complete classic schedule. The output is canonical for a given input and
/// seed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub seed: u64,
    pub rounds: u32,
    pub assignments: Vec<ScheduledAssignment>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ScheduleError {
    #[error("the roster must contain at least one participant and one track")]
    EmptyRoster,
    #[error("there are more tracks ({tracks}) than participants ({participants})")]
    NotEnoughParticipants { tracks: usize, participants: usize },
    #[error("duplicate participant: {0:?}")]
    DuplicateParticipant(ParticipantId),
    #[error("duplicate track: {0:?}")]
    DuplicateTrack(TrackCode),
    #[error("track owner is not in the roster: {0:?}")]
    MissingOwner(TrackCode),
    #[error("requested {requested} rounds, but the maximum is {max}")]
    TooManyRounds { requested: u32, max: u32 },
    #[error("no perfect matching exists in round {round}")]
    Infeasible { round: u32 },
}

impl Schedule {
    /// Generate a randomized partial Latin rectangle.
    ///
    /// Each round is a perfect matching over tracks and participants. A person
    /// never receives their own track and never receives a track twice. The
    /// result is deterministic for the same participant set, track set, round
    /// count, and seed. Input order does not matter.
    pub fn generate(
        participants: &[ParticipantId],
        tracks: &[TrackSpec],
        rounds: u32,
        seed: u64,
    ) -> Result<Self, ScheduleError> {
        if participants.is_empty() || tracks.is_empty() {
            return Err(ScheduleError::EmptyRoster);
        }

        let mut people = participants.to_vec();
        people.sort_by(|a, b| a.0.cmp(&b.0));
        let mut seen_people = HashSet::with_capacity(people.len());
        for person in &people {
            if !seen_people.insert(person.clone()) {
                return Err(ScheduleError::DuplicateParticipant(person.clone()));
            }
        }

        let mut specs = tracks.to_vec();
        specs.sort_by(|a, b| a.code.0.cmp(&b.code.0));
        let mut seen_tracks = HashSet::with_capacity(specs.len());
        for spec in &specs {
            if !seen_tracks.insert(spec.code.clone()) {
                return Err(ScheduleError::DuplicateTrack(spec.code.clone()));
            }
        }

        if specs.len() > people.len() {
            return Err(ScheduleError::NotEnoughParticipants {
                tracks: specs.len(),
                participants: people.len(),
            });
        }

        let max_rounds = people.len().saturating_sub(1) as u32;
        if rounds > max_rounds {
            return Err(ScheduleError::TooManyRounds {
                requested: rounds,
                max: max_rounds,
            });
        }

        let person_index: HashMap<ParticipantId, usize> = people
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, person)| (person, index))
            .collect();

        let owners: Vec<usize> = specs
            .iter()
            .map(|spec| {
                person_index
                    .get(&spec.owner)
                    .copied()
                    .ok_or_else(|| ScheduleError::MissingOwner(spec.code.clone()))
            })
            .collect::<Result<_, _>>()?;

        let mut last_round = 0;
        for attempt in 0..MAX_ATTEMPTS {
            let attempt_seed = seed.wrapping_add(u64::from(attempt).wrapping_mul(SPLITMIX_GOLDEN));
            match attempt_schedule(&people, &specs, &owners, rounds, attempt_seed) {
                Ok(mut assignments) => {
                    assignments.sort_by(|a, b| {
                        a.round
                            .cmp(&b.round)
                            .then_with(|| a.track.0.cmp(&b.track.0))
                            .then_with(|| a.participant.0.cmp(&b.participant.0))
                    });
                    return Ok(Self {
                        seed,
                        rounds,
                        assignments,
                    });
                }
                Err(round) => last_round = round,
            }
        }

        Err(ScheduleError::Infeasible { round: last_round })
    }
}

/// Try one deterministic schedule attempt. A matching can paint itself into a
/// corner when there are fewer tracks than people; the caller retries with a
/// derived seed. Returns the first failed round.
fn attempt_schedule(
    people: &[ParticipantId],
    specs: &[TrackSpec],
    owners: &[usize],
    rounds: u32,
    seed: u64,
) -> Result<Vec<ScheduledAssignment>, u32> {
    let mut history: Vec<HashSet<usize>> = vec![HashSet::new(); specs.len()];
    let mut rng = SplitMix64::new(seed);
    let mut assignments = Vec::with_capacity(rounds as usize * specs.len());

    for round in 0..rounds {
        let adjacency: Vec<Vec<usize>> = specs
            .iter()
            .enumerate()
            .map(|(track_index, _)| {
                let mut eligible: Vec<usize> = (0..people.len())
                    .filter(|person| {
                        *person != owners[track_index] && !history[track_index].contains(person)
                    })
                    .collect();
                rng.shuffle(&mut eligible);
                eligible
            })
            .collect();

        let mut order: Vec<usize> = (0..specs.len()).collect();
        rng.shuffle(&mut order);

        let mut match_person: Vec<Option<usize>> = vec![None; people.len()];
        for &track_index in &order {
            let mut visited = vec![false; people.len()];
            if !try_kuhn(track_index, &adjacency, &mut visited, &mut match_person) {
                return Err(round);
            }
        }

        for (person_index, track_index) in match_person.iter().enumerate() {
            let Some(track_index) = track_index else {
                continue;
            };
            history[*track_index].insert(person_index);
            assignments.push(ScheduledAssignment {
                round,
                track: specs[*track_index].code.clone(),
                participant: people[person_index].clone(),
            });
        }
    }

    Ok(assignments)
}

/// Kuhn's augmenting-path matching. Track vertices are matched to people.
fn try_kuhn(
    track_index: usize,
    adjacency: &[Vec<usize>],
    visited: &mut [bool],
    match_person: &mut [Option<usize>],
) -> bool {
    for &person_index in &adjacency[track_index] {
        if visited[person_index] {
            continue;
        }
        visited[person_index] = true;
        let can_take = match match_person[person_index] {
            None => true,
            Some(other_track) => try_kuhn(other_track, adjacency, visited, match_person),
        };
        if can_take {
            match_person[person_index] = Some(track_index);
            return true;
        }
    }
    false
}

/// Small deterministic PRNG. SplitMix64 is enough for schedule randomization;
/// a cryptographic generator is not required. Keep the schedule reproducible.
#[derive(Debug, Clone)]
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }

    fn next_bounded(&mut self, upper: usize) -> usize {
        assert!(upper > 0, "upper bound must be positive");
        let upper = upper as u64;
        let zone = u64::MAX - (u64::MAX % upper);
        loop {
            let value = self.next_u64();
            if value < zone {
                return (value % upper) as usize;
            }
        }
    }

    fn shuffle<T>(&mut self, values: &mut [T]) {
        if values.len() < 2 {
            return;
        }
        for index in (1..values.len()).rev() {
            let other = self.next_bounded(index + 1);
            values.swap(index, other);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people(count: usize) -> Vec<ParticipantId> {
        (0..count)
            .map(|index| ParticipantId::new(format!("P{index:02}")))
            .collect()
    }

    fn tracks(people: &[ParticipantId]) -> Vec<TrackSpec> {
        people
            .iter()
            .enumerate()
            .map(|(index, owner)| TrackSpec {
                code: TrackCode::new(format!("T{index:02}")),
                owner: owner.clone(),
            })
            .collect()
    }

    fn assert_valid(schedule: &Schedule, people: &[ParticipantId], tracks: &[TrackSpec]) {
        let mut seen_pairs = HashSet::new();
        for round in 0..schedule.rounds {
            let mut round_people = HashSet::new();
            let mut round_tracks = HashSet::new();
            for assignment in schedule
                .assignments
                .iter()
                .filter(|assignment| assignment.round == round)
            {
                assert!(round_people.insert(assignment.participant.clone()));
                assert!(round_tracks.insert(assignment.track.clone()));
                let spec = tracks
                    .iter()
                    .find(|spec| spec.code == assignment.track)
                    .expect("track exists");
                assert_ne!(spec.owner, assignment.participant, "own track");
                assert!(
                    seen_pairs.insert((assignment.track.clone(), assignment.participant.clone())),
                    "repeat pair"
                );
            }
            assert_eq!(round_tracks.len(), tracks.len(), "all tracks assigned");
        }
        assert_eq!(
            schedule.assignments.len(),
            schedule.rounds as usize * tracks.len()
        );
        assert!(
            schedule
                .assignments
                .iter()
                .all(|assignment| people.contains(&assignment.participant))
        );
    }

    #[test]
    fn a_full_event_has_no_repeats_or_own_tracks() {
        let people = people(8);
        let tracks = tracks(&people);
        let schedule = Schedule::generate(&people, &tracks, 7, 42).unwrap();
        assert_valid(&schedule, &people, &tracks);
        assert_eq!(schedule.assignments.len(), 56);
    }

    #[test]
    fn a_small_event_is_valid() {
        let people = people(3);
        let tracks = tracks(&people);
        let schedule = Schedule::generate(&people, &tracks, 2, 7).unwrap();
        assert_valid(&schedule, &people, &tracks);
        assert_eq!(schedule.assignments.len(), 6);
    }

    #[test]
    fn the_same_seed_produces_the_same_schedule() {
        let people = people(8);
        let tracks = tracks(&people);
        let first = Schedule::generate(&people, &tracks, 7, 123).unwrap();
        let second = Schedule::generate(&people, &tracks, 7, 123).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn input_order_does_not_change_the_schedule() {
        let people = people(6);
        let tracks = tracks(&people);
        let first = Schedule::generate(&people, &tracks, 5, 9).unwrap();

        let mut reversed_people = people.clone();
        reversed_people.reverse();
        let mut reversed_tracks = tracks.clone();
        reversed_tracks.reverse();
        let second = Schedule::generate(&reversed_people, &reversed_tracks, 5, 9).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn more_rounds_than_people_are_refused() {
        let people = people(4);
        let tracks = tracks(&people);
        assert_eq!(
            Schedule::generate(&people, &tracks, 4, 1),
            Err(ScheduleError::TooManyRounds {
                requested: 4,
                max: 3
            })
        );
    }

    #[test]
    fn duplicate_people_are_refused() {
        let people = vec![
            ParticipantId::new("A"),
            ParticipantId::new("A"),
            ParticipantId::new("B"),
        ];
        let tracks = vec![TrackSpec {
            code: TrackCode::new("T"),
            owner: ParticipantId::new("A"),
        }];
        assert_eq!(
            Schedule::generate(&people, &tracks, 1, 1),
            Err(ScheduleError::DuplicateParticipant(ParticipantId::new("A")))
        );
    }

    #[test]
    fn duplicate_tracks_are_refused() {
        let people = people(2);
        let tracks = vec![
            TrackSpec {
                code: TrackCode::new("T"),
                owner: people[0].clone(),
            },
            TrackSpec {
                code: TrackCode::new("T"),
                owner: people[1].clone(),
            },
        ];
        assert_eq!(
            Schedule::generate(&people, &tracks, 1, 1),
            Err(ScheduleError::DuplicateTrack(TrackCode::new("T")))
        );
    }

    #[test]
    fn a_missing_owner_is_refused() {
        let people = people(2);
        let tracks = vec![TrackSpec {
            code: TrackCode::new("T"),
            owner: ParticipantId::new("missing"),
        }];
        assert_eq!(
            Schedule::generate(&people, &tracks, 1, 1),
            Err(ScheduleError::MissingOwner(TrackCode::new("T")))
        );
    }

    #[test]
    fn more_tracks_than_people_is_refused() {
        let people = people(2);
        let tracks = vec![
            TrackSpec {
                code: TrackCode::new("T1"),
                owner: people[0].clone(),
            },
            TrackSpec {
                code: TrackCode::new("T2"),
                owner: people[1].clone(),
            },
            TrackSpec {
                code: TrackCode::new("T3"),
                owner: people[0].clone(),
            },
        ];
        assert_eq!(
            Schedule::generate(&people, &tracks, 1, 1),
            Err(ScheduleError::NotEnoughParticipants {
                tracks: 3,
                participants: 2
            })
        );
    }

    #[test]
    fn fewer_tracks_than_people_is_allowed() {
        let people = people(6);
        let tracks = tracks(&people)[..3].to_vec();
        let schedule = Schedule::generate(&people, &tracks, 5, 11).unwrap();
        assert_valid(&schedule, &people, &tracks);
        assert_eq!(schedule.assignments.len(), 15);
    }

    #[test]
    fn zero_rounds_returns_an_empty_schedule() {
        let people = people(4);
        let tracks = tracks(&people);
        let schedule = Schedule::generate(&people, &tracks, 0, 1).unwrap();
        assert!(schedule.assignments.is_empty());
        assert_eq!(schedule.rounds, 0);
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn generated_schedules_are_valid(
            person_count in 2usize..14,
            requested_rounds in 0u32..20,
            seed in any::<u64>(),
        ) {
            let people = people(person_count);
            let tracks = tracks(&people);
            let rounds = requested_rounds % person_count as u32;
            let schedule = Schedule::generate(&people, &tracks, rounds, seed).unwrap();
            assert_valid(&schedule, &people, &tracks);
        }
    }
}
