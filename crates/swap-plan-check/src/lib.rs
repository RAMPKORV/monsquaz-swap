// The checker mirrors the index-based verified model on purpose.
#![allow(clippy::needless_range_loop)]

//! Pure plan checker and Hall-certificate verifier for the classic swap
//! planner.
//!
//! This crate has no database, no I/O, no async runtime, and no Discord or
//! Tauri types. It is a dev-only proof oracle. No product crate imports it.
//!
//! The model is deliberately bounded:
//!
//! - at most [`MAX_PEOPLE`] active participants;
//! - at most [`MAX_TRACKS`] active tracks;
//! - at most [`MAX_ROUNDS`] rounds;
//! - at most [`MAX_ASSIGNMENTS`] assignments in one round.
//!
//! The bounds cover the classic event limit of 12 participants. A larger event
//! needs a new proof and a new bound.
//!
//! Track codes are dense integers. `owners[track]` is the owner of the track,
//! or [`NO_OWNER`] when the track has no owner. The classic planner uses one
//! track per participant; this model also accepts a track set that is larger
//! than the roster so that it can express infeasible plans.

pub mod corpus;

/// Maximum number of active participants in the pilot model.
pub const MAX_PEOPLE: usize = 16;
/// Maximum number of active tracks in the pilot model.
pub const MAX_TRACKS: usize = 16;
/// Maximum number of rounds in the pilot model.
pub const MAX_ROUNDS: usize = 16;
/// Maximum number of assignments in one round.
pub const MAX_ASSIGNMENTS: usize = 16;
/// Sentinel owner for a track that is not in the event.
pub const NO_OWNER: u16 = u16::MAX;

/// One track handoff in one round.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Assignment {
    pub participant: u16,
    pub track: u16,
}

impl Assignment {
    /// Create an assignment.
    pub const fn new(participant: u16, track: u16) -> Self {
        Self { participant, track }
    }
}

/// One round of a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Round {
    pub assignments: [Assignment; MAX_ASSIGNMENTS],
    pub len: usize,
}

impl Default for Round {
    fn default() -> Self {
        Self {
            assignments: [Assignment::default(); MAX_ASSIGNMENTS],
            len: 0,
        }
    }
}

/// The scheduling policy for a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Policy {
    /// Every round must cover every active participant and every active track.
    Strict,
    /// A round can hold a subset. The host records the gap.
    Flexible,
}

/// The plan input. The arrays are fixed size. The `_len` fields select the
/// used prefix. A length larger than the array capacity is a typed reject.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanInput {
    pub active_people: [u16; MAX_PEOPLE],
    pub active_people_len: usize,
    pub active_tracks: [u16; MAX_TRACKS],
    pub active_tracks_len: usize,
    pub owners: [u16; MAX_TRACKS],
    pub policy: Policy,
    pub rounds: [Round; MAX_ROUNDS],
    pub rounds_len: usize,
}

impl Default for PlanInput {
    fn default() -> Self {
        Self {
            active_people: [0; MAX_PEOPLE],
            active_people_len: 0,
            active_tracks: [0; MAX_TRACKS],
            active_tracks_len: 0,
            owners: [NO_OWNER; MAX_TRACKS],
            policy: Policy::Strict,
            rounds: [Round::default(); MAX_ROUNDS],
            rounds_len: 0,
        }
    }
}

/// A typed plan reject reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanReject {
    /// A length field is larger than the fixed capacity.
    Capacity,
    /// An active participant id is outside the dense id range.
    ParticipantOutOfRange { participant: u16 },
    /// An active track id is outside the dense id range.
    TrackOutOfRange { track: u16 },
    /// The active roster contains the same participant twice.
    DuplicateActiveParticipant { participant: u16 },
    /// The active track set contains the same track twice.
    DuplicateActiveTrack { track: u16 },
    /// An assignment names a participant that is not active.
    UnknownParticipant { round: usize, participant: u16 },
    /// An assignment names a track that is not active.
    UnknownTrack { round: usize, track: u16 },
    /// An active track has no owner.
    MissingOwner { track: u16 },
    /// A participant holds the track that they seeded.
    OwnTrack {
        round: usize,
        participant: u16,
        track: u16,
    },
    /// A participant holds the same track that they held in an earlier round.
    RepeatedPair {
        round: usize,
        participant: u16,
        track: u16,
    },
    /// A round assigns the same participant twice.
    DuplicateParticipantInRound { round: usize, participant: u16 },
    /// A round assigns the same track twice.
    DuplicateTrackInRound { round: usize, track: u16 },
    /// A strict round does not cover an active participant.
    MissingParticipant { round: usize, participant: u16 },
    /// A strict round does not cover an active track.
    MissingTrack { round: usize, track: u16 },
}

