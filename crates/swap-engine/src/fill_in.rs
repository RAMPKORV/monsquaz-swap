//! Fill-ins for a stranded classic track: the eligible people, the host
//! appointment, and the due time of a fill-in offer.

use std::collections::{HashMap, HashSet};

use swap_core::{AssignmentStatus, Membership, Readiness, Role, RoundStatus, TrackState};
use swap_db::{
    AssignmentRecord, AssignmentRepo, AuditRepo, Connection, Db, ParticipantRecord,
    ParticipantRepo, PlanRepo, RoundRepo, SeenRepo, TrackRecord, TrackRepo,
};

use crate::delivery::enqueue_effect;
use crate::effects::Effect;
use crate::engine::{
    EngineError, begin, commit, insert_offer, load_event, move_track, parse_settings,
    require_open_event,
};
use crate::ids::assignment_id;

/// D-050 tiers. Tier 1 is a dedicated fill-in. Tier 2 is a seeded
/// participant who takes an extra turn when no tier-1 person is eligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FillInTier {
    Volunteer,
    ExtraTurn,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillInCandidate {
    pub tier: FillInTier,
    /// Submitted edits so far, in all roles.
    pub edits: usize,
    pub participant_id: String,
    pub display_name: String,
    pub seen: usize,
    pub cap: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillInOutcome {
    pub assignment_id: String,
    pub track_code: String,
    pub due_at: i64,
}

/// One stranded slot: a track in one classic round, with the facts that
/// decide who can take it (D-050).
pub(crate) struct Slot {
    event_id: String,
    track: TrackRecord,
    /// The seeded participants that the current plan gives this track in
    /// any round.
    planned: HashSet<String>,
    /// The people who submitted their own planned turn in this round.
    finished_turn: HashSet<String>,
    /// The people who already took a tier-2 fill-in turn in this round
    /// (D-063). A pre-claim skip does not count.
    extra_turn_taken: HashSet<String>,
    active: HashSet<String>,
    edits: HashMap<String, usize>,
}

impl Slot {
    pub(crate) fn load(
        conn: &Connection,
        event_id: &str,
        track_code: &str,
        index: u32,
    ) -> Result<Self, EngineError> {
        let track = TrackRepo::get(conn, event_id, track_code)?
            .ok_or_else(|| EngineError::TrackNotFound(track_code.to_string()))?;
        let planned = match PlanRepo::latest(conn, event_id)? {
            Some(plan) => PlanRepo::list_assignments(conn, event_id, plan.version)?
                .into_iter()
                .filter(|row| row.track_code == track_code)
                .map(|row| row.participant_id)
                .collect(),
            None => HashSet::new(),
        };
        let assignments = AssignmentRepo::list_for_event(conn, event_id)?;
        let mut finished_turn = HashSet::new();
        let mut extra_turn_taken = HashSet::new();
        let mut active = HashSet::new();
        let mut edits: HashMap<String, usize> = HashMap::new();
        for assignment in &assignments {
            let in_round = assignment.round == index;
            let fill_in = in_round && is_fill_in_turn(event_id, assignment);
            match assignment.status {
                AssignmentStatus::Submitted => {
                    *edits.entry(assignment.participant_id.clone()).or_default() += 1;
                    if in_round {
                        if fill_in {
                            extra_turn_taken.insert(assignment.participant_id.clone());
                        } else {
                            finished_turn.insert(assignment.participant_id.clone());
                        }
                    }
                }
                AssignmentStatus::Claimed => {
                    active.insert(assignment.participant_id.clone());
                    if fill_in {
                        extra_turn_taken.insert(assignment.participant_id.clone());
                    }
                }
                AssignmentStatus::SkippedAfterClaim if fill_in => {
                    extra_turn_taken.insert(assignment.participant_id.clone());
                }
                AssignmentStatus::Offered | AssignmentStatus::Overdue => {
                    active.insert(assignment.participant_id.clone());
                }
                _ => {}
            }
        }
        Ok(Self {
            event_id: event_id.to_string(),
            track,
            planned,
            finished_turn,
            extra_turn_taken,
            active,
            edits,
        })
    }

    /// The tier of one person for this slot, or the reason why the person
    /// cannot take it. Both tiers need an active, ready person with no active
    /// assignment, who does not own the track and has never seen it.
    fn tier(
        &self,
        conn: &Connection,
        participant: &ParticipantRecord,
    ) -> Result<Result<FillInTier, &'static str>, EngineError> {
        let user_id = participant.user_id.as_str();
        if participant.membership != Membership::Active {
            return Ok(Err("the person is not active"));
        }
        if participant.readiness != Readiness::Ready {
            return Ok(Err("the person is not ready"));
        }
        if self.active.contains(user_id) {
            return Ok(Err("the person already holds a track"));
        }
        if self.track.owner_id == user_id {
            return Ok(Err("the person owns the track"));
        }
        if SeenRepo::has(conn, &self.event_id, &self.track.code, user_id)? {
            return Ok(Err("the person has seen the track"));
        }
        match participant.role {
            Role::FillIn => {
                let seen = SeenRepo::list_for_participant(conn, &self.event_id, user_id)?.len();
                if participant.max_handoffs > 0 && seen as u32 >= participant.max_handoffs {
                    return Ok(Err("the person reached the handoff cap"));
                }
                Ok(Ok(FillInTier::Volunteer))
            }
            Role::Participant => {
                // An extra turn does not count toward the handoff cap.
                if !self.finished_turn.contains(user_id) {
                    return Ok(Err(
                        "the person did not submit their own turn in this round",
                    ));
                }
                if self.extra_turn_taken.contains(user_id) {
                    return Ok(Err("the person already took a fill-in turn in this round"));
                }
                if self.planned.contains(user_id) {
                    return Ok(Err("the plan gives the person this track"));
                }
                Ok(Ok(FillInTier::ExtraTurn))
            }
        }
    }

    /// Every eligible person in rank order across both tiers. The chain uses
    /// this list so it can fall from tier 1 to tier 2 inside one offer budget
    /// (D-064). Inside a tier: fewest edits, then a stable hash.
    pub(crate) fn ranked(&self, conn: &Connection) -> Result<Vec<FillInCandidate>, EngineError> {
        let mut candidates = Vec::new();
        for participant in ParticipantRepo::list_for_event(conn, &self.event_id)? {
            let Ok(tier) = self.tier(conn, &participant)? else {
                continue;
            };
            let seen =
                SeenRepo::list_for_participant(conn, &self.event_id, &participant.user_id)?.len();
            candidates.push(FillInCandidate {
                tier,
                edits: self.edits.get(&participant.user_id).copied().unwrap_or(0),
                participant_id: participant.user_id,
                display_name: participant.display_name,
                seen,
                cap: participant.max_handoffs,
            });
        }
        candidates.sort_by_cached_key(|candidate| {
            (
                candidate.tier,
                candidate.edits,
                swap_files::sha256_hex(
                    format!(
                        "fill-in-rank:{}:{}:{}",
                        self.event_id, self.track.code, candidate.participant_id
                    )
                    .as_bytes(),
                ),
            )
        });
        Ok(candidates)
    }

    /// The eligible people in rank order. Tier 2 shows only when no tier-1
    /// person is eligible.
    pub(crate) fn candidates(
        &self,
        conn: &Connection,
    ) -> Result<Vec<FillInCandidate>, EngineError> {
        let mut candidates = self.ranked(conn)?;
        let best = candidates.iter().map(|candidate| candidate.tier).min();
        candidates.retain(|candidate| Some(candidate.tier) == best);
        Ok(candidates)
    }
}

