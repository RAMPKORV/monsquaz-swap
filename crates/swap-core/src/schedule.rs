use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ids::{ParticipantId, TrackCode};

const MAX_ATTEMPTS: u32 = 128;
const BALANCED_ATTEMPTS: u32 = 8;
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

/// How the classic schedule shapes its partial Latin rectangle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleStrategy {
    /// Seeded min-cost matching with a social-pair penalty.
    #[default]
    Balanced,
    /// Seeded row-by-row perfect matching without a pair objective.
    RandomMatching,
    /// Randomly permuted cyclic Latin square. Falls back to matching when the
    /// roster does not have exactly one track per participant.
    Cyclic,
}

impl ScheduleStrategy {
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "balanced" => Self::Balanced,
            "random_matching" => Self::RandomMatching,
            "cyclic" => Self::Cyclic,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::RandomMatching => "random_matching",
            Self::Cyclic => "cyclic",
        }
    }
}

/// A complete classic schedule. The output is canonical for a given input,
/// strategy, and seed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub seed: u64,
    pub rounds: u32,
    #[serde(default)]
    pub strategy: ScheduleStrategy,
    /// True when the returned assignments are the random-matching candidate.
    #[serde(default)]
    pub fallback: bool,
    pub assignments: Vec<ScheduledAssignment>,
}

/// Social-balance measurements for a generated schedule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceReport {
    /// Extra handoffs beyond the first for each participant pair.
    pub repeated_handoffs: u32,
    /// Highest number of holder-to-holder handoffs for one participant pair.
    /// Seed-owner handoffs are excluded.
    pub max_pair_meetings: u32,
    /// Distinct participant pairs that met as consecutive holders. Seed-owner
    /// handoffs are excluded.
    pub unique_pairs: usize,
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
    /// Generate a randomized partial Latin rectangle with the balanced
    /// strategy. This is the compatibility entry point.
    pub fn generate(
        participants: &[ParticipantId],
        tracks: &[TrackSpec],
        rounds: u32,
        seed: u64,
    ) -> Result<Self, ScheduleError> {
        Self::generate_with(
            participants,
            tracks,
            rounds,
            seed,
            ScheduleStrategy::Balanced,
        )
    }

    /// Generate a randomized partial Latin rectangle.
    ///
    /// Each round is a perfect matching over tracks and participants. A person
    /// never receives their own track and never receives a track twice. The
    /// result is deterministic for the same participant set, track set, round
    /// count, strategy, and seed. Input order does not matter.
    pub fn generate_with(
        participants: &[ParticipantId],
        tracks: &[TrackSpec],
        rounds: u32,
        seed: u64,
        strategy: ScheduleStrategy,
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

        let (mut assignments, fallback) = match strategy {
            ScheduleStrategy::Cyclic => {
                match cyclic_schedule(&people, &specs, &owners, rounds, seed) {
                    Some(assignments) => (assignments, false),
                    None => (
                        random_schedule(&people, &specs, &owners, rounds, seed)
                            .map_err(|round| ScheduleError::Infeasible { round })?,
                        true,
                    ),
                }
            }
            ScheduleStrategy::RandomMatching => (
                random_schedule(&people, &specs, &owners, rounds, seed)
                    .map_err(|round| ScheduleError::Infeasible { round })?,
                false,
            ),
            ScheduleStrategy::Balanced => {
                let balanced = balanced_schedule(&people, &specs, &owners, rounds, seed);
                let random = random_schedule(&people, &specs, &owners, rounds, seed);
                match (balanced, random) {
                    (Ok(balanced), Ok(random)) => {
                        let balanced_report = balance_of(&balanced);
                        let random_report = balance_of(&random);
                        if (
                            random_report.repeated_handoffs,
                            random_report.max_pair_meetings,
                        ) < (
                            balanced_report.repeated_handoffs,
                            balanced_report.max_pair_meetings,
                        ) {
                            (random, true)
                        } else {
                            (balanced, false)
                        }
                    }
                    (Ok(balanced), Err(_)) => (balanced, false),
                    (Err(_), Ok(random)) => (random, true),
                    (Err(round), Err(_)) => {
                        return Err(ScheduleError::Infeasible { round });
                    }
                }
            }
        };

        assignments.sort_by(|a, b| {
            a.round
                .cmp(&b.round)
                .then_with(|| a.track.0.cmp(&b.track.0))
                .then_with(|| a.participant.0.cmp(&b.participant.0))
        });
        Ok(Self {
            seed,
            rounds,
            strategy,
            fallback,
            assignments,
        })
    }

    /// Measure repeated participant pairs across consecutive handoffs. The
    /// first round is excluded because the seed owner is not stored in the
    /// schedule.
    pub fn balance(&self) -> BalanceReport {
        let mut previous: HashMap<&str, &str> = HashMap::new();
        let mut pairs: HashMap<(&str, &str), u32> = HashMap::new();
        let mut sorted: Vec<&ScheduledAssignment> = self.assignments.iter().collect();
        sorted.sort_by_key(|assignment| {
            (
                assignment.round,
                assignment.track.0.as_str(),
                assignment.participant.0.as_str(),
            )
        });
        for assignment in sorted {
            let track = assignment.track.0.as_str();
            let person = assignment.participant.0.as_str();
            if let Some(prior) = previous.insert(track, person) {
                let key = if prior <= person {
                    (prior, person)
                } else {
                    (person, prior)
                };
                *pairs.entry(key).or_default() += 1;
            }
        }
        let repeated_handoffs = pairs.values().map(|count| count.saturating_sub(1)).sum();
        let max_pair_meetings = pairs.values().copied().max().unwrap_or(0);
        BalanceReport {
            repeated_handoffs,
            max_pair_meetings,
            unique_pairs: pairs.len(),
        }
    }
}

