//! Verified flat-plan core for the classic swap planner.
//!
//! The plan is flat:
//!
//! - `participants[i]` and `tracks[i]` are the assignment at flat position `i`;
//! - `round_starts[r]` and `round_lens[r]` select the slice for round `r`.
//!
//! No nested copied struct is used. This avoids the Verus limitation that
//! blocked the round-based executable twin.
//!
//! Verify:
//!
//! ```bash
//! /tmp/opencode/verus/verus --no-cheating --time flat_plan_check.rs
//! ```
//!
//! Compile the conformance binary:
//!
//! ```bash
//! /tmp/opencode/verus/verus --compile flat_plan_check.rs
//! ```

use vstd::array::*;
use vstd::prelude::*;
use vstd::seq_lib::*;
use vstd::set::*;
use vstd::set_lib::*;

verus! {

pub const MAX_PEOPLE: usize = 16;
pub const MAX_TRACKS: usize = 16;
pub const MAX_ROUNDS: usize = 16;
pub const MAX_ASSIGNMENTS: usize = 16;
pub const MAX_TOTAL: usize = 256;
pub const NO_OWNER: u16 = 65535u16;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Strict,
    Flexible,
}

#[derive(Clone, Copy)]
pub struct FlatPlan {
    pub active_people: [u16; MAX_PEOPLE],
    pub active_people_len: usize,
    pub active_tracks: [u16; MAX_TRACKS],
    pub active_tracks_len: usize,
    pub owners: [u16; MAX_TRACKS],
    pub round_starts: [usize; MAX_ROUNDS],
    pub round_lens: [usize; MAX_ROUNDS],
    pub rounds_len: usize,
    pub participants: [u16; MAX_TOTAL],
    pub tracks: [u16; MAX_TOTAL],
    pub assignments_len: usize,
    pub policy: Policy,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FlatReject {
    Capacity,
    ParticipantOutOfRange { participant: u16 },
    TrackOutOfRange { track: u16 },
    DuplicateActiveParticipant { participant: u16 },
    DuplicateActiveTrack { track: u16 },
    UnknownParticipant { position: usize, participant: u16 },
    UnknownTrack { position: usize, track: u16 },
    MissingOwner { track: u16 },
    OwnTrack { position: usize, participant: u16, track: u16 },
    RepeatedPair { position: usize, participant: u16, track: u16 },
    DuplicateParticipantInRound { round: usize, participant: u16 },
    DuplicateTrackInRound { round: usize, track: u16 },
    MissingParticipant { round: usize, participant: u16 },
    MissingTrack { round: usize, track: u16 },
}

#[derive(Clone, Copy)]
pub struct FlatCertificate {
    pub round: usize,
    pub tracks: [u16; MAX_TRACKS],
    pub tracks_len: usize,
    pub neighbors: [u16; MAX_PEOPLE],
    pub neighbors_len: usize,
}

// ---------------------------------------------------------------------------
// Specification
// ---------------------------------------------------------------------------

pub open spec fn people_prefix(input: &FlatPlan) -> Seq<u16> {
    input.active_people@.take(input.active_people_len as int)
}

pub open spec fn tracks_prefix(input: &FlatPlan) -> Seq<u16> {
    input.active_tracks@.take(input.active_tracks_len as int)
}

pub open spec fn people_set(input: &FlatPlan) -> Set<u16> {
    people_prefix(input).to_set()
}

pub open spec fn tracks_set(input: &FlatPlan) -> Set<u16> {
    tracks_prefix(input).to_set()
}

pub open spec fn below(i: int, limit: int) -> bool {
    0 <= i < limit
}

pub open spec fn round_below(r: int, limit: int) -> bool {
    0 <= r < limit
}

pub open spec fn round_index(input: &FlatPlan, r: int) -> bool {
    0 <= r < input.rounds_len as int
}

pub open spec fn capacity_ok(input: &FlatPlan) -> bool {
    &&& input.active_people_len <= MAX_PEOPLE
    &&& input.active_tracks_len <= MAX_TRACKS
    &&& input.rounds_len <= MAX_ROUNDS
    &&& input.assignments_len <= MAX_TOTAL
    &&& forall|r: int| #[trigger] round_index(input, r) ==> round_start(input, r) + round_len(
        input,
        r,
    ) <= input.assignments_len as int
    &&& forall|r: int| #[trigger] round_index(input, r) ==> round_start(input, r) < MAX_TOTAL
    &&& forall|r: int| #[trigger] round_index(input, r) ==> round_len(input, r) <= MAX_ASSIGNMENTS
    &&& (input.rounds_len == 0 ==> input.assignments_len == 0)
    &&& (input.rounds_len > 0 ==> round_start(input, 0) == 0)
    &&& forall|r: int| #[trigger] round_index(input, r) && r > 0 ==> round_start(input, r)
        == round_start(input, r - 1) + round_len(input, r - 1)
    &&& (input.rounds_len > 0 ==> round_start(input, input.rounds_len as int - 1) + round_len(
        input,
        input.rounds_len as int - 1,
    ) == input.assignments_len as int)
}

pub open spec fn people_in_range(input: &FlatPlan) -> bool {
    forall|i: int|
        0 <= i < input.active_people_len as int && i < MAX_PEOPLE ==> (
        input.active_people@[i] as int) < MAX_PEOPLE
}

pub open spec fn tracks_in_range(input: &FlatPlan) -> bool {
    forall|i: int|
        0 <= i < input.active_tracks_len as int && i < MAX_TRACKS ==> (
        input.active_tracks@[i] as int) < MAX_TRACKS
}

pub open spec fn people_distinct(input: &FlatPlan) -> bool {
    people_set(input).len() == input.active_people_len as int
}

pub open spec fn tracks_distinct(input: &FlatPlan) -> bool {
    tracks_set(input).len() == input.active_tracks_len as int
}

pub open spec fn person_in_set(input: &FlatPlan, p: u16) -> bool {
    people_set(input).contains(p)
}

pub open spec fn track_in_set(input: &FlatPlan, t: u16) -> bool {
    tracks_set(input).contains(t)
}

pub open spec fn owner_of(input: &FlatPlan, t: u16) -> u16
    recommends
        (t as int) < MAX_TRACKS,
{
    input.owners@[t as int]
}

pub open spec fn all_tracks_owned(input: &FlatPlan) -> bool {
    forall|i: int|
        0 <= i < input.active_tracks_len as int && i < MAX_TRACKS
            ==> owner_of(input, input.active_tracks@[i]) != NO_OWNER
}

pub open spec fn participant_at(input: &FlatPlan, pos: int) -> u16
    recommends
        0 <= pos < input.assignments_len as int && pos < MAX_TOTAL,
{
    input.participants@[pos]
}

pub open spec fn track_at(input: &FlatPlan, pos: int) -> u16
    recommends
        0 <= pos < input.assignments_len as int && pos < MAX_TOTAL,
{
    input.tracks@[pos]
}