/// A planned turn has the id `assignment_id(event, round, track)`. Any other
/// assignment is a fill-in turn.
pub(crate) fn is_fill_in_turn(event_id: &str, assignment: &AssignmentRecord) -> bool {
    assignment.id != assignment_id(event_id, assignment.round, &assignment.track_code)
}

pub(crate) fn open_round_deadline(
    conn: &Connection,
    event_id: &str,
    round_no: u32,
) -> Result<i64, EngineError> {
    let round = RoundRepo::get(conn, event_id, round_no)?
        .ok_or(EngineError::RoundNotOpen { round: round_no })?;
    if round.status != RoundStatus::Open {
        return Err(EngineError::RoundNotOpen { round: round_no });
    }
    round
        .deadline_at
        .ok_or(EngineError::RoundDeadlineMissing { round: round_no })
}

/// The default fill-in due time: the later of the round deadline and the
/// fill-in window from now.
pub(crate) fn default_due_at(round_deadline: i64, fill_in_window_secs: i64, now: i64) -> i64 {
    round_deadline.max(now + fill_in_window_secs)
}

/// List the people who can take a stranded track in an open round, in rank
/// order (D-050).
pub fn fill_in_candidates(
    db: &Db,
    event_id: &str,
    track_code: &str,
    round_no: u32,
) -> Result<Vec<FillInCandidate>, EngineError> {
    let event = load_event(db, event_id)?;
    require_open_event(&event, "fill-in candidates")?;
    if round_no < 2 {
        return Err(EngineError::RoundNotOpen { round: round_no });
    }
    open_round_deadline(db.conn(), event_id, round_no)?;
    Slot::load(db.conn(), event_id, track_code, round_no - 2)?.candidates(db.conn())
}

