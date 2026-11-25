//! Deterministic classic-event simulation.
//!
//! The harness drives the real `swap-engine` against an in-memory database, a
//! temporary content store, and the fake transport-free effect list. It checks
//! the scheduler, seen ledger, file versions, reveal, and anonymity rules after
//! every run.

use std::collections::{HashMap, HashSet};

use swap_core::{AssignmentStatus, EventState};
use swap_db::{AssignmentRepo, Db, DbError, EventRepo, FileRepo, SeenRepo, TrackRepo};
use swap_engine::{
    Effect, EngineError, EventSettings, JoinRequest, PassthroughPreparer, SubmitRequest, claim,
    create_event, expire_offer, generate_schedule, join_with_seed, lock_roster, open_round, reveal,
    skip, start_event, submit,
};
use swap_files::BlobStore;
use thiserror::Error;

const BASE_TIME: i64 = 1_000_000;
const ROUND_SPAN: i64 = 200_000;
const CLAIM_WINDOW: i64 = 86_400;
const EVENT_ID: &str = "sim-event";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    Reliable,
    Expires,
    SkipBeforeClaim,
    SkipAfterClaim,
}

impl Profile {
    pub fn from_index(index: usize) -> Self {
        match index % 4 {
            0 => Self::Reliable,
            1 => Self::Expires,
            2 => Self::SkipBeforeClaim,
            _ => Self::SkipAfterClaim,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub participants: usize,
    pub rounds: Option<u32>,
    pub seed: u64,
    pub profiles: Vec<Profile>,
}

impl SimulationConfig {
    pub fn new(participants: usize) -> Self {
        Self {
            participants,
            rounds: None,
            seed: 42,
            profiles: Vec::new(),
        }
    }

    fn profile_for(&self, index: usize) -> Profile {
        if self.profiles.is_empty() {
            Profile::from_index(index)
        } else {
            self.profiles[index % self.profiles.len()]
        }
    }
}

#[derive(Debug, Error)]
pub enum SimError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("invalid simulation config: {0}")]
    Config(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimCheck {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectSummary {
    pub dms: usize,
    pub offers: usize,
    pub files: usize,
    pub channels: usize,
}

impl EffectSummary {
    pub fn total(&self) -> usize {
        self.dms + self.offers + self.files + self.channels
    }
}

#[derive(Debug, Clone)]
pub struct SimulationReport {
    pub state: EventState,
    pub participants: usize,
    pub rounds: u32,
    pub tracks: usize,
    pub assignments: usize,
    pub seen_pairs: usize,
    pub file_versions: usize,
    pub revealed_tracks: usize,
    pub effects: EffectSummary,
    pub effect_log: Vec<Effect>,
    pub checks: Vec<SimCheck>,
}

impl SimulationReport {
    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }
}

pub fn run(config: &SimulationConfig) -> Result<SimulationReport, SimError> {
    if config.participants < 2 {
        return Err(SimError::Config(
            "the simulation needs at least two participants".to_string(),
        ));
    }
    if config.rounds == Some(0) {
        return Err(SimError::Config("rounds must be positive".to_string()));
    }

    let directory = tempfile::tempdir()?;
    let store = BlobStore::new(directory.path().join("blobs"));
    let db = Db::open_in_memory()?;
    let settings = EventSettings {
        rounds: config.rounds,
        ..EventSettings::default()
    };
    create_event(
        &db,
        EVENT_ID,
        "Simulation",
        config.seed,
        &settings,
        BASE_TIME,
    )?;

    let mut effects = Vec::new();
    let mut user_index = HashMap::new();
    for index in 0..config.participants {
        let user_id = format!("user-{index:02}");
        let display_name = format!("Person {index}");
        user_index.insert(user_id.clone(), index);
        let bytes = seed_bytes(config.seed, index);
        let outcome = join_with_seed(
            &db,
            &store,
            &PassthroughPreparer,
            &JoinRequest {
                event_id: EVENT_ID,
                user_id: &user_id,
                display_name: &display_name,
                timezone: "Europe/Stockholm",
                max_handoffs: 3,
                filename: "seed.it",
                bytes: &bytes,
            },
            BASE_TIME,
        )?;
        effects.extend(outcome.effects);
    }

    lock_roster(&db, EVENT_ID, BASE_TIME + 1)?;
    let schedule = generate_schedule(&db, EVENT_ID, BASE_TIME + 2)?;
    start_event(&db, EVENT_ID, BASE_TIME + 3)?;
    let plan = schedule.assignments.clone();
    let rounds = schedule.rounds;

    let owner_by_track: HashMap<String, String> = TrackRepo::list_for_event(db.conn(), EVENT_ID)?
        .into_iter()
        .map(|track| (track.code, track.owner_id))
        .collect();
    let track_codes: HashSet<String> = owner_by_track.keys().cloned().collect();
    let display_names: Vec<String> = (0..config.participants)
        .flat_map(|index| [format!("Person {index}"), format!("user-{index:02}")])
        .collect();

    let mut checks = Vec::new();
    let mut round_people: Vec<HashSet<String>> = Vec::new();
    let mut round_tracks: Vec<HashSet<String>> = Vec::new();

    for round in 0..rounds {
        let now = BASE_TIME + 10 + i64::from(round) * ROUND_SPAN;
        let opened = open_round(&db, EVENT_ID, round, now)?;
        effects.extend(opened.effects.clone());

        for (user_id, assignment_id, _track) in offers(&opened.effects) {
            let index = user_index[&user_id];
            match config.profile_for(index) {
                Profile::Reliable => {
                    let claimed = claim(&db, &store, EVENT_ID, &assignment_id, &user_id, now + 10)?;
                    effects.extend(claimed.effects);
                    let bytes = seed_bytes(
                        config.seed ^ u64::from(round + 1),
                        index + (round as usize + 1) * 100,
                    );
                    let submitted = submit(
                        &db,
                        &store,
                        &PassthroughPreparer,
                        &SubmitRequest {
                            event_id: EVENT_ID,
                            user_id: &user_id,
                            filename: "edit.it",
                            bytes: &bytes,
                        },
                        now + 20,
                    )?;
                    effects.extend(submitted.effects);
                }
                Profile::Expires => {
                    expire_offer(&db, EVENT_ID, &assignment_id, now + CLAIM_WINDOW + 1)?;
                }
                Profile::SkipBeforeClaim => {
                    let skipped = skip(&db, EVENT_ID, &user_id, now + 10)?;
                    effects.extend(skipped.effects);
                }
                Profile::SkipAfterClaim => {
                    let claimed = claim(&db, &store, EVENT_ID, &assignment_id, &user_id, now + 10)?;
                    effects.extend(claimed.effects);
                    let skipped = skip(&db, EVENT_ID, &user_id, now + 20)?;
                    effects.extend(skipped.effects);
                }
            }
        }

        let active = AssignmentRepo::list_active(db.conn(), EVENT_ID)?;
        checks.push(check(
            "round.no_active_assignments",
            active.is_empty(),
            format!("round {round} left {} active assignments", active.len()),
        ));

        let mut people = HashSet::new();
        let mut tracks = HashSet::new();
        for assignment in plan.iter().filter(|assignment| assignment.round == round) {
            people.insert(assignment.participant_id.clone());
            tracks.insert(assignment.track_code.clone());
        }
        round_people.push(people);
        round_tracks.push(tracks);
    }

    let reveal_time = BASE_TIME + 10 + i64::from(rounds) * ROUND_SPAN;
    let revealed = reveal(&db, EVENT_ID, reveal_time)?;
    let state = EventRepo::get(db.conn(), EVENT_ID)?
        .map(|event| event.state)
        .unwrap_or(EventState::Draft);

    let tracks = TrackRepo::list_for_event(db.conn(), EVENT_ID)?;
    let assignments = AssignmentRepo::list_for_event(db.conn(), EVENT_ID)?;
    let seen = SeenRepo::list_for_event(db.conn(), EVENT_ID)?;
    let mut file_versions = 0;
    let mut files_ok = true;
    let mut files_detail = String::new();
    for track in &tracks {
        let versions = FileRepo::list_for_track(db.conn(), EVENT_ID, &track.code)?;
        file_versions += versions.len();
        if versions.is_empty() {
            files_ok = false;
            files_detail = format!("track {} has no file", track.code);
        }
        for file in &versions {
            if file.original_sha256.is_none() || file.storage_path.is_empty() {
                files_ok = false;
                files_detail = format!("track {} has an incomplete file record", track.code);
            }
        }
    }

    let mut summary = EffectSummary::default();
    for effect in &effects {
        match effect {
            Effect::Dm { .. } => summary.dms += 1,
            Effect::Offer { .. } => summary.offers += 1,
            Effect::TrackFile { .. } => summary.files += 1,
            Effect::Channel { .. } => summary.channels += 1,
        }
    }

    let mut pair_set = HashSet::new();
    let mut own_track_ok = true;
    let mut own_track_detail = String::new();
    let mut plan_round_ok = true;
    for assignment in &plan {
        if let Some(owner) = owner_by_track.get(&assignment.track_code)
            && owner == &assignment.participant_id
        {
            own_track_ok = false;
            own_track_detail = format!(
                "{} owns {}",
                assignment.participant_id, assignment.track_code
            );
        }
        if !pair_set.insert((
            assignment.track_code.clone(),
            assignment.participant_id.clone(),
        )) {
            plan_round_ok = false;
        }
    }
    checks.push(check(
        "scheduler.no_own_track",
        own_track_ok,
        own_track_detail,
    ));
    checks.push(check(
        "scheduler.no_repeat_pair",
        pair_set.len() == plan.len(),
        format!(
            "{} unique pairs for {} plan assignments",
            pair_set.len(),
            plan.len()
        ),
    ));
    checks.push(check(
        "scheduler.rounds_are_unique",
        plan_round_ok
            && round_people.iter().all(|set| set.len() == tracks.len())
            && round_tracks.iter().all(|set| set.len() == tracks.len()),
        "each round has one person and one track per entry".to_string(),
    ));

    let plan_set: HashSet<(&str, &str, u32)> = plan
        .iter()
        .map(|assignment| {
            (
                assignment.track_code.as_str(),
                assignment.participant_id.as_str(),
                assignment.round,
            )
        })
        .collect();
    let actual_set: HashSet<(&str, &str, u32)> = assignments
        .iter()
        .map(|assignment| {
            (
                assignment.track_code.as_str(),
                assignment.participant_id.as_str(),
                assignment.round,
            )
        })
        .collect();
    checks.push(check(
        "scheduler.plan_matches_assignments",
        plan_set == actual_set,
        format!(
            "{} plan pairs, {} assignment rows",
            plan_set.len(),
            actual_set.len()
        ),
    ));

    let mut seen_ok = true;
    let mut seen_detail = String::new();
    let mut expected_seen = 0;
    for assignment in &assignments {
        let expected = matches!(
            assignment.status,
            AssignmentStatus::Claimed
                | AssignmentStatus::Submitted
                | AssignmentStatus::SkippedAfterClaim
        );
        let actual = SeenRepo::has(
            db.conn(),
            EVENT_ID,
            &assignment.track_code,
            &assignment.participant_id,
        )?;
        if expected != actual {
            seen_ok = false;
            seen_detail = format!(
                "{} on {}: expected seen={expected}, got {actual}",
                assignment.participant_id, assignment.track_code
            );
        }
        if expected {
            expected_seen += 1;
        }
    }
    checks.push(check(
        "seen.matches_claim_rule",
        seen_ok && seen.len() == expected_seen,
        format!(
            "{} seen rows, {} expected; {seen_detail}",
            seen.len(),
            expected_seen
        ),
    ));
    checks.push(check(
        "files.every_track_has_a_version",
        files_ok,
        files_detail,
    ));

    let submitted_count = assignments
        .iter()
        .filter(|assignment| assignment.status == AssignmentStatus::Submitted)
        .count();
    checks.push(check(
        "files.all_versions_retained",
        file_versions == tracks.len() + submitted_count,
        format!(
            "{file_versions} files for {} seeds and {submitted_count} submissions",
            tracks.len()
        ),
    ));

    let claim_count = assignments
        .iter()
        .filter(|assignment| {
            matches!(
                assignment.status,
                AssignmentStatus::Submitted | AssignmentStatus::SkippedAfterClaim
            )
        })
        .count();
    checks.push(check(
        "effects.one_file_per_claim",
        summary.files == claim_count,
        format!("{} file effects for {claim_count} claims", summary.files),
    ));
    checks.push(check(
        "effects.one_offer_per_assignment",
        summary.offers == plan.len(),
        format!(
            "{} offer effects for {} plan assignments",
            summary.offers,
            plan.len()
        ),
    ));

    let (opaque_ok, opaque_detail) = audit_outbound(&effects, &track_codes, &display_names);
    checks.push(check("anonymity.opaque_outbound", opaque_ok, opaque_detail));

    checks.push(check(
        "event.revealed",
        state == EventState::Reveal,
        format!("state is {state:?}"),
    ));
    checks.push(check(
        "reveal.complete",
        revealed.tracks.len() == tracks.len(),
        format!(
            "{} revealed tracks for {} tracks",
            revealed.tracks.len(),
            tracks.len()
        ),
    ));

    Ok(SimulationReport {
        state,
        participants: config.participants,
        rounds,
        tracks: tracks.len(),
        assignments: assignments.len(),
        seen_pairs: seen.len(),
        file_versions,
        revealed_tracks: revealed.tracks.len(),
        effects: summary,
        effect_log: effects,
        checks,
    })
}

fn offers(effects: &[Effect]) -> Vec<(String, String, String)> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Offer {
                user_id,
                assignment_id,
                track_code,
                ..
            } => Some((user_id.clone(), assignment_id.clone(), track_code.clone())),
            _ => None,
        })
        .collect()
}

