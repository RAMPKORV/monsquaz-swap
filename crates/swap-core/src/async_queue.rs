//! Pure async queue selection.
//!
//! Async mode uses one global queue. A participant can receive a track only
//! when they are Ready and hold no active offer. The queue prefers tracks that
//! the participant has not worked on and whose people they have not met.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::ids::{ParticipantId, TrackCode};

/// One eligible track for one participant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsyncCandidate {
    pub track: TrackCode,
    /// Meetings between this participant and the track owner or past holders.
    pub pair_meetings: u32,
}

/// One selected offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AsyncChoice {
    pub participant: ParticipantId,
    pub track: TrackCode,
}

/// Choose one track for each participant from the global queue.
///
/// The function is deterministic for the same input and seed. It schedules
/// participants with fewer candidates first. Each track is used at most once
/// in the wave. The score prefers fewer pair meetings; the tie-break is a
/// stable hash, never a group label.
pub fn choose_async_offers(
    participants: &[ParticipantId],
    candidates: &BTreeMap<ParticipantId, Vec<AsyncCandidate>>,
    seed: u64,
) -> Vec<AsyncChoice> {
    let mut ordered: Vec<(ParticipantId, Vec<AsyncCandidate>)> = participants
        .iter()
        .filter_map(|participant| {
            let mut options = candidates.get(participant)?.clone();
            options.sort_by(|left, right| {
                (
                    left.pair_meetings,
                    tie_break(seed, &participant.0, &left.track.0),
                )
                    .cmp(&(
                        right.pair_meetings,
                        tie_break(seed, &participant.0, &right.track.0),
                    ))
            });
            Some((participant.clone(), options))
        })
        .collect();
    ordered.sort_by(|left, right| {
        left.1
            .len()
            .cmp(&right.1.len())
            .then_with(|| left.0.0.cmp(&right.0.0))
    });
    let options: Vec<Vec<AsyncCandidate>> =
        ordered.iter().map(|(_, options)| options.clone()).collect();
    let mut track_owner: HashMap<String, usize> = HashMap::new();
    for person in 0..ordered.len() {
        let mut visited = HashSet::new();
        assign_person(person, &options, &mut visited, &mut track_owner);
    }
    let mut choices: Vec<AsyncChoice> = track_owner
        .into_iter()
        .map(|(track, person)| AsyncChoice {
            participant: ordered[person].0.clone(),
            track: TrackCode::new(track),
        })
        .collect();
    choices.sort_by(|left, right| left.participant.0.cmp(&right.participant.0));
    choices
}

fn assign_person(
    person: usize,
    options: &[Vec<AsyncCandidate>],
    visited: &mut HashSet<String>,
    track_owner: &mut HashMap<String, usize>,
) -> bool {
    for candidate in &options[person] {
        if !visited.insert(candidate.track.0.clone()) {
            continue;
        }
        if let Some(other) = track_owner.get(&candidate.track.0).copied() {
            if assign_person(other, options, visited, track_owner) {
                track_owner.insert(candidate.track.0.clone(), person);
                return true;
            }
        } else {
            track_owner.insert(candidate.track.0.clone(), person);
            return true;
        }
    }
    false
}

fn tie_break(seed: u64, participant: &str, track: &str) -> u64 {
    splitmix(seed ^ stable_hash(participant) ^ stable_hash(track).rotate_left(17))
}

fn splitmix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 0xCBF2_9CE4_8422_2325;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn person(index: usize) -> ParticipantId {
        ParticipantId::new(format!("P{index:02}"))
    }

    fn candidate(track: &str, pair_meetings: u32) -> AsyncCandidate {
        AsyncCandidate {
            track: TrackCode::new(track),
            pair_meetings,
        }
    }

    // scenario: The global queue prefers social diversity
    #[test]
    fn the_queue_prefers_an_unfamiliar_track() {
        let people = vec![person(0)];
        let mut candidates = BTreeMap::new();
        candidates.insert(
            people[0].clone(),
            vec![candidate("FAMILIAR", 3), candidate("FRESH", 0)],
        );
        let choices = choose_async_offers(&people, &candidates, 7);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].track.0, "FRESH");
    }

    #[test]
    fn one_track_is_used_at_most_once() {
        let people = vec![person(0), person(1)];
        let mut candidates = BTreeMap::new();
        candidates.insert(people[0].clone(), vec![candidate("ONLY", 0)]);
        candidates.insert(people[1].clone(), vec![candidate("ONLY", 0)]);
        let choices = choose_async_offers(&people, &candidates, 7);
        assert_eq!(choices.len(), 1);
    }

    #[test]
    fn the_constrained_participant_goes_first() {
        let people = vec![person(0), person(1)];
        let mut candidates = BTreeMap::new();
        candidates.insert(
            people[0].clone(),
            vec![candidate("A", 0), candidate("B", 0)],
        );
        candidates.insert(people[1].clone(), vec![candidate("A", 0)]);
        let choices = choose_async_offers(&people, &candidates, 7);
        assert_eq!(choices.len(), 2);
        assert!(choices.iter().any(|choice| choice.participant == people[0]));
        assert!(choices.iter().any(|choice| choice.participant == people[1]));
        assert_ne!(choices[0].track, choices[1].track);
    }

    #[test]
    fn the_choice_is_deterministic() {
        let people = vec![person(0), person(1), person(2)];
        let mut candidates = BTreeMap::new();
        for person in &people {
            candidates.insert(
                person.clone(),
                vec![candidate("A", 1), candidate("B", 0), candidate("C", 0)],
            );
        }
        let first = choose_async_offers(&people, &candidates, 99);
        let second = choose_async_offers(&people, &candidates, 99);
        assert_eq!(first, second);
    }

    #[test]
    fn the_tie_break_depends_on_seed_participant_and_track() {
        let mut values = std::collections::HashSet::new();
        for seed in 0..4 {
            for participant in 0..4 {
                for track in 0..4 {
                    values.insert(tie_break(
                        seed,
                        &format!("P{participant:02}"),
                        &format!("T{track:02}"),
                    ));
                }
            }
        }
        assert_eq!(values.len(), 64, "tie-break collisions");
    }

    #[test]
    fn a_derangement_still_matches_everyone() {
        let people = (0..4).map(person).collect::<Vec<_>>();
        let mut candidates = BTreeMap::new();
        for (index, person) in people.iter().enumerate() {
            let options = (0..4)
                .filter(|track| *track != index)
                .map(|track| candidate(&format!("T{track}"), 0))
                .collect();
            candidates.insert(person.clone(), options);
        }
        let choices = choose_async_offers(&people, &candidates, 7);
        assert_eq!(choices.len(), 4);
        let tracks: std::collections::HashSet<&str> = choices
            .iter()
            .map(|choice| choice.track.0.as_str())
            .collect();
        assert_eq!(tracks.len(), 4);
    }
}
