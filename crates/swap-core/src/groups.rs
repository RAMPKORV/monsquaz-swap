//! Classic group shape, lookup, cadence, and soft-bound warnings.
//!
//! These helpers are pure. The engine and the host use the same rules so the
//! planner preview and the generated plan agree.

use serde::{Deserialize, Serialize};

use std::collections::{HashMap, HashSet};

use thiserror::Error;

use crate::ids::ParticipantId;
use crate::schedule::{ScheduledAssignment, TrackSpec, match_round};

pub const MIN_GROUP_SIZE: u32 = 5;
pub const MAX_GROUP_SIZE: u32 = 12;
pub const MAX_GROUP_COUNT: u32 = 4;
pub const MIN_PHASES: u32 = 3;
pub const MAX_PHASES: u32 = 5;

/// A suggested group count and the balanced size for that count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupShape {
    pub count: u32,
    pub size: u32,
}

/// Return the lookup suggestion for a roster size.
///
/// The ranges follow `docs/EVENT_SETTINGS.md`:
/// 1 group up to 9 people, 2 for 10–13, 3 for 14–18 and 23–32, and 4 for
/// 19–22 and 33–40. Larger rosters use 4 groups.
pub fn suggest_groups(participants: usize) -> GroupShape {
    let count = match participants {
        0..=9 => 1,
        10..=13 => 2,
        14..=18 => 3,
        19..=22 => 4,
        23..=32 => 3,
        33..=40 => 4,
        _ => 4,
    };
    let size = participants.div_ceil(count);
    GroupShape {
        count: count as u32,
        size: size as u32,
    }
}

/// Split a roster into group sizes.
///
/// A positive `size` is a maximum group size. This can produce unequal groups,
/// for example 12 and 5. A zero `size` asks for balanced groups.
pub fn group_sizes(participants: usize, count: u32, size: u32) -> Vec<u32> {
    if participants == 0 {
        return Vec::new();
    }
    if size > 0 {
        let mut remaining = participants;
        let mut sizes = Vec::new();
        while remaining > 0 && sizes.len() < MAX_GROUP_COUNT as usize {
            let next = remaining.min(size as usize);
            sizes.push(next as u32);
            remaining -= next;
        }
        if remaining > 0 {
            // The configured shape cannot hold the roster. Put the overflow in
            // the last group so no participant disappears. The warning path
            // reports the large group.
            if let Some(last) = sizes.last_mut() {
                *last += remaining as u32;
            }
        }
        return sizes;
    }
    let count = count.clamp(1, MAX_GROUP_COUNT) as usize;
    let balanced = participants.div_ceil(count);
    let mut sizes = Vec::new();
    let mut remaining = participants;
    while remaining > 0 {
        let next = remaining.min(balanced);
        sizes.push(next as u32);
        remaining -= next;
    }
    sizes
}

/// Return the default K-round cadence for a classic event.
///
/// K is about T/4. The result keeps the phase count between 3 and 5 when the
/// event is long enough. The result is at least 1.
pub fn default_cadence(rounds: u32) -> u32 {
    if rounds <= 1 {
        return 1;
    }
    let mut cadence = (rounds + 2) / 4;
    cadence = cadence.max(1).min(rounds);
    if rounds.div_ceil(cadence) > MAX_PHASES {
        cadence = rounds.div_ceil(MAX_PHASES).max(1);
    }
    if rounds.div_ceil(cadence) < MIN_PHASES {
        cadence = 1;
    }
    cadence
}

/// Return the number of phases for a cadence.
pub fn phase_count(rounds: u32, cadence: u32) -> u32 {
    if rounds == 0 || cadence == 0 {
        return 0;
    }
    rounds.div_ceil(cadence)
}

/// A soft-bound or cadence warning. These never block a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupWarning {
    Small { size: u32 },
    Large { size: u32 },
    Unequal { smallest: u32, largest: u32 },
    Cadence { phases: u32 },
    Exhausted { size: u32, rounds: u32 },
}

