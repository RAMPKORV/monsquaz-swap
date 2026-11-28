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
    #[error("free track owner is in the roster: {0:?}")]
    FreeTrackOwnerInRoster(TrackCode),
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
        Self::generate_with_free_tracks(participants, tracks, &[], rounds, seed, strategy)
    }

    /// Generate a schedule that also carries free tracks. A free track is a
    /// seed whose owner left before the roster lock (D-051). The owner is not
    /// in the roster, so no own-track rule applies to a free track. When the
    /// tracks outnumber the people, every person still gets one track in each
    /// round, and the tracks that rest change from round to round.
    pub fn generate_with_free_tracks(
        participants: &[ParticipantId],
        tracks: &[TrackSpec],
        free_tracks: &[TrackCode],
        rounds: u32,
        seed: u64,
        strategy: ScheduleStrategy,
    ) -> Result<Self, ScheduleError> {
        for code in free_tracks {
            let Some(spec) = tracks.iter().find(|spec| &spec.code == code) else {
                return Err(ScheduleError::MissingOwner(code.clone()));
            };
            if participants.contains(&spec.owner) {
                // A free track belongs to a person who left before the roster
                // lock. A caller cannot use the free list to bypass the
                // own-track rule for a person in the roster.
                return Err(ScheduleError::FreeTrackOwnerInRoster(code.clone()));
            }
        }
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

        let owned_tracks = specs
            .iter()
            .filter(|spec| !free_tracks.contains(&spec.code))
            .count();
        if owned_tracks > people.len() {
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
                if free_tracks.contains(&spec.code) {
                    return Ok(NO_OWNER);
                }
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

/// The owner index of a free track. No person has this index.
const NO_OWNER: usize = usize::MAX;
/// The base cost that lets a rested track win the next round.
const REST_BASE_COST: i64 = 900;

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
        if owner == NO_OWNER || track_for_owner[owner].replace(track).is_some() {
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
        let mut unmatched = 0;
        for &track_index in &order {
            let mut visited = vec![false; people.len()];
            if !try_kuhn(track_index, &adjacency, &mut visited, &mut match_person) {
                unmatched += 1;
                // A track can rest only when the tracks outnumber the people.
                if specs.len() - unmatched < specs.len().min(people.len()) {
                    return Err(round);
                }
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
    let mut rests = vec![0u32; specs.len()];
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

        // With more tracks than people, one or more tracks rest in a round.
        // A track that rested more often gets a lower cost, so the rest
        // rotates. Without extra tracks the costs stay as before.
        let extra_tracks = track_count > person_count;
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
                let mut cost = meetings * 100 + recent_penalty + jitter;
                if extra_tracks {
                    cost += REST_BASE_COST - (i64::from(rests[track]) * 300).min(REST_BASE_COST);
                }
                let edge = flow.add_edge(track_node(track), person_node(person), 1, cost);
                candidates[track].push((person, edge));
            }
        }

        if flow
            .min_cost_flow(source, sink, track_count.min(person_count))
            .is_none()
        {
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
                if extra_tracks {
                    rests[track] += 1;
                    continue;
                }
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

    // scenario: Every participant receives one unseen track per round
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
    fn generate_with_requires_a_roster_and_tracks() {
        let people = people(2);
        let tracks = tracks(&people);
        assert_eq!(
            Schedule::generate_with(&[], &tracks, 1, 1, ScheduleStrategy::Balanced),
            Err(ScheduleError::EmptyRoster)
        );
        assert_eq!(
            Schedule::generate_with(&people, &[], 1, 1, ScheduleStrategy::Balanced),
            Err(ScheduleError::EmptyRoster)
        );
    }

    #[test]
    fn balance_counts_repeated_handoffs() {
        let assignment = |round, track: &str, participant: &str| ScheduledAssignment {
            round,
            track: TrackCode::new(track),
            participant: ParticipantId::new(participant),
        };
        let schedule = Schedule {
            seed: 0,
            rounds: 3,
            strategy: ScheduleStrategy::Balanced,
            fallback: false,
            assignments: vec![
                assignment(0, "T1", "P02"),
                assignment(1, "T1", "P01"),
                assignment(2, "T1", "P02"),
            ],
        };
        assert_eq!(
            schedule.balance(),
            BalanceReport {
                repeated_handoffs: 1,
                max_pair_meetings: 2,
                unique_pairs: 1,
            }
        );
    }

    #[test]
    fn an_equal_roster_uses_the_cyclic_square_without_fallback() {
        let people = people(6);
        let tracks = tracks(&people);
        let schedule =
            Schedule::generate_with(&people, &tracks, 5, 7, ScheduleStrategy::Cyclic).unwrap();
        assert!(!schedule.fallback);
    }

    #[test]
    fn balanced_keeps_the_first_best_attempt() {
        let people = people(6);
        let tracks = tracks(&people);
        let owners: Vec<usize> = (0..people.len()).collect();
        let mut saw_tie = false;
        for seed in 0u64..40 {
            let attempts: Vec<Vec<ScheduledAssignment>> = (0..BALANCED_ATTEMPTS)
                .filter_map(|attempt| {
                    let attempt_seed =
                        seed.wrapping_add(u64::from(attempt).wrapping_mul(SPLITMIX_GOLDEN));
                    attempt_balanced(&people, &tracks, &owners, 5, attempt_seed).ok()
                })
                .collect();
            let chosen = balanced_schedule(&people, &tracks, &owners, 5, seed).unwrap();
            let chosen_report = balance_of(&chosen);
            let best = attempts
                .iter()
                .map(|attempt| balance_of(attempt))
                .min_by_key(|report| (report.repeated_handoffs, report.max_pair_meetings))
                .unwrap();
            assert_eq!(
                (
                    chosen_report.repeated_handoffs,
                    chosen_report.max_pair_meetings
                ),
                (best.repeated_handoffs, best.max_pair_meetings),
                "seed {seed}"
            );
            let first = attempts
                .iter()
                .find(|attempt| {
                    let report = balance_of(attempt);
                    (report.repeated_handoffs, report.max_pair_meetings)
                        == (best.repeated_handoffs, best.max_pair_meetings)
                })
                .unwrap();
            assert_eq!(&chosen, first, "seed {seed}");
            if attempts
                .iter()
                .filter(|attempt| {
                    let report = balance_of(attempt);
                    (report.repeated_handoffs, report.max_pair_meetings)
                        == (best.repeated_handoffs, best.max_pair_meetings)
                })
                .count()
                > 1
            {
                saw_tie = true;
            }
        }
        assert!(saw_tie, "no tied attempts in the sample");
    }

    #[test]
    fn a_tie_between_random_and_balanced_keeps_balanced() {
        let mut saw_tie = false;
        for count in 3..=6 {
            let people = people(count);
            let tracks = tracks(&people);
            let owners: Vec<usize> = (0..count).collect();
            for seed in 0..200 {
                let balanced =
                    balanced_schedule(&people, &tracks, &owners, count as u32 - 1, seed).unwrap();
                let random =
                    random_schedule(&people, &tracks, &owners, count as u32 - 1, seed).unwrap();
                let balanced_report = balance_of(&balanced);
                let random_report = balance_of(&random);
                if (
                    random_report.repeated_handoffs,
                    random_report.max_pair_meetings,
                ) == (
                    balanced_report.repeated_handoffs,
                    balanced_report.max_pair_meetings,
                ) {
                    let chosen = Schedule::generate_with(
                        &people,
                        &tracks,
                        count as u32 - 1,
                        seed,
                        ScheduleStrategy::Balanced,
                    )
                    .unwrap();
                    assert!(!chosen.fallback, "count {count}, seed {seed}");
                    saw_tie = true;
                }
            }
        }
        assert!(saw_tie, "no balanced/random tie in the sample");
    }

    #[test]
    fn ordered_pair_is_canonical() {
        assert_eq!(ordered_pair(5, 2), (2, 5));
        assert_eq!(ordered_pair(2, 5), (2, 5));
        assert_eq!(ordered_pair(2, 2), (2, 2));
    }

    #[test]
    fn match_round_refuses_a_mismatched_shape() {
        let forbidden = HashSet::new();
        assert_eq!(match_round(2, 3, &forbidden, 1), None);
        assert_eq!(match_round(0, 0, &forbidden, 1), None);
        assert!(match_round(3, 3, &forbidden, 1).is_some());
    }

    #[test]
    fn bounded_values_stay_in_range() {
        let mut rng = SplitMix64::new(42);
        for upper in 2..=17 {
            for _ in 0..100 {
                assert!(rng.next_bounded(upper) < upper);
            }
        }
    }

    #[test]
    fn shuffle_handles_small_slices() {
        let mut two = ["a", "b"];
        SplitMix64::new(2).shuffle(&mut two);
        assert_eq!(two, ["b", "a"]);
        let mut two = ["a", "b"];
        SplitMix64::new(0).shuffle(&mut two);
        assert_eq!(two, ["a", "b"]);
        let mut three = ["a", "b", "c"];
        SplitMix64::new(1).shuffle(&mut three);
        assert_eq!(three, ["a", "b", "c"]);
    }

    #[test]
    fn a_known_seed_produces_a_known_schedule() {
        let people = people(4);
        let tracks = tracks(&people);
        let schedule = Schedule::generate(&people, &tracks, 3, 42).unwrap();
        assert!(!schedule.fallback);
        let actual: Vec<(u32, &str, &str)> = schedule
            .assignments
            .iter()
            .map(|assignment| {
                (
                    assignment.round,
                    assignment.track.0.as_str(),
                    assignment.participant.0.as_str(),
                )
            })
            .collect();
        let expected = vec![
            (0, "T00", "P01"),
            (0, "T01", "P00"),
            (0, "T02", "P03"),
            (0, "T03", "P02"),
            (1, "T00", "P03"),
            (1, "T01", "P02"),
            (1, "T02", "P00"),
            (1, "T03", "P01"),
            (2, "T00", "P02"),
            (2, "T01", "P03"),
            (2, "T02", "P01"),
            (2, "T03", "P00"),
        ];
        assert_eq!(actual, expected);
    }

    fn schedule_fingerprint(schedule: &Schedule) -> u64 {
        let mut hash = 0xCBF2_9CE4_8422_2325_u64;
        let mut eat = |bytes: &[u8]| {
            for byte in bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
            }
        };
        for assignment in &schedule.assignments {
            eat(&assignment.round.to_le_bytes());
            eat(assignment.track.0.as_bytes());
            eat(b":");
            eat(assignment.participant.0.as_bytes());
            eat(b";");
        }
        hash
    }

    #[test]
    fn known_seeds_produce_known_schedules() {
        // A deterministic schedule is part of the contract. The fingerprints
        // protect the cost and recent-pair terms that no report-only test sees.
        let cases = [
            (5usize, 4u32, 1u64, 0x6b6d1c414ea88d75),
            (6, 5, 7, 0x1e34d1469cc6cfad),
            (7, 6, 42, 0x9abd566c8d2c6190),
            (8, 7, 42, 0x7b80ef698f9a9485),
            (8, 7, 123, 0xea38576fbb62f795),
            (8, 7, 1, 0xcbe8c0b0e464028d),
        ];
        for (count, rounds, seed, expected) in cases {
            let people = people(count);
            let tracks = tracks(&people);
            let schedule = Schedule::generate(&people, &tracks, rounds, seed).unwrap();
            assert_eq!(
                schedule_fingerprint(&schedule),
                expected,
                "{count} people, {rounds} rounds, seed {seed}"
            );
        }
    }

    #[test]
    fn known_cyclic_seeds_produce_known_schedules() {
        let people = people(8);
        let tracks = tracks(&people);
        for (seed, expected) in [(5u64, 0xeaa17af2c39b6d3d), (42, 0xfc6c8fc33f994105)] {
            let schedule =
                Schedule::generate_with(&people, &tracks, 7, seed, ScheduleStrategy::Cyclic)
                    .unwrap();
            assert_eq!(schedule_fingerprint(&schedule), expected, "seed {seed}");
        }
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

    /// D-051: a seed whose owner left before the roster lock keeps
    /// circulating. The roster has one track more than people, so one track
    /// rests in each round. Every person still gets one track per round.
    #[test]
    fn a_seed_without_its_owner_circulates() {
        for strategy in [
            ScheduleStrategy::Balanced,
            ScheduleStrategy::Cyclic,
            ScheduleStrategy::RandomMatching,
        ] {
            for seed in 0..20u64 {
                let roster = people(5);
                let mut specs = tracks(&roster);
                let orphan = TrackSpec {
                    code: TrackCode::new("T99"),
                    owner: ParticipantId::new("left"),
                };
                specs.push(orphan.clone());
                let schedule = Schedule::generate_with_free_tracks(
                    &roster,
                    &specs,
                    std::slice::from_ref(&orphan.code),
                    4,
                    seed,
                    strategy,
                )
                .unwrap();
                let mut pairs = HashSet::new();
                for round in 0..4 {
                    let rows: Vec<_> = schedule
                        .assignments
                        .iter()
                        .filter(|assignment| assignment.round == round)
                        .collect();
                    assert_eq!(rows.len(), 5, "every person works in round {round}");
                    let people_in_round: HashSet<_> =
                        rows.iter().map(|row| row.participant.clone()).collect();
                    assert_eq!(people_in_round.len(), 5);
                    let tracks_in_round: HashSet<_> =
                        rows.iter().map(|row| row.track.clone()).collect();
                    assert_eq!(tracks_in_round.len(), 5);
                    for row in rows {
                        let spec = specs.iter().find(|spec| spec.code == row.track).unwrap();
                        assert_ne!(spec.owner, row.participant, "own track");
                        assert!(pairs.insert((row.track.clone(), row.participant.clone())));
                    }
                }
                assert!(
                    schedule
                        .assignments
                        .iter()
                        .any(|assignment| assignment.track == orphan.code),
                    "the seed without its owner circulates"
                );
            }
        }
    }

    #[test]
    fn a_free_track_must_name_a_known_track() {
        let roster = people(3);
        let specs = tracks(&roster);
        assert_eq!(
            Schedule::generate_with_free_tracks(
                &roster,
                &specs,
                &[TrackCode::new("nope")],
                2,
                1,
                ScheduleStrategy::Balanced,
            ),
            Err(ScheduleError::MissingOwner(TrackCode::new("nope")))
        );
    }

    #[test]
    fn a_free_track_owner_in_the_roster_is_refused() {
        let roster = people(3);
        let specs = tracks(&roster);
        assert_eq!(
            Schedule::generate_with_free_tracks(
                &roster,
                &specs,
                &[specs[0].code.clone()],
                2,
                1,
                ScheduleStrategy::Balanced,
            ),
            Err(ScheduleError::FreeTrackOwnerInRoster(specs[0].code.clone()))
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