pub open spec fn seen_prefix(input: &FlatPlan, pos: int) -> Set<(u16, u16)>
    decreases pos,
{
    if pos <= 0 {
        Set::empty()
    } else {
        seen_prefix(input, pos - 1).insert((participant_at(input, pos - 1), track_at(input, pos - 1)))
    }
}

pub open spec fn seen_at(input: &FlatPlan, pos: int, participant: u16, track: u16) -> bool {
    seen_prefix(input, pos).contains((participant, track))
}

pub open spec fn round_start(input: &FlatPlan, r: int) -> int
    recommends
        0 <= r <= input.rounds_len as int,
{
    if r < input.rounds_len as int {
        input.round_starts@[r] as int
    } else {
        input.assignments_len as int
    }
}

pub open spec fn round_len(input: &FlatPlan, r: int) -> int
    recommends
        0 <= r < input.rounds_len as int,
{
    input.round_lens@[r] as int
}

pub open spec fn in_round(input: &FlatPlan, r: int, pos: int) -> bool {
    &&& 0 <= r < input.rounds_len as int
    &&& round_start(input, r) <= pos < round_start(input, r) + round_len(input, r)
}

pub open spec fn assignment_ok(input: &FlatPlan, pos: int) -> bool {
    &&& 0 <= pos < input.assignments_len as int
    &&& pos < MAX_TOTAL
    &&& person_in_set(input, participant_at(input, pos))
    &&& track_in_set(input, track_at(input, pos))
    &&& (track_at(input, pos) as int) < MAX_TRACKS
    &&& owner_of(input, track_at(input, pos)) != NO_OWNER
    &&& owner_of(input, track_at(input, pos)) != participant_at(input, pos)
    &&& !seen_at(input, pos, participant_at(input, pos), track_at(input, pos))
}

pub open spec fn round_people_distinct(input: &FlatPlan, r: int) -> bool {
    Seq::new(
        round_len(input, r) as nat,
        |i: int| participant_at(input, round_start(input, r) + i),
    ).to_set().len() == round_len(input, r)
}

pub open spec fn round_tracks_distinct(input: &FlatPlan, r: int) -> bool {
    Seq::new(
        round_len(input, r) as nat,
        |i: int| track_at(input, round_start(input, r) + i),
    ).to_set().len() == round_len(input, r)
}

pub open spec fn round_has_person(input: &FlatPlan, r: int, p: u16) -> bool {
    exists|i: int| {
        &&& 0 <= i < round_len(input, r)
        &&& in_round(input, r, round_start(input, r) + i)
        &&& #[trigger] participant_at(input, round_start(input, r) + i) == p
    }
}

pub open spec fn round_has_track(input: &FlatPlan, r: int, t: u16) -> bool {
    exists|i: int| {
        &&& 0 <= i < round_len(input, r)
        &&& in_round(input, r, round_start(input, r) + i)
        &&& #[trigger] track_at(input, round_start(input, r) + i) == t
    }
}

pub open spec fn round_ok(input: &FlatPlan, r: int) -> bool {
    &&& 0 <= r < input.rounds_len as int
    &&& round_start(input, r) + round_len(input, r) <= input.assignments_len as int
    &&& forall|pos: int| in_round(input, r, pos) ==> assignment_ok(input, pos)
    &&& round_people_distinct(input, r)
    &&& round_tracks_distinct(input, r)
    &&& (input.policy == Policy::Strict ==> forall|p: u16|
        person_in_set(input, p) ==> round_has_person(input, r, p))
    &&& (input.policy == Policy::Strict ==> forall|t: u16|
        track_in_set(input, t) ==> round_has_track(input, r, t))
}

pub open spec fn positions_ok(input: &FlatPlan) -> bool {
    forall|pos: int|
        0 <= pos < input.assignments_len as int && pos < MAX_TOTAL ==> assignment_ok(input, pos)
}

pub open spec fn plan_ok(input: &FlatPlan) -> bool {
    &&& capacity_ok(input)
    &&& people_in_range(input)
    &&& tracks_in_range(input)
    &&& people_distinct(input)
    &&& tracks_distinct(input)
    &&& all_tracks_owned(input)
    &&& positions_ok(input)
    &&& forall|r: int| #[trigger] round_index(input, r) ==> round_ok(input, r)
}

pub open spec fn no_repeated_pair(input: &FlatPlan) -> bool {
    forall|p1: int, p2: int|
        0 <= p1 < p2 < input.assignments_len as int && p1 < MAX_TOTAL && p2 < MAX_TOTAL
            ==> (participant_at(input, p1), track_at(input, p1)) != (
            participant_at(input, p2),
            track_at(input, p2),
        )
}

pub open spec fn no_own_track(input: &FlatPlan) -> bool {
    forall|pos: int|
        0 <= pos < input.assignments_len as int && pos < MAX_TOTAL ==> owner_of(
            input,
            track_at(input, pos),
        ) != participant_at(input, pos)
}

