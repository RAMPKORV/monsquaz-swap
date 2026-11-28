//! Deterministic conformance corpus for the Rust checker and the verified
//! twin.
//!
//! The generator is test code. It must not be part of the trusted base of the
//! proof. The verified twin reads the encoded corpus and prints its own
//! verdicts. A diff of the two verdict files is the conformance result.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

use crate::{
    Assignment, HallCertificate, MAX_ASSIGNMENTS, MAX_PEOPLE, MAX_ROUNDS, MAX_TRACKS, NO_OWNER,
    PlanInput, PlanReject, Policy, check_plan, verify_hall_certificate,
};

/// One generated plan and the case that it exercises.
#[derive(Clone, Copy, Debug)]
pub struct GeneratedPlan {
    pub id: usize,
    pub kind: CaseKind,
    pub plan: PlanInput,
}

/// One generated certificate and the plan that it refers to.
#[derive(Clone, Copy, Debug)]
pub struct GeneratedCert {
    pub id: usize,
    pub plan_id: usize,
    pub cert: HallCertificate,
}

/// The corpus.
#[derive(Clone, Debug, Default)]
pub struct Corpus {
    pub plans: Vec<GeneratedPlan>,
    pub certs: Vec<GeneratedCert>,
}

/// The plan case kinds. The generator cycles through every kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CaseKind {
    ValidStrict,
    ValidFlexible,
    RepeatedPair,
    OwnTrack,
    MissingTrackStrict,
    MissingPersonStrict,
    DuplicatePersonInRound,
    DuplicateTrackInRound,
    EmptyRoundStrict,
    EmptyRoundFlexible,
    UnknownTrack,
    UnknownParticipant,
    ParticipantOutOfRange,
    TrackOutOfRange,
    MissingOwner,
    DuplicateActivePerson,
    DuplicateActiveTrack,
    HallInfeasible,
    Capacity,
    RoundLenOverflow,
    ActivePeopleOverflow,
    ActiveTracksOverflow,
}

impl CaseKind {
    /// Every kind, in a stable order.
    pub const ALL: [Self; 22] = [
        Self::ValidStrict,
        Self::ValidFlexible,
        Self::RepeatedPair,
        Self::OwnTrack,
        Self::MissingTrackStrict,
        Self::MissingPersonStrict,
        Self::DuplicatePersonInRound,
        Self::DuplicateTrackInRound,
        Self::EmptyRoundStrict,
        Self::EmptyRoundFlexible,
        Self::UnknownTrack,
        Self::UnknownParticipant,
        Self::ParticipantOutOfRange,
        Self::TrackOutOfRange,
        Self::MissingOwner,
        Self::DuplicateActivePerson,
        Self::DuplicateActiveTrack,
        Self::HallInfeasible,
        Self::Capacity,
        Self::RoundLenOverflow,
        Self::ActivePeopleOverflow,
        Self::ActiveTracksOverflow,
    ];

    /// A short label for the report.
    pub const fn label(self) -> &'static str {
        match self {
            Self::ValidStrict => "valid_strict",
            Self::ValidFlexible => "valid_flexible",
            Self::RepeatedPair => "repeated_pair",
            Self::OwnTrack => "own_track",
            Self::MissingTrackStrict => "missing_track_strict",
            Self::MissingPersonStrict => "missing_person_strict",
            Self::DuplicatePersonInRound => "duplicate_person_in_round",
            Self::DuplicateTrackInRound => "duplicate_track_in_round",
            Self::EmptyRoundStrict => "empty_round_strict",
            Self::EmptyRoundFlexible => "empty_round_flexible",
            Self::UnknownTrack => "unknown_track",
            Self::UnknownParticipant => "unknown_participant",
            Self::ParticipantOutOfRange => "participant_out_of_range",
            Self::TrackOutOfRange => "track_out_of_range",
            Self::MissingOwner => "missing_owner",
            Self::DuplicateActivePerson => "duplicate_active_person",
            Self::DuplicateActiveTrack => "duplicate_active_track",
            Self::HallInfeasible => "hall_infeasible",
            Self::Capacity => "capacity",
            Self::RoundLenOverflow => "round_len_overflow",
            Self::ActivePeopleOverflow => "active_people_overflow",
            Self::ActiveTracksOverflow => "active_tracks_overflow",
        }
    }
}