/// A randomized cyclic Latin square. It is valid when every participant owns
/// exactly one track. The row, column, and symbol permutations make the square
/// look varied while preserving the Latin property.
fn cyclic_schedule(
    people: &[ParticipantId],
    specs: &[TrackSpec],
    owners: &[usize],
    rounds: u32,
    seed: u64,
) -> Option<Vec<ScheduledAssignment>> {
    if people.len() != specs.len() {
        return None;
    }
    let count = people.len();
    let mut track_for_owner = vec![None; count];
    for (track, owner) in owners.iter().copied().enumerate() {
        if track_for_owner[owner].replace(track).is_some() {
            return None;
        }
    }
    let track_for_owner: Vec<usize> = track_for_owner.into_iter().collect::<Option<_>>()?;
    let mut rng = SplitMix64::new(seed ^ 0xC1C1_C1C1_5EED_5EED);
    let mut order: Vec<usize> = (0..count).collect();
    rng.shuffle(&mut order);
    let mut offsets: Vec<usize> = (1..count).collect();
    rng.shuffle(&mut offsets);

    let mut assignments = Vec::with_capacity(rounds as usize * count);
    for round in 0..rounds {
        let offset = offsets[round as usize];
        for index in 0..count {
            let owner_index = order[index];
            let person_index = order[(index + offset) % count];
            assignments.push(ScheduledAssignment {
                round,
                track: specs[track_for_owner[owner_index]].code.clone(),
                participant: people[person_index].clone(),
            });
        }
    }
    Some(assignments)
}

fn random_schedule(
    people: &[ParticipantId],
    specs: &[TrackSpec],
    owners: &[usize],
    rounds: u32,
    seed: u64,
) -> Result<Vec<ScheduledAssignment>, u32> {
    let mut last_round = 0;
    for attempt in 0..MAX_ATTEMPTS {
        let attempt_seed = seed.wrapping_add(u64::from(attempt).wrapping_mul(SPLITMIX_GOLDEN));
        match attempt_random(people, specs, owners, rounds, attempt_seed) {
            Ok(assignments) => return Ok(assignments),
            Err(round) => last_round = round,
        }
    }
    Err(last_round)
}

fn balanced_schedule(
    people: &[ParticipantId],
    specs: &[TrackSpec],
    owners: &[usize],
    rounds: u32,
    seed: u64,
) -> Result<Vec<ScheduledAssignment>, u32> {
    let mut best: Option<(BalanceReport, Vec<ScheduledAssignment>)> = None;
    let mut last_round = 0;
    for attempt in 0..BALANCED_ATTEMPTS {
        let attempt_seed = seed.wrapping_add(u64::from(attempt).wrapping_mul(SPLITMIX_GOLDEN));
        match attempt_balanced(people, specs, owners, rounds, attempt_seed) {
            Ok(assignments) => {
                let report = balance_of(&assignments);
                let replace = best.as_ref().is_none_or(|(current, _)| {
                    (report.repeated_handoffs, report.max_pair_meetings)
                        < (current.repeated_handoffs, current.max_pair_meetings)
                });
                if replace {
                    best = Some((report, assignments));
                }
            }
            Err(round) => last_round = round,
        }
    }
    best.map(|(_, assignments)| assignments).ok_or(last_round)
}