impl PlanReject {
    /// Stable numeric code for corpus comparison and audit records.
    pub const fn code(self) -> u8 {
        match self {
            Self::Capacity => 1,
            Self::ParticipantOutOfRange { .. } => 13,
            Self::TrackOutOfRange { .. } => 14,
            Self::DuplicateActiveParticipant { .. } => 2,
            Self::DuplicateActiveTrack { .. } => 3,
            Self::UnknownParticipant { .. } => 4,
            Self::UnknownTrack { .. } => 5,
            Self::MissingOwner { .. } => 6,
            Self::OwnTrack { .. } => 7,
            Self::RepeatedPair { .. } => 8,
            Self::DuplicateParticipantInRound { .. } => 9,
            Self::DuplicateTrackInRound { .. } => 10,
            Self::MissingParticipant { .. } => 11,
            Self::MissingTrack { .. } => 12,
        }
    }
}

/// A Hall-violator certificate.
///
/// `tracks` is a non-empty subset of active tracks. `neighbors` is a superset
/// of the eligible holders of those tracks. The certificate proves that no
/// injective assignment exists for `tracks` when `neighbors.len()` is smaller
/// than `tracks.len()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HallCertificate {
    /// The round that the certificate examines. The seen history is the set of
    /// pairs in rounds `0..round`. The round can equal `rounds_len` for the
    /// next round.
    pub round: usize,
    pub tracks: [u16; MAX_TRACKS],
    pub tracks_len: usize,
    pub neighbors: [u16; MAX_PEOPLE],
    pub neighbors_len: usize,
}

impl Default for HallCertificate {
    fn default() -> Self {
        Self {
            round: 0,
            tracks: [0; MAX_TRACKS],
            tracks_len: 0,
            neighbors: [0; MAX_PEOPLE],
            neighbors_len: 0,
        }
    }
}

