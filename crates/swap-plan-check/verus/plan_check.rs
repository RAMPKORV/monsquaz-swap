//! Verified twin of the plan checker and the Hall-certificate verifier.
//!
//! Verify with the pinned Verus toolchain:
//!
//! ```bash
//! /tmp/opencode/verus/verus --no-cheating --time --compile plan_check.rs
//! ```
//!
//! The file is not part of the Cargo build. The corpus binary at the bottom is
//! ordinary Rust and is not verified. The functions inside the `verus!` block
//! are verified.

use vstd::prelude::*;
use vstd::array::*;
use vstd::seq_lib::*;
use vstd::set_lib::*;

verus! {

pub const MAX_PEOPLE: usize = 16;
pub const MAX_TRACKS: usize = 16;
pub const MAX_ROUNDS: usize = 16;
pub const MAX_ASSIGNMENTS: usize = 16;
pub const NO_OWNER: u16 = 65535u16;

#[derive(Clone, Copy)]
pub struct Assignment {
    pub participant: u16,
    pub track: u16,
}

#[derive(Clone, Copy)]
pub struct Round {
    pub assignments: [Assignment; MAX_ASSIGNMENTS],
    pub len: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Strict,
    Flexible,
}

#[derive(Clone)]
pub struct PlanInput {
    pub active_people: [u16; MAX_PEOPLE],
    pub active_people_len: usize,
    pub active_tracks: [u16; MAX_TRACKS],
    pub active_tracks_len: usize,
    pub owners: [u16; MAX_TRACKS],
    pub policy: Policy,
    pub rounds: Vec<Round>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlanReject {
    Capacity,
    ParticipantOutOfRange { participant: u16 },
    TrackOutOfRange { track: u16 },
    DuplicateActiveParticipant { participant: u16 },
    DuplicateActiveTrack { track: u16 },
    UnknownParticipant { round: usize, participant: u16 },
    UnknownTrack { round: usize, track: u16 },
    MissingOwner { track: u16 },
    OwnTrack { round: usize, participant: u16, track: u16 },
    RepeatedPair { round: usize, participant: u16, track: u16 },
    DuplicateParticipantInRound { round: usize, participant: u16 },
    DuplicateTrackInRound { round: usize, track: u16 },
    MissingParticipant { round: usize, participant: u16 },
    MissingTrack { round: usize, track: u16 },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct HallCertificate {
    pub round: usize,
    pub tracks: [u16; MAX_TRACKS],
    pub tracks_len: usize,
    pub neighbors: [u16; MAX_PEOPLE],
    pub neighbors_len: usize,
}

// ---------------------------------------------------------------------------
// Specification
// ---------------------------------------------------------------------------

pub open spec fn people_prefix(input: &PlanInput) -> Seq<u16> {
    input.active_people@.take(input.active_people_len as int)
}

pub open spec fn tracks_prefix(input: &PlanInput) -> Seq<u16> {
    input.active_tracks@.take(input.active_tracks_len as int)
}

pub open spec fn cert_tracks_prefix(cert: &HallCertificate) -> Seq<u16> {
    cert.tracks@.take(cert.tracks_len as int)
}

pub open spec fn cert_neighbors_prefix(cert: &HallCertificate) -> Seq<u16> {
    cert.neighbors@.take(cert.neighbors_len as int)
}

pub open spec fn people_set(input: &PlanInput) -> Set<u16> {
    people_prefix(input).to_set()
}

pub open spec fn tracks_set(input: &PlanInput) -> Set<u16> {
    tracks_prefix(input).to_set()
}

pub open spec fn cert_tracks_set(cert: &HallCertificate) -> Set<u16> {
    cert_tracks_prefix(cert).to_set()
}

pub open spec fn cert_neighbors_set(cert: &HallCertificate) -> Set<u16> {
    cert_neighbors_prefix(cert).to_set()
}

pub open spec fn capacity_ok(input: &PlanInput) -> bool {
    &&& input.active_people_len <= MAX_PEOPLE
    &&& input.active_tracks_len <= MAX_TRACKS
    &&& input.rounds.len() <= MAX_ROUNDS
    &&& forall|r: int| 0 <= r < input.rounds@.len() ==> input.rounds@[r].len <= MAX_ASSIGNMENTS
}

pub open spec fn people_in_range(input: &PlanInput) -> bool {
    forall|i: int|
        0 <= i < input.active_people_len as int && i < MAX_PEOPLE ==> (
        input.active_people@[i] as int) < MAX_PEOPLE
}

pub open spec fn tracks_in_range(input: &PlanInput) -> bool {
    forall|i: int|
        0 <= i < input.active_tracks_len as int && i < MAX_TRACKS ==> (
        input.active_tracks@[i] as int) < MAX_TRACKS
}

pub open spec fn people_distinct(input: &PlanInput) -> bool {
    people_set(input).len() == input.active_people_len as int
}

pub open spec fn tracks_distinct(input: &PlanInput) -> bool {
    tracks_set(input).len() == input.active_tracks_len as int
}

pub open spec fn person_in_set(input: &PlanInput, p: u16) -> bool {
    people_set(input).contains(p)
}

pub open spec fn track_in_set(input: &PlanInput, t: u16) -> bool {
    tracks_set(input).contains(t)
}

pub open spec fn owner_of(input: &PlanInput, t: u16) -> u16
    recommends
        (t as int) < MAX_TRACKS,
{
    input.owners@[t as int]
}

pub open spec fn all_tracks_owned(input: &PlanInput) -> bool {
    forall|i: int|
        0 <= i < input.active_tracks_len as int && i < MAX_TRACKS
            ==> owner_of(input, input.active_tracks@[i]) != NO_OWNER
}

pub open spec fn round_count(input: &PlanInput) -> int {
    input.rounds@.len() as int
}

pub open spec fn round_len(input: &PlanInput, r: int) -> int {
    input.rounds@[r].len as int
}

pub open spec fn in_plan_rounds(input: &PlanInput, r: int) -> bool {
    0 <= r < round_count(input)
}

pub open spec fn assignment_at(input: &PlanInput, r: int, i: int) -> Assignment
    recommends
        0 <= r < round_count(input),
        0 <= i < MAX_ASSIGNMENTS,
{
    input.rounds@[r].assignments@[i]
}

pub open spec fn seen_before(
    input: &PlanInput,
    round: int,
    participant: u16,
    track: u16,
) -> bool {
    exists|r: int, i: int| {
        &&& 0 <= r < round
        &&& r < MAX_ROUNDS
        &&& 0 <= i < round_len(input, r)
        &&& i < MAX_ASSIGNMENTS
        &&& assignment_at(input, r, i).participant == participant
        &&& assignment_at(input, r, i).track == track
    }
}

pub open spec fn round_value_has_person(round: Round, p: u16) -> bool {
    exists|i: int| {
        &&& 0 <= i < round.len as int
        &&& i < MAX_ASSIGNMENTS
        &&& round.assignments@[i].participant == p
    }
}

pub open spec fn round_value_has_track(round: Round, t: u16) -> bool {
    exists|i: int| {
        &&& 0 <= i < round.len as int
        &&& i < MAX_ASSIGNMENTS
        &&& round.assignments@[i].track == t
    }
}

pub open spec fn round_has_person(input: &PlanInput, r: int, p: u16) -> bool {
    round_value_has_person(input.rounds@[r], p)
}

pub open spec fn round_has_track(input: &PlanInput, r: int, t: u16) -> bool {
    round_value_has_track(input.rounds@[r], t)
}

pub open spec fn round_people_distinct(input: &PlanInput, r: int) -> bool {
    Seq::new(
        round_len(input, r) as nat,
        |i: int| assignment_at(input, r, i).participant,
    ).to_set().len() == round_len(input, r)
}

pub open spec fn round_tracks_distinct(input: &PlanInput, r: int) -> bool {
    Seq::new(
        round_len(input, r) as nat,
        |i: int| assignment_at(input, r, i).track,
    ).to_set().len() == round_len(input, r)
}

pub open spec fn assignment_ok(input: &PlanInput, r: int, i: int) -> bool {
    let a = assignment_at(input, r, i);
    &&& person_in_set(input, a.participant)
    &&& track_in_set(input, a.track)
    &&& (a.track as int) < MAX_TRACKS
    &&& owner_of(input, a.track) != NO_OWNER
    &&& owner_of(input, a.track) != a.participant
    &&& !seen_before(input, r, a.participant, a.track)
}

pub open spec fn round_ok(input: &PlanInput, r: int) -> bool {
    &&& forall|i: int|
        0 <= i < round_len(input, r) && i < MAX_ASSIGNMENTS ==> assignment_ok(input, r, i)
    &&& round_people_distinct(input, r)
    &&& round_tracks_distinct(input, r)
    &&& match input.policy {
        Policy::Strict => strict_round_complete(input, r),
        Policy::Flexible => true,
    }
}

pub open spec fn plan_ok(input: &PlanInput) -> bool {
    &&& capacity_ok(input)
    &&& people_in_range(input)
    &&& tracks_in_range(input)
    &&& people_distinct(input)
    &&& tracks_distinct(input)
    &&& all_tracks_owned(input)
    &&& forall|r: int| #[trigger] in_plan_rounds(input, r) ==> round_ok(input, r)
}

/// No participant holds the same track in two different positions.
pub open spec fn no_repeated_pair(input: &PlanInput) -> bool {
    forall|r1: int, i1: int, r2: int, i2: int|
        0 <= r1 < round_count(input) && r1 < MAX_ROUNDS && 0 <= i1 < round_len(input, r1)
            && i1 < MAX_ASSIGNMENTS && 0 <= r2 < round_count(input) && r2 < MAX_ROUNDS && 0
            <= i2 < round_len(input, r2) && i2 < MAX_ASSIGNMENTS && (r1 < r2 || (r1 == r2 && i1
            < i2)) ==> !(assignment_at(input, r1, i1).participant == assignment_at(
            input,
            r2,
            i2,
        ).participant && assignment_at(input, r1, i1).track == assignment_at(input, r2, i2).track)
}

/// No participant holds their own track.
pub open spec fn no_own_track(input: &PlanInput) -> bool {
    forall|r: int, i: int|
        0 <= r < round_count(input) && r < MAX_ROUNDS && 0 <= i < round_len(input, r)
            && i < MAX_ASSIGNMENTS
            ==> owner_of(input, assignment_at(input, r, i).track) != assignment_at(
            input,
            r,
            i,
        ).participant
}

pub open spec fn strict_round_complete(input: &PlanInput, r: int) -> bool {
    (forall|p: u16| person_in_set(input, p) ==> #[trigger] round_has_person(input, r, p))
        && (forall|t: u16| track_in_set(input, t) ==> #[trigger] round_has_track(input, r, t))
}

/// A strict plan covers every active person and every active track in every
/// round.
pub open spec fn strict_complete(input: &PlanInput) -> bool {
    input.policy == Policy::Strict ==> forall|r: int|
        #[trigger] in_plan_rounds(input, r) ==> strict_round_complete(input, r)
}

pub open spec fn eligible_at(
    input: &PlanInput,
    round: int,
    participant: u16,
    track: u16,
) -> bool {
    &&& person_in_set(input, participant)
    &&& track_in_set(input, track)
    &&& (track as int) < MAX_TRACKS
    &&& owner_of(input, track) != NO_OWNER
    &&& owner_of(input, track) != participant
    &&& !seen_before(input, round, participant, track)
}

pub open spec fn cert_capacity_ok(input: &PlanInput, cert: &HallCertificate) -> bool {
    &&& capacity_ok(input)
    &&& cert.tracks_len <= MAX_TRACKS
    &&& cert.neighbors_len <= MAX_PEOPLE
    &&& cert.round <= input.rounds.len()
}

pub open spec fn cert_tracks_distinct(cert: &HallCertificate) -> bool {
    cert_tracks_set(cert).len() == cert.tracks_len as int
}

pub open spec fn cert_neighbors_distinct(cert: &HallCertificate) -> bool {
    cert_neighbors_set(cert).len() == cert.neighbors_len as int
}

pub open spec fn cert_tracks_active(input: &PlanInput, cert: &HallCertificate) -> bool {
    forall|i: int| {
        0 <= i < cert.tracks_len as int && i < MAX_TRACKS ==> (cert.tracks@[i] as int)
            < MAX_TRACKS && track_in_set(input, cert.tracks@[i])
    }
}

pub open spec fn cert_neighbors_active(input: &PlanInput, cert: &HallCertificate) -> bool {
    forall|i: int| {
        0 <= i < cert.neighbors_len as int && i < MAX_PEOPLE ==> person_in_set(
            input,
            cert.neighbors@[i],
        ) && (cert.neighbors@[i] as int) < MAX_PEOPLE
    }
}

pub open spec fn cert_completeness(input: &PlanInput, cert: &HallCertificate) -> bool {
    forall|ti: int, p: u16| {
        &&& 0 <= ti < cert.tracks_len as int
        &&& ti < MAX_TRACKS
        &&& person_in_set(input, p)
        &&& p != owner_of(input, cert.tracks@[ti])
        &&& !seen_before(input, cert.round as int, p, cert.tracks@[ti])
            ==> cert_neighbors_set(cert).contains(p)
    }
}

pub open spec fn cert_ok(input: &PlanInput, cert: &HallCertificate) -> bool {
    &&& cert_capacity_ok(input, cert)
    &&& people_in_range(input)
    &&& tracks_in_range(input)
    &&& cert.tracks_len > 0
    &&& cert_tracks_distinct(cert)
    &&& cert_tracks_active(input, cert)
    &&& cert_neighbors_distinct(cert)
    &&& cert_neighbors_active(input, cert)
    &&& cert_completeness(input, cert)
    &&& cert.neighbors_len < cert.tracks_len
}

// ---------------------------------------------------------------------------
// Soundness theorems
// ---------------------------------------------------------------------------

proof fn lemma_prefix_member<T>(s: Seq<T>, n: int, i: int, x: T)
    requires
        0 <= i < n <= s.len(),
        s[i] == x,
    ensures
        s.take(n).to_set().contains(x),
{
    assert(s.take(n)[i] == x);
    assert(s.take(n).contains(x));
    s.take(n).to_set_ensures();
}

proof fn lemma_capacity_round(input: &PlanInput, r: int)
    requires
        capacity_ok(input),
        0 <= r < round_count(input),
    ensures
        input.rounds@[r].len <= MAX_ASSIGNMENTS,
{
    assert(capacity_ok(input));
}

proof fn lemma_person_set_witness(input: &PlanInput, p: u16)
    requires
        capacity_ok(input),
        person_in_set(input, p),
    ensures
        exists|j: int|
            0 <= j < input.active_people_len as int && j < MAX_PEOPLE
                && input.active_people@[j] == p,
{
    people_prefix(input).to_set_ensures();
    assert(people_prefix(input).contains(p));
}

proof fn lemma_track_set_witness(input: &PlanInput, t: u16)
    requires
        capacity_ok(input),
        track_in_set(input, t),
    ensures
        exists|j: int|
            0 <= j < input.active_tracks_len as int && j < MAX_TRACKS
                && input.active_tracks@[j] == t,
{
    tracks_prefix(input).to_set_ensures();
    assert(tracks_prefix(input).contains(t));
}

proof fn lemma_prefix_no_duplicates<T>(s: Seq<T>, n: int)
    requires
        n <= s.len(),
        s.take(n).to_set().len() == n,
    ensures
        s.take(n).no_duplicates(),
{
    s.take(n).lemma_no_dup_set_cardinality();
}

proof fn lemma_round_people_pairwise(input: &PlanInput, r: int)
    requires
        0 <= r < MAX_ROUNDS,
        round_people_distinct(input, r),
    ensures
        forall|i: int, j: int|
            0 <= i < round_len(input, r) && i < MAX_ASSIGNMENTS && 0 <= j < round_len(input, r)
                && j < MAX_ASSIGNMENTS && i != j ==> assignment_at(
                input,
                r,
                i,
            ).participant != assignment_at(input, r, j).participant,
{
    let n = round_len(input, r);
    let s = Seq::new(n as nat, |i: int| assignment_at(input, r, i).participant);
    assert(s.take(n) =~= s);
    assert(round_people_distinct(input, r));
    lemma_prefix_no_duplicates(s, n);
    assert(s.take(n).no_duplicates());
    assert forall|i: int, j: int|
        0 <= i < n && i < MAX_ASSIGNMENTS && 0 <= j < n && j < MAX_ASSIGNMENTS && i != j
            implies assignment_at(input, r, i).participant != assignment_at(
            input,
            r,
            j,
        ).participant by {
        assert(s[i] == assignment_at(input, r, i).participant);
        assert(s[j] == assignment_at(input, r, j).participant);
        assert(s[i] != s[j]);
    }
}

pub proof fn theorem_check_plan_sound(input: &PlanInput)
    requires
        plan_ok(input),
    ensures
        no_repeated_pair(input),
        no_own_track(input),
        strict_complete(input),
{
    assert forall|r1: int, i1: int, r2: int, i2: int|
        0 <= r1 < round_count(input) && r1 < MAX_ROUNDS && 0 <= i1 < round_len(input, r1)
            && i1 < MAX_ASSIGNMENTS && 0 <= r2 < round_count(input) && r2 < MAX_ROUNDS && 0
            <= i2 < round_len(input, r2) && i2 < MAX_ASSIGNMENTS && (r1 < r2 || (r1 == r2 && i1
            < i2)) implies !(assignment_at(input, r1, i1).participant == assignment_at(
            input,
            r2,
            i2,
        ).participant && assignment_at(input, r1, i1).track == assignment_at(input, r2, i2).track)
    by {
        if r1 == r2 {
            // Same round: distinct participants exclude an equal pair.
            assert(in_plan_rounds(input, r1));
            assert(round_ok(input, r1));
            lemma_round_people_pairwise(input, r1);
        } else {
            // Earlier round: the later assignment must not have seen the pair.
            if assignment_at(input, r1, i1).participant == assignment_at(
                input,
                r2,
                i2,
            ).participant && assignment_at(input, r1, i1).track == assignment_at(
                input,
                r2,
                i2,
            ).track {
                let p = assignment_at(input, r1, i1).participant;
                let t = assignment_at(input, r1, i1).track;
                assert(seen_before(input, r2, p, t)) by {
                    assert(assignment_at(input, r1, i1).participant == p);
                    assert(assignment_at(input, r1, i1).track == t);
                }
                assert(in_plan_rounds(input, r2));
                assert(round_ok(input, r2));
                assert(assignment_ok(input, r2, i2));
                assert(false);
            }
        }
    }
    assert forall|r: int, i: int|
        0 <= r < round_count(input) && r < MAX_ROUNDS && 0 <= i < round_len(input, r) && i
            < MAX_ASSIGNMENTS implies owner_of(input, assignment_at(input, r, i).track)
            != assignment_at(input, r, i).participant by {
        assert(in_plan_rounds(input, r));
        assert(round_ok(input, r));
        assert(assignment_ok(input, r, i));
    }
    assert(no_repeated_pair(input));
    assert(no_own_track(input));
    assert(strict_complete(input));
}

proof fn lemma_certificate_counting(
    input: &PlanInput,
    cert: &HallCertificate,
    holder: spec_fn(u16) -> u16,
)
    requires
        cert_ok(input, cert),
        forall|t: u16|
            cert_tracks_set(cert).contains(t) ==> eligible_at(
                input,
                cert.round as int,
                holder(t),
                t,
            ),
        cert_tracks_set(cert).injective_on(holder),
    ensures
        false,
{
    let t_set = cert_tracks_set(cert);
    let n_set = cert_neighbors_set(cert);
    assert(t_set.len() == cert.tracks_len as int);
    assert(n_set.len() == cert.neighbors_len as int);
    assert(t_set.map(holder).subset_of(n_set)) by {
        assert forall|p: u16| t_set.map(holder).contains(p) implies n_set.contains(p) by {
            let t = choose|t: u16| t_set.contains(t) && holder(t) == p;
            assert(eligible_at(input, cert.round as int, holder(t), t));
            assert(cert_completeness(input, cert));
            assert(n_set.contains(holder(t)));
        }
    }
    lemma_len_subset(t_set.map(holder), n_set);
    lemma_map_size(t_set, t_set.map(holder), holder);
    assert(t_set.len() <= n_set.len());
    assert(n_set.len() < t_set.len());
}

// ---------------------------------------------------------------------------
// Executable refinement: GAP
// ---------------------------------------------------------------------------
//
// The executable checker is not in this file. Verus 0.2026.11.22.1ce6a59
// does not relate a copied `Round`/`Assignment` value read from an array or a
// vector to its specification view. The historical seen scan therefore cannot
// prove `assignment_at(input, r, i) == local_copy` for nested rounds. The
// specification and the soundness theorems below are verified. The flat-plan
// checker is the follow-up target. See README.md.

} // verus!

fn main() {}