/// Return the soft warnings for a group shape.
///
/// `cadence` is the raw event setting. The exhausted warning only applies when
/// the host selected that policy.
pub fn group_warnings(sizes: &[u32], rounds: u32, cadence: &str) -> Vec<GroupWarning> {
    let mut warnings = Vec::new();
    if sizes.is_empty() {
        return warnings;
    }
    let smallest = *sizes.iter().min().unwrap_or(&0);
    let largest = *sizes.iter().max().unwrap_or(&0);
    if smallest < MIN_GROUP_SIZE {
        warnings.push(GroupWarning::Small { size: smallest });
    }
    if largest > MAX_GROUP_SIZE {
        warnings.push(GroupWarning::Large { size: largest });
    }
    if largest.saturating_sub(smallest) > 1 {
        warnings.push(GroupWarning::Unequal { smallest, largest });
    }
    let cadence_rounds = default_cadence(rounds);
    let phases = phase_count(rounds, cadence_rounds);
    if rounds > 0 && !(MIN_PHASES..=MAX_PHASES).contains(&phases) {
        warnings.push(GroupWarning::Cadence { phases });
    }
    if cadence.eq_ignore_ascii_case("exhausted") && largest > rounds.saturating_add(1) {
        warnings.push(GroupWarning::Exhausted {
            size: largest,
            rounds,
        });
    }
    warnings
}

/// One phase of a group schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupPhase {
    pub index: u32,
    pub start_round: u32,
    pub rounds: u32,
    /// True when the phase used the global pool because a group could not fill.
    pub global_pool: bool,
}

/// A complete classic group schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupSchedule {
    pub assignments: Vec<ScheduledAssignment>,
    pub phases: Vec<GroupPhase>,
    pub cadence: u32,
    pub group_sizes: Vec<u32>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GroupScheduleError {
    #[error("the group schedule needs at least two groups")]
    TooFewGroups,
    #[error("the group sizes do not cover the roster")]
    GroupSizes,
    #[error("a group has a different number of people and tracks")]
    GroupShape,
    #[error("no matching exists in phase {phase}, round {round}")]
    Infeasible { phase: u32, round: u32 },
}