pub open spec fn strict_round_complete(input: &FlatPlan, r: int) -> bool {
    (forall|p: u16| person_in_set(input, p) ==> #[trigger] round_has_person(input, r, p))
        && (forall|t: u16| track_in_set(input, t) ==> #[trigger] round_has_track(input, r, t))
}

pub open spec fn strict_complete(input: &FlatPlan) -> bool {
    input.policy == Policy::Strict ==> forall|r: int|
        #[trigger] round_index(input, r) ==> strict_round_complete(input, r)
}

// Certificate specification.

pub open spec fn cert_tracks_set(cert: &FlatCertificate) -> Set<u16> {
    cert.tracks@.take(cert.tracks_len as int).to_set()
}

pub open spec fn cert_neighbors_set(cert: &FlatCertificate) -> Set<u16> {
    cert.neighbors@.take(cert.neighbors_len as int).to_set()
}

pub open spec fn cert_capacity_ok(input: &FlatPlan, cert: &FlatCertificate) -> bool {
    &&& capacity_ok(input)
    &&& cert.tracks_len <= MAX_TRACKS
    &&& cert.neighbors_len <= MAX_PEOPLE
    &&& cert.round <= input.rounds_len
}

pub open spec fn cert_tracks_distinct(cert: &FlatCertificate) -> bool {
    cert_tracks_set(cert).len() == cert.tracks_len as int
}

pub open spec fn cert_tracks_active(input: &FlatPlan, cert: &FlatCertificate) -> bool {
    forall|i: int| {
        0 <= i < cert.tracks_len as int && i < MAX_TRACKS ==> (cert.tracks@[i] as int)
            < MAX_TRACKS && track_in_set(input, cert.tracks@[i])
    }
}

pub open spec fn cert_neighbors_distinct(cert: &FlatCertificate) -> bool {
    cert_neighbors_set(cert).len() == cert.neighbors_len as int
}

pub open spec fn cert_neighbors_active(input: &FlatPlan, cert: &FlatCertificate) -> bool {
    forall|i: int| {
        0 <= i < cert.neighbors_len as int && i < MAX_PEOPLE ==> person_in_set(
            input,
            cert.neighbors@[i],
        ) && (cert.neighbors@[i] as int) < MAX_PEOPLE
    }
}

pub open spec fn seen_before_round(input: &FlatPlan, r: int, participant: u16, track: u16) -> bool {
    &&& 0 <= r <= input.rounds_len as int
    &&& seen_at(input, round_start(input, r), participant, track)
}

pub open spec fn eligible_at(
    input: &FlatPlan,
    r: int,
    participant: u16,
    track: u16,
) -> bool {
    &&& person_in_set(input, participant)
    &&& track_in_set(input, track)
    &&& (track as int) < MAX_TRACKS
    &&& owner_of(input, track) != NO_OWNER
    &&& owner_of(input, track) != participant
    &&& !seen_before_round(input, r, participant, track)
}

pub open spec fn cert_completeness(input: &FlatPlan, cert: &FlatCertificate) -> bool {
    forall|ti: int, j: int| {
        &&& 0 <= ti < cert.tracks_len as int
        &&& ti < MAX_TRACKS
        &&& 0 <= j < input.active_people_len as int
        &&& j < MAX_PEOPLE
        &&& input.active_people@[j] != owner_of(input, cert.tracks@[ti])
        &&& !seen_before_round(input, cert.round as int, input.active_people@[j], cert.tracks@[ti])
            ==> cert_neighbors_set(cert).contains(input.active_people@[j])
    }
}

pub open spec fn cert_ok(input: &FlatPlan, cert: &FlatCertificate) -> bool {
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
// Lemmas
// ---------------------------------------------------------------------------

proof fn lemma_seen_prefix_step(input: &FlatPlan, pos: int)
    requires
        0 <= pos < input.assignments_len as int,
        pos < MAX_TOTAL,
    ensures
        seen_prefix(input, pos + 1) == seen_prefix(input, pos).insert(
            (participant_at(input, pos), track_at(input, pos)),
        ),
{
}

proof fn lemma_seen_prefix_contains(input: &FlatPlan, pos1: int, pos2: int)
    requires
        0 <= pos1 < pos2 <= input.assignments_len as int,
        pos1 < MAX_TOTAL,
        pos2 <= MAX_TOTAL,
    ensures
        seen_prefix(input, pos2).contains((participant_at(input, pos1), track_at(input, pos1))),
    decreases pos2,
{
    if pos2 == pos1 + 1 {
        lemma_seen_prefix_step(input, pos1);
        assert(seen_prefix(input, pos1 + 1).contains((
            participant_at(input, pos1),
            track_at(input, pos1),
        )));
    } else {
        lemma_seen_prefix_contains(input, pos1, pos2 - 1);
        lemma_seen_prefix_step(input, pos2 - 1);
        assert(seen_prefix(input, pos2 - 1).contains((
            participant_at(input, pos1),
            track_at(input, pos1),
        )));
        assert(seen_prefix(input, pos2).contains((
            participant_at(input, pos1),
            track_at(input, pos1),
        )));
    }
}

proof fn lemma_seen_prefix_witness(input: &FlatPlan, pos: int, p: u16, t: u16)
    requires
        pos <= MAX_TOTAL,
        seen_at(input, pos, p, t),
    ensures
        exists|j: int|
            0 <= j < pos && j < MAX_TOTAL && participant_at(input, j) == p && track_at(
                input,
                j,
            ) == t,
    decreases pos,
{
    if pos <= 0 {
        assert(false);
    } else if participant_at(input, pos - 1) == p && track_at(input, pos - 1) == t {
        assert(0 <= pos - 1 < pos);
    } else {
        assert(seen_prefix(input, pos) == seen_prefix(input, pos - 1).insert(
            (participant_at(input, pos - 1), track_at(input, pos - 1)),
        ));
        lemma_set_insert_different(
            seen_prefix(input, pos - 1),
            (p, t),
            (participant_at(input, pos - 1), track_at(input, pos - 1)),
        );
        assert(seen_prefix(input, pos - 1).contains((p, t)));
        lemma_seen_prefix_witness(input, pos - 1, p, t);
    }
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

proof fn lemma_round_people_pairwise(input: &FlatPlan, r: int)
    requires
        0 <= r < input.rounds_len as int,
        round_people_distinct(input, r),
    ensures
        forall|i: int, j: int|
            0 <= i < round_len(input, r) && i < MAX_TOTAL && 0 <= j < round_len(input, r) && j
                < MAX_TOTAL && i != j ==> #[trigger] participant_at(
                input,
                round_start(input, r) + i,
            ) != #[trigger] participant_at(input, round_start(input, r) + j),
{
    let n = round_len(input, r);
    let s = Seq::new(n as nat, |i: int| participant_at(input, round_start(input, r) + i));
    assert(s.take(n) =~= s);
    assert(round_people_distinct(input, r));
    lemma_prefix_no_duplicates(s, n);
    assert(s.take(n).no_duplicates());
    assert forall|i: int, j: int|
        0 <= i < n && i < MAX_TOTAL && 0 <= j < n && j < MAX_TOTAL && i != j implies #[trigger] participant_at(
        input,
        round_start(input, r) + i,
    ) != #[trigger] participant_at(input, round_start(input, r) + j) by {
        assert(s[i] == participant_at(input, round_start(input, r) + i));
        assert(s[j] == participant_at(input, round_start(input, r) + j));
        assert(s[i] != s[j]);
    }
}

pub proof fn theorem_flat_plan_sound(input: &FlatPlan)
    requires
        plan_ok(input),
    ensures
        no_repeated_pair(input),
        no_own_track(input),
        strict_complete(input),
{
    assert forall|p1: int, p2: int|
        0 <= p1 < p2 < input.assignments_len as int && p1 < MAX_TOTAL && p2 < MAX_TOTAL implies (
        participant_at(input, p1),
        track_at(input, p1),
    ) != (participant_at(input, p2), track_at(input, p2)) by {
        lemma_seen_prefix_contains(input, p1, p2);
        assert(seen_prefix(input, p2).contains((participant_at(input, p1), track_at(input, p1))));
        assert(positions_ok(input));
        assert(assignment_ok(input, p2));
        assert(!seen_at(input, p2, participant_at(input, p2), track_at(input, p2)));
    }
    assert forall|pos: int|
        0 <= pos < input.assignments_len as int && pos < MAX_TOTAL implies owner_of(
            input,
            track_at(input, pos),
        ) != participant_at(input, pos) by {
        assert(positions_ok(input));
        assert(assignment_ok(input, pos));
    }
    assert(strict_complete(input));
}

proof fn lemma_flat_certificate_counting(
    input: &FlatPlan,
    cert: &FlatCertificate,
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
            cert.tracks@.take(cert.tracks_len as int).to_set_ensures();
            assert(cert.tracks@.take(cert.tracks_len as int).contains(t));
            let ti = choose|ti: int|
                0 <= ti < cert.tracks_len as int && cert.tracks@[ti] == t;
            lemma_flat_person_witness(input, holder(t));
            let j = choose|j: int|
                0 <= j < input.active_people_len as int && j < MAX_PEOPLE
                    && input.active_people@[j] == holder(t);
            assert(input.active_people@[j] != owner_of(input, cert.tracks@[ti]));
            assert(!seen_before_round(input, cert.round as int, input.active_people@[j], cert.tracks@[ti]));
            assert(n_set.contains(input.active_people@[j]));
        }
    }
    lemma_len_subset(t_set.map(holder), n_set);
    lemma_map_size(t_set, t_set.map(holder), holder);
    assert(t_set.len() <= n_set.len());
    assert(n_set.len() < t_set.len());
}