fn balance_of(assignments: &[ScheduledAssignment]) -> BalanceReport {
    let schedule = Schedule {
        seed: 0,
        rounds: assignments.iter().map(|a| a.round + 1).max().unwrap_or(0),
        strategy: ScheduleStrategy::Balanced,
        fallback: false,
        assignments: assignments.to_vec(),
    };
    schedule.balance()
}

fn attempt_random(
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

/// Balanced matching with a min-cost flow. The cost penalises participant
/// pairs that already met, especially in the last two rounds, and adds a small
/// seeded jitter so equal-cost matchings vary.
fn attempt_balanced(
    people: &[ParticipantId],
    specs: &[TrackSpec],
    owners: &[usize],
    rounds: u32,
    seed: u64,
) -> Result<Vec<ScheduledAssignment>, u32> {
    let mut history: Vec<HashSet<usize>> = vec![HashSet::new(); specs.len()];
    let mut last_holder = owners.to_vec();
    let mut met: HashMap<(usize, usize), u32> = HashMap::new();
    let mut recent: HashMap<(usize, usize), u32> = HashMap::new();
    let mut rng = SplitMix64::new(seed ^ 0xBADA_55ED_1234_5678);
    let mut assignments = Vec::with_capacity(rounds as usize * specs.len());

    for round in 0..rounds {
        let track_count = specs.len();
        let person_count = people.len();
        let source = 0;
        let track_node = |track: usize| 1 + track;
        let person_node = |person: usize| 1 + track_count + person;
        let sink = 1 + track_count + person_count;
        let mut flow = MinCostFlow::new(sink + 1);

        for track in 0..track_count {
            flow.add_edge(source, track_node(track), 1, 0);
        }
        for person in 0..person_count {
            flow.add_edge(person_node(person), sink, 1, 0);
        }

        let mut candidates: Vec<Vec<(usize, usize)>> = vec![Vec::new(); track_count];
        for track in 0..track_count {
            for person in 0..person_count {
                if person == owners[track] || history[track].contains(&person) {
                    continue;
                }
                let pair = ordered_pair(last_holder[track], person);
                let meetings = i64::from(*met.get(&pair).unwrap_or(&0));
                let recent_penalty = match recent.get(&pair) {
                    Some(last) if round.saturating_sub(*last) <= 2 => 25,
                    _ => 0,
                };
                let jitter = rng.next_bounded(8) as i64;
                let cost = meetings * 100 + recent_penalty + jitter;
                let edge = flow.add_edge(track_node(track), person_node(person), 1, cost);
                candidates[track].push((person, edge));
            }
        }

        if flow.min_cost_flow(source, sink, track_count).is_none() {
            return Err(round);
        }

        let mut person_of_track = vec![usize::MAX; track_count];
        for (track, candidates) in candidates.iter().enumerate() {
            for (person, edge) in candidates {
                if flow.graph[track_node(track)][*edge].cap == 0 {
                    person_of_track[track] = *person;
                    break;
                }
            }
        }
        for (track, person) in person_of_track.into_iter().enumerate() {
            if person == usize::MAX {
                return Err(round);
            }
            if round > 0 {
                let pair = ordered_pair(last_holder[track], person);
                *met.entry(pair).or_default() += 1;
                recent.insert(pair, round);
            }
            history[track].insert(person);
            last_holder[track] = person;
            assignments.push(ScheduledAssignment {
                round,
                track: specs[track].code.clone(),
                participant: people[person].clone(),
            });
        }
    }

    Ok(assignments)
}

fn ordered_pair(left: usize, right: usize) -> (usize, usize) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
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

/// Match each track to one person, excluding the `(person, track)` pairs in
/// `forbidden`. The result is a `person` index for each track. The function is
/// deterministic for the same input and seed. It is used by the group
/// scheduler, where a group must avoid own tracks and every track that each
/// person has seen.
pub(crate) fn match_round(
    people: usize,
    tracks: usize,
    forbidden: &HashSet<(usize, usize)>,
    seed: u64,
) -> Option<Vec<usize>> {
    if people != tracks || tracks == 0 {
        return None;
    }
    let mut rng = SplitMix64::new(seed);
    let adjacency: Vec<Vec<usize>> = (0..tracks)
        .map(|track| {
            let mut eligible: Vec<usize> = (0..people)
                .filter(|person| !forbidden.contains(&(*person, track)))
                .collect();
            rng.shuffle(&mut eligible);
            eligible
        })
        .collect();
    let mut order: Vec<usize> = (0..tracks).collect();
    rng.shuffle(&mut order);
    let mut match_person: Vec<Option<usize>> = vec![None; people];
    for track in order {
        let mut visited = vec![false; people];
        if !try_kuhn(track, &adjacency, &mut visited, &mut match_person) {
            return None;
        }
    }
    let mut person_of_track = vec![usize::MAX; tracks];
    for (person, track) in match_person.iter().enumerate() {
        if let Some(track) = track {
            person_of_track[*track] = person;
        }
    }
    if person_of_track.contains(&usize::MAX) {
        return None;
    }
    Some(person_of_track)
}

#[derive(Debug, Clone)]
struct FlowEdge {
    to: usize,
    reverse: usize,
    cap: i32,
    cost: i64,
}

#[derive(Debug, Clone)]
struct MinCostFlow {
    graph: Vec<Vec<FlowEdge>>,
}

impl MinCostFlow {
    fn new(nodes: usize) -> Self {
        Self {
            graph: vec![Vec::new(); nodes],
        }
    }

    fn add_edge(&mut self, from: usize, to: usize, cap: i32, cost: i64) -> usize {
        let forward = self.graph[from].len();
        let reverse = self.graph[to].len();
        self.graph[from].push(FlowEdge {
            to,
            reverse,
            cap,
            cost,
        });
        self.graph[to].push(FlowEdge {
            to: from,
            reverse: forward,
            cap: 0,
            cost: -cost,
        });
        forward
    }

    /// Successive shortest augmenting paths. Small graphs make SPFA simpler
    /// than maintaining potentials.
    fn min_cost_flow(&mut self, source: usize, sink: usize, amount: usize) -> Option<i64> {
        let mut flow = 0;
        let mut total_cost: i64 = 0;
        while flow < amount {
            let nodes = self.graph.len();
            let mut distance = vec![i64::MAX; nodes];
            let mut previous: Vec<(usize, usize)> = vec![(usize::MAX, usize::MAX); nodes];
            let mut in_queue = vec![false; nodes];
            let mut queue = VecDeque::new();
            distance[source] = 0;
            queue.push_back(source);
            in_queue[source] = true;
            while let Some(node) = queue.pop_front() {
                in_queue[node] = false;
                for (edge_index, edge) in self.graph[node].iter().enumerate() {
                    if edge.cap == 0 {
                        continue;
                    }
                    let next = edge.to;
                    let candidate = distance[node].saturating_add(edge.cost);
                    if candidate < distance[next] {
                        distance[next] = candidate;
                        previous[next] = (node, edge_index);
                        if !in_queue[next] {
                            in_queue[next] = true;
                            queue.push_back(next);
                        }
                    }
                }
            }
            if distance[sink] == i64::MAX {
                return None;
            }
            let mut node = sink;
            while node != source {
                let (from, edge_index) = previous[node];
                if from == usize::MAX {
                    return None;
                }
                let reverse = self.graph[from][edge_index].reverse;
                self.graph[from][edge_index].cap -= 1;
                self.graph[node][reverse].cap += 1;
                node = from;
            }
            total_cost = total_cost.saturating_add(distance[sink]);
            flow += 1;
        }
        Some(total_cost)
    }
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
    use proptest::prelude::*;

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
        assert_eq!(schedule.strategy, ScheduleStrategy::Balanced);
        assert!(!schedule.fallback);
    }

    #[test]
    fn every_strategy_is_valid() {
        for strategy in [
            ScheduleStrategy::Balanced,
            ScheduleStrategy::RandomMatching,
            ScheduleStrategy::Cyclic,
        ] {
            for (count, rounds) in [(3, 2), (6, 5), (8, 7), (12, 4)] {
                let people = people(count);
                let tracks = tracks(&people);
                let schedule =
                    Schedule::generate_with(&people, &tracks, rounds, 77, strategy).unwrap();
                assert_valid(&schedule, &people, &tracks);
                assert_eq!(schedule.strategy, strategy);
            }
        }
    }

    #[test]
    fn an_impossible_roster_is_refused_for_every_strategy() {
        let people = people(3);
        let tracks = vec![
            TrackSpec {
                code: TrackCode::new("T1"),
                owner: people[0].clone(),
            },
            TrackSpec {
                code: TrackCode::new("T2"),
                owner: people[0].clone(),
            },
            TrackSpec {
                code: TrackCode::new("T3"),
                owner: people[0].clone(),
            },
        ];
        for strategy in [
            ScheduleStrategy::Balanced,
            ScheduleStrategy::RandomMatching,
            ScheduleStrategy::Cyclic,
        ] {
            assert_eq!(
                Schedule::generate_with(&people, &tracks, 1, 3, strategy),
                Err(ScheduleError::Infeasible { round: 0 })
            );
        }
    }

    #[test]
    fn balanced_does_not_fail_when_random_matching_succeeds() {
        for people_count in 3..9 {
            let people = people(people_count);
            for track_count in 1..=people_count {
                let tracks: Vec<TrackSpec> = (0..track_count)
                    .map(|index| TrackSpec {
                        code: TrackCode::new(format!("T{index:02}")),
                        owner: people[index % people_count].clone(),
                    })
                    .collect();
                let rounds = people_count as u32 - 1;
                for seed in 0..10 {
                    let random = Schedule::generate_with(
                        &people,
                        &tracks,
                        rounds,
                        seed,
                        ScheduleStrategy::RandomMatching,
                    );
                    if random.is_ok() {
                        assert!(
                            Schedule::generate_with(
                                &people,
                                &tracks,
                                rounds,
                                seed,
                                ScheduleStrategy::Balanced,
                            )
                            .is_ok(),
                            "balanced failed for {people_count} people, {track_count} tracks, seed {seed}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_schedule_without_new_fields_still_decodes() {
        let schedule: Schedule =
            serde_json::from_str(r#"{"seed":1,"rounds":0,"assignments":[]}"#).unwrap();
        assert_eq!(schedule.strategy, ScheduleStrategy::Balanced);
        assert!(!schedule.fallback);
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
        for strategy in [
            ScheduleStrategy::Balanced,
            ScheduleStrategy::RandomMatching,
            ScheduleStrategy::Cyclic,
        ] {
            let first = Schedule::generate_with(&people, &tracks, 7, 123, strategy).unwrap();
            let second = Schedule::generate_with(&people, &tracks, 7, 123, strategy).unwrap();
            assert_eq!(first, second);
        }
    }

    #[test]
    fn input_order_does_not_change_the_schedule() {
        let people = people(6);
        let tracks = tracks(&people);
        for strategy in [
            ScheduleStrategy::Balanced,
            ScheduleStrategy::RandomMatching,
            ScheduleStrategy::Cyclic,
        ] {
            let first = Schedule::generate_with(&people, &tracks, 5, 9, strategy).unwrap();

            let mut reversed_people = people.clone();
            reversed_people.reverse();
            let mut reversed_tracks = tracks.clone();
            reversed_tracks.reverse();
            let second =
                Schedule::generate_with(&reversed_people, &reversed_tracks, 5, 9, strategy)
                    .unwrap();
            assert_eq!(first, second);
        }
    }

    #[test]
    fn two_seeds_produce_different_schedules() {
        let people = people(8);
        let tracks = tracks(&people);
        let first = Schedule::generate(&people, &tracks, 7, 1).unwrap();
        let second = Schedule::generate(&people, &tracks, 7, 2).unwrap();
        assert_ne!(first.assignments, second.assignments);
    }

    #[test]
    fn cyclic_falls_back_for_an_unbalanced_roster() {
        let people = people(6);
        let tracks = tracks(&people)[..3].to_vec();
        let schedule =
            Schedule::generate_with(&people, &tracks, 5, 7, ScheduleStrategy::Cyclic).unwrap();
        assert!(schedule.fallback);
        assert_valid(&schedule, &people, &tracks);
    }

    #[test]
    fn cyclic_is_not_the_bare_shift() {
        let people = people(8);
        let tracks = tracks(&people);
        let schedule =
            Schedule::generate_with(&people, &tracks, 7, 5, ScheduleStrategy::Cyclic).unwrap();
        let bare: Vec<usize> = schedule
            .assignments
            .iter()
            .filter(|assignment| assignment.round == 0)
            .map(|assignment| {
                let track_index = tracks
                    .iter()
                    .position(|track| track.code == assignment.track)
                    .unwrap();
                let person_index = people
                    .iter()
                    .position(|person| person == &assignment.participant)
                    .unwrap();
                (person_index + 8 - (track_index + 1) % 8) % 8
            })
            .collect();
        assert!(bare.iter().any(|offset| *offset != 0));
    }

    #[test]
    fn balanced_does_not_trail_random_on_the_public_report() {
        for (people_count, rounds, seeds) in [
            (4usize, 3u32, vec![0u32, 1, 2, 3, 4, 5, 185, 337]),
            (6usize, 5u32, vec![14, 70]),
        ] {
            let people = people(people_count);
            let tracks = tracks(&people);
            for seed in seeds {
                let balanced = Schedule::generate_with(
                    &people,
                    &tracks,
                    rounds,
                    u64::from(seed),
                    ScheduleStrategy::Balanced,
                )
                .unwrap();
                let random = Schedule::generate_with(
                    &people,
                    &tracks,
                    rounds,
                    u64::from(seed),
                    ScheduleStrategy::RandomMatching,
                )
                .unwrap();
                let balanced_report = balanced.balance();
                let random_report = random.balance();
                assert!(
                    balanced_report.repeated_handoffs <= random_report.repeated_handoffs,
                    "{people_count} people, seed {seed}: balanced {:?} vs random {:?}",
                    balanced_report,
                    random_report
                );
                assert!(
                    balanced_report.max_pair_meetings <= random_report.max_pair_meetings,
                    "{people_count} people, seed {seed}: balanced {:?} vs random {:?}",
                    balanced_report,
                    random_report
                );
            }
        }
    }

    #[test]
    fn balanced_keeps_repeated_handoffs_at_least_as_low_as_random() {
        let people = people(8);
        let tracks = tracks(&people);
        for seed in 1..=5 {
            let balanced =
                Schedule::generate_with(&people, &tracks, 7, seed, ScheduleStrategy::Balanced)
                    .unwrap();
            let random = Schedule::generate_with(
                &people,
                &tracks,
                7,
                seed,
                ScheduleStrategy::RandomMatching,
            )
            .unwrap();
            let balanced_report = balanced.balance();
            let random_report = random.balance();
            assert!(
                balanced_report.repeated_handoffs <= random_report.repeated_handoffs,
                "balanced {balanced_report:?} vs random {random_report:?} for seed {seed}"
            );
            assert!(
                balanced_report.max_pair_meetings <= random_report.max_pair_meetings,
                "balanced {balanced_report:?} vs random {random_report:?} for seed {seed}"
            );
        }
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

    #[test]
    fn strategies_parse_and_serialize() {
        for strategy in [
            ScheduleStrategy::Balanced,
            ScheduleStrategy::RandomMatching,
            ScheduleStrategy::Cyclic,
        ] {
            assert_eq!(ScheduleStrategy::parse(strategy.as_str()), Some(strategy));
            let json = serde_json::to_string(&strategy).unwrap();
            let decoded: ScheduleStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, strategy);
        }
        assert_eq!(ScheduleStrategy::parse("unknown"), None);
    }

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

        #[test]
        fn all_strategies_are_valid_for_generated_inputs(
            person_count in 2usize..10,
            requested_rounds in 0u32..12,
            seed in any::<u64>(),
            strategy_index in 0usize..3,
        ) {
            let strategies = [
                ScheduleStrategy::Balanced,
                ScheduleStrategy::RandomMatching,
                ScheduleStrategy::Cyclic,
            ];
            let people = people(person_count);
            let tracks = tracks(&people);
            let rounds = requested_rounds % person_count as u32;
            let schedule = Schedule::generate_with(
                &people,
                &tracks,
                rounds,
                seed,
                strategies[strategy_index],
            )
            .unwrap();
            assert_valid(&schedule, &people, &tracks);
        }
    }
}