/// Generate a group-rotating classic schedule.
///
/// Participants are split into base groups in the given order. Tracks stay in
/// the group of their owner. At each phase, participants rotate forward by one
/// group and the group matches its K rounds. A matching never uses an own
/// track or a track that the person has seen. When a group cannot fill a phase,
/// the phase uses the global pool. The result is deterministic for the same
/// input and seed.
pub fn group_schedule(
    participants: &[ParticipantId],
    tracks: &[TrackSpec],
    group_sizes: &[u32],
    rounds: u32,
    cadence: u32,
    seed: u64,
) -> Result<GroupSchedule, GroupScheduleError> {
    if group_sizes.len() < 2 {
        return Err(GroupScheduleError::TooFewGroups);
    }
    if participants.is_empty()
        || participants.len() != tracks.len()
        || group_sizes.iter().map(|size| *size as usize).sum::<usize>() != participants.len()
    {
        return Err(GroupScheduleError::GroupSizes);
    }
    let cadence = cadence.max(1).min(rounds.max(1));

    let mut base_groups: Vec<Vec<ParticipantId>> = Vec::with_capacity(group_sizes.len());
    let mut group_of_person: HashMap<ParticipantId, usize> = HashMap::new();
    let mut start = 0;
    for (group, size) in group_sizes.iter().enumerate() {
        let end = start + *size as usize;
        let members = participants[start..end].to_vec();
        for person in &members {
            group_of_person.insert(person.clone(), group);
        }
        base_groups.push(members);
        start = end;
    }

    let mut group_tracks: Vec<Vec<TrackSpec>> = vec![Vec::new(); group_sizes.len()];
    for track in tracks {
        let group = group_of_person
            .get(&track.owner)
            .copied()
            .ok_or(GroupScheduleError::GroupSizes)?;
        group_tracks[group].push(track.clone());
    }
    for (group, members) in base_groups.iter().enumerate() {
        if group_tracks[group].len() != members.len() {
            return Err(GroupScheduleError::GroupShape);
        }
        group_tracks[group].sort_by(|left, right| left.code.0.cmp(&right.code.0));
    }

    let person_index: HashMap<ParticipantId, usize> = participants
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, person)| (person, index))
        .collect();
    let track_index: HashMap<String, usize> = tracks
        .iter()
        .enumerate()
        .map(|(index, track)| (track.code.0.clone(), index))
        .collect();
    let mut seen = vec![vec![false; tracks.len()]; participants.len()];
    let mut assignments = Vec::with_capacity(rounds as usize * participants.len());
    let mut phases = Vec::new();
    let phase_count = rounds.div_ceil(cadence);
    let group_count = group_sizes.len();

    for phase in 0..phase_count {
        let start_round = phase * cadence;
        let phase_rounds = cadence.min(rounds - start_round);
        let mut phase_assignments = Vec::new();
        let mut phase_seen = seen.clone();
        let mut local_ok = true;

        for group in 0..group_count {
            let source = (group + group_count - (phase as usize % group_count)) % group_count;
            let people = &base_groups[source];
            let group_tracks = &group_tracks[group];
            for round in 0..phase_rounds {
                let mut forbidden = HashSet::new();
                for (local_person, person) in people.iter().enumerate() {
                    let global_person = person_index[person];
                    for (local_track, track) in group_tracks.iter().enumerate() {
                        let global_track = track_index[&track.code.0];
                        if track.owner == *person || phase_seen[global_person][global_track] {
                            forbidden.insert((local_person, local_track));
                        }
                    }
                }
                let match_seed = seed
                    ^ (phase as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    ^ ((group as u64) << 32)
                    ^ round as u64;
                let Some(matched) =
                    match_round(people.len(), group_tracks.len(), &forbidden, match_seed)
                else {
                    local_ok = false;
                    break;
                };
                for (local_track, local_person) in matched.iter().enumerate() {
                    let person = &people[*local_person];
                    let track = &group_tracks[local_track];
                    phase_assignments.push(ScheduledAssignment {
                        round: start_round + round,
                        track: track.code.clone(),
                        participant: person.clone(),
                    });
                    phase_seen[person_index[person]][track_index[&track.code.0]] = true;
                }
            }
            if !local_ok {
                break;
            }
        }

        if local_ok {
            assignments.extend(phase_assignments);
            seen = phase_seen;
            phases.push(GroupPhase {
                index: phase,
                start_round,
                rounds: phase_rounds,
                global_pool: false,
            });
            continue;
        }

        // A group could not fill its phase. Use the global pool for the whole
        // phase, preserve the seen ledger, and restore groups next phase.
        phase_assignments.clear();
        phase_seen = seen.clone();
        for round in 0..phase_rounds {
            let mut forbidden = HashSet::new();
            for (person_index, person) in participants.iter().enumerate() {
                for (track_index, track) in tracks.iter().enumerate() {
                    if track.owner == *person || phase_seen[person_index][track_index] {
                        forbidden.insert((person_index, track_index));
                    }
                }
            }
            let match_seed = seed
                ^ 0xC0FF_EE00_1234_5678
                ^ (phase as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ round as u64;
            let Some(matched) =
                match_round(participants.len(), tracks.len(), &forbidden, match_seed)
            else {
                return Err(GroupScheduleError::Infeasible { phase, round });
            };
            for (track_index, person_index) in matched.iter().enumerate() {
                phase_assignments.push(ScheduledAssignment {
                    round: start_round + round,
                    track: tracks[track_index].code.clone(),
                    participant: participants[*person_index].clone(),
                });
                phase_seen[*person_index][track_index] = true;
            }
        }
        assignments.extend(phase_assignments);
        seen = phase_seen;
        phases.push(GroupPhase {
            index: phase,
            start_round,
            rounds: phase_rounds,
            global_pool: true,
        });
    }

    assignments.sort_by(|left, right| {
        left.round
            .cmp(&right.round)
            .then_with(|| left.track.0.cmp(&right.track.0))
            .then_with(|| left.participant.0.cmp(&right.participant.0))
    });
    Ok(GroupSchedule {
        assignments,
        phases,
        cadence,
        group_sizes: group_sizes.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lookup_matches_the_setting_table() {
        assert_eq!(suggest_groups(9), GroupShape { count: 1, size: 9 });
        assert_eq!(suggest_groups(10), GroupShape { count: 2, size: 5 });
        assert_eq!(suggest_groups(18), GroupShape { count: 3, size: 6 });
        assert_eq!(suggest_groups(20), GroupShape { count: 4, size: 5 });
        assert_eq!(suggest_groups(23), GroupShape { count: 3, size: 8 });
        assert_eq!(suggest_groups(40), GroupShape { count: 4, size: 10 });
        assert_eq!(suggest_groups(41), GroupShape { count: 4, size: 11 });
    }

    #[test]
    fn manual_sizes_can_be_unequal() {
        assert_eq!(group_sizes(17, 2, 12), vec![12, 5]);
        assert_eq!(group_sizes(17, 2, 0), vec![9, 8]);
        assert_eq!(group_sizes(13, 4, 0), vec![4, 4, 4, 1]);
    }

    #[test]
    fn the_default_cadence_targets_three_to_five_phases() {
        assert_eq!(default_cadence(8), 2);
        assert_eq!(phase_count(8, 2), 4);
        assert_eq!(default_cadence(4), 1);
        assert_eq!(phase_count(4, 1), 4);
        assert_eq!(default_cadence(20), 5);
        assert_eq!(phase_count(20, 5), 4);
        assert_eq!(default_cadence(3), 1);
        assert_eq!(phase_count(3, 1), 3);
    }

    #[test]
    fn soft_warnings_do_not_block() {
        let warnings = group_warnings(&[3, 3], 8, "K rounds");
        assert!(warnings.contains(&GroupWarning::Small { size: 3 }));
        assert!(!warnings.contains(&GroupWarning::Cadence { phases: 4 }));
        let warnings = group_warnings(&[12, 5], 8, "K rounds");
        assert!(warnings.contains(&GroupWarning::Unequal {
            smallest: 5,
            largest: 12
        }));
        assert!(!warnings.contains(&GroupWarning::Large { size: 12 }));
    }

    #[test]
    fn the_exhausted_policy_warns_only_when_it_cannot_fire() {
        assert!(
            group_warnings(&[10], 8, "Exhausted").contains(&GroupWarning::Exhausted {
                size: 10,
                rounds: 8
            })
        );
        assert!(
            !group_warnings(&[8], 8, "Exhausted")
                .contains(&GroupWarning::Exhausted { size: 8, rounds: 8 })
        );
    }

    fn roster(count: usize) -> (Vec<ParticipantId>, Vec<TrackSpec>) {
        let people = (0..count)
            .map(|index| ParticipantId::new(format!("P{index:02}")))
            .collect::<Vec<_>>();
        let tracks = (0..count)
            .map(|index| TrackSpec {
                code: crate::ids::TrackCode::new(format!("T{index:02}")),
                owner: ParticipantId::new(format!("P{index:02}")),
            })
            .collect::<Vec<_>>();
        (people, tracks)
    }

    #[test]
    fn group_rotation_keeps_tracks_and_avoids_repeats() {
        let (people, tracks) = roster(10);
        let schedule = group_schedule(&people, &tracks, &[5, 5], 8, 2, 42).unwrap();
        assert_eq!(schedule.phases.len(), 4);
        assert!(schedule.phases.iter().all(|phase| !phase.global_pool));
        assert_eq!(schedule.assignments.len(), 80);

        let mut seen = std::collections::HashSet::new();
        for assignment in &schedule.assignments {
            assert_ne!(
                assignment.track.0,
                assignment.participant.0.replace('P', "T")
            );
            assert!(
                seen.insert((assignment.participant.0.clone(), assignment.track.0.clone())),
                "repeat for {} on {}",
                assignment.participant.0,
                assignment.track.0
            );
            let phase = (assignment.round / 2) as usize;
            let track_group = assignment.track.0[1..].parse::<usize>().unwrap() / 5;
            let person_group = assignment.participant.0[1..].parse::<usize>().unwrap() / 5;
            let expected = (track_group + 2 - (phase % 2)) % 2;
            assert_eq!(person_group, expected, "round {}", assignment.round);
        }
        for round in 0..8 {
            let people_in_round = schedule
                .assignments
                .iter()
                .filter(|assignment| assignment.round == round)
                .map(|assignment| assignment.participant.0.as_str())
                .collect::<std::collections::HashSet<_>>();
            let tracks_in_round = schedule
                .assignments
                .iter()
                .filter(|assignment| assignment.round == round)
                .map(|assignment| assignment.track.0.as_str())
                .collect::<std::collections::HashSet<_>>();
            assert_eq!(people_in_round.len(), 10);
            assert_eq!(tracks_in_round.len(), 10);
        }
    }

    #[test]
    fn an_extra_round_falls_back_to_the_global_pool() {
        let (people, tracks) = roster(10);
        let schedule = group_schedule(&people, &tracks, &[5, 5], 9, 2, 42).unwrap();
        assert_eq!(schedule.phases.len(), 5);
        assert!(schedule.phases.last().unwrap().global_pool);
        assert_eq!(schedule.assignments.len(), 90);
        let mut seen = std::collections::HashSet::new();
        for assignment in &schedule.assignments {
            assert!(seen.insert((assignment.participant.0.clone(), assignment.track.0.clone())));
        }
    }

    #[test]
    fn the_group_schedule_is_deterministic() {
        let (people, tracks) = roster(10);
        let first = group_schedule(&people, &tracks, &[5, 5], 8, 2, 42).unwrap();
        let second = group_schedule(&people, &tracks, &[5, 5], 8, 2, 42).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn group_sizes_must_cover_the_roster() {
        let (people, tracks) = roster(10);
        assert_eq!(
            group_schedule(&people, &tracks, &[5, 4], 8, 2, 42),
            Err(GroupScheduleError::GroupSizes)
        );
        assert_eq!(
            group_schedule(&people, &tracks, &[5], 8, 2, 42),
            Err(GroupScheduleError::TooFewGroups)
        );
    }
}