// ---------------------------------------------------------------------------
// Executable flat checker
// ---------------------------------------------------------------------------

proof fn lemma_flat_person_witness(input: &FlatPlan, p: u16)
    requires
        capacity_ok(input),
        person_in_set(input, p),
    ensures
        exists|i: int|
            0 <= i < input.active_people_len as int && i < MAX_PEOPLE
                && input.active_people@[i] == p,
{
    people_prefix(input).to_set_ensures();
    assert(people_prefix(input).contains(p));
}

proof fn lemma_flat_track_witness(input: &FlatPlan, t: u16)
    requires
        capacity_ok(input),
        track_in_set(input, t),
    ensures
        exists|i: int|
            0 <= i < input.active_tracks_len as int && i < MAX_TRACKS
                && input.active_tracks@[i] == t,
{
    tracks_prefix(input).to_set_ensures();
    assert(tracks_prefix(input).contains(t));
}

proof fn lemma_flat_assignment_range(input: &FlatPlan, pos: int)
    requires
        capacity_ok(input),
        people_in_range(input),
        tracks_in_range(input),
        positions_ok(input),
        0 <= pos < input.assignments_len as int,
        pos < MAX_TOTAL,
    ensures
        (participant_at(input, pos) as int) < MAX_PEOPLE,
        (track_at(input, pos) as int) < MAX_TRACKS,
        person_in_set(input, participant_at(input, pos)),
        track_in_set(input, track_at(input, pos)),
{
    assert(assignment_ok(input, pos));
    lemma_flat_person_witness(input, participant_at(input, pos));
    lemma_flat_track_witness(input, track_at(input, pos));
}

fn contains_person(input: &FlatPlan, p: u16) -> (found: bool)
    requires
        capacity_ok(input),
    ensures
        found ==> person_in_set(input, p),
{
    assert(capacity_ok(input));
    let mut i: usize = 0;
    while i < input.active_people_len
        invariant
            0 <= i <= input.active_people_len,
            input.active_people_len <= MAX_PEOPLE,
        decreases input.active_people_len - i,
    {
        assert(i < MAX_PEOPLE);
        if input.active_people[i] == p {
            assert(person_in_set(input, p)) by {
                assert(input.active_people@[i as int] == p);
                assert(people_prefix(input)[i as int] == p);
                assert(people_prefix(input).contains(p));
                people_prefix(input).to_set_ensures();
            }
            return true;
        }
        i = i + 1;
    }
    false
}

fn contains_track(input: &FlatPlan, t: u16) -> (found: bool)
    requires
        capacity_ok(input),
    ensures
        found ==> track_in_set(input, t),
{
    assert(capacity_ok(input));
    let mut i: usize = 0;
    while i < input.active_tracks_len
        invariant
            0 <= i <= input.active_tracks_len,
            input.active_tracks_len <= MAX_TRACKS,
        decreases input.active_tracks_len - i,
    {
        assert(i < MAX_TRACKS);
        if input.active_tracks[i] == t {
            assert(track_in_set(input, t)) by {
                assert(input.active_tracks@[i as int] == t);
                assert(tracks_prefix(input)[i as int] == t);
                assert(tracks_prefix(input).contains(t));
                tracks_prefix(input).to_set_ensures();
            }
            return true;
        }
        i = i + 1;
    }
    false
}

fn people_in_range_exec(input: &FlatPlan) -> (ok: bool)
    requires
        capacity_ok(input),
    ensures
        ok ==> people_in_range(input),
{
    assert(capacity_ok(input));
    let mut i: usize = 0;
    while i < input.active_people_len
        invariant
            0 <= i <= input.active_people_len,
            input.active_people_len <= MAX_PEOPLE,
            forall|j: int| 0 <= j < i ==> (input.active_people@[j] as int) < MAX_PEOPLE,
        decreases input.active_people_len - i,
    {
        assert(i < MAX_PEOPLE);
        if input.active_people[i] as usize >= MAX_PEOPLE {
            return false;
        }
        i = i + 1;
    }
    true
}

fn tracks_in_range_exec(input: &FlatPlan) -> (ok: bool)
    requires
        capacity_ok(input),
    ensures
        ok ==> tracks_in_range(input),
{
    assert(capacity_ok(input));
    let mut i: usize = 0;
    while i < input.active_tracks_len
        invariant
            0 <= i <= input.active_tracks_len,
            input.active_tracks_len <= MAX_TRACKS,
            forall|j: int| 0 <= j < i ==> (input.active_tracks@[j] as int) < MAX_TRACKS,
        decreases input.active_tracks_len - i,
    {
        assert(i < MAX_TRACKS);
        if input.active_tracks[i] as usize >= MAX_TRACKS {
            return false;
        }
        i = i + 1;
    }
    true
}

fn people_distinct_exec(input: &FlatPlan) -> (ok: bool)
    requires
        capacity_ok(input),
    ensures
        ok ==> people_distinct(input),
{
    assert(capacity_ok(input));
    assert(input.active_people_len <= MAX_PEOPLE);
    let mut seen: Vec<u16> = Vec::new();
    let mut i: usize = 0;
    while i < input.active_people_len
        invariant
            0 <= i <= input.active_people_len,
            input.active_people_len <= MAX_PEOPLE,
            seen@ == input.active_people@.take(i as int),
            seen@.no_duplicates(),
        decreases input.active_people_len - i,
    {
        assert(i < MAX_PEOPLE);
        let p = input.active_people[i];
        if seen.contains(&p) {
            return false;
        }
        seen.push(p);
        assert(input.active_people@.take(i as int + 1) == input.active_people@.take(i as int).push(
            p,
        ));
        proof {
            lemma_no_dup_in_concat(seen@.drop_last(), seq![p]);
        }
        i = i + 1;
    }
    assert(seen@ == input.active_people@.take(input.active_people_len as int));
    assert(seen@.no_duplicates());
    proof {
        seen@.unique_seq_to_set();
    }
    assert(seen@.len() == seen@.to_set().len());
    true
}