/// Check a plan. The function returns `Ok(())` only when every rule holds.
///
/// The check order is stable and shared with the verified twin. A caller can
/// use [`PlanReject::code`] to compare verdicts.
pub fn check_plan(input: &PlanInput) -> Result<(), PlanReject> {
    if !capacity_ok(input) {
        return Err(PlanReject::Capacity);
    }
    for &participant in &input.active_people[..input.active_people_len] {
        if participant as usize >= MAX_PEOPLE {
            return Err(PlanReject::ParticipantOutOfRange { participant });
        }
    }
    for &track in &input.active_tracks[..input.active_tracks_len] {
        if track as usize >= MAX_TRACKS {
            return Err(PlanReject::TrackOutOfRange { track });
        }
    }

    if let Some(participant) = duplicate_active_person(input) {
        return Err(PlanReject::DuplicateActiveParticipant { participant });
    }
    if let Some(track) = duplicate_active_track(input) {
        return Err(PlanReject::DuplicateActiveTrack { track });
    }
    for &track in &input.active_tracks[..input.active_tracks_len] {
        if track as usize >= MAX_TRACKS || input.owners[track as usize] == NO_OWNER {
            return Err(PlanReject::MissingOwner { track });
        }
    }

    for round_index in 0..input.rounds_len {
        let round = &input.rounds[round_index];
        for assignment_index in 0..round.len {
            let assignment = round.assignments[assignment_index];
            if !contains(
                &input.active_people[..input.active_people_len],
                assignment.participant,
            ) {
                return Err(PlanReject::UnknownParticipant {
                    round: round_index,
                    participant: assignment.participant,
                });
            }
            if !contains(
                &input.active_tracks[..input.active_tracks_len],
                assignment.track,
            ) {
                return Err(PlanReject::UnknownTrack {
                    round: round_index,
                    track: assignment.track,
                });
            }
            if assignment.track as usize >= MAX_TRACKS
                || input.owners[assignment.track as usize] == NO_OWNER
            {
                return Err(PlanReject::MissingOwner {
                    track: assignment.track,
                });
            }
            if input.owners[assignment.track as usize] == assignment.participant {
                return Err(PlanReject::OwnTrack {
                    round: round_index,
                    participant: assignment.participant,
                    track: assignment.track,
                });
            }
            if seen_in_previous_round(input, round_index, assignment) {
                return Err(PlanReject::RepeatedPair {
                    round: round_index,
                    participant: assignment.participant,
                    track: assignment.track,
                });
            }
        }

        if let Some(participant) = duplicate_participant_in_round(round) {
            return Err(PlanReject::DuplicateParticipantInRound {
                round: round_index,
                participant,
            });
        }
        if let Some(track) = duplicate_track_in_round(round) {
            return Err(PlanReject::DuplicateTrackInRound {
                round: round_index,
                track,
            });
        }

        if input.policy == Policy::Strict {
            for &participant in &input.active_people[..input.active_people_len] {
                if !round_has_participant(round, participant) {
                    return Err(PlanReject::MissingParticipant {
                        round: round_index,
                        participant,
                    });
                }
            }
            for &track in &input.active_tracks[..input.active_tracks_len] {
                if !round_has_track(round, track) {
                    return Err(PlanReject::MissingTrack {
                        round: round_index,
                        track,
                    });
                }
            }
        }
    }

    Ok(())
}

/// Verify a Hall-violator certificate.
///
/// The function returns `true` only when all of these rules hold:
///
/// 1. every length is inside the fixed capacity;
/// 2. the round is not after the next round;
/// 3. the track subset is non-empty and duplicate free;
/// 4. every certificate track is active;
/// 5. the neighbor set is duplicate free;
/// 6. every neighbor is an active participant;
/// 7. every eligible holder of every certificate track is in the neighbor set;
/// 8. the neighbor set is smaller than the track subset.
///
/// Rule 7 makes the neighbor set a superset of the true neighborhood. A
/// superset is safe: if the superset is smaller than the track subset, the true
/// neighborhood is smaller too.
pub fn verify_hall_certificate(input: &PlanInput, cert: &HallCertificate) -> bool {
    if !capacity_ok(input)
        || cert.tracks_len > MAX_TRACKS
        || cert.neighbors_len > MAX_PEOPLE
        || cert.round > input.rounds_len
    {
        return false;
    }
    if input.active_people[..input.active_people_len]
        .iter()
        .any(|participant| *participant as usize >= MAX_PEOPLE)
        || input.active_tracks[..input.active_tracks_len]
            .iter()
            .any(|track| *track as usize >= MAX_TRACKS)
    {
        return false;
    }
    if cert.tracks_len == 0 {
        return false;
    }
    if !all_distinct(&cert.tracks[..cert.tracks_len]) {
        return false;
    }
    for &track in &cert.tracks[..cert.tracks_len] {
        if track as usize >= MAX_TRACKS
            || !contains(&input.active_tracks[..input.active_tracks_len], track)
        {
            return false;
        }
    }
    if !all_distinct(&cert.neighbors[..cert.neighbors_len]) {
        return false;
    }
    for &participant in &cert.neighbors[..cert.neighbors_len] {
        if !contains(&input.active_people[..input.active_people_len], participant) {
            return false;
        }
    }
    for &track in &cert.tracks[..cert.tracks_len] {
        for &participant in &input.active_people[..input.active_people_len] {
            let owner = input.owners[track as usize];
            if participant != owner
                && !seen_in_previous_rounds(input, cert.round, participant, track)
                && !contains(&cert.neighbors[..cert.neighbors_len], participant)
            {
                return false;
            }
        }
    }
    cert.neighbors_len < cert.tracks_len
}