fn audit_outbound(
    effects: &[Effect],
    track_codes: &HashSet<String>,
    display_names: &[String],
) -> (bool, String) {
    for effect in effects {
        match effect {
            Effect::TrackFile {
                filename, caption, ..
            } => {
                let Some((stem, extension)) = filename.split_once('.') else {
                    return (false, format!("file {filename} has no extension"));
                };
                if !track_codes.contains(stem) || extension != "it" {
                    return (false, format!("file {filename} is not opaque"));
                }
                if contains_name(filename, display_names) || contains_name(caption, display_names) {
                    return (false, format!("file {filename} leaks a name"));
                }
            }
            Effect::Dm { content, .. } | Effect::Channel { content, .. } => {
                if contains_name(content, display_names) {
                    return (false, "a message leaks a participant name".to_string());
                }
            }
            Effect::Offer { .. } => {}
        }
    }
    (true, String::new())
}

fn contains_name(value: &str, names: &[String]) -> bool {
    names.iter().any(|name| value.contains(name))
}

fn check(name: &'static str, passed: bool, detail: String) -> SimCheck {
    SimCheck {
        name,
        passed,
        detail,
    }
}

fn seed_bytes(seed: u64, salt: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; 64];
    bytes[..4].copy_from_slice(b"IMPM");
    let mixed = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(salt as u64);
    for (index, byte) in bytes[4..].iter_mut().enumerate() {
        *byte = (mixed.rotate_left(index as u32 % 63) & 0xFF) as u8;
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_simulation_passes_every_check() {
        let report = run(&SimulationConfig::new(8)).unwrap();
        assert!(report.all_passed(), "{:?}", report.checks);
        assert_eq!(report.rounds, 7);
        assert_eq!(report.revealed_tracks, 8);
        assert!(report.effects.offers >= 56);
        assert!(report.effects.files >= 1);
    }
}