fn tracks_distinct_exec(input: &FlatPlan) -> (ok: bool)
    requires
        capacity_ok(input),
    ensures
        ok ==> tracks_distinct(input),
{
    assert(capacity_ok(input));
    assert(input.active_tracks_len <= MAX_TRACKS);
    let mut seen: Vec<u16> = Vec::new();
    let mut i: usize = 0;
    while i < input.active_tracks_len
        invariant
            0 <= i <= input.active_tracks_len,
            input.active_tracks_len <= MAX_TRACKS,
            seen@ == input.active_tracks@.take(i as int),
            seen@.no_duplicates(),
        decreases input.active_tracks_len - i,
    {
        assert(i < MAX_TRACKS);
        let t = input.active_tracks[i];
        if seen.contains(&t) {
            return false;
        }
        seen.push(t);
        assert(input.active_tracks@.take(i as int + 1) == input.active_tracks@.take(i as int).push(
            t,
        ));
        proof {
            lemma_no_dup_in_concat(seen@.drop_last(), seq![t]);
        }
        i = i + 1;
    }
    assert(seen@ == input.active_tracks@.take(input.active_tracks_len as int));
    assert(seen@.no_duplicates());
    proof {
        seen@.unique_seq_to_set();
    }
    assert(seen@.len() == seen@.to_set().len());
    true
}

fn all_tracks_owned_exec(input: &FlatPlan) -> (ok: bool)
    requires
        capacity_ok(input),
        tracks_in_range(input),
    ensures
        ok ==> all_tracks_owned(input),
{
    assert(capacity_ok(input));
    assert(tracks_in_range(input));
    assert(input.active_tracks_len <= MAX_TRACKS);
    let mut i: usize = 0;
    while i < input.active_tracks_len
        invariant
            0 <= i <= input.active_tracks_len,
            input.active_tracks_len <= MAX_TRACKS,
            tracks_in_range(input),
            forall|j: int| 0 <= j < i ==> (input.active_tracks@[j] as int) < MAX_TRACKS,
            forall|j: int| 0 <= j < i ==> owner_of(input, input.active_tracks@[j]) != NO_OWNER,
        decreases input.active_tracks_len - i,
    {
        assert(i < MAX_TRACKS);
        assert((input.active_tracks@[i as int] as int) < MAX_TRACKS);
        let t = input.active_tracks[i];
        assert(t == input.active_tracks@[i as int]);
        if input.owners[t as usize] == NO_OWNER {
            return false;
        }
        i = i + 1;
    }
    true
}

fn seen_before_pos_exec(input: &FlatPlan, pos: usize, p: u16, t: u16) -> (found: bool)
    requires
        capacity_ok(input),
        pos <= input.assignments_len,
    ensures
        found == seen_at(input, pos as int, p, t),
{
    let mut i: usize = 0;
    while i < pos
        invariant
            0 <= i <= pos,
            pos <= input.assignments_len,
            input.assignments_len <= MAX_TOTAL,
            forall|j: int|
                0 <= j < i && j < MAX_TOTAL ==> (participant_at(input, j), track_at(input, j))
                    != (p, t),
        decreases pos - i,
    {
        assert(i < MAX_TOTAL);
        let pi = input.participants[i];
        let ti = input.tracks[i];
        if pi == p && ti == t {
            assert(seen_at(input, pos as int, p, t)) by {
                lemma_seen_prefix_contains(input, i as int, pos as int);
                assert((participant_at(input, i as int), track_at(input, i as int)) == (p, t));
            }
            return true;
        }
        assert((participant_at(input, i as int), track_at(input, i as int)) != (p, t));
        i = i + 1;
    }
    assert(!seen_at(input, pos as int, p, t)) by {
        if seen_at(input, pos as int, p, t) {
            lemma_seen_prefix_witness(input, pos as int, p, t);
            let j = choose|j: int|
                0 <= j < pos as int && j < MAX_TOTAL && participant_at(input, j) == p && track_at(
                    input,
                    j,
                ) == t;
            assert(false);
        }
    }
    false
}

fn round_has_person_exec(input: &FlatPlan, r: usize, p: u16) -> (found: bool)
    requires
        capacity_ok(input),
        r < input.rounds_len,
    ensures
        found ==> round_has_person(input, r as int, p),
{
    assert(capacity_ok(input));
    assert(round_index(input, r as int));
    let start = input.round_starts[r];
    let len = input.round_lens[r];
    assert(round_start(input, r as int) == start as int);
    assert(round_len(input, r as int) == len as int);
    assert(round_index(input, r as int));
    assert(round_start(input, r as int) + round_len(input, r as int) <= input.assignments_len as int);
    assert(start + len <= input.assignments_len);
    let mut i: usize = 0;
    while i < len
        invariant
            0 <= i <= len,
            r < input.rounds_len,
            round_start(input, r as int) == start as int,
            round_len(input, r as int) == len as int,
            start + len <= input.assignments_len,
            input.assignments_len <= MAX_TOTAL,
        decreases len - i,
    {
        assert(start + i < MAX_TOTAL);
        assert((i as int) < round_len(input, r as int));
        if input.participants[start + i] == p {
            assert(round_has_person(input, r as int, p)) by {
                assert(0 <= i as int);
                assert((i as int) < round_len(input, r as int));
                assert(round_start(input, r as int) == start as int);
                assert(in_round(input, r as int, round_start(input, r as int) + i as int));
                assert(participant_at(input, round_start(input, r as int) + i as int) == p);
            }
            return true;
        }
        i = i + 1;
    }
    false
}

fn round_has_track_exec(input: &FlatPlan, r: usize, t: u16) -> (found: bool)
    requires
        capacity_ok(input),
        r < input.rounds_len,
    ensures
        found ==> round_has_track(input, r as int, t),
{
    assert(capacity_ok(input));
    assert(round_index(input, r as int));
    let start = input.round_starts[r];
    let len = input.round_lens[r];
    assert(round_start(input, r as int) == start as int);
    assert(round_len(input, r as int) == len as int);
    assert(round_index(input, r as int));
    assert(round_start(input, r as int) + round_len(input, r as int) <= input.assignments_len as int);
    assert(start + len <= input.assignments_len);
    let mut i: usize = 0;
    while i < len
        invariant
            0 <= i <= len,
            r < input.rounds_len,
            round_start(input, r as int) == start as int,
            round_len(input, r as int) == len as int,
            start + len <= input.assignments_len,
            input.assignments_len <= MAX_TOTAL,
        decreases len - i,
    {
        assert(start + i < MAX_TOTAL);
        assert((i as int) < round_len(input, r as int));
        if input.tracks[start + i] == t {
            assert(round_has_track(input, r as int, t)) by {
                assert(0 <= i as int);
                assert((i as int) < round_len(input, r as int));
                assert(round_start(input, r as int) == start as int);
                assert(in_round(input, r as int, round_start(input, r as int) + i as int));
                assert(track_at(input, round_start(input, r as int) + i as int) == t);
            }
            return true;
        }
        i = i + 1;
    }
    false
}