/// Assign a stranded track to one eligible person in an open round. The host
/// can set the due time; the default is the later of the round deadline and
/// the fill-in window from now. A due time in the past is refused.
pub fn assign_fill_in(
    db: &Db,
    event_id: &str,
    track_code: &str,
    round_no: u32,
    user_id: &str,
    due_at: Option<i64>,
    now: i64,
) -> Result<FillInOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_open_event(&event, "fill-in assignment")?;
    let settings = parse_settings(&event)?;
    if round_no < 2 {
        return Err(EngineError::RoundNotOpen { round: round_no });
    }
    if due_at.is_some_and(|due_at| due_at <= now) {
        return Err(EngineError::FillInDueInPast);
    }
    if due_at.is_some_and(|due_at| due_at > now + MAX_FILL_IN_DUE_SECS) {
        return Err(EngineError::FillInDueTooFar);
    }
    let transaction = begin(db)?;
    let deadline = open_round_deadline(&transaction, event_id, round_no)?;
    let due_at =
        due_at.unwrap_or_else(|| default_due_at(deadline, settings.fill_in_window_secs, now));
    if AssignmentRepo::list_active(&transaction, event_id)?
        .iter()
        .any(|assignment| assignment.track_code == track_code)
    {
        return Err(EngineError::FillInNotEligible(
            "the track has an active holder".to_string(),
        ));
    }
    ensure_stranded(&transaction, event_id, track_code, round_no - 2)?;
    let participant = ParticipantRepo::get(&transaction, event_id, user_id)?
        .ok_or_else(|| EngineError::ParticipantNotFound(user_id.to_string()))?;
    let slot = Slot::load(&transaction, event_id, track_code, round_no - 2)?;
    slot.tier(&transaction, &participant)?
        .map_err(|reason| EngineError::FillInNotEligible(reason.to_string()))?;
    if !slot
        .candidates(&transaction)?
        .iter()
        .any(|candidate| candidate.participant_id == user_id)
    {
        return Err(EngineError::FillInNotEligible(
            "a dedicated fill-in is eligible for the track".to_string(),
        ));
    }
    let index = round_no - 2;
    crate::fill_in_chain::host_took_over(&transaction, event_id, track_code, index, now)?;
    let id = offer_fill_in(
        &transaction,
        event_id,
        track_code,
        index,
        user_id,
        due_at,
        due_at,
        "Host",
        now,
    )?;
    commit(transaction)?;
    Ok(FillInOutcome {
        assignment_id: id,
        track_code: track_code.to_string(),
        due_at,
    })
}

/// The longest due time that the host can set for a fill-in, in seconds. It
/// matches the longest fill-in window (14 days).
pub const MAX_FILL_IN_DUE_SECS: i64 = 14 * 24 * 60 * 60;