fn capacity_ok(input: &PlanInput) -> bool {
    input.active_people_len <= MAX_PEOPLE
        && input.active_tracks_len <= MAX_TRACKS
        && input.rounds_len <= MAX_ROUNDS
        && input
            .rounds
            .iter()
            .all(|round| round.len <= MAX_ASSIGNMENTS)
}

fn duplicate_active_person(input: &PlanInput) -> Option<u16> {
    let people = &input.active_people[..input.active_people_len];
    for (index, &participant) in people.iter().enumerate() {
        if people[..index].contains(&participant) {
            return Some(participant);
        }
    }
    None
}

fn duplicate_active_track(input: &PlanInput) -> Option<u16> {
    let tracks = &input.active_tracks[..input.active_tracks_len];
    for (index, &track) in tracks.iter().enumerate() {
        if tracks[..index].contains(&track) {
            return Some(track);
        }
    }
    None
}

fn duplicate_participant_in_round(round: &Round) -> Option<u16> {
    let assignments = &round.assignments[..round.len];
    for (index, assignment) in assignments.iter().enumerate() {
        if assignments[..index]
            .iter()
            .any(|other| other.participant == assignment.participant)
        {
            return Some(assignment.participant);
        }
    }
    None
}

fn duplicate_track_in_round(round: &Round) -> Option<u16> {
    let assignments = &round.assignments[..round.len];
    for (index, assignment) in assignments.iter().enumerate() {
        if assignments[..index]
            .iter()
            .any(|other| other.track == assignment.track)
        {
            return Some(assignment.track);
        }
    }
    None
}

fn round_has_participant(round: &Round, participant: u16) -> bool {
    round.assignments[..round.len]
        .iter()
        .any(|assignment| assignment.participant == participant)
}

fn round_has_track(round: &Round, track: u16) -> bool {
    round.assignments[..round.len]
        .iter()
        .any(|assignment| assignment.track == track)
}

fn seen_in_previous_round(input: &PlanInput, round_index: usize, assignment: Assignment) -> bool {
    seen_in_previous_rounds(input, round_index, assignment.participant, assignment.track)
}

fn seen_in_previous_rounds(
    input: &PlanInput,
    round_index: usize,
    participant: u16,
    track: u16,
) -> bool {
    input.rounds[..round_index].iter().any(|round| {
        round.assignments[..round.len]
            .iter()
            .any(|assignment| assignment.participant == participant && assignment.track == track)
    })
}

fn contains(values: &[u16], value: u16) -> bool {
    values.contains(&value)
}