fn capacity_ok_exec(input: &FlatPlan) -> (ok: bool)
    ensures
        ok ==> capacity_ok(input),
{
    if input.active_people_len > MAX_PEOPLE {
        return false;
    }
    if input.active_tracks_len > MAX_TRACKS {
        return false;
    }
    if input.rounds_len > MAX_ROUNDS {
        return false;
    }
    if input.assignments_len > MAX_TOTAL {
        return false;
    }
    let mut r: usize = 0;
    while r < input.rounds_len
        invariant
            0 <= r <= input.rounds_len,
            input.rounds_len <= MAX_ROUNDS,
            input.assignments_len <= MAX_TOTAL,
            forall|k: int| #[trigger] round_below(k, r as int) ==> round_start(input, k) + round_len(input, k)
                <= input.assignments_len as int,
            forall|k: int| #[trigger] round_below(k, r as int) ==> round_start(input, k) < MAX_TOTAL,
            forall|k: int| #[trigger] round_below(k, r as int) ==> round_len(input, k) <= MAX_ASSIGNMENTS,
            forall|k: int| #[trigger] round_below(k, r as int) && k > 0 ==> round_start(input, k) == round_start(input, k - 1)
                + round_len(input, k - 1),
            (r > 0 ==> round_start(input, 0) == 0),
        decreases input.rounds_len - r,
    {
        assert(r < MAX_ROUNDS);
        let start = input.round_starts[r];
        let len = input.round_lens[r];
        assert(round_start(input, r as int) == start as int);
        assert(round_len(input, r as int) == len as int);
        if start >= MAX_TOTAL || len > MAX_ASSIGNMENTS {
            return false;
        }
        assert(start < MAX_TOTAL);
        assert(len <= MAX_ASSIGNMENTS);
        if start + len > input.assignments_len {
            return false;
        }
        assert(start + len <= MAX_TOTAL + MAX_TOTAL);
        if r == 0 {
            if start != 0 {
                return false;
            }
        } else {
            let prev_start = input.round_starts[r - 1];
            let prev_len = input.round_lens[r - 1];
            assert(round_below(r as int - 1, r as int));
            assert(round_start(input, r as int - 1) < MAX_TOTAL);
            assert(round_len(input, r as int - 1) <= MAX_ASSIGNMENTS);
            assert(round_start(input, r as int - 1) == prev_start as int);
            assert(round_len(input, r as int - 1) == prev_len as int);
            assert(prev_start < MAX_TOTAL);
            assert(prev_len <= MAX_ASSIGNMENTS);
            assert(prev_start + prev_len <= MAX_TOTAL + MAX_TOTAL);
            if start != prev_start + prev_len {
                return false;
            }
            assert(round_start(input, r as int) == round_start(input, r as int - 1)
                + round_len(input, r as int - 1));
        }
        assert(round_start(input, r as int) < MAX_TOTAL);
        assert(round_len(input, r as int) <= MAX_TOTAL);
        assert(round_start(input, r as int) + round_len(input, r as int)
            <= input.assignments_len as int);
        assert forall|k: int|
            #[trigger] round_below(k, r as int + 1) implies round_start(input, k) < MAX_TOTAL by {
            assert(round_below(k, r as int + 1));
            assert(0 <= k);
            if k < r as int {
                assert(round_below(k, r as int));
            } else {
                assert(k == r as int);
            }
        }
        assert forall|k: int|
            #[trigger] round_below(k, r as int + 1) implies round_len(input, k) <= MAX_ASSIGNMENTS by {
            assert(round_below(k, r as int + 1));
            assert(0 <= k);
            if k < r as int {
                assert(round_below(k, r as int));
            } else {
                assert(k == r as int);
            }
        }
        assert forall|k: int|
            #[trigger] round_below(k, r as int + 1) implies round_start(input, k) + round_len(input, k)
                <= input.assignments_len as int by {
            assert(round_below(k, r as int + 1));
            assert(0 <= k);
            if k < r as int {
                assert(round_below(k, r as int));
            } else {
                assert(k == r as int);
            }
        }
        assert forall|k: int|
            #[trigger] round_below(k, r as int + 1) && k > 0 implies round_start(input, k)
                == round_start(input, k - 1) + round_len(input, k - 1) by {
            if k < r as int {
                assert(round_below(k, r as int));
            } else {
                assert(k == r as int);
                assert(round_start(input, k) == round_start(input, r as int));
                assert(round_start(input, r as int) == round_start(input, r as int - 1)
                    + round_len(input, r as int - 1));
            }
        }
        r = r + 1;
    }
    if input.rounds_len > 0 {
        let last = input.rounds_len - 1;
        assert(round_below(last as int, input.rounds_len as int));
        let start = input.round_starts[last];
        let len = input.round_lens[last];
        assert(round_start(input, last as int) == start as int);
        assert(round_len(input, last as int) == len as int);
        assert(round_start(input, last as int) < MAX_TOTAL);
        assert(round_len(input, last as int) <= MAX_ASSIGNMENTS);
        assert(start < MAX_TOTAL);
        assert(len <= MAX_ASSIGNMENTS);
        assert(start + len <= MAX_TOTAL + MAX_TOTAL);
        if start + len != input.assignments_len {
            return false;
        }
    } else if input.assignments_len != 0 {
        return false;
    }
    assert(r == input.rounds_len);
    assert forall|k: int|
        #[trigger] round_index(input, k) implies round_start(input, k) + round_len(input, k)
            <= input.assignments_len as int by {
        assert(0 <= k);
        assert(k < r as int);
        assert(round_below(k, r as int));
    }
    assert forall|k: int|
        #[trigger] round_index(input, k) implies round_start(input, k) < MAX_TOTAL by {
        assert(0 <= k);
        assert(k < r as int);
        assert(round_below(k, r as int));
    }
    assert forall|k: int|
        #[trigger] round_index(input, k) implies round_len(input, k) <= MAX_ASSIGNMENTS by {
        assert(0 <= k);
        assert(k < r as int);
        assert(round_below(k, r as int));
    }
    assert forall|k: int|
        #[trigger] round_index(input, k) && k > 0 implies round_start(input, k) == round_start(
            input,
            k - 1,
        ) + round_len(input, k - 1) by {
        assert(0 <= k);
        assert(k < r as int);
        assert(round_below(k, r as int));
    }
    assert(capacity_ok(input));
    true
}