/// Refuse a fill-in for a cell that is not stranded. The planned holder must
/// have skipped, expired, or never received the cell, and the track must not
/// already have a submitted turn in the round (MSW-084).
pub(crate) fn ensure_stranded(
    conn: &Connection,
    event_id: &str,
    track_code: &str,
    index: u32,
) -> Result<(), EngineError> {
    let rows: Vec<AssignmentRecord> = AssignmentRepo::list_for_event(conn, event_id)?
        .into_iter()
        .filter(|assignment| assignment.round == index && assignment.track_code == track_code)
        .collect();
    if rows
        .iter()
        .any(|assignment| assignment.status == AssignmentStatus::Submitted)
    {
        return Err(EngineError::FillInNotEligible(
            "the track already has a submitted turn in this round".to_string(),
        ));
    }
    let planned_holder = match PlanRepo::latest(conn, event_id)? {
        Some(plan) => PlanRepo::list_assignments(conn, event_id, plan.version)?
            .into_iter()
            .find(|row| row.round == index && row.track_code == track_code)
            .map(|row| row.participant_id),
        None => None,
    };
    let Some(holder) = planned_holder else {
        return Ok(());
    };
    if rows.is_empty() {
        // The planned turn became a gap: the person left before the round.
        return Ok(());
    }
    let latest = rows
        .iter()
        .filter(|assignment| assignment.participant_id == holder)
        .max_by_key(|assignment| (assignment.offered_at, assignment.created_at));
    match latest.map(|assignment| assignment.status) {
        Some(AssignmentStatus::SkippedBeforeClaim)
        | Some(AssignmentStatus::SkippedAfterClaim)
        | Some(AssignmentStatus::Expired)
        | Some(AssignmentStatus::Revoked)
        | Some(AssignmentStatus::Superseded) => Ok(()),
        Some(_) => Err(EngineError::FillInNotEligible(
            "the planned turn is not stranded".to_string(),
        )),
        None => Ok(()),
    }
}

/// Insert one fill-in offer and queue its DM in the caller's transaction.
/// The caller checks eligibility first; the offer guard refuses a seen pair.
/// `due_at` is the edit due time and `claim_deadline_at` is the last instant
/// when the person can claim (D-069). The caller passes the same value when
/// the host sets the due time.
#[allow(clippy::too_many_arguments)]
pub(crate) fn offer_fill_in(
    conn: &Connection,
    event_id: &str,
    track_code: &str,
    index: u32,
    user_id: &str,
    due_at: i64,
    claim_deadline_at: i64,
    actor: &str,
    now: i64,
) -> Result<String, EngineError> {
    let id = format!(
        "{}-{}",
        assignment_id(event_id, index, track_code),
        &swap_files::sha256_hex(format!("fill-in:{event_id}:{user_id}:{now}").as_bytes())[..8]
    );
    insert_offer(
        conn,
        &AssignmentRecord {
            id: id.clone(),
            event_id: event_id.to_string(),
            track_code: track_code.to_string(),
            participant_id: user_id.to_string(),
            round: index,
            status: AssignmentStatus::Offered,
            offered_at: now,
            claimed_at: None,
            due_at: Some(due_at),
            submitted_at: None,
            created_at: now,
            updated_at: now,
            extension_secs: 0,
            claim_deadline_at: Some(claim_deadline_at),
        },
    )?;
    move_track(conn, event_id, track_code, TrackState::Offered)?;
    let effect = Effect::Offer {
        user_id: user_id.to_string(),
        event_id: event_id.to_string(),
        assignment_id: id.clone(),
        track_code: track_code.to_string(),
        due_at: claim_deadline_at,
        claim_window_secs: (claim_deadline_at - now).max(1),
    };
    enqueue_effect(conn, event_id, &effect, now)?;
    AuditRepo::insert(
        conn,
        event_id,
        now,
        actor,
        "Fill-in offered",
        Some(track_code),
        &serde_json::json!({
            "round": index + 2,
            "participantId": user_id,
            "dueAt": due_at,
        })
        .to_string(),
    )?;
    Ok(id)
}

/// One active fill-in offer or turn. The next classic round waits on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitingFillIn {
    pub assignment_id: String,
    pub track_code: String,
    pub round_no: u32,
    pub participant_id: String,
    pub status: AssignmentStatus,
    pub due_at: Option<i64>,
}

/// A planned classic offer has the id `assignment_id(event, round, track)`.
/// Every other active assignment in a classic round is a fill-in.
pub fn waiting_fill_ins(
    conn: &Connection,
    event_id: &str,
) -> Result<Vec<WaitingFillIn>, EngineError> {
    Ok(AssignmentRepo::list_active(conn, event_id)?
        .into_iter()
        .filter(|assignment| is_fill_in_turn(event_id, assignment))
        .map(|assignment| WaitingFillIn {
            round_no: assignment.round + 2,
            assignment_id: assignment.id,
            track_code: assignment.track_code,
            participant_id: assignment.participant_id,
            status: assignment.status,
            due_at: assignment.due_at,
        })
        .collect())
}