fn all_distinct(values: &[u16]) -> bool {
    for (index, &value) in values.iter().enumerate() {
        if values[..index].contains(&value) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cyclic_plan(people: usize, policy: Policy) -> PlanInput {
        assert!((2..=MAX_PEOPLE).contains(&people));
        let mut input = PlanInput {
            active_people_len: people,
            active_tracks_len: people,
            policy,
            rounds_len: people - 1,
            ..PlanInput::default()
        };
        for person in 0..people {
            input.active_people[person] = person as u16;
            input.active_tracks[person] = person as u16;
            input.owners[person] = person as u16;
        }
        for round in 0..people - 1 {
            input.rounds[round].len = people;
            for person in 0..people {
                input.rounds[round].assignments[person] =
                    Assignment::new(person as u16, ((person + round + 1) % people) as u16);
            }
        }
        input
    }

    #[test]
    fn strict_cyclic_plan_is_ok() {
        assert_eq!(check_plan(&cyclic_plan(6, Policy::Strict)), Ok(()));
    }

    #[test]
    fn flexible_gap_plan_is_ok() {
        let mut input = cyclic_plan(6, Policy::Flexible);
        input.rounds[0].len = 3;
        input.rounds[1].len = 0;
        assert_eq!(check_plan(&input), Ok(()));
    }

    #[test]
    fn repeated_pair_is_rejected() {
        let mut input = cyclic_plan(4, Policy::Strict);
        input.rounds[2].assignments[0] = input.rounds[0].assignments[0];
        assert_eq!(
            check_plan(&input),
            Err(PlanReject::RepeatedPair {
                round: 2,
                participant: 0,
                track: 1,
            })
        );
    }

    #[test]
    fn own_track_is_rejected() {
        let mut input = cyclic_plan(4, Policy::Strict);
        input.rounds[0].assignments[0] = Assignment::new(0, 0);
        assert_eq!(
            check_plan(&input),
            Err(PlanReject::OwnTrack {
                round: 0,
                participant: 0,
                track: 0,
            })
        );
    }

    #[test]
    fn missing_track_is_rejected_in_strict_policy() {
        let mut input = cyclic_plan(4, Policy::Strict);
        input.active_tracks[4] = 4;
        input.active_tracks_len = 5;
        input.owners[4] = 2;
        assert_eq!(
            check_plan(&input),
            Err(PlanReject::MissingTrack { round: 0, track: 4 })
        );
    }

    #[test]
    fn missing_assignment_is_allowed_in_flexible_policy() {
        let mut input = cyclic_plan(4, Policy::Flexible);
        input.rounds[0].len = 2;
        assert_eq!(check_plan(&input), Ok(()));
    }

    #[test]
    fn duplicate_person_in_round_is_rejected() {
        let mut input = cyclic_plan(4, Policy::Strict);
        input.rounds[0].assignments[1].participant = 0;
        assert_eq!(
            check_plan(&input),
            Err(PlanReject::DuplicateParticipantInRound {
                round: 0,
                participant: 0,
            })
        );
    }

    #[test]
    fn duplicate_track_in_round_is_rejected() {
        let mut input = cyclic_plan(4, Policy::Strict);
        input.rounds[0].assignments[2].track = 1;
        assert_eq!(
            check_plan(&input),
            Err(PlanReject::DuplicateTrackInRound { round: 0, track: 1 })
        );
    }

    #[test]
    fn empty_strict_round_is_rejected() {
        let mut input = cyclic_plan(4, Policy::Strict);
        input.rounds[1].len = 0;
        assert_eq!(
            check_plan(&input),
            Err(PlanReject::MissingParticipant {
                round: 1,
                participant: 0,
            })
        );
    }

    #[test]
    fn hall_certificate_accepts_small_neighborhood() {
        let input = two_people_three_tracks();
        let cert = HallCertificate {
            round: 0,
            tracks: [0, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            tracks_len: 3,
            neighbors: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            neighbors_len: 2,
        };
        assert!(verify_hall_certificate(&input, &cert));
    }

    #[test]
    fn hall_certificate_rejects_an_omitted_neighbor() {
        let input = two_people_three_tracks();
        let cert = HallCertificate {
            round: 0,
            tracks: [0, 1, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            tracks_len: 3,
            neighbors: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            neighbors_len: 1,
        };
        assert!(!verify_hall_certificate(&input, &cert));
    }

    #[test]
    fn hall_certificate_rejects_equal_sizes() {
        let input = two_people_three_tracks();
        let cert = HallCertificate {
            round: 0,
            tracks: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            tracks_len: 2,
            neighbors: [0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            neighbors_len: 2,
        };
        assert!(!verify_hall_certificate(&input, &cert));
    }

    #[test]
    fn hall_certificate_accepts_empty_neighborhood_after_full_exposure() {
        let input = cyclic_plan(4, Policy::Strict);
        let cert = HallCertificate {
            round: 3,
            tracks: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            tracks_len: 1,
            neighbors: [0; MAX_PEOPLE],
            neighbors_len: 0,
        };
        assert!(verify_hall_certificate(&input, &cert));
    }

    fn two_people_three_tracks() -> PlanInput {
        let mut input = PlanInput {
            active_people_len: 2,
            active_tracks_len: 3,
            ..PlanInput::default()
        };
        input.active_people[0] = 0;
        input.active_people[1] = 1;
        for track in 0..3 {
            input.active_tracks[track] = track as u16;
        }
        input.owners[0] = 0;
        input.owners[1] = 1;
        input.owners[2] = 0;
        input
    }
}