fn check_round_shape(input: &FlatPlan, r: usize) -> (res: Result<(), FlatReject>)
    requires
        capacity_ok(input),
        positions_ok(input),
        people_in_range(input),
        tracks_in_range(input),
        r < input.rounds_len,
    ensures
        res.is_ok() ==> round_ok(input, r as int),
{
    assert(capacity_ok(input));
    assert(r < MAX_ROUNDS);
    let start = input.round_starts[r];
    let len = input.round_lens[r];
    assert(round_start(input, r as int) == start as int);
    assert(round_len(input, r as int) == len as int);
    assert(round_index(input, r as int));
    assert(round_start(input, r as int) + round_len(input, r as int) <= input.assignments_len as int);
    assert(start + len <= input.assignments_len);

    // Duplicate participants in the round.
    let mut seen_people: Vec<u16> = Vec::new();
    let mut i: usize = 0;
    while i < len
        invariant
            0 <= i <= len,
            start + len <= input.assignments_len,
            input.assignments_len <= MAX_TOTAL,
            round_start(input, r as int) == start as int,
            round_len(input, r as int) == len as int,
            seen_people@ == Seq::new(i as nat, |j: int| participant_at(input, start as int + j)),
            seen_people@.no_duplicates(),
        decreases len - i,
    {
        assert(start + i < MAX_TOTAL);
        let p = input.participants[start + i];
        if seen_people.contains(&p) {
            return Err(FlatReject::DuplicateParticipantInRound { round: r, participant: p });
        }
        seen_people.push(p);
        assert(Seq::new(i as nat + 1, |j: int| participant_at(input, start as int + j))
            == Seq::new(i as nat, |j: int| participant_at(input, start as int + j)).push(p));
        proof {
            lemma_no_dup_in_concat(seen_people@.drop_last(), seq![p]);
        }
        i = i + 1;
    }
    assert(seen_people@ == Seq::new(len as nat, |j: int| participant_at(input, start as int + j)));
    proof {
        seen_people@.unique_seq_to_set();
    }
    assert(seen_people@.len() == seen_people@.to_set().len());
    assert(round_people_distinct(input, r as int));

    // Duplicate tracks in the round.
    let mut seen_tracks: Vec<u16> = Vec::new();
    let mut i: usize = 0;
    while i < len
        invariant
            0 <= i <= len,
            start + len <= input.assignments_len,
            input.assignments_len <= MAX_TOTAL,
            round_start(input, r as int) == start as int,
            round_len(input, r as int) == len as int,
            seen_tracks@ == Seq::new(i as nat, |j: int| track_at(input, start as int + j)),
            seen_tracks@.no_duplicates(),
        decreases len - i,
    {
        assert(start + i < MAX_TOTAL);
        let t = input.tracks[start + i];
        if seen_tracks.contains(&t) {
            return Err(FlatReject::DuplicateTrackInRound { round: r, track: t });
        }
        seen_tracks.push(t);
        assert(Seq::new(i as nat + 1, |j: int| track_at(input, start as int + j))
            == Seq::new(i as nat, |j: int| track_at(input, start as int + j)).push(t));
        proof {
            lemma_no_dup_in_concat(seen_tracks@.drop_last(), seq![t]);
        }
        i = i + 1;
    }
    assert(seen_tracks@ == Seq::new(len as nat, |j: int| track_at(input, start as int + j)));
    proof {
        seen_tracks@.unique_seq_to_set();
    }
    assert(seen_tracks@.len() == seen_tracks@.to_set().len());
    assert(round_tracks_distinct(input, r as int));

    // Every flat position in the round is a valid assignment.
    assert forall|pos: int| in_round(input, r as int, pos) implies assignment_ok(input, pos) by {
        assert(round_start(input, r as int) <= pos < round_start(input, r as int) + round_len(
            input,
            r as int,
        ));
        assert(positions_ok(input));
    }

    match input.policy {
        Policy::Strict => {
        let mut p_index: usize = 0;
        while p_index < input.active_people_len
            invariant
                capacity_ok(input),
                people_in_range(input),
                r < input.rounds_len,
                input.active_people_len <= MAX_PEOPLE,
                0 <= p_index <= input.active_people_len,
                forall|j: int|
                    0 <= j < p_index && j < MAX_PEOPLE ==> round_has_person(
                        input,
                        r as int,
                        input.active_people@[j],
                    ),
            decreases input.active_people_len - p_index,
        {
            assert(p_index < MAX_PEOPLE);
            let p = input.active_people[p_index];
            if !round_has_person_exec(input, r, p) {
                return Err(FlatReject::MissingParticipant { round: r, participant: p });
            }
            p_index = p_index + 1;
        }
        assert(p_index == input.active_people_len);
        assert forall|p: u16| person_in_set(input, p) implies round_has_person(
            input,
            r as int,
            p,
        ) by {
            lemma_flat_person_witness(input, p);
            let j = choose|j: int|
                0 <= j < input.active_people_len as int && j < MAX_PEOPLE
                    && input.active_people@[j] == p;
            assert(0 <= j < p_index as int);
            assert(round_has_person(input, r as int, input.active_people@[j]));
        }

        let mut t_index: usize = 0;
        while t_index < input.active_tracks_len
            invariant
                capacity_ok(input),
                tracks_in_range(input),
                r < input.rounds_len,
                input.active_tracks_len <= MAX_TRACKS,
                0 <= t_index <= input.active_tracks_len,
                forall|j: int|
                    0 <= j < t_index && j < MAX_TRACKS ==> round_has_track(
                        input,
                        r as int,
                        input.active_tracks@[j],
                    ),
            decreases input.active_tracks_len - t_index,
        {
            assert(t_index < MAX_TRACKS);
            let t = input.active_tracks[t_index];
            if !round_has_track_exec(input, r, t) {
                return Err(FlatReject::MissingTrack { round: r, track: t });
            }
            t_index = t_index + 1;
        }
        assert(t_index == input.active_tracks_len);
        assert forall|t: u16| track_in_set(input, t) implies round_has_track(
            input,
            r as int,
            t,
        ) by {
            lemma_flat_track_witness(input, t);
            let j = choose|j: int|
                0 <= j < input.active_tracks_len as int && j < MAX_TRACKS
                    && input.active_tracks@[j] == t;
            assert(0 <= j < t_index as int);
            assert(round_has_track(input, r as int, input.active_tracks@[j]));
        }
        assert(round_ok(input, r as int));
        },
        Policy::Flexible => {
            assert(round_ok(input, r as int));
        },
    }
    Ok(())
}

