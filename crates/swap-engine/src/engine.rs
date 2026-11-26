use chrono::{TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use swap_core::{
    AssignmentStatus, EventState, Membership, ParticipantId, Readiness, Role, Schedule, StateError,
    TrackCode, TrackSpec, TrackState,
};
use swap_db::{
    AssignmentRecord, AssignmentRepo, AuditRepo, Connection, Db, DbError, DeadlineRepo,
    EventRecord, EventRepo, FileRecord, FileRepo, HostDecisionRecord, HostDecisionRepo,
    ParticipantRecord, ParticipantRepo, PlanAssignmentRecord, PlanRecord, PlanRepo,
    ProcessedAttachmentRepo, SeenRepo, TrackRecord, TrackRepo, Transaction, TransactionBehavior,
};
use swap_files::{BlobStore, FileError, StoredBlob, ValidatedFile};
use thiserror::Error;

use crate::delivery::{DeliveryError, PendingDelivery, enqueue_effect, enqueue_pending};
use crate::effects::Effect;
use crate::ids::{assignment_id, plan_commitment, track_code};
use crate::prepare::{DeliveryPreparer, PreparedBytes};
use crate::settings::{EventSettings, Mode, SettingsError};

#[derive(Debug, Error)]
pub enum EngineError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    File(#[from] FileError),
    #[error(transparent)]
    Store(#[from] swap_files::StoreError),
    #[error("the file could not be prepared for delivery: {0}")]
    Prepare(#[from] swap_files::PrepareError),
    #[error(transparent)]
    Schedule(#[from] swap_core::ScheduleError),
    #[error(transparent)]
    Settings(#[from] SettingsError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Delivery(#[from] DeliveryError),
    #[error(transparent)]
    State(#[from] StateError),
    #[error("event not found: {0}")]
    EventNotFound(String),
    #[error("event already exists: {0}")]
    EventExists(String),
    #[error("the event id is not a safe identifier")]
    InvalidEventId,
    #[error("the event name is empty")]
    EmptyEventName,
    #[error("participant not found: {0}")]
    ParticipantNotFound(String),
    #[error("the event is in state {state:?}; {action} requires {expected}")]
    WrongEventState {
        action: &'static str,
        state: EventState,
        expected: &'static str,
    },
    #[error("the participant has already seeded a track")]
    AlreadySeeded,
    #[error("the participant left the event")]
    ParticipantLeft,
    #[error("the participant must seed a track before this action")]
    NotSeeded,
    #[error("the participant is not ready for a new offer: {0}")]
    ParticipantNotReady(String),
    #[error("the event has no active participants")]
    NoParticipants,
    #[error("the event has no seeded tracks")]
    NoSeededTracks,
    #[error("the event needs at least two active participants to schedule a round")]
    NoRounds,
    #[error("the event has no committed plan")]
    NoPlan,
    #[error("round {round} is outside the plan (0..{rounds})")]
    RoundOutOfRange { round: u32, rounds: u32 },
    #[error("round {round} still has active assignments")]
    RoundNotResolved { round: u32 },
    #[error("round {round} has not started")]
    PlanNotFinished { round: u32 },
    #[error("round {round} is already open or resolved")]
    RoundAlreadyOpened { round: u32 },
    #[error("round {expected} must open before round {requested}")]
    RoundOutOfOrder { expected: u32, requested: u32 },
    #[error("track {0} has no file")]
    NoFile(String),
    #[error("track {0} cannot reach the requested state")]
    TrackPath(String),
    #[error("assignment not found: {0}")]
    AssignmentNotFound(String),
    #[error("the assignment does not belong to this participant")]
    NotAssignmentOwner,
    #[error("the assignment is in state {0:?}")]
    WrongAssignmentState(AssignmentStatus),
    #[error("the offer has expired")]
    OfferExpired,
    #[error("the offer is not due yet")]
    OfferNotDue,
    #[error("the participant has no active assignment")]
    NoActiveAssignment,
    #[error("the file format changed from .{current} to .{submitted}")]
    FormatChange { current: String, submitted: String },
    #[error("the reveal is blocked by {count} active assignments")]
    PendingAssignments { count: usize, ids: Vec<String> },
    #[error("the attachment was already processed")]
    AttachmentAlreadyProcessed,
    #[error("an extension must be between 1 and 12 hours")]
    InvalidExtensionHours,
    #[error("host decision not found: {0}")]
    DecisionNotFound(String),
    #[error("the host decision is not open")]
    DecisionNotOpen,
    #[error("the host decision has an unexpected kind: {0}")]
    WrongDecisionKind(String),
    #[error("the assignment has used the maximum number of extensions")]
    ExtensionLimit,
    #[error("the assignment has no edit deadline")]
    NoDeadline,
}

#[derive(Debug, Clone)]
pub struct JoinRequest<'a> {
    pub event_id: &'a str,
    pub user_id: &'a str,
    pub display_name: &'a str,
    pub timezone: &'a str,
    pub max_handoffs: u32,
    pub filename: &'a str,
    pub bytes: &'a [u8],
}

#[derive(Debug, Clone)]
pub struct SubmitRequest<'a> {
    pub event_id: &'a str,
    pub user_id: &'a str,
    pub filename: &'a str,
    pub bytes: &'a [u8],
}

#[derive(Debug, Clone, Copy)]
pub struct SubmissionSource<'a> {
    pub message_id: &'a str,
    pub attachment_id: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinOutcome {
    pub track_code: String,
    pub sha256: String,
    pub original_sha256: String,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockOutcome {
    pub active: usize,
    pub fill_ins: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleOutcome {
    pub version: u32,
    pub rounds: u32,
    pub commitment: String,
    pub fallback: bool,
    pub assignments: Vec<PlanAssignmentRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRoundOutcome {
    pub round: u32,
    pub offers: usize,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimOutcome {
    pub assignment_id: String,
    pub track_code: String,
    pub filename: String,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitOutcome {
    pub version: u32,
    pub sha256: String,
    pub original_sha256: String,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkipOutcome {
    pub status: AssignmentStatus,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpireOutcome {
    pub assignment_id: String,
    pub track_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreditEntry {
    pub round: u32,
    pub participant_id: String,
    pub display_name: String,
    pub status: AssignmentStatus,
    pub fill_in: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackCredits {
    pub track_code: String,
    pub seed_owner: String,
    pub seed_owner_name: String,
    pub entries: Vec<CreditEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealOutcome {
    pub tracks: Vec<TrackCredits>,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DeadlineReport {
    pub overdue: usize,
    pub reminders: usize,
    pub decisions: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionStatus {
    Approved,
    PendingHost,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionOutcome {
    pub status: ExtensionStatus,
    pub due_at: Option<i64>,
    pub decision_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlineDecisionOutcome {
    pub action: String,
    pub due_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventStatus {
    pub event_id: String,
    pub state: EventState,
    pub rounds: u32,
    pub committed: bool,
    pub active_assignments: usize,
    pub submitted_assignments: usize,
}

pub fn create_event(
    db: &Db,
    id: &str,
    name: &str,
    seed: u64,
    settings: &EventSettings,
    now: i64,
) -> Result<(), EngineError> {
    if !is_safe_id(id) {
        return Err(EngineError::InvalidEventId);
    }
    if name.trim().is_empty() {
        return Err(EngineError::EmptyEventName);
    }
    settings.validate()?;
    if EventRepo::get(db.conn(), id)?.is_some() {
        return Err(EngineError::EventExists(id.to_string()));
    }
    EventRepo::insert(
        db.conn(),
        &EventRecord {
            id: id.to_string(),
            name: name.trim().to_string(),
            state: EventState::Signup,
            seed,
            settings_json: serde_json::to_string(settings)?,
            created_at: now,
            updated_at: now,
        },
    )?;
    Ok(())
}

pub fn signup(
    db: &Db,
    event_id: &str,
    user_id: &str,
    display_name: &str,
    timezone: &str,
    max_handoffs: u32,
    now: i64,
) -> Result<(), EngineError> {
    let event = load_event(db, event_id)?;
    require_state(&event, EventState::Signup, "signup", "signup")?;
    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if current.state != EventState::Signup {
        return Err(EngineError::WrongEventState {
            action: "signup",
            state: current.state,
            expected: "signup",
        });
    }
    if let Some(mut existing) = ParticipantRepo::get(&transaction, event_id, user_id)? {
        match existing.membership {
            Membership::SignedUp => {
                existing.display_name = display_name.to_string();
                existing.timezone = timezone.to_string();
                existing.max_handoffs = max_handoffs;
                existing.updated_at = now;
                ParticipantRepo::upsert(&transaction, &existing)?;
                commit(transaction)?;
                return Ok(());
            }
            Membership::Seeded | Membership::Active => return Err(EngineError::AlreadySeeded),
            Membership::Left | Membership::Removed => return Err(EngineError::ParticipantLeft),
        }
    }
    ParticipantRepo::upsert(
        &transaction,
        &ParticipantRecord {
            event_id: event_id.to_string(),
            user_id: user_id.to_string(),
            display_name: display_name.to_string(),
            role: Role::Participant,
            membership: Membership::SignedUp,
            readiness: Readiness::Ready,
            timezone: timezone.to_string(),
            max_handoffs,
            created_at: now,
            updated_at: now,
        },
    )?;
    commit(transaction)?;
    Ok(())
}

pub fn join_with_seed(
    db: &Db,
    store: &BlobStore,
    preparer: &dyn DeliveryPreparer,
    request: &JoinRequest<'_>,
    now: i64,
) -> Result<JoinOutcome, EngineError> {
    let event = load_event(db, request.event_id)?;
    require_state(&event, EventState::Signup, "joining", "signup")?;
    let settings = parse_settings(&event)?;
    let validated = settings
        .file_policy()
        .validate(request.filename, request.bytes)?;

    if let Some(existing) = ParticipantRepo::get(db.conn(), request.event_id, request.user_id)? {
        match existing.membership {
            Membership::Seeded | Membership::Active => return Err(EngineError::AlreadySeeded),
            Membership::Left | Membership::Removed => return Err(EngineError::ParticipantLeft),
            Membership::SignedUp => {}
        }
    }

    let tracks = TrackRepo::list_for_event(db.conn(), request.event_id)?;
    if tracks.iter().any(|track| track.owner_id == request.user_id) {
        return Err(EngineError::AlreadySeeded);
    }
    let code = unique_track_code(&tracks, event.seed);

    let prepared = preparer.prepare(request.bytes, validated.kind, &validated.extension)?;
    let original_blob = store.store(request.bytes)?;
    let delivery_blob = if prepared.delivery == request.bytes {
        original_blob.clone()
    } else {
        store.store(&prepared.delivery)?
    };
    let participant = ParticipantRecord {
        event_id: request.event_id.to_string(),
        user_id: request.user_id.to_string(),
        display_name: request.display_name.to_string(),
        role: Role::Participant,
        membership: Membership::Seeded,
        readiness: Readiness::Ready,
        timezone: request.timezone.to_string(),
        max_handoffs: request.max_handoffs,
        created_at: now,
        updated_at: now,
    };
    let track = TrackRecord {
        event_id: request.event_id.to_string(),
        code: code.clone(),
        owner_id: request.user_id.to_string(),
        state: TrackState::Seeded,
        created_at: now,
    };
    let file = file_record(
        request.event_id,
        &code,
        1,
        request.user_id,
        &validated,
        StoredVersion {
            delivery: &delivery_blob,
            original: &original_blob,
            prepared: &prepared,
        },
        now,
    );

    let receipt = Effect::Dm {
        user_id: request.user_id.to_string(),
        content: format!(
            "Seed received for track {code}. SHA-256: {}. Size: {} bytes.",
            original_blob.sha256, original_blob.size
        ),
    };
    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, request.event_id)?
        .ok_or_else(|| EngineError::EventNotFound(request.event_id.to_string()))?;
    if current.state != EventState::Signup {
        return Err(EngineError::WrongEventState {
            action: "seed intake",
            state: current.state,
            expected: "signup",
        });
    }
    ParticipantRepo::upsert(&transaction, &participant)?;
    TrackRepo::insert(&transaction, &track)?;
    FileRepo::insert(&transaction, &file)?;
    enqueue_effect(&transaction, request.event_id, &receipt, now)?;
    commit(transaction)?;

    let effects = vec![receipt];
    Ok(JoinOutcome {
        track_code: code,
        sha256: delivery_blob.sha256,
        original_sha256: original_blob.sha256,
        effects,
    })
}

pub fn lock_roster(db: &Db, event_id: &str, now: i64) -> Result<LockOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_state(&event, EventState::Signup, "roster lock", "signup")?;
    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if current.state != EventState::Signup {
        return Err(EngineError::WrongEventState {
            action: "roster lock",
            state: current.state,
            expected: "signup",
        });
    }
    let participants = ParticipantRepo::list_for_event(&transaction, event_id)?;
    if participants.is_empty() {
        return Err(EngineError::NoParticipants);
    }
    let tracks = TrackRepo::list_for_event(&transaction, event_id)?;
    if tracks.is_empty() {
        return Err(EngineError::NoSeededTracks);
    }

    let mut active = 0;
    let mut fill_ins = 0;
    for mut participant in participants {
        if !matches!(
            participant.membership,
            Membership::SignedUp | Membership::Seeded
        ) {
            continue;
        }
        let has_track = tracks
            .iter()
            .any(|track| track.owner_id == participant.user_id);
        if has_track {
            participant.membership = Membership::Active;
            participant.role = Role::Participant;
            active += 1;
        } else {
            participant.membership = Membership::Active;
            participant.role = Role::FillIn;
            fill_ins += 1;
        }
        participant.updated_at = now;
        ParticipantRepo::upsert(&transaction, &participant)?;
    }

    if active == 0 {
        return Err(EngineError::NoSeededTracks);
    }

    EventRepo::set_state(&transaction, event_id, EventState::ScheduleReview, now)?;
    commit(transaction)?;
    Ok(LockOutcome { active, fill_ins })
}

pub fn generate_schedule(
    db: &Db,
    event_id: &str,
    now: i64,
) -> Result<ScheduleOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_state(
        &event,
        EventState::ScheduleReview,
        "schedule generation",
        "schedule review",
    )?;
    if parse_settings(&event)?.mode != Mode::Classic {
        return Err(EngineError::WrongEventState {
            action: "classic scheduling",
            state: event.state,
            expected: "classic mode",
        });
    }
    let settings = parse_settings(&event)?;
    let participants = ParticipantRepo::list_for_event(db.conn(), event_id)?;
    let active: Vec<&ParticipantRecord> = participants
        .iter()
        .filter(|participant| {
            participant.membership == Membership::Active && participant.role == Role::Participant
        })
        .collect();
    if active.is_empty() {
        return Err(EngineError::NoParticipants);
    }
    let active_ids: Vec<&str> = active
        .iter()
        .map(|participant| participant.user_id.as_str())
        .collect();
    let tracks: Vec<TrackRecord> = TrackRepo::list_for_event(db.conn(), event_id)?
        .into_iter()
        .filter(|track| active_ids.contains(&track.owner_id.as_str()))
        .collect();
    if tracks.len() != active.len() {
        return Err(EngineError::NoSeededTracks);
    }
    let max_rounds = active.len().saturating_sub(1) as u32;
    if max_rounds == 0 {
        return Err(EngineError::NoRounds);
    }
    let rounds = settings.rounds.unwrap_or(max_rounds);
    if rounds == 0 || rounds > max_rounds {
        return Err(EngineError::Schedule(
            swap_core::ScheduleError::TooManyRounds {
                requested: rounds,
                max: max_rounds,
            },
        ));
    }

    let people: Vec<ParticipantId> = active
        .iter()
        .map(|participant| ParticipantId::new(participant.user_id.clone()))
        .collect();
    let specs: Vec<TrackSpec> = tracks
        .iter()
        .map(|track| TrackSpec {
            code: TrackCode::new(track.code.clone()),
            owner: ParticipantId::new(track.owner_id.clone()),
        })
        .collect();
    let strategy = match settings.scheduler.as_deref() {
        Some(value) => swap_core::ScheduleStrategy::parse(value).ok_or(SettingsError::Scheduler)?,
        None => swap_core::ScheduleStrategy::default(),
    };
    let previous = PlanRepo::latest(db.conn(), event_id)?;
    let version = previous.map(|plan| plan.version + 1).unwrap_or(1);
    let plan_seed = event.seed.wrapping_add(u64::from(version - 1));
    let schedule = Schedule::generate_with(&people, &specs, rounds, plan_seed, strategy)?;

    let settings_json = serde_json::to_string(&settings)?;
    let planned: Vec<PlanAssignmentRecord> = schedule
        .assignments
        .iter()
        .map(|assignment| PlanAssignmentRecord {
            event_id: event_id.to_string(),
            plan_version: version,
            round: assignment.round,
            track_code: assignment.track.0.clone(),
            participant_id: assignment.participant.0.clone(),
        })
        .collect();
    let plan = PlanRecord {
        event_id: event_id.to_string(),
        version,
        seed: plan_seed,
        rounds,
        settings_json,
        commitment: String::new(),
        committed_at: now,
        reason: None,
    };
    let commitment = plan_commitment(&plan, &planned);
    let plan = PlanRecord {
        commitment: commitment.clone(),
        ..plan
    };

    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if current.state != EventState::ScheduleReview {
        return Err(EngineError::WrongEventState {
            action: "schedule generation",
            state: current.state,
            expected: "schedule review",
        });
    }
    PlanRepo::insert(&transaction, &plan)?;
    for assignment in &planned {
        PlanRepo::insert_assignment(&transaction, assignment)?;
    }
    commit(transaction)?;

    Ok(ScheduleOutcome {
        version,
        rounds,
        commitment,
        fallback: schedule.fallback,
        assignments: planned,
    })
}

pub fn start_event(db: &Db, event_id: &str, now: i64) -> Result<(), EngineError> {
    let event = load_event(db, event_id)?;
    require_state(
        &event,
        EventState::ScheduleReview,
        "event start",
        "schedule review",
    )?;
    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if current.state != EventState::ScheduleReview {
        return Err(EngineError::WrongEventState {
            action: "event start",
            state: current.state,
            expected: "schedule review",
        });
    }
    if PlanRepo::latest(&transaction, event_id)?.is_none() {
        return Err(EngineError::NoPlan);
    }
    let next = current.state.transition_to(EventState::Active)?;
    EventRepo::set_state(&transaction, event_id, next, now)?;
    commit(transaction)?;
    Ok(())
}

pub fn open_round(
    db: &Db,
    event_id: &str,
    round: u32,
    now: i64,
) -> Result<OpenRoundOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_state(&event, EventState::Active, "round open", "active")?;
    let plan = PlanRepo::latest(db.conn(), event_id)?.ok_or(EngineError::NoPlan)?;
    if round >= plan.rounds {
        return Err(EngineError::RoundOutOfRange {
            round,
            rounds: plan.rounds,
        });
    }
    let assignments = AssignmentRepo::list_for_event(db.conn(), event_id)?;
    if assignments
        .iter()
        .any(|assignment| assignment.round == round)
    {
        return Err(EngineError::RoundAlreadyOpened { round });
    }
    for previous in 0..round {
        if !assignments
            .iter()
            .any(|assignment| assignment.round == previous)
        {
            return Err(EngineError::RoundOutOfOrder {
                expected: previous,
                requested: round,
            });
        }
    }
    let active_assignments = AssignmentRepo::list_active(db.conn(), event_id)?;
    if let Some(active) = active_assignments.first() {
        return Err(EngineError::RoundNotResolved {
            round: active.round,
        });
    }
    let settings = parse_settings(&event)?;
    let planned = PlanRepo::list_assignments(db.conn(), event_id, plan.version)?;
    let for_round: Vec<&PlanAssignmentRecord> = planned
        .iter()
        .filter(|assignment| assignment.round == round)
        .collect();
    if for_round.is_empty() {
        return Err(EngineError::RoundOutOfRange {
            round,
            rounds: plan.rounds,
        });
    }

    let mut effects = Vec::with_capacity(for_round.len());
    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if current.state != EventState::Active {
        return Err(EngineError::WrongEventState {
            action: "round open",
            state: current.state,
            expected: "active",
        });
    }
    if let Some(active) = AssignmentRepo::list_active(&transaction, event_id)?.first() {
        return Err(EngineError::RoundNotResolved {
            round: active.round,
        });
    }
    let participants = ParticipantRepo::list_for_event(&transaction, event_id)?;
    for planned_assignment in &for_round {
        let participant = participants
            .iter()
            .find(|participant| participant.user_id == planned_assignment.participant_id)
            .ok_or_else(|| {
                EngineError::ParticipantNotFound(planned_assignment.participant_id.clone())
            })?;
        if participant.membership != Membership::Active || participant.readiness != Readiness::Ready
        {
            return Err(EngineError::ParticipantNotReady(
                planned_assignment.participant_id.clone(),
            ));
        }
        let id = assignment_id(event_id, round, &planned_assignment.track_code);
        let due_at = now + settings.claim_window_secs;
        AssignmentRepo::insert(
            &transaction,
            &AssignmentRecord {
                id: id.clone(),
                event_id: event_id.to_string(),
                track_code: planned_assignment.track_code.clone(),
                participant_id: planned_assignment.participant_id.clone(),
                round,
                status: AssignmentStatus::Offered,
                offered_at: now,
                claimed_at: None,
                due_at: Some(due_at),
                submitted_at: None,
                created_at: now,
                updated_at: now,
            },
        )?;
        move_track(
            &transaction,
            event_id,
            &planned_assignment.track_code,
            TrackState::Offered,
        )?;
        let effect = Effect::Offer {
            user_id: planned_assignment.participant_id.clone(),
            event_id: event_id.to_string(),
            assignment_id: id,
            track_code: planned_assignment.track_code.clone(),
            due_at,
            claim_window_secs: settings.claim_window_secs,
        };
        enqueue_effect(&transaction, event_id, &effect, now)?;
        effects.push(effect);
    }
    commit(transaction)?;

    Ok(OpenRoundOutcome {
        round,
        offers: effects.len(),
        effects,
    })
}

pub fn claim(
    db: &Db,
    store: &BlobStore,
    event_id: &str,
    assignment_id: &str,
    user_id: &str,
    now: i64,
) -> Result<ClaimOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_open_event(&event, "claim")?;
    let settings = parse_settings(&event)?;
    let assignment = AssignmentRepo::get(db.conn(), assignment_id)?
        .ok_or_else(|| EngineError::AssignmentNotFound(assignment_id.to_string()))?;
    if assignment.participant_id != user_id {
        return Err(EngineError::NotAssignmentOwner);
    }
    if assignment.status != AssignmentStatus::Offered {
        return Err(EngineError::WrongAssignmentState(assignment.status));
    }
    let due_at = assignment.due_at.ok_or(EngineError::OfferExpired)?;
    if now > due_at {
        return Err(EngineError::OfferExpired);
    }
    let next_due = now + settings.round_window_secs;
    let current = latest_file(db, event_id, &assignment.track_code)?;
    let filename = format!("{}.{}", assignment.track_code, current.extension);
    let caption = format!(
        "Track {}. Your turn. Due at {next_due} UTC.",
        assignment.track_code
    );
    let bytes = store.read(&current.sha256)?;
    let pending = PendingDelivery::File {
        user_id: user_id.to_string(),
        track_code: assignment.track_code.clone(),
        filename: filename.clone(),
        sha256: current.sha256.clone(),
        caption: caption.clone(),
    };

    let transaction = begin(db)?;
    open_event_in(&transaction, event_id, "claim")?;
    if !AssignmentRepo::claim(&transaction, assignment.id.as_str(), now, next_due, now)? {
        return Err(EngineError::WrongAssignmentState(assignment.status));
    }
    SeenRepo::record(
        &transaction,
        event_id,
        &assignment.track_code,
        user_id,
        "claim",
        now,
    )?;
    move_track(
        &transaction,
        event_id,
        &assignment.track_code,
        TrackState::InCustody,
    )?;
    enqueue_pending(&transaction, event_id, &pending, now)?;
    commit(transaction)?;

    let effects = vec![Effect::TrackFile {
        user_id: user_id.to_string(),
        track_code: assignment.track_code.clone(),
        filename: filename.clone(),
        bytes,
        caption,
    }];

    Ok(ClaimOutcome {
        assignment_id: assignment.id,
        track_code: assignment.track_code,
        filename,
        effects,
    })
}

pub fn submit(
    db: &Db,
    store: &BlobStore,
    preparer: &dyn DeliveryPreparer,
    request: &SubmitRequest<'_>,
    now: i64,
) -> Result<SubmitOutcome, EngineError> {
    submit_inner(db, store, preparer, request, None, now)
}

pub fn submit_attachment(
    db: &Db,
    store: &BlobStore,
    preparer: &dyn DeliveryPreparer,
    request: &SubmitRequest<'_>,
    source: SubmissionSource<'_>,
    now: i64,
) -> Result<SubmitOutcome, EngineError> {
    submit_inner(db, store, preparer, request, Some(source), now)
}

fn submit_inner(
    db: &Db,
    store: &BlobStore,
    preparer: &dyn DeliveryPreparer,
    request: &SubmitRequest<'_>,
    source: Option<SubmissionSource<'_>>,
    now: i64,
) -> Result<SubmitOutcome, EngineError> {
    let event = load_event(db, request.event_id)?;
    require_open_event(&event, "submission")?;
    let settings = parse_settings(&event)?;
    let assignment =
        AssignmentRepo::latest_for_participant(db.conn(), request.event_id, request.user_id)?
            .ok_or(EngineError::NoActiveAssignment)?;
    if !matches!(
        assignment.status,
        AssignmentStatus::Claimed | AssignmentStatus::Overdue | AssignmentStatus::Submitted
    ) {
        return Err(EngineError::NoActiveAssignment);
    }
    let current = latest_file(db, request.event_id, &assignment.track_code)?;
    let validated = settings
        .file_policy()
        .validate(request.filename, request.bytes)?;
    if validated.extension != current.extension {
        return Err(EngineError::FormatChange {
            current: current.extension,
            submitted: validated.extension,
        });
    }

    let prepared = preparer.prepare(request.bytes, validated.kind, &validated.extension)?;
    let original_blob = store.store(request.bytes)?;
    let delivery_blob = if prepared.delivery == request.bytes {
        original_blob.clone()
    } else {
        store.store(&prepared.delivery)?
    };
    let version = current.version + 1;
    let file = file_record(
        request.event_id,
        &assignment.track_code,
        version,
        request.user_id,
        &validated,
        StoredVersion {
            delivery: &delivery_blob,
            original: &original_blob,
            prepared: &prepared,
        },
        now,
    );
    let receipt = Effect::Dm {
        user_id: request.user_id.to_string(),
        content: format!(
            "Received version {version} of track {}. SHA-256: {}. Latest version wins.",
            assignment.track_code, original_blob.sha256
        ),
    };
    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, request.event_id)?
        .ok_or_else(|| EngineError::EventNotFound(request.event_id.to_string()))?;
    if !matches!(current.state, EventState::Active | EventState::Paused) {
        return Err(EngineError::WrongEventState {
            action: "submission",
            state: current.state,
            expected: "active or paused",
        });
    }
    FileRepo::insert(&transaction, &file)?;
    if !AssignmentRepo::submit(&transaction, &assignment.id, now, now)? {
        return Err(EngineError::WrongAssignmentState(assignment.status));
    }
    SeenRepo::record(
        &transaction,
        request.event_id,
        &assignment.track_code,
        request.user_id,
        "submit",
        now,
    )?;
    for kind in ["extension", "overdue"] {
        if let Some(open) =
            HostDecisionRepo::open_for_assignment(&transaction, &assignment.id, kind)?
        {
            HostDecisionRepo::close(
                &transaction,
                &open.id,
                "addressed",
                now,
                "a submission arrived",
            )?;
        }
    }
    move_track(
        &transaction,
        request.event_id,
        &assignment.track_code,
        TrackState::Submitted,
    )?;
    if let Some(source) = source
        && !ProcessedAttachmentRepo::mark(
            &transaction,
            request.user_id,
            source.message_id,
            source.attachment_id,
            now,
        )?
    {
        return Err(EngineError::AttachmentAlreadyProcessed);
    }
    enqueue_effect(&transaction, request.event_id, &receipt, now)?;
    commit(transaction)?;

    let effects = vec![receipt];
    Ok(SubmitOutcome {
        version,
        sha256: delivery_blob.sha256,
        original_sha256: original_blob.sha256,
        effects,
    })
}

pub fn skip(db: &Db, event_id: &str, user_id: &str, now: i64) -> Result<SkipOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_open_event(&event, "skip")?;
    let assignment = AssignmentRepo::latest_for_participant(db.conn(), event_id, user_id)?
        .ok_or(EngineError::NoActiveAssignment)?;
    let (status, seen_reason) = match assignment.status {
        AssignmentStatus::Offered => (AssignmentStatus::SkippedBeforeClaim, None),
        AssignmentStatus::Claimed | AssignmentStatus::Overdue => (
            AssignmentStatus::SkippedAfterClaim,
            Some("skip_after_claim"),
        ),
        other => return Err(EngineError::WrongAssignmentState(other)),
    };

    let receipt = Effect::Dm {
        user_id: user_id.to_string(),
        content: format!(
            "You skipped track {}. The track returns to the pool unchanged.",
            assignment.track_code
        ),
    };
    let transaction = begin(db)?;
    open_event_in(&transaction, event_id, "skip")?;
    for kind in ["extension", "overdue"] {
        if let Some(open) =
            HostDecisionRepo::open_for_assignment(&transaction, &assignment.id, kind)?
        {
            HostDecisionRepo::close(
                &transaction,
                &open.id,
                "addressed",
                now,
                "the participant skipped",
            )?;
        }
    }
    AssignmentRepo::update_status(&transaction, &assignment.id, status, now)?;
    if let Some(reason) = seen_reason {
        SeenRepo::record(
            &transaction,
            event_id,
            &assignment.track_code,
            user_id,
            reason,
            now,
        )?;
    }
    move_track(
        &transaction,
        event_id,
        &assignment.track_code,
        TrackState::Pool,
    )?;
    enqueue_effect(&transaction, event_id, &receipt, now)?;
    commit(transaction)?;

    let effects = vec![receipt];
    Ok(SkipOutcome { status, effects })
}

pub fn expire_offer(
    db: &Db,
    event_id: &str,
    assignment_id: &str,
    now: i64,
) -> Result<ExpireOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_open_event(&event, "offer expiry")?;
    let assignment = AssignmentRepo::get(db.conn(), assignment_id)?
        .ok_or_else(|| EngineError::AssignmentNotFound(assignment_id.to_string()))?;
    if assignment.status != AssignmentStatus::Offered {
        return Err(EngineError::WrongAssignmentState(assignment.status));
    }
    if assignment.due_at.is_some_and(|due_at| now < due_at) {
        return Err(EngineError::OfferNotDue);
    }

    let transaction = begin(db)?;
    open_event_in(&transaction, event_id, "offer expiry")?;
    AssignmentRepo::update_status(&transaction, &assignment.id, AssignmentStatus::Expired, now)?;
    move_track(
        &transaction,
        event_id,
        &assignment.track_code,
        TrackState::Pool,
    )?;
    commit(transaction)?;

    Ok(ExpireOutcome {
        assignment_id: assignment.id,
        track_code: assignment.track_code,
    })
}

pub fn reveal(db: &Db, event_id: &str, now: i64) -> Result<RevealOutcome, EngineError> {
    reveal_with_channel(db, event_id, None, now)
}

pub fn reveal_with_channel(
    db: &Db,
    event_id: &str,
    reveal_channel_id: Option<&str>,
    now: i64,
) -> Result<RevealOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    match event.state {
        EventState::Active | EventState::Paused => {}
        _ => {
            return Err(EngineError::WrongEventState {
                action: "reveal",
                state: event.state,
                expected: "active or paused",
            });
        }
    }
    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if !matches!(current.state, EventState::Active | EventState::Paused) {
        return Err(EngineError::WrongEventState {
            action: "reveal",
            state: current.state,
            expected: "active or paused",
        });
    }
    let active = AssignmentRepo::list_active(&transaction, event_id)?;
    if !active.is_empty() {
        return Err(EngineError::PendingAssignments {
            count: active.len(),
            ids: active.into_iter().map(|assignment| assignment.id).collect(),
        });
    }
    let participants = ParticipantRepo::list_for_event(&transaction, event_id)?;
    let assignments = AssignmentRepo::list_for_event(&transaction, event_id)?;
    if let Some(plan) = PlanRepo::latest(&transaction, event_id)? {
        for round in 0..plan.rounds {
            if !assignments
                .iter()
                .any(|assignment| assignment.round == round)
            {
                return Err(EngineError::PlanNotFinished { round });
            }
        }
    }
    let participant_name = |user_id: &str| {
        participants
            .iter()
            .find(|participant| participant.user_id == user_id)
            .map(|participant| participant.display_name.clone())
            .unwrap_or_else(|| "Removed participant".to_string())
    };
    let mut tracks = Vec::new();
    for track in TrackRepo::list_for_event(&transaction, event_id)? {
        let entries = assignments
            .iter()
            .filter(|assignment| assignment.track_code == track.code)
            .filter(|assignment| {
                matches!(
                    assignment.status,
                    AssignmentStatus::Submitted
                        | AssignmentStatus::SkippedAfterClaim
                        | AssignmentStatus::SkippedBeforeClaim
                )
            })
            .map(|assignment| CreditEntry {
                round: assignment.round,
                participant_id: assignment.participant_id.clone(),
                display_name: participant_name(&assignment.participant_id),
                status: assignment.status,
                fill_in: participants
                    .iter()
                    .find(|participant| participant.user_id == assignment.participant_id)
                    .is_some_and(|participant| participant.role == Role::FillIn),
            })
            .collect();
        tracks.push(TrackCredits {
            track_code: track.code,
            seed_owner_name: participant_name(&track.owner_id),
            seed_owner: track.owner_id,
            entries,
        });
    }
    let effects = reveal_channel_id
        .filter(|channel_id| !channel_id.trim().is_empty())
        .map(|channel_id| {
            reveal_messages(&event.name, &tracks)
                .into_iter()
                .map(|content| Effect::Channel {
                    channel_id: channel_id.to_string(),
                    content,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let next = current.state.transition_to(EventState::Reveal)?;
    EventRepo::set_state(&transaction, event_id, next, now)?;
    for (index, effect) in effects.iter().enumerate() {
        // Keep split reveal messages in order. The id remains stable per chunk.
        enqueue_effect(&transaction, event_id, effect, now + index as i64)?;
    }
    commit(transaction)?;
    Ok(RevealOutcome { tracks, effects })
}

fn reveal_messages(event_name: &str, tracks: &[TrackCredits]) -> Vec<String> {
    const LIMIT: usize = 1_800;
    let mut messages = Vec::new();
    let mut current = format!("{event_name} — reveal");
    for track in tracks {
        let editors = {
            let names = track
                .entries
                .iter()
                .filter(|entry| entry.status == AssignmentStatus::Submitted)
                .map(|entry| entry.display_name.clone())
                .collect::<Vec<_>>();
            if names.is_empty() {
                "no edits".to_string()
            } else {
                names.join(", ")
            }
        };
        let line = format!(
            "\n{} — seed: {}; edits: {}",
            track.track_code, track.seed_owner_name, editors
        );
        if current.chars().count() + line.chars().count() > LIMIT {
            messages.push(current);
            current = format!("{event_name} — reveal (continued)");
        }
        current.push_str(&line);
    }
    messages.push(current);
    let mut bounded = Vec::new();
    for message in messages {
        let chars: Vec<char> = message.chars().collect();
        if chars.len() <= 1_900 {
            bounded.push(message);
        } else {
            for chunk in chars.chunks(1_900) {
                bounded.push(chunk.iter().collect());
            }
        }
    }
    bounded
}

pub fn request_extension(
    db: &Db,
    event_id: &str,
    user_id: &str,
    hours: i64,
    now: i64,
) -> Result<ExtensionOutcome, EngineError> {
    if !(1..=12).contains(&hours) {
        return Err(EngineError::InvalidExtensionHours);
    }
    let event = load_event(db, event_id)?;
    require_open_event(&event, "extension")?;
    let transaction = begin(db)?;
    open_event_in(&transaction, event_id, "extension")?;
    let assignment = AssignmentRepo::latest_for_participant(&transaction, event_id, user_id)?
        .ok_or(EngineError::NoActiveAssignment)?;
    if !matches!(
        assignment.status,
        AssignmentStatus::Claimed | AssignmentStatus::Overdue
    ) {
        return Err(EngineError::NoActiveAssignment);
    }
    let track = TrackRepo::get(&transaction, event_id, &assignment.track_code)?
        .ok_or_else(|| EngineError::NoFile(assignment.track_code.clone()))?;
    if track.state != TrackState::InCustody {
        return Err(EngineError::WrongAssignmentState(assignment.status));
    }
    let due_at = assignment.due_at.ok_or(EngineError::NoDeadline)?;
    DeadlineRepo::ensure(&transaction, &assignment.id)?;
    let deadline =
        DeadlineRepo::get(&transaction, &assignment.id)?.ok_or_else(|| EngineError::NoDeadline)?;
    if deadline.extension_count >= 2 {
        commit(transaction)?;
        return Ok(ExtensionOutcome {
            status: ExtensionStatus::Refused,
            due_at: None,
            decision_id: None,
        });
    }
    if deadline.extension_count == 0 && hours <= 6 {
        let base = due_at.max(now);
        let new_due = base + hours * 3_600;
        if !AssignmentRepo::extend(&transaction, &assignment.id, new_due, now)? {
            return Err(EngineError::NoActiveAssignment);
        }
        DeadlineRepo::increment_extension(&transaction, &assignment.id)?;
        if let Some(open) =
            HostDecisionRepo::open_for_assignment(&transaction, &assignment.id, "overdue")?
        {
            HostDecisionRepo::close(
                &transaction,
                &open.id,
                "addressed",
                now,
                "extension approved",
            )?;
        }
        AuditRepo::insert(
            &transaction,
            event_id,
            now,
            "worker",
            "Extension approved",
            None,
            &serde_json::json!({ "hours": hours, "assignmentId": assignment.id }).to_string(),
        )?;
        enqueue_effect(
            &transaction,
            event_id,
            &Effect::Dm {
                user_id: user_id.to_string(),
                content: format!(
                    "Extension approved for {hours} hours. Track {} keeps its current holder.",
                    assignment.track_code
                ),
            },
            now,
        )?;
        commit(transaction)?;
        return Ok(ExtensionOutcome {
            status: ExtensionStatus::Approved,
            due_at: Some(new_due),
            decision_id: None,
        });
    }
    if let Some(open) =
        HostDecisionRepo::open_for_assignment(&transaction, &assignment.id, "extension")?
    {
        commit(transaction)?;
        return Ok(ExtensionOutcome {
            status: ExtensionStatus::PendingHost,
            due_at: Some(due_at),
            decision_id: Some(open.id),
        });
    }
    let decision_id = format!("{}:extension:{now}", assignment.id);
    HostDecisionRepo::insert(
        &transaction,
        &HostDecisionRecord {
            id: decision_id.clone(),
            event_id: event_id.to_string(),
            assignment_id: Some(assignment.id.clone()),
            kind: "extension".to_string(),
            status: "open".to_string(),
            requested_hours: Some(hours),
            created_at: now,
            decided_at: None,
            reason: None,
        },
    )?;
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "worker",
        "Extension requested",
        None,
        &serde_json::json!({ "hours": hours, "assignmentId": assignment.id }).to_string(),
    )?;
    enqueue_effect(
        &transaction,
        event_id,
        &Effect::HostAlert {
            key: decision_id.clone(),
            content: "A participant requested an extension. Open the host decision card."
                .to_string(),
        },
        now,
    )?;
    enqueue_effect(
        &transaction,
        event_id,
        &Effect::Dm {
            user_id: user_id.to_string(),
            content: "The host will review your extension request.".to_string(),
        },
        now,
    )?;
    commit(transaction)?;
    Ok(ExtensionOutcome {
        status: ExtensionStatus::PendingHost,
        due_at: Some(due_at),
        decision_id: Some(decision_id),
    })
}

pub fn decide_extension(
    db: &Db,
    decision_id: &str,
    approve: bool,
    reason: &str,
    now: i64,
) -> Result<ExtensionOutcome, EngineError> {
    let transaction = begin(db)?;
    let decision = HostDecisionRepo::get(&transaction, decision_id)?
        .ok_or_else(|| EngineError::DecisionNotFound(decision_id.to_string()))?;
    if decision.status != "open" {
        return Err(EngineError::DecisionNotOpen);
    }
    if decision.kind != "extension" {
        return Err(EngineError::WrongDecisionKind(decision.kind));
    }
    let event = EventRepo::get(&transaction, &decision.event_id)?
        .ok_or_else(|| EngineError::EventNotFound(decision.event_id.clone()))?;
    require_open_event(&event, "extension decision")?;
    let assignment_id = decision
        .assignment_id
        .clone()
        .ok_or_else(|| EngineError::DecisionNotFound(decision_id.to_string()))?;
    let assignment = AssignmentRepo::get(&transaction, &assignment_id)?
        .ok_or_else(|| EngineError::AssignmentNotFound(assignment_id.clone()))?;
    let track = TrackRepo::get(&transaction, &assignment.event_id, &assignment.track_code)?
        .ok_or_else(|| EngineError::NoFile(assignment.track_code.clone()))?;
    if track.state != TrackState::InCustody {
        return Err(EngineError::WrongAssignmentState(assignment.status));
    }
    let due_at = assignment.due_at.ok_or(EngineError::NoDeadline)?;
    if approve {
        DeadlineRepo::ensure(&transaction, &assignment_id)?;
        let deadline = DeadlineRepo::get(&transaction, &assignment_id)?
            .ok_or_else(|| EngineError::NoDeadline)?;
        if deadline.extension_count >= 2 {
            HostDecisionRepo::close(&transaction, decision_id, "refused", now, "extension limit")?;
            commit(transaction)?;
            return Ok(ExtensionOutcome {
                status: ExtensionStatus::Refused,
                due_at: Some(due_at),
                decision_id: Some(decision_id.to_string()),
            });
        }
        let hours = decision.requested_hours.unwrap_or(6);
        let new_due = due_at.max(now) + hours * 3_600;
        if !AssignmentRepo::extend(&transaction, &assignment_id, new_due, now)? {
            return Err(EngineError::NoActiveAssignment);
        }
        DeadlineRepo::increment_extension(&transaction, &assignment_id)?;
        HostDecisionRepo::close(&transaction, decision_id, "approved", now, reason)?;
        if let Some(open) =
            HostDecisionRepo::open_for_assignment(&transaction, &assignment_id, "overdue")?
        {
            HostDecisionRepo::close(&transaction, &open.id, "addressed", now, reason)?;
        }
        AuditRepo::insert(
            &transaction,
            &decision.event_id,
            now,
            "Host",
            "Extension approved",
            None,
            &serde_json::json!({ "hours": hours, "assignmentId": assignment_id }).to_string(),
        )?;
        enqueue_effect(
            &transaction,
            &decision.event_id,
            &Effect::Dm {
                user_id: assignment.participant_id.clone(),
                content: format!("The host approved your extension for {hours} hours."),
            },
            now,
        )?;
        commit(transaction)?;
        return Ok(ExtensionOutcome {
            status: ExtensionStatus::Approved,
            due_at: Some(new_due),
            decision_id: Some(decision_id.to_string()),
        });
    }
    HostDecisionRepo::close(&transaction, decision_id, "refused", now, reason)?;
    AuditRepo::insert(
        &transaction,
        &decision.event_id,
        now,
        "Host",
        "Extension refused",
        None,
        &serde_json::json!({ "assignmentId": assignment_id }).to_string(),
    )?;
    enqueue_effect(
        &transaction,
        &decision.event_id,
        &Effect::Dm {
            user_id: assignment.participant_id.clone(),
            content:
                "The host refused the extension. Submit, skip, or ask the host for a decision."
                    .to_string(),
        },
        now,
    )?;
    commit(transaction)?;
    Ok(ExtensionOutcome {
        status: ExtensionStatus::Refused,
        due_at: Some(due_at),
        decision_id: Some(decision_id.to_string()),
    })
}

pub fn decide_deadline(
    db: &Db,
    assignment_id: &str,
    action: &str,
    hours: i64,
    reason: &str,
    now: i64,
) -> Result<DeadlineDecisionOutcome, EngineError> {
    let transaction = begin(db)?;
    let assignment = AssignmentRepo::get(&transaction, assignment_id)?
        .ok_or_else(|| EngineError::AssignmentNotFound(assignment_id.to_string()))?;
    let event = EventRepo::get(&transaction, &assignment.event_id)?
        .ok_or_else(|| EngineError::EventNotFound(assignment.event_id.clone()))?;
    require_open_event(&event, "deadline decision")?;
    if !matches!(
        assignment.status,
        AssignmentStatus::Claimed | AssignmentStatus::Overdue
    ) {
        return Err(EngineError::WrongAssignmentState(assignment.status));
    }
    let track = TrackRepo::get(&transaction, &assignment.event_id, &assignment.track_code)?
        .ok_or_else(|| EngineError::NoFile(assignment.track_code.clone()))?;
    if track.state != TrackState::InCustody {
        return Err(EngineError::WrongAssignmentState(assignment.status));
    }
    let due_at = assignment.due_at.ok_or(EngineError::NoDeadline)?;
    let decision = HostDecisionRepo::open_for_assignment(&transaction, assignment_id, "overdue")?;
    match action {
        "extend" => {
            if !(1..=12).contains(&hours) {
                return Err(EngineError::InvalidExtensionHours);
            }
            DeadlineRepo::ensure(&transaction, assignment_id)?;
            let deadline = DeadlineRepo::get(&transaction, assignment_id)?
                .ok_or_else(|| EngineError::NoDeadline)?;
            if deadline.extension_count >= 2 {
                return Err(EngineError::ExtensionLimit);
            }
            let new_due = due_at.max(now) + hours * 3_600;
            if !AssignmentRepo::extend(&transaction, assignment_id, new_due, now)? {
                return Err(EngineError::NoActiveAssignment);
            }
            DeadlineRepo::increment_extension(&transaction, assignment_id)?;
            if let Some(decision) = decision {
                HostDecisionRepo::close(&transaction, &decision.id, "addressed", now, reason)?;
            }
            AuditRepo::insert(
                &transaction,
                &assignment.event_id,
                now,
                "Host",
                "Deadline extended",
                None,
                &serde_json::json!({ "hours": hours, "assignmentId": assignment_id }).to_string(),
            )?;
            enqueue_effect(
                &transaction,
                &assignment.event_id,
                &Effect::Dm {
                    user_id: assignment.participant_id.clone(),
                    content: format!("The host extended your deadline by {hours} hours."),
                },
                now,
            )?;
            commit(transaction)?;
            Ok(DeadlineDecisionOutcome {
                action: "extend".to_string(),
                due_at: Some(new_due),
            })
        }
        "missed" => {
            SeenRepo::record(
                &transaction,
                &assignment.event_id,
                &assignment.track_code,
                &assignment.participant_id,
                "deadline_missed",
                now,
            )?;
            AssignmentRepo::update_status(
                &transaction,
                assignment_id,
                AssignmentStatus::SkippedAfterClaim,
                now,
            )?;
            move_track(
                &transaction,
                &assignment.event_id,
                &assignment.track_code,
                TrackState::Pool,
            )?;
            if let Some(decision) = decision {
                HostDecisionRepo::close(&transaction, &decision.id, "addressed", now, reason)?;
            }
            AuditRepo::insert(
                &transaction,
                &assignment.event_id,
                now,
                "Host",
                "Turn marked missed",
                None,
                &serde_json::json!({ "assignmentId": assignment_id }).to_string(),
            )?;
            enqueue_effect(
                &transaction,
                &assignment.event_id,
                &Effect::Dm {
                    user_id: assignment.participant_id.clone(),
                    content: format!(
                        "The host marked the turn for track {} as missed. The track returns to the pool.",
                        assignment.track_code
                    ),
                },
                now,
            )?;
            commit(transaction)?;
            Ok(DeadlineDecisionOutcome {
                action: "missed".to_string(),
                due_at: None,
            })
        }
        "wait" => {
            AuditRepo::insert(
                &transaction,
                &assignment.event_id,
                now,
                "Host",
                "Deadline kept open",
                None,
                &serde_json::json!({ "assignmentId": assignment_id }).to_string(),
            )?;
            commit(transaction)?;
            Ok(DeadlineDecisionOutcome {
                action: "wait".to_string(),
                due_at: Some(due_at),
            })
        }
        _ => Err(EngineError::WrongAssignmentState(assignment.status)),
    }
}

pub fn open_decisions(db: &Db, event_id: &str) -> Result<Vec<HostDecisionRecord>, EngineError> {
    Ok(HostDecisionRepo::list_open(db.conn(), event_id)?)
}

pub fn tick_deadlines(db: &Db, now: i64) -> Result<DeadlineReport, EngineError> {
    let events = EventRepo::list_active(db.conn())?;
    let mut report = DeadlineReport::default();
    for event in events {
        let transaction = begin(db)?;
        let current = EventRepo::get(&transaction, &event.id)?
            .ok_or_else(|| EngineError::EventNotFound(event.id.clone()))?;
        if current.state != EventState::Active {
            continue;
        }
        let participants = ParticipantRepo::list_for_event(&transaction, &event.id)?;
        let assignments = AssignmentRepo::list_active(&transaction, &event.id)?;
        for assignment in assignments {
            if assignment.status == AssignmentStatus::Offered {
                continue;
            }
            let Some(due_at) = assignment.due_at else {
                continue;
            };
            DeadlineRepo::ensure(&transaction, &assignment.id)?;
            let claimed_at = assignment.claimed_at.unwrap_or(assignment.offered_at);
            if now >= due_at {
                if assignment.status == AssignmentStatus::Claimed {
                    AssignmentRepo::update_status(
                        &transaction,
                        &assignment.id,
                        AssignmentStatus::Overdue,
                        now,
                    )?;
                    report.overdue += 1;
                    if HostDecisionRepo::open_for_assignment(
                        &transaction,
                        &assignment.id,
                        "overdue",
                    )?
                    .is_none()
                    {
                        HostDecisionRepo::insert(
                            &transaction,
                            &HostDecisionRecord {
                                id: format!("{}:overdue:{now}", assignment.id),
                                event_id: event.id.clone(),
                                assignment_id: Some(assignment.id.clone()),
                                kind: "overdue".to_string(),
                                status: "open".to_string(),
                                requested_hours: None,
                                created_at: now,
                                decided_at: None,
                                reason: None,
                            },
                        )?;
                        AuditRepo::insert(
                            &transaction,
                            &event.id,
                            now,
                            "worker",
                            "Deadline passed",
                            None,
                            &serde_json::json!({ "assignmentId": assignment.id }).to_string(),
                        )?;
                        enqueue_effect(
                            &transaction,
                            &event.id,
                            &Effect::HostAlert {
                                key: assignment.id.clone(),
                                content: "A track deadline needs a host decision.".to_string(),
                            },
                            now,
                        )?;
                        report.decisions += 1;
                    }
                }
                if DeadlineRepo::mark_reminder(&transaction, &assignment.id, "deadline_reminded")? {
                    enqueue_effect(
                        &transaction,
                        &event.id,
                        &Effect::Dm {
                            user_id: assignment.participant_id.clone(),
                            content: format!(
                                "The edit deadline for track {} has passed. The host decides the next step.",
                                assignment.track_code
                            ),
                        },
                        now,
                    )?;
                    report.reminders += 1;
                }
                continue;
            }
            let total = due_at - claimed_at;
            if total <= 0 || now < claimed_at {
                continue;
            }
            let percent = ((now - claimed_at) * 100) / total;
            let quiet = participant_timezone(&participants, &assignment.participant_id)
                .is_some_and(|tz| quiet_hours(now, tz));
            for (step, column) in [
                (50_i64, "reminded_50"),
                (80, "reminded_80"),
                (95, "reminded_95"),
            ] {
                if percent < step || quiet {
                    continue;
                }
                if DeadlineRepo::mark_reminder(&transaction, &assignment.id, column)? {
                    enqueue_effect(
                        &transaction,
                        &event.id,
                        &Effect::Dm {
                            user_id: assignment.participant_id.clone(),
                            content: format!(
                                "Track {}: {step} percent of the edit window has passed. Send your edit before the deadline.",
                                assignment.track_code
                            ),
                        },
                        now,
                    )?;
                    report.reminders += 1;
                }
            }
        }
        commit(transaction)?;
    }
    Ok(report)
}

fn participant_timezone(participants: &[ParticipantRecord], user_id: &str) -> Option<Tz> {
    participants
        .iter()
        .find(|participant| participant.user_id == user_id)
        .and_then(|participant| participant.timezone.parse().ok())
}

fn quiet_hours(now: i64, timezone: Tz) -> bool {
    Utc.timestamp_opt(now, 0)
        .single()
        .map(|time| time.with_timezone(&timezone).hour())
        .is_some_and(|hour| !(8..22).contains(&hour))
}

pub fn event_status(db: &Db, event_id: &str) -> Result<EventStatus, EngineError> {
    let event = load_event(db, event_id)?;
    let plan = PlanRepo::latest(db.conn(), event_id)?;
    let assignments = AssignmentRepo::list_for_event(db.conn(), event_id)?;
    Ok(EventStatus {
        event_id: event.id,
        state: event.state,
        rounds: plan.as_ref().map(|plan| plan.rounds).unwrap_or(0),
        committed: plan.is_some(),
        active_assignments: assignments
            .iter()
            .filter(|assignment| {
                matches!(
                    assignment.status,
                    AssignmentStatus::Offered
                        | AssignmentStatus::Claimed
                        | AssignmentStatus::Overdue
                )
            })
            .count(),
        submitted_assignments: assignments
            .iter()
            .filter(|assignment| assignment.status == AssignmentStatus::Submitted)
            .count(),
    })
}

fn begin(db: &Db) -> Result<Transaction<'_>, EngineError> {
    Ok(
        Transaction::new_unchecked(db.conn(), TransactionBehavior::Immediate)
            .map_err(DbError::from)?,
    )
}

fn commit(transaction: Transaction<'_>) -> Result<(), EngineError> {
    transaction.commit().map_err(DbError::from)?;
    Ok(())
}

fn load_event(db: &Db, event_id: &str) -> Result<EventRecord, EngineError> {
    EventRepo::get(db.conn(), event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))
}

fn parse_settings(event: &EventRecord) -> Result<EventSettings, EngineError> {
    Ok(serde_json::from_str(&event.settings_json)?)
}

fn require_state(
    event: &EventRecord,
    expected: EventState,
    action: &'static str,
    expected_label: &'static str,
) -> Result<(), EngineError> {
    if event.state == expected {
        Ok(())
    } else {
        Err(EngineError::WrongEventState {
            action,
            state: event.state,
            expected: expected_label,
        })
    }
}

fn open_event_in(
    conn: &Connection,
    event_id: &str,
    action: &'static str,
) -> Result<EventRecord, EngineError> {
    let event = EventRepo::get(conn, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if !matches!(event.state, EventState::Active | EventState::Paused) {
        return Err(EngineError::WrongEventState {
            action,
            state: event.state,
            expected: "active or paused",
        });
    }
    Ok(event)
}

fn require_open_event(event: &EventRecord, action: &'static str) -> Result<(), EngineError> {
    match event.state {
        EventState::Active | EventState::Paused => Ok(()),
        state => Err(EngineError::WrongEventState {
            action,
            state,
            expected: "active or paused",
        }),
    }
}

fn unique_track_code(tracks: &[TrackRecord], seed: u64) -> String {
    let mut salt = 0u32;
    loop {
        let candidate = track_code(seed, salt);
        if !tracks.iter().any(|track| track.code == candidate) {
            return candidate;
        }
        salt = salt.wrapping_add(1);
    }
}

struct StoredVersion<'a> {
    delivery: &'a StoredBlob,
    original: &'a StoredBlob,
    prepared: &'a PreparedBytes,
}

fn file_record(
    event_id: &str,
    track_code: &str,
    version: u32,
    holder_id: &str,
    validated: &ValidatedFile,
    stored: StoredVersion<'_>,
    now: i64,
) -> FileRecord {
    FileRecord {
        id: format!("{event_id}:{track_code}:{version}"),
        event_id: event_id.to_string(),
        track_code: track_code.to_string(),
        version,
        holder_id: holder_id.to_string(),
        sha256: stored.delivery.sha256.clone(),
        size: stored.delivery.size,
        extension: validated.extension.clone(),
        storage_path: stored.delivery.relative_path.clone(),
        original_sha256: Some(stored.original.sha256.clone()),
        original_storage_path: Some(stored.original.relative_path.clone()),
        audio_verified: stored.prepared.audio_verified,
        scrub_summary_json: stored.prepared.scrub_summary_json.clone(),
        created_at: now,
    }
}

fn latest_file(db: &Db, event_id: &str, track_code: &str) -> Result<FileRecord, EngineError> {
    FileRepo::list_for_track(db.conn(), event_id, track_code)?
        .pop()
        .ok_or_else(|| EngineError::NoFile(track_code.to_string()))
}

fn move_track(
    connection: &Connection,
    event_id: &str,
    track_code: &str,
    target: TrackState,
) -> Result<(), EngineError> {
    for _ in 0..4 {
        let track = TrackRepo::get(connection, event_id, track_code)?
            .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
        if track.state == target {
            return Ok(());
        }
        let next = if track.state.can_transition_to(target) {
            target
        } else if track.state.can_transition_to(TrackState::Pool) {
            TrackState::Pool
        } else {
            return Err(EngineError::TrackPath(track_code.to_string()));
        };
        TrackRepo::set_state(connection, event_id, track_code, next)?;
    }
    Ok(())
}

fn is_safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
}

#[cfg(test)]
mod reveal_tests {
    use super::*;

    #[test]
    fn reveal_messages_respect_the_discord_limit() {
        let track = TrackCredits {
            track_code: "SAAAA".to_string(),
            seed_owner: "u1".to_string(),
            seed_owner_name: "Seed".to_string(),
            entries: vec![CreditEntry {
                round: 0,
                participant_id: "u2".to_string(),
                display_name: "x".repeat(3_000),
                status: AssignmentStatus::Submitted,
                fill_in: false,
            }],
        };
        let messages = reveal_messages("Long reveal", &[track]);
        assert!(messages.len() > 1);
        assert!(
            messages
                .iter()
                .all(|message| message.chars().count() <= 1_900)
        );
    }
}