/// A small deterministic generator. Do not use this for production.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    /// Create a generator with a seed.
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Return the next value.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Return a value in `0..bound`. `bound` must be greater than zero.
    pub fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }

    /// Return a boolean.
    pub fn flip(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

/// Generate a corpus with at least `plan_count` plans and `cert_count`
/// certificates.
pub fn generate(plan_count: usize, cert_count: usize) -> Corpus {
    let mut rng = Rng::new(0x5EED_1337_2026_1128);
    let mut plans = Vec::with_capacity(plan_count + 5);
    for index in 0..plan_count {
        let kind = CaseKind::ALL[index % CaseKind::ALL.len()];
        let mut people = 2 + index % 11;
        if matches!(
            kind,
            CaseKind::RepeatedPair | CaseKind::DuplicateTrackInRound
        ) && people < 3
        {
            people = 3;
        }
        let plan = make_plan(kind, people, &mut rng);
        plans.push(GeneratedPlan {
            id: index,
            kind,
            plan,
        });
    }

    let fixture_two_three = plans.len();
    plans.push(GeneratedPlan {
        id: fixture_two_three,
        kind: CaseKind::HallInfeasible,
        plan: fixture_two_people_three_tracks(),
    });
    let fixture_three_four = plans.len();
    plans.push(GeneratedPlan {
        id: fixture_three_four,
        kind: CaseKind::HallInfeasible,
        plan: fixture_three_people_four_tracks(),
    });
    let fixture_full_four = plans.len();
    plans.push(GeneratedPlan {
        id: fixture_full_four,
        kind: CaseKind::ValidStrict,
        plan: cyclic_plan(4, Policy::Strict),
    });
    let mut certs = Vec::with_capacity(cert_count);
    for index in 0..cert_count {
        let (cert, plan_id) = match index % 12 {
            0 => (
                HallCertificate {
                    round: 0,
                    tracks: array_tracks(&[0, 1, 2]),
                    tracks_len: 3,
                    neighbors: array_people(&[0, 1]),
                    neighbors_len: 2,
                },
                fixture_two_three,
            ),
            1 => (
                HallCertificate {
                    round: 0,
                    tracks: array_tracks(&[0, 1, 2, 3]),
                    tracks_len: 4,
                    neighbors: array_people(&[0, 1, 2]),
                    neighbors_len: 3,
                },
                fixture_three_four,
            ),
            2 => (
                HallCertificate {
                    round: 3,
                    tracks: array_tracks(&[0]),
                    tracks_len: 1,
                    neighbors: array_people(&[]),
                    neighbors_len: 0,
                },
                fixture_full_four,
            ),
            3 => (
                HallCertificate {
                    round: 0,
                    tracks: array_tracks(&[0, 1, 2]),
                    tracks_len: 3,
                    neighbors: array_people(&[0]),
                    neighbors_len: 1,
                },
                fixture_two_three,
            ),
            4 => (
                HallCertificate {
                    round: 0,
                    tracks: array_tracks(&[0, 1]),
                    tracks_len: 2,
                    neighbors: array_people(&[0, 1]),
                    neighbors_len: 2,
                },
                fixture_two_three,
            ),
            5 => (
                HallCertificate {
                    round: 1,
                    tracks: array_tracks(&[0]),
                    tracks_len: 1,
                    neighbors: array_people(&[]),
                    neighbors_len: 0,
                },
                fixture_two_three,
            ),
            6 => (
                HallCertificate {
                    round: 0,
                    tracks: array_tracks(&[0, 0]),
                    tracks_len: 2,
                    neighbors: array_people(&[1]),
                    neighbors_len: 1,
                },
                fixture_two_three,
            ),
            7 => (
                HallCertificate {
                    round: 0,
                    tracks: array_tracks(&[0, 1, 2]),
                    tracks_len: 3,
                    neighbors: array_people(&[0, 0]),
                    neighbors_len: 2,
                },
                fixture_two_three,
            ),
            8 => (
                HallCertificate {
                    round: 0,
                    tracks: array_tracks(&[0, 1, 13]),
                    tracks_len: 3,
                    neighbors: array_people(&[0, 1]),
                    neighbors_len: 2,
                },
                fixture_three_four,
            ),
            9 => (
                HallCertificate {
                    round: 0,
                    tracks: array_tracks(&[0, 1, 2]),
                    tracks_len: 3,
                    neighbors: array_people(&[0, 7]),
                    neighbors_len: 2,
                },
                fixture_three_four,
            ),
            10 => (
                HallCertificate {
                    round: 0,
                    tracks: array_tracks(&[0, 1, 2, 3]),
                    tracks_len: 4,
                    neighbors: array_people(&[0, 1]),
                    neighbors_len: 2,
                },
                fixture_three_four,
            ),
            _ => random_certificate(&plans, &mut rng),
        };
        certs.push(GeneratedCert {
            id: index,
            plan_id,
            cert,
        });
    }

    Corpus { plans, certs }
}

fn random_certificate(plans: &[GeneratedPlan], rng: &mut Rng) -> (HallCertificate, usize) {
    let candidates: Vec<&GeneratedPlan> = plans
        .iter()
        .filter(|plan| {
            check_plan(&plan.plan).is_ok()
                && plan.plan.active_tracks_len > 0
                && plan.plan.active_people_len > 0
        })
        .collect();
    let plan = candidates[rng.below(candidates.len())];
    let round = rng.below(plan.plan.rounds_len.min(MAX_ROUNDS) + 1);
    let active_tracks = &plan.plan.active_tracks[..plan.plan.active_tracks_len];
    let subset_len = 1 + rng.below(active_tracks.len().min(4));
    let mut tracks = [0u16; MAX_TRACKS];
    let mut tracks_len = 0;
    let mut chosen = BTreeSet::new();
    let mut attempts = 0usize;
    let attempt_limit = subset_len * active_tracks.len() + MAX_TRACKS;
    while tracks_len < subset_len && attempts < attempt_limit {
        attempts += 1;
        let index = rng.below(active_tracks.len());
        let track = active_tracks[index];
        if track as usize >= MAX_TRACKS {
            continue;
        }
        if chosen.insert(track) {
            tracks[tracks_len] = track;
            tracks_len += 1;
        }
    }
    for &track in active_tracks {
        if tracks_len >= subset_len {
            break;
        }
        if (track as usize) < MAX_TRACKS && chosen.insert(track) {
            tracks[tracks_len] = track;
            tracks_len += 1;
        }
    }

    let mut neighbors = [0u16; MAX_PEOPLE];
    let mut neighbors_len = 0;
    for participant in &plan.plan.active_people[..plan.plan.active_people_len] {
        let eligible = tracks[..tracks_len].iter().any(|&track| {
            plan.plan.owners[track as usize] != *participant
                && !seen_before(&plan.plan, round, *participant, track)
        });
        if eligible && neighbors_len < MAX_PEOPLE {
            neighbors[neighbors_len] = *participant;
            neighbors_len += 1;
        }
    }

    if neighbors_len >= tracks_len && neighbors_len > 0 {
        neighbors_len -= 1;
    }
    (
        HallCertificate {
            round,
            tracks,
            tracks_len,
            neighbors,
            neighbors_len,
        },
        plan.id,
    )
}

fn seen_before(input: &PlanInput, round: usize, participant: u16, track: u16) -> bool {
    input.rounds[..round.min(MAX_ROUNDS)].iter().any(|round| {
        round.assignments[..round.len]
            .iter()
            .any(|assignment| assignment.participant == participant && assignment.track == track)
    })
}

fn make_plan(kind: CaseKind, people: usize, rng: &mut Rng) -> PlanInput {
    let mut plan = match kind {
        CaseKind::ValidFlexible | CaseKind::EmptyRoundFlexible => {
            cyclic_plan(people, Policy::Flexible)
        }
        _ => cyclic_plan(people, Policy::Strict),
    };

    match kind {
        CaseKind::ValidStrict => {}
        CaseKind::ValidFlexible => {
            let drops = 1 + rng.below(people);
            for _ in 0..drops {
                if plan.rounds_len > 0 {
                    let round_index = rng.below(plan.rounds_len);
                    if plan.rounds[round_index].len > 0 {
                        plan.rounds[round_index].len -= 1;
                    }
                }
            }
        }
        CaseKind::RepeatedPair => {
            plan.rounds[1].assignments[1] = plan.rounds[0].assignments[0];
        }
        CaseKind::OwnTrack => {
            plan.rounds[0].assignments[0] = Assignment::new(0, 0);
        }
        CaseKind::MissingTrackStrict | CaseKind::HallInfeasible => {
            let extra = people;
            plan.active_tracks[people] = extra as u16;
            plan.active_tracks_len = people + 1;
            plan.owners[extra] = 0;
        }
        CaseKind::MissingPersonStrict => {
            let extra = people;
            plan.active_people[people] = extra as u16;
            plan.active_people_len = people + 1;
        }
        CaseKind::DuplicatePersonInRound => {
            plan.rounds[0].assignments[1].participant = 0;
        }
        CaseKind::DuplicateTrackInRound => {
            plan.rounds[0].assignments[2].track = plan.rounds[0].assignments[0].track;
        }
        CaseKind::EmptyRoundStrict => {
            plan.rounds[0].len = 0;
        }
        CaseKind::EmptyRoundFlexible => {
            plan.rounds[0].len = 0;
        }
        CaseKind::UnknownTrack => {
            plan.rounds[0].assignments[0].track = 13;
        }
        CaseKind::UnknownParticipant => {
            plan.rounds[0].assignments[0].participant = 13;
        }
        CaseKind::ParticipantOutOfRange => {
            plan.active_people[0] = 20;
        }
        CaseKind::TrackOutOfRange => {
            plan.active_tracks[0] = 20;
        }
        CaseKind::MissingOwner => {
            let extra = (MAX_TRACKS - 1) as u16;
            plan.active_tracks[people] = extra;
            plan.active_tracks_len = people + 1;
            plan.owners[extra as usize] = NO_OWNER;
        }
        CaseKind::DuplicateActivePerson => {
            plan.active_people[people] = plan.active_people[0];
            plan.active_people_len = people + 1;
        }
        CaseKind::DuplicateActiveTrack => {
            plan.active_tracks[people] = plan.active_tracks[0];
            plan.active_tracks_len = people + 1;
        }
        CaseKind::Capacity => {
            plan.rounds_len = MAX_ROUNDS + 1;
        }
        CaseKind::RoundLenOverflow => {
            plan.rounds[0].len = MAX_ASSIGNMENTS + 1;
        }
        CaseKind::ActivePeopleOverflow => {
            plan.active_people_len = MAX_PEOPLE + 1;
        }
        CaseKind::ActiveTracksOverflow => {
            plan.active_tracks_len = MAX_TRACKS + 1;
        }
    }
    plan
}

/// Build a valid classic exchange plan. Round `k` assigns person `p` to track
/// `(p + k + 1) % people`, which is a cyclic Latin square with the own track
/// removed.
pub fn cyclic_plan(people: usize, policy: Policy) -> PlanInput {
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

fn fixture_two_people_three_tracks() -> PlanInput {
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

fn fixture_three_people_four_tracks() -> PlanInput {
    let mut input = PlanInput {
        active_people_len: 3,
        active_tracks_len: 4,
        ..PlanInput::default()
    };
    for person in 0..3 {
        input.active_people[person] = person as u16;
    }
    for track in 0..4 {
        input.active_tracks[track] = track as u16;
    }
    input.owners[0] = 0;
    input.owners[1] = 1;
    input.owners[2] = 0;
    input.owners[3] = 1;
    input
}

fn array_tracks(values: &[u16]) -> [u16; MAX_TRACKS] {
    let mut array = [0u16; MAX_TRACKS];
    array[..values.len()].copy_from_slice(values);
    array
}

fn array_people(values: &[u16]) -> [u16; MAX_PEOPLE] {
    let mut array = [0u16; MAX_PEOPLE];
    array[..values.len()].copy_from_slice(values);
    array
}

/// Write the corpus and the Rust verdicts to disk.
pub fn write_corpus(corpus: &Corpus, corpus_path: &Path, expected_path: &Path) -> io::Result<()> {
    if let Some(parent) = corpus_path.parent() {
        fs::create_dir_all(parent)?;
    }
    if let Some(parent) = expected_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut text = String::new();
    for plan in &corpus.plans {
        let input = &plan.plan;
        write!(
            text,
            "PLAN {} {} {}",
            plan.id,
            policy_code(input.policy),
            input.active_people_len
        )
        .unwrap();
        for value in &input.active_people[..input.active_people_len.min(MAX_PEOPLE)] {
            write!(text, " {value}").unwrap();
        }
        write!(text, " {}", input.active_tracks_len).unwrap();
        for value in &input.active_tracks[..input.active_tracks_len.min(MAX_TRACKS)] {
            write!(text, " {value}").unwrap();
        }
        for value in &input.owners {
            write!(text, " {value}").unwrap();
        }
        write!(text, " {}", input.rounds_len).unwrap();
        for round in &input.rounds[..input.rounds_len.min(MAX_ROUNDS)] {
            write!(text, " {}", round.len).unwrap();
            for assignment in &round.assignments[..round.len.min(MAX_ASSIGNMENTS)] {
                write!(text, " {} {}", assignment.participant, assignment.track).unwrap();
            }
        }
        text.push('\n');
    }

    for cert in &corpus.certs {
        let value = &cert.cert;
        write!(
            text,
            "CERT {} {} {} {}",
            cert.id, cert.plan_id, value.round, value.tracks_len
        )
        .unwrap();
        for track in &value.tracks[..value.tracks_len] {
            write!(text, " {track}").unwrap();
        }
        write!(text, " {}", value.neighbors_len).unwrap();
        for neighbor in &value.neighbors[..value.neighbors_len] {
            write!(text, " {neighbor}").unwrap();
        }
        text.push('\n');
    }

    let mut expected = String::new();
    for plan in &corpus.plans {
        let code = match check_plan(&plan.plan) {
            Ok(()) => 0,
            Err(reject) => reject.code(),
        };
        writeln!(expected, "PLAN {} {}", plan.id, code).unwrap();
    }
    for cert in &corpus.certs {
        let accepted = verify_hall_certificate(&corpus.plans[cert.plan_id].plan, &cert.cert);
        writeln!(expected, "CERT {} {}", cert.id, u8::from(accepted)).unwrap();
    }

    fs::write(corpus_path, text)?;
    fs::write(expected_path, expected)?;
    Ok(())
}

/// Decode a corpus line length in the verified twin. This format is shared.
pub const fn policy_code(policy: Policy) -> u8 {
    match policy {
        Policy::Strict => 0,
        Policy::Flexible => 1,
    }
}

/// The number of plans and certificates in a generated corpus.
pub fn summary(corpus: &Corpus) -> String {
    let mut counts = std::collections::BTreeMap::new();
    for plan in &corpus.plans {
        *counts.entry(plan.kind).or_insert(0usize) += 1;
    }
    let mut lines = String::new();
    for kind in CaseKind::ALL {
        writeln!(
            lines,
            "{}: {}",
            kind.label(),
            counts.get(&kind).copied().unwrap_or(0)
        )
        .unwrap();
    }
    writeln!(lines, "plans: {}", corpus.plans.len()).unwrap();
    writeln!(lines, "certs: {}", corpus.certs.len()).unwrap();
    lines
}

/// Count the distinct reject codes that the Rust checker returns.
pub fn reject_codes(corpus: &Corpus) -> BTreeSet<u8> {
    corpus
        .plans
        .iter()
        .filter_map(|plan| check_plan(&plan.plan).err().map(PlanReject::code))
        .collect()
}