pub fn check_flat_plan(input: &FlatPlan) -> (res: Result<(), FlatReject>)
    ensures
        res.is_ok() ==> plan_ok(input),
{
    if !capacity_ok_exec(input) {
        return Err(FlatReject::Capacity);
    }
    if !people_in_range_exec(input) {
        return Err(FlatReject::ParticipantOutOfRange { participant: 0 });
    }
    if !tracks_in_range_exec(input) {
        return Err(FlatReject::TrackOutOfRange { track: 0 });
    }
    if !people_distinct_exec(input) {
        return Err(FlatReject::DuplicateActiveParticipant { participant: 0 });
    }
    if !tracks_distinct_exec(input) {
        return Err(FlatReject::DuplicateActiveTrack { track: 0 });
    }
    if !all_tracks_owned_exec(input) {
        return Err(FlatReject::MissingOwner { track: 0 });
    }

    let mut seen: Vec<(u16, u16)> = Vec::new();
    let mut pos: usize = 0;
    while pos < input.assignments_len
        invariant
            capacity_ok(input),
            0 <= pos <= input.assignments_len,
            input.assignments_len <= MAX_TOTAL,
            seen@.to_set() == seen_prefix(input, pos as int),
            seen@.no_duplicates(),
            forall|q: int| 0 <= q < pos ==> assignment_ok(input, q),
        decreases input.assignments_len - pos,
    {
        assert(pos < MAX_TOTAL);
        let p = input.participants[pos];
        let t = input.tracks[pos];
        if !contains_person(input, p) {
            return Err(FlatReject::UnknownParticipant { position: pos, participant: p });
        }
        if !contains_track(input, t) {
            return Err(FlatReject::UnknownTrack { position: pos, track: t });
        }
        if t as usize >= MAX_TRACKS || input.owners[t as usize] == NO_OWNER {
            return Err(FlatReject::MissingOwner { track: t });
        }
        if input.owners[t as usize] == p {
            return Err(FlatReject::OwnTrack { position: pos, participant: p, track: t });
        }
        if seen.contains(&(p, t)) {
            return Err(FlatReject::RepeatedPair { position: pos, participant: p, track: t });
        }
        assert(!seen_prefix(input, pos as int).contains((p, t)));
        assert(assignment_ok(input, pos as int));
        proof {
            Seq::lemma_push_to_set_commute(seen@, (p, t));
        }
        seen.push((p, t));
        assert(seen@.to_set() == seen_prefix(input, pos as int).insert((p, t)));
        proof {
            lemma_seen_prefix_step(input, pos as int);
        }
        assert(seen@.to_set() == seen_prefix(input, pos as int + 1));
        assert forall|q: int| 0 <= q < pos as int + 1 implies assignment_ok(input, q) by {
            if q < pos as int {
                assert(assignment_ok(input, q));
            } else {
                assert(q == pos as int);
                assert(assignment_ok(input, pos as int));
            }
        }
        pos = pos + 1;
    }
    assert(positions_ok(input)) by {
        assert forall|q: int|
            0 <= q < input.assignments_len as int && q < MAX_TOTAL implies assignment_ok(
                input,
                q,
            ) by {
            assert(0 <= q < pos as int);
        }
    }

    let mut r: usize = 0;
    while r < input.rounds_len
        invariant
            capacity_ok(input),
            positions_ok(input),
            people_in_range(input),
            tracks_in_range(input),
            people_distinct(input),
            tracks_distinct(input),
            all_tracks_owned(input),
            0 <= r <= input.rounds_len,
            forall|rr: int| 0 <= rr < r ==> round_ok(input, rr),
        decreases input.rounds_len - r,
    {
        match check_round_shape(input, r) {
            Ok(()) => {},
            Err(reject) => return Err(reject),
        }
        r = r + 1;
    }
    assert(plan_ok(input));
    Ok(())
}


// The executable certificate verifier is the remaining gap. The Hall counting
// theorem `lemma_flat_certificate_counting` is proved above. The Rust
// certificate verifier is tested against an independent oracle. A flat
// executable verifier is the next target.


impl FlatReject {
    pub fn code(&self) -> u8 {
        match self {
            FlatReject::Capacity => 1,
            FlatReject::DuplicateActiveParticipant { .. } => 2,
            FlatReject::DuplicateActiveTrack { .. } => 3,
            FlatReject::UnknownParticipant { .. } => 4,
            FlatReject::UnknownTrack { .. } => 5,
            FlatReject::MissingOwner { .. } => 6,
            FlatReject::OwnTrack { .. } => 7,
            FlatReject::RepeatedPair { .. } => 8,
            FlatReject::DuplicateParticipantInRound { .. } => 9,
            FlatReject::DuplicateTrackInRound { .. } => 10,
            FlatReject::MissingParticipant { .. } => 11,
            FlatReject::MissingTrack { .. } => 12,
            FlatReject::ParticipantOutOfRange { .. } => 13,
            FlatReject::TrackOutOfRange { .. } => 14,
        }
    }
}

} // verus!

fn parse_plan(line: &str) -> Option<FlatPlan> {
    let mut it = line.split_whitespace();
    if it.next()? != "PLAN" {
        return None;
    }
    let _id: usize = it.next()?.parse().ok()?;
    let policy = match it.next()? {
        "0" => Policy::Strict,
        _ => Policy::Flexible,
    };
    let mut plan = FlatPlan {
        active_people: [0; MAX_PEOPLE],
        active_people_len: 0,
        active_tracks: [0; MAX_TRACKS],
        active_tracks_len: 0,
        owners: [NO_OWNER; MAX_TRACKS],
        round_starts: [0; MAX_ROUNDS],
        round_lens: [0; MAX_ROUNDS],
        rounds_len: 0,
        participants: [0; MAX_TOTAL],
        tracks: [0; MAX_TOTAL],
        assignments_len: 0,
        policy,
    };
    let people_len: usize = it.next()?.parse().ok()?;
    plan.active_people_len = people_len;
    for i in 0..people_len.min(MAX_PEOPLE) {
        plan.active_people[i] = it.next()?.parse().ok()?;
    }
    let tracks_len: usize = it.next()?.parse().ok()?;
    plan.active_tracks_len = tracks_len;
    for i in 0..tracks_len.min(MAX_TRACKS) {
        plan.active_tracks[i] = it.next()?.parse().ok()?;
    }
    for i in 0..MAX_TRACKS {
        plan.owners[i] = it.next()?.parse().ok()?;
    }
    let rounds_len: usize = it.next()?.parse().ok()?;
    plan.rounds_len = rounds_len;
    let mut pos = 0usize;
    for r in 0..rounds_len.min(MAX_ROUNDS) {
        let round_len: usize = it.next()?.parse().ok()?;
        plan.round_starts[r] = pos;
        plan.round_lens[r] = round_len;
        for _ in 0..round_len.min(MAX_TOTAL) {
            let participant: u16 = it.next()?.parse().ok()?;
            let track: u16 = it.next()?.parse().ok()?;
            if pos < MAX_TOTAL {
                plan.participants[pos] = participant;
                plan.tracks[pos] = track;
            }
            pos += 1;
        }
    }
    plan.assignments_len = pos.min(MAX_TOTAL);
    Some(plan)
}

fn main() {
    let path = match std::env::args().nth(1) {
        Some(path) => path,
        None => {
            eprintln!("usage: flat_plan_check CORPUS");
            std::process::exit(2);
        }
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("cannot read {path}: {error}");
            std::process::exit(2);
        }
    };
    for line in text.lines() {
        if !line.starts_with("PLAN ") {
            continue;
        }
        let Some(plan) = parse_plan(line) else {
            eprintln!("bad plan line");
            std::process::exit(2);
        };
        let id = line.split_whitespace().nth(1).unwrap_or("0");
        let code = match check_flat_plan(&plan) {
            Ok(()) => 0,
            Err(reject) => reject.code(),
        };
        println!("PLAN {id} {code}");
    }
}
