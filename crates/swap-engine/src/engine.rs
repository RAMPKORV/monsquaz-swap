use chrono::{TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use swap_core::{
    AssignmentStatus, EventState, Membership, ParticipantId, Readiness, Role, Schedule, StateError,
    TrackCode, TrackSpec, TrackState, album_order,
};
use swap_db::{
    AssignmentRecord, AssignmentRepo, AuditRepo, Connection, Db, DbError, DeadlineRepo,
    EventRecord, EventRepo, FileRecord, FileRepo, HostDecisionRecord, HostDecisionRepo,
    ParticipantRecord, ParticipantRepo, PlanAssignmentRecord, PlanRecord, PlanRepo,
    ProcessedAttachmentRepo, SeenRepo, TrackCommentRecord, TrackCommentRepo, TrackRecord,
    TrackRepo, TrackTitleRecord, TrackTitleRepo, Transaction, TransactionBehavior,
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
    #[error("track not found: {0}")]
    TrackNotFound(String),
    #[error("the track title is empty")]
    EmptyTitle,
    #[error("the track title is longer than 80 characters")]
    TitleTooLong,
    #[error("only the latest holder or the host can name this track")]
    NotTitleHolder,
    #[error("the host has already named this track")]
    TitleAlreadySet,
    #[error("the comment is empty")]
    EmptyComment,
    #[error("the comment is longer than 1000 characters")]
    CommentTooLong,
    #[error("the comment kind must be comment or questionnaire")]
    InvalidCommentKind,
    #[error("the comment was not found: {0}")]
    CommentNotFound(String),
    #[error(transparent)]
    GroupSchedule(#[from] swap_core::GroupScheduleError),
    #[error("the groups do not need repair")]
    GroupRepairNotNeeded,
    #[error("the comment is already redacted")]
    CommentAlreadyRedacted,
    #[error("the participant did not work on this track")]
    NotTrackContributor,
    #[error("plan cell not found: round {round} track {track}")]
    PlanCellNotFound { round: u32, track: String },
    #[error("the participant is not assigned in this round")]
    PlanParticipantNotInRound,
    #[error("the assignment did not change")]
    PlanNoChange,
    #[error("the change would give {participant} their own track {track}")]
    PlanOwnTrack { participant: String, track: String },
    #[error("the change would give {participant} track {track} twice")]
    PlanRepeat { participant: String, track: String },
    #[error("the plan changed; reload and try again")]
    PlanChanged,
    #[error("the plan is not valid: {0}")]
    PlanInvalid(String),
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
pub struct GroupRepairOutcome {
    pub version: u32,
    pub commitment: String,
    pub previous_sizes: Vec<u32>,
    pub sizes: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanEditCandidate {
    pub participant_id: String,
    pub participant_name: String,
    pub current_track: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartOutcome {
    pub plan_version: u32,
    pub commitment: String,
    pub effects: Vec<Effect>,
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
    pub title: Option<String>,
    pub seed_owner: String,
    pub seed_owner_name: String,
    pub entries: Vec<CreditEntry>,
    pub comments: Vec<RevealComment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealOutcome {
    pub tracks: Vec<TrackCredits>,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealPackage {
    pub event_name: String,
    pub event_state: EventState,
    pub published: bool,
    pub album: Vec<RevealTrack>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealTrack {
    pub position: u32,
    pub track_code: String,
    pub title: Option<String>,
    pub seed_owner_id: String,
    pub seed_owner_name: String,
    pub credits: Vec<CreditEntry>,
    pub comments: Vec<RevealComment>,
    pub versions: Vec<RevealVersion>,
    pub stats: RevealStats,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealComment {
    pub id: String,
    pub participant_id: String,
    pub author_name: String,
    pub kind: String,
    pub body: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealVersion {
    pub version: u32,
    pub holder_id: String,
    pub holder_name: String,
    pub role: RevealVersionRole,
    pub round: Option<u32>,
    pub file_id: String,
    pub extension: String,
    pub created_at: i64,
    pub playable: bool,
    pub final_version: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevealVersionRole {
    Seed,
    Edit,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RevealStats {
    pub edits: u32,
    pub fill_ins: u32,
    pub skips: u32,
    pub contributors: u32,
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
    generate_schedule_internal(db, event_id, now, None, None)
}

/// Merge or re-split groups before the event starts.
///
/// The repair runs only when the current plan has a group below the soft
/// minimum or a size difference larger than one. A routine plan generation
/// does not merge groups.
pub fn repair_group_schedule(
    db: &Db,
    event_id: &str,
    now: i64,
) -> Result<GroupRepairOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_state(
        &event,
        EventState::ScheduleReview,
        "group repair",
        "schedule review",
    )?;
    let plan = PlanRepo::latest(db.conn(), event_id)?.ok_or(EngineError::NoPlan)?;
    let settings: EventSettings = serde_json::from_str(&plan.settings_json).unwrap_or_default();
    let participants = ParticipantRepo::list_for_event(db.conn(), event_id)?;
    let active = participants
        .iter()
        .filter(|participant| {
            participant.membership == Membership::Active
                && participant.role == Role::Participant
                && participant.readiness == Readiness::Ready
        })
        .count();
    if active == 0 {
        return Err(EngineError::NoParticipants);
    }
    let count = settings
        .group_count
        .unwrap_or(1)
        .clamp(1, swap_core::MAX_GROUP_COUNT);
    let previous_sizes = swap_core::group_sizes(active, count, settings.group_size.unwrap_or(0));
    if previous_sizes.len() <= 1 {
        return Err(EngineError::GroupRepairNotNeeded);
    }
    let smallest = previous_sizes.iter().copied().min().unwrap_or(0);
    let largest = previous_sizes.iter().copied().max().unwrap_or(0);
    if smallest >= swap_core::MIN_GROUP_SIZE && largest.saturating_sub(smallest) <= 1 {
        return Err(EngineError::GroupRepairNotNeeded);
    }
    let next_count = if smallest < swap_core::MIN_GROUP_SIZE {
        count.saturating_sub(1).max(1)
    } else {
        count
    };
    let sizes = swap_core::group_sizes(active, next_count, 0);
    let schedule = generate_schedule_internal(
        db,
        event_id,
        now,
        Some((next_count, 0)),
        Some("group repair"),
    )?;
    Ok(GroupRepairOutcome {
        version: schedule.version,
        commitment: schedule.commitment,
        previous_sizes,
        sizes,
    })
}

fn generate_schedule_internal(
    db: &Db,
    event_id: &str,
    now: i64,
    group_override: Option<(u32, u32)>,
    plan_reason: Option<&str>,
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
            participant.membership == Membership::Active
                && participant.role == Role::Participant
                && participant.readiness == Readiness::Ready
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
    let mut settings = settings;
    if let Some((count, size)) = group_override {
        let sizes = swap_core::group_sizes(active.len(), count, size);
        settings.group_count = Some(sizes.len() as u32);
        settings.group_size = Some(sizes.iter().copied().max().unwrap_or(0));
        settings.group_mode = Some("Manual".to_string());
    } else if settings.mode == Mode::Classic {
        match settings.group_mode.as_deref().unwrap_or("Lookup") {
            "Manual" => {
                let count = settings
                    .group_count
                    .unwrap_or(1)
                    .clamp(1, swap_core::MAX_GROUP_COUNT);
                let size = settings.group_size.unwrap_or(0);
                let sizes = swap_core::group_sizes(active.len(), count, size);
                settings.group_count = Some(sizes.len() as u32);
                settings.group_size = Some(sizes.iter().copied().max().unwrap_or(0));
            }
            "Single group" => {
                settings.group_count = Some(1);
                settings.group_size = Some(active.len() as u32);
            }
            _ => {
                let shape = swap_core::suggest_groups(active.len());
                settings.group_count = Some(shape.count);
                settings.group_size = Some(shape.size);
            }
        }
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
    let (assignments, fallback) =
        if settings.mode == Mode::Classic && settings.group_count.unwrap_or(1) > 1 {
            let mut grouped: Vec<&ParticipantRecord> = active.clone();
            grouped.sort_by(|left, right| {
                left.display_name
                    .cmp(&right.display_name)
                    .then_with(|| left.user_id.cmp(&right.user_id))
            });
            let group_people: Vec<ParticipantId> = grouped
                .iter()
                .map(|participant| ParticipantId::new(participant.user_id.clone()))
                .collect();
            let group_specs: Vec<TrackSpec> = tracks
                .iter()
                .map(|track| TrackSpec {
                    code: TrackCode::new(track.code.clone()),
                    owner: ParticipantId::new(track.owner_id.clone()),
                })
                .collect();
            let group_sizes = swap_core::group_sizes(
                grouped.len(),
                settings.group_count.unwrap_or(1),
                settings.group_size.unwrap_or(0),
            );
            let cadence = if settings.swap_cadence.as_deref() == Some("Exhausted") {
                let largest = group_sizes.iter().copied().max().unwrap_or(1);
                largest.saturating_sub(1).max(1)
            } else {
                swap_core::default_cadence(rounds)
            };
            let group_plan = swap_core::group_schedule(
                &group_people,
                &group_specs,
                &group_sizes,
                rounds,
                cadence,
                plan_seed,
            )?;
            let fallback = group_plan.phases.iter().any(|phase| phase.global_pool);
            (group_plan.assignments, fallback)
        } else {
            let schedule = Schedule::generate_with(&people, &specs, rounds, plan_seed, strategy)?;
            (schedule.assignments, schedule.fallback)
        };

    let settings_json = serde_json::to_string(&settings)?;
    let planned: Vec<PlanAssignmentRecord> = assignments
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
        reason: plan_reason.map(str::to_string),
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
        fallback,
        assignments: planned,
    })
}

type PlanRoster = (
    std::collections::BTreeSet<String>,
    std::collections::BTreeMap<String, String>,
    std::collections::BTreeMap<String, String>,
);

fn plan_roster(conn: &Connection, event_id: &str) -> Result<PlanRoster, EngineError> {
    let participants = ParticipantRepo::list_for_event(conn, event_id)?;
    let active = participants
        .iter()
        .filter(|participant| {
            participant.membership == Membership::Active
                && participant.role == Role::Participant
                && participant.readiness == Readiness::Ready
        })
        .map(|participant| participant.user_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let names = participants
        .into_iter()
        .map(|participant| (participant.user_id, participant.display_name))
        .collect::<std::collections::BTreeMap<_, _>>();
    let owners = TrackRepo::list_for_event(conn, event_id)?
        .into_iter()
        .map(|track| (track.code, track.owner_id))
        .collect::<std::collections::BTreeMap<_, _>>();
    Ok((active, owners, names))
}

fn swapped_assignments(
    assignments: &[PlanAssignmentRecord],
    round: u32,
    track_code: &str,
    participant_id: &str,
) -> Result<Vec<PlanAssignmentRecord>, EngineError> {
    let target = assignments
        .iter()
        .find(|assignment| assignment.round == round && assignment.track_code == track_code)
        .cloned()
        .ok_or_else(|| EngineError::PlanCellNotFound {
            round,
            track: track_code.to_string(),
        })?;
    if target.participant_id == participant_id {
        return Err(EngineError::PlanNoChange);
    }
    let incoming = assignments
        .iter()
        .find(|assignment| assignment.round == round && assignment.participant_id == participant_id)
        .cloned()
        .ok_or(EngineError::PlanParticipantNotInRound)?;
    let displaced = target.participant_id;
    let mut swapped = assignments.to_vec();
    for assignment in &mut swapped {
        if assignment.round == round
            && assignment.track_code == track_code
            && assignment.participant_id == displaced
        {
            assignment.participant_id = participant_id.to_string();
        } else if assignment.round == round
            && assignment.track_code == incoming.track_code
            && assignment.participant_id == participant_id
        {
            assignment.participant_id = displaced.clone();
        }
    }
    Ok(swapped)
}

fn validate_plan_assignments(
    assignments: &[PlanAssignmentRecord],
    rounds: u32,
    active: &std::collections::BTreeSet<String>,
    owners: &std::collections::BTreeMap<String, String>,
) -> Result<(), EngineError> {
    let active_tracks = owners
        .iter()
        .filter(|(_, owner)| active.contains(*owner))
        .map(|(track, _)| track.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if active.is_empty() || active_tracks.is_empty() {
        return Err(EngineError::NoParticipants);
    }
    let mut pairs = std::collections::BTreeSet::new();
    for assignment in assignments {
        let owner = owners
            .get(&assignment.track_code)
            .ok_or_else(|| EngineError::TrackNotFound(assignment.track_code.clone()))?;
        if !active.contains(&assignment.participant_id) {
            return Err(EngineError::PlanInvalid(format!(
                "{} is not active and ready",
                assignment.participant_id
            )));
        }
        if !active.contains(owner) {
            return Err(EngineError::PlanInvalid(format!(
                "track {} belongs to an inactive participant",
                assignment.track_code
            )));
        }
        if owner == &assignment.participant_id {
            return Err(EngineError::PlanOwnTrack {
                participant: assignment.participant_id.clone(),
                track: assignment.track_code.clone(),
            });
        }
        if !pairs.insert((
            assignment.participant_id.clone(),
            assignment.track_code.clone(),
        )) {
            return Err(EngineError::PlanRepeat {
                participant: assignment.participant_id.clone(),
                track: assignment.track_code.clone(),
            });
        }
    }
    for round in 0..rounds {
        let people = assignments
            .iter()
            .filter(|assignment| assignment.round == round)
            .map(|assignment| assignment.participant_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let tracks = assignments
            .iter()
            .filter(|assignment| assignment.round == round)
            .map(|assignment| assignment.track_code.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if people.len() != active.len() || tracks.len() != active_tracks.len() {
            return Err(EngineError::PlanInvalid(format!(
                "round {round} does not cover every active person and track"
            )));
        }
        if !active.iter().all(|person| people.contains(person.as_str())) {
            return Err(EngineError::PlanInvalid(format!(
                "round {round} is missing an active person"
            )));
        }
        if !active_tracks
            .iter()
            .all(|track| tracks.contains(track.as_str()))
        {
            return Err(EngineError::PlanInvalid(format!(
                "round {round} is missing an active track"
            )));
        }
    }
    Ok(())
}

/// List the people in this round who can take the track without breaking a
/// rule. The list is empty when no single-cell swap is valid.
pub fn plan_edit_candidates(
    db: &Db,
    event_id: &str,
    round: u32,
    track_code: &str,
) -> Result<Vec<PlanEditCandidate>, EngineError> {
    let event = load_event(db, event_id)?;
    require_state(
        &event,
        EventState::ScheduleReview,
        "schedule edit",
        "schedule review",
    )?;
    let plan = PlanRepo::latest(db.conn(), event_id)?.ok_or(EngineError::NoPlan)?;
    if round >= plan.rounds {
        return Err(EngineError::RoundOutOfRange {
            round,
            rounds: plan.rounds,
        });
    }
    let assignments = PlanRepo::list_assignments(db.conn(), event_id, plan.version)?;
    let (active, owners, names) = plan_roster(db.conn(), event_id)?;
    validate_plan_assignments(&assignments, plan.rounds, &active, &owners)?;
    let target = assignments
        .iter()
        .find(|assignment| assignment.round == round && assignment.track_code == track_code)
        .ok_or_else(|| EngineError::PlanCellNotFound {
            round,
            track: track_code.to_string(),
        })?;
    let mut candidates = Vec::new();
    for assignment in assignments.iter().filter(|assignment| {
        assignment.round == round && assignment.participant_id != target.participant_id
    }) {
        let swapped =
            swapped_assignments(&assignments, round, track_code, &assignment.participant_id)?;
        if validate_plan_assignments(&swapped, plan.rounds, &active, &owners).is_ok() {
            candidates.push(PlanEditCandidate {
                participant_id: assignment.participant_id.clone(),
                participant_name: names
                    .get(&assignment.participant_id)
                    .cloned()
                    .unwrap_or_else(|| assignment.participant_id.clone()),
                current_track: assignment.track_code.clone(),
            });
        }
    }
    candidates.sort_by(|left, right| left.participant_name.cmp(&right.participant_name));
    Ok(candidates)
}

/// Change one assignment before the event starts.
///
/// The incoming participant and the current holder swap their tracks in this
/// round. The whole plan is validated and a new plan version with a new
/// commitment is written. The old version stays in the database.
pub fn edit_plan_assignment(
    db: &Db,
    event_id: &str,
    round: u32,
    track_code: &str,
    participant_id: &str,
    expected_version: u32,
    now: i64,
) -> Result<ScheduleOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_state(
        &event,
        EventState::ScheduleReview,
        "schedule edit",
        "schedule review",
    )?;
    let plan = PlanRepo::latest(db.conn(), event_id)?.ok_or(EngineError::NoPlan)?;
    if plan.version != expected_version {
        return Err(EngineError::PlanChanged);
    }
    if round >= plan.rounds {
        return Err(EngineError::RoundOutOfRange {
            round,
            rounds: plan.rounds,
        });
    }
    let assignments = PlanRepo::list_assignments(db.conn(), event_id, plan.version)?;
    let (active, owners, _) = plan_roster(db.conn(), event_id)?;
    validate_plan_assignments(&assignments, plan.rounds, &active, &owners)?;
    let target = assignments
        .iter()
        .find(|assignment| assignment.round == round && assignment.track_code == track_code)
        .cloned()
        .ok_or_else(|| EngineError::PlanCellNotFound {
            round,
            track: track_code.to_string(),
        })?;
    let displaced = target.participant_id.clone();
    let mut assignments = swapped_assignments(&assignments, round, track_code, participant_id)?;
    validate_plan_assignments(&assignments, plan.rounds, &active, &owners)?;

    let version = plan.version + 1;
    for assignment in &mut assignments {
        assignment.plan_version = version;
    }
    let mut next = PlanRecord {
        event_id: event_id.to_string(),
        version,
        seed: plan.seed,
        rounds: plan.rounds,
        settings_json: plan.settings_json,
        commitment: String::new(),
        committed_at: now,
        reason: Some("manual edit".to_string()),
    };
    let commitment = plan_commitment(&next, &assignments);
    next.commitment = commitment.clone();

    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if current.state != EventState::ScheduleReview {
        return Err(EngineError::WrongEventState {
            action: "schedule edit",
            state: current.state,
            expected: "schedule review",
        });
    }
    if PlanRepo::latest(&transaction, event_id)?.map(|latest| latest.version) != Some(plan.version)
    {
        return Err(EngineError::PlanChanged);
    }
    PlanRepo::insert(&transaction, &next)?;
    for assignment in &assignments {
        PlanRepo::insert_assignment(&transaction, assignment)?;
    }
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Schedule edited",
        Some(&format!("Plan v{version}")),
        &serde_json::json!({
            "round": round,
            "trackCode": track_code,
            "from": displaced,
            "to": participant_id,
            "commitment": commitment
        })
        .to_string(),
    )?;
    commit(transaction)?;

    Ok(ScheduleOutcome {
        version,
        rounds: plan.rounds,
        commitment,
        fallback: false,
        assignments,
    })
}

pub fn start_event(db: &Db, event_id: &str, now: i64) -> Result<StartOutcome, EngineError> {
    start_event_with_channel(db, event_id, None, now)
}

/// Start the event and publish the plan commitment in the announcement
/// channel. The state change and the outbound message commit together.
pub fn start_event_with_channel(
    db: &Db,
    event_id: &str,
    channel_id: Option<&str>,
    now: i64,
) -> Result<StartOutcome, EngineError> {
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
    let plan = PlanRepo::latest(&transaction, event_id)?.ok_or(EngineError::NoPlan)?;
    let assignments = PlanRepo::list_assignments(&transaction, event_id, plan.version)?;
    let (active, owners, _) = plan_roster(&transaction, event_id)?;
    validate_plan_assignments(&assignments, plan.rounds, &active, &owners)?;
    let next = current.state.transition_to(EventState::Active)?;
    EventRepo::set_state(&transaction, event_id, next, now)?;
    let effect = channel_id
        .filter(|channel_id| !channel_id.trim().is_empty())
        .map(|channel_id| Effect::Channel {
            channel_id: channel_id.to_string(),
            content: format!(
                "Schedule locked for {}. Commitment: {}. Compare this code with your plan.",
                event.name,
                &plan.commitment[..16.min(plan.commitment.len())]
            ),
        });
    if let Some(effect) = &effect {
        enqueue_effect(&transaction, event_id, effect, now)?;
    }
    commit(transaction)?;
    Ok(StartOutcome {
        plan_version: plan.version,
        commitment: plan.commitment,
        effects: effect.into_iter().collect(),
    })
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
    let (title_request, fallback_request) = if assignment.status != AssignmentStatus::Submitted
        && let Some(plan) = PlanRepo::latest(&transaction, request.event_id)?
        && assignment.round + 1 == plan.rounds
        && TrackTitleRepo::get(&transaction, request.event_id, &assignment.track_code)?.is_none()
    {
        let final_request = Effect::Dm {
            user_id: request.user_id.to_string(),
            content: format!(
                "Your edit closes track {}. Name the final track with /swap title in the server.",
                assignment.track_code
            ),
        };
        let fallback = title_fallback_holder(
            &transaction,
            request.event_id,
            &assignment.track_code,
            assignment.round,
        )?
        .map(|(user_id, previous_round)| Effect::Dm {
            user_id,
            content: format!(
                "You submitted track {} in round {}. It has no final title yet. If the final holder does not name it, name it with /swap title title:<name> track:{}.",
                assignment.track_code,
                previous_round + 1,
                assignment.track_code
            ),
        });
        (Some(final_request), fallback)
    } else {
        (None, None)
    };
    if let Some(effect) = &title_request {
        enqueue_effect(&transaction, request.event_id, effect, now)?;
    }
    if let Some(effect) = &fallback_request {
        enqueue_effect(&transaction, request.event_id, effect, now)?;
    }
    commit(transaction)?;

    let mut effects = vec![receipt];
    if let Some(effect) = title_request {
        effects.push(effect);
    }
    if let Some(effect) = fallback_request {
        effects.push(effect);
    }
    Ok(SubmitOutcome {
        version,
        sha256: delivery_blob.sha256,
        original_sha256: original_blob.sha256,
        effects,
    })
}

/// Return the participant who submitted the same track in the round before
/// the final round. That person can name the track when the final holder
/// does not. The round number gives the recipient context for the track code.
fn title_fallback_holder(
    conn: &Connection,
    event_id: &str,
    track_code: &str,
    final_round: u32,
) -> Result<Option<(String, u32)>, EngineError> {
    let assignments = AssignmentRepo::list_for_event(conn, event_id)?;
    Ok(assignments
        .into_iter()
        .filter(|assignment| {
            assignment.track_code == track_code
                && assignment.status == AssignmentStatus::Submitted
                && assignment.round < final_round
        })
        .max_by_key(|assignment| assignment.round)
        .map(|assignment| (assignment.participant_id, assignment.round)))
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
    let titles = TrackTitleRepo::list_for_event(&transaction, event_id)?;
    let all_comments = TrackCommentRepo::list_for_event(&transaction, event_id)?;
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
            title: titles
                .iter()
                .find(|title| title.track_code == track.code)
                .map(|title| title.title.clone()),
            comments: all_comments
                .iter()
                .filter(|comment| comment.track_code == track.code && comment.redacted_at.is_none())
                .map(|comment| RevealComment {
                    id: comment.id.clone(),
                    participant_id: comment.participant_id.clone(),
                    author_name: participant_name(&comment.participant_id),
                    kind: comment.kind.clone(),
                    body: comment.body.clone(),
                    created_at: comment.created_at,
                })
                .collect(),
            track_code: track.code,
            seed_owner_name: participant_name(&track.owner_id),
            seed_owner: track.owner_id,
            entries,
        });
    }
    let order = album_order(
        &tracks
            .iter()
            .map(|track| track.track_code.clone())
            .collect::<Vec<_>>(),
        event.seed,
    );
    tracks.sort_by_key(|track| {
        order
            .iter()
            .position(|code| code == &track.track_code)
            .unwrap_or(usize::MAX)
    });
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
    let event_name = safe_reveal_text(event_name);
    let mut messages = Vec::new();
    let mut current = format!("{event_name} — reveal");
    for track in tracks {
        let editors = {
            let names = track
                .entries
                .iter()
                .filter(|entry| entry.status == AssignmentStatus::Submitted)
                .map(|entry| {
                    let name = safe_reveal_text(&entry.display_name);
                    if entry.fill_in {
                        format!("{name} (fill-in)")
                    } else {
                        name
                    }
                })
                .collect::<Vec<_>>();
            if names.is_empty() {
                "no edits".to_string()
            } else {
                names.join(", ")
            }
        };
        let title = track
            .title
            .as_deref()
            .map(safe_reveal_text)
            .unwrap_or_else(|| track.track_code.clone());
        let gaps = track
            .entries
            .iter()
            .filter(|entry| entry.status != AssignmentStatus::Submitted)
            .count();
        let mut line = format!(
            "\n{} ({}) — seed: {}; edits: {}{}",
            title,
            track.track_code,
            safe_reveal_text(&track.seed_owner_name),
            editors,
            if gaps > 0 {
                format!("; gaps: {gaps}")
            } else {
                String::new()
            }
        );
        for comment in &track.comments {
            line.push_str(&format!(
                "\n  {}: {}",
                safe_reveal_text(&comment.author_name),
                safe_reveal_text(&comment.body)
            ));
        }
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

fn safe_reveal_text(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .replace("@everyone", "@ everyone")
        .replace("@here", "@ here")
        .replace("<@", "< @")
        .replace("<#", "< #")
}

/// Build the host-only reveal package. The package contains names and comments.
/// It never contains file bytes or absolute paths. Audio is addressed by opaque
/// file id through the preview protocol.
pub fn reveal_package(db: &Db, event_id: &str) -> Result<RevealPackage, EngineError> {
    let event = load_event(db, event_id)?;
    let participants = ParticipantRepo::list_for_event(db.conn(), event_id)?;
    let assignments = AssignmentRepo::list_for_event(db.conn(), event_id)?;
    let titles = TrackTitleRepo::list_for_event(db.conn(), event_id)?;
    let comments = TrackCommentRepo::list_for_event(db.conn(), event_id)?;
    let name_of = |user_id: &str| {
        participants
            .iter()
            .find(|participant| participant.user_id == user_id)
            .map(|participant| participant.display_name.clone())
            .unwrap_or_else(|| "Removed participant".to_string())
    };
    let mut tracks = Vec::new();
    for track in TrackRepo::list_for_event(db.conn(), event_id)? {
        let mut credits = Vec::new();
        let mut stats = RevealStats::default();
        let mut contributors = std::collections::BTreeSet::new();
        contributors.insert(track.owner_id.clone());
        for assignment in assignments
            .iter()
            .filter(|assignment| assignment.track_code == track.code)
        {
            match assignment.status {
                AssignmentStatus::Submitted => {
                    let fill_in = participants
                        .iter()
                        .find(|participant| participant.user_id == assignment.participant_id)
                        .is_some_and(|participant| participant.role == Role::FillIn);
                    credits.push(CreditEntry {
                        round: assignment.round,
                        participant_id: assignment.participant_id.clone(),
                        display_name: name_of(&assignment.participant_id),
                        status: assignment.status,
                        fill_in,
                    });
                    stats.edits += 1;
                    if fill_in {
                        stats.fill_ins += 1;
                    }
                    contributors.insert(assignment.participant_id.clone());
                }
                AssignmentStatus::SkippedAfterClaim | AssignmentStatus::SkippedBeforeClaim => {
                    credits.push(CreditEntry {
                        round: assignment.round,
                        participant_id: String::new(),
                        display_name: "No edit".to_string(),
                        status: assignment.status,
                        fill_in: false,
                    });
                    stats.skips += 1;
                }
                _ => {}
            }
        }
        credits.sort_by_key(|entry| entry.round);
        stats.contributors = contributors.len() as u32;
        let track_comments = comments
            .iter()
            .filter(|comment| comment.track_code == track.code && comment.redacted_at.is_none())
            .map(|comment| RevealComment {
                id: comment.id.clone(),
                participant_id: comment.participant_id.clone(),
                author_name: name_of(&comment.participant_id),
                kind: comment.kind.clone(),
                body: comment.body.clone(),
                created_at: comment.created_at,
            })
            .collect();
        let files = FileRepo::list_for_track(db.conn(), event_id, &track.code)?;
        let final_version = files.last().map(|file| file.version).unwrap_or(0);
        let versions = files
            .iter()
            .map(|file| {
                let round = assignments
                    .iter()
                    .find(|assignment| {
                        assignment.track_code == track.code
                            && assignment.participant_id == file.holder_id
                            && assignment.status == AssignmentStatus::Submitted
                    })
                    .map(|assignment| assignment.round);
                RevealVersion {
                    version: file.version,
                    holder_id: file.holder_id.clone(),
                    holder_name: name_of(&file.holder_id),
                    role: if file.version == 1 {
                        RevealVersionRole::Seed
                    } else {
                        RevealVersionRole::Edit
                    },
                    round,
                    file_id: file.id.clone(),
                    extension: file.extension.clone(),
                    created_at: file.created_at,
                    playable: is_playable(&file.extension),
                    final_version: file.version == final_version,
                }
            })
            .collect();
        tracks.push(RevealTrack {
            position: 0,
            track_code: track.code.clone(),
            title: titles
                .iter()
                .find(|title| title.track_code == track.code)
                .map(|title| title.title.clone()),
            seed_owner_id: track.owner_id.clone(),
            seed_owner_name: name_of(&track.owner_id),
            credits,
            comments: track_comments,
            versions,
            stats,
        });
    }
    let order = album_order(
        &tracks
            .iter()
            .map(|track| track.track_code.clone())
            .collect::<Vec<_>>(),
        event.seed,
    );
    let mut album = Vec::with_capacity(tracks.len());
    for (index, code) in order.iter().enumerate() {
        if let Some(mut track) = tracks
            .iter()
            .find(|track| &track.track_code == code)
            .cloned()
        {
            track.position = index as u32 + 1;
            album.push(track);
        }
    }
    Ok(RevealPackage {
        event_name: event.name,
        event_state: event.state,
        published: matches!(event.state, EventState::Reveal | EventState::Archived),
        album,
    })
}

fn is_playable(extension: &str) -> bool {
    matches!(
        extension,
        "it" | "mptm" | "xm" | "mod" | "s3m" | "mtm" | "669" | "wav" | "flac" | "mp3" | "ogg"
    )
}

pub fn set_track_title(
    db: &Db,
    event_id: &str,
    track_code: &str,
    title: &str,
    actor_id: &str,
    now: i64,
) -> Result<TrackTitleRecord, EngineError> {
    let event = load_event(db, event_id)?;
    if !matches!(event.state, EventState::Active | EventState::Paused) {
        return Err(EngineError::WrongEventState {
            action: "title",
            state: event.state,
            expected: "active or paused",
        });
    }
    let title = title.trim();
    if title.is_empty() {
        return Err(EngineError::EmptyTitle);
    }
    if title.chars().count() > 80 {
        return Err(EngineError::TitleTooLong);
    }
    let transaction = begin(db)?;
    open_event_in(&transaction, event_id, "title")?;
    let track = TrackRepo::get(&transaction, event_id, track_code)?
        .ok_or_else(|| EngineError::TrackNotFound(track_code.to_string()))?;
    let mut fallback = false;
    let mut final_holder: Option<String> = None;
    if actor_id != "host" {
        if TrackTitleRepo::get(&transaction, event_id, track_code)?.is_some() {
            return Err(EngineError::TitleAlreadySet);
        }
        let assignments = AssignmentRepo::list_for_event(&transaction, event_id)?;
        let final_round = PlanRepo::latest(&transaction, event_id)?
            .map(|plan| plan.rounds.saturating_sub(1))
            .ok_or(EngineError::NoPlan)?;
        final_holder = assignments
            .iter()
            .filter(|assignment| {
                assignment.track_code == track_code
                    && assignment.status == AssignmentStatus::Submitted
                    && assignment.round == final_round
            })
            .max_by_key(|assignment| assignment.round)
            .map(|assignment| assignment.participant_id.clone());
        let previous = assignments
            .iter()
            .filter(|assignment| {
                assignment.track_code == track_code
                    && assignment.status == AssignmentStatus::Submitted
                    && assignment.round < final_round
            })
            .max_by_key(|assignment| assignment.round)
            .map(|assignment| assignment.participant_id.as_str());
        match final_holder.as_deref() {
            Some(holder) if holder == actor_id => {}
            Some(_) if previous == Some(actor_id) => fallback = true,
            _ => return Err(EngineError::NotTitleHolder),
        }
    }
    let record = TrackTitleRecord {
        event_id: event_id.to_string(),
        track_code: track.code,
        title: title.to_string(),
        set_by: actor_id.to_string(),
        created_at: now,
    };
    TrackTitleRepo::upsert(&transaction, &record)?;
    let (action, detail) = if fallback {
        (
            "Track title fallback",
            serde_json::json!({
                "trackCode": track_code,
                "actor": actor_id,
                "finalHolder": final_holder,
                "fallback": true
            }),
        )
    } else {
        (
            "Track title set",
            serde_json::json!({
                "trackCode": track_code,
                "actor": actor_id,
                "fallback": false
            }),
        )
    };
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        if actor_id == "host" { "Host" } else { actor_id },
        action,
        None,
        &detail.to_string(),
    )?;
    commit(transaction)?;
    Ok(record)
}

pub fn set_track_title_for_user(
    db: &Db,
    event_id: &str,
    user_id: &str,
    title: &str,
    now: i64,
) -> Result<TrackTitleRecord, EngineError> {
    set_track_title_for_user_on_track(db, event_id, user_id, None, title, now)
}

/// Set a title for a track from an earlier round. The optional track code
/// lets a previous submitter name the track without a new plan version.
pub fn set_track_title_for_user_on_track(
    db: &Db,
    event_id: &str,
    user_id: &str,
    track_code: Option<&str>,
    title: &str,
    now: i64,
) -> Result<TrackTitleRecord, EngineError> {
    if let Some(track_code) = track_code {
        return set_track_title(db, event_id, track_code, title, user_id, now);
    }
    let assignment = AssignmentRepo::latest_for_participant(db.conn(), event_id, user_id)?
        .filter(|assignment| assignment.status == AssignmentStatus::Submitted)
        .ok_or(EngineError::NotTitleHolder)?;
    let final_round = PlanRepo::latest(db.conn(), event_id)?
        .map(|plan| plan.rounds.saturating_sub(1))
        .ok_or(EngineError::NoPlan)?;
    if assignment.round != final_round {
        return Err(EngineError::NotTitleHolder);
    }
    set_track_title(db, event_id, &assignment.track_code, title, user_id, now)
}

pub fn add_track_comment(
    db: &Db,
    event_id: &str,
    track_code: &str,
    participant_id: &str,
    kind: &str,
    body: &str,
    now: i64,
) -> Result<TrackCommentRecord, EngineError> {
    let event = load_event(db, event_id)?;
    if !matches!(event.state, EventState::Active | EventState::Paused) {
        return Err(EngineError::WrongEventState {
            action: "comment",
            state: event.state,
            expected: "active or paused",
        });
    }
    if !matches!(kind, "comment" | "questionnaire") {
        return Err(EngineError::InvalidCommentKind);
    }
    let body = body.trim();
    if body.is_empty() {
        return Err(EngineError::EmptyComment);
    }
    if body.chars().count() > 1_000 {
        return Err(EngineError::CommentTooLong);
    }
    let transaction = begin(db)?;
    open_event_in(&transaction, event_id, "comment")?;
    let track = TrackRepo::get(&transaction, event_id, track_code)?
        .ok_or_else(|| EngineError::TrackNotFound(track_code.to_string()))?;
    ParticipantRepo::get(&transaction, event_id, participant_id)?
        .ok_or_else(|| EngineError::ParticipantNotFound(participant_id.to_string()))?;
    let contributed = track.owner_id == participant_id
        || AssignmentRepo::list_for_event(&transaction, event_id)?
            .iter()
            .any(|assignment| {
                assignment.track_code == track_code && assignment.participant_id == participant_id
            });
    if !contributed {
        return Err(EngineError::NotTrackContributor);
    }
    let record = TrackCommentRecord {
        id: crate::ids::comment_id(event_id, track_code, participant_id, kind, body, now),
        event_id: event_id.to_string(),
        track_code: track_code.to_string(),
        participant_id: participant_id.to_string(),
        kind: kind.to_string(),
        body: body.to_string(),
        redacted_at: None,
        created_at: now,
    };
    TrackCommentRepo::insert(&transaction, &record)?;
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Track comment added",
        None,
        &serde_json::json!({ "trackCode": track_code, "kind": kind }).to_string(),
    )?;
    commit(transaction)?;
    Ok(record)
}

pub fn redact_track_comment(db: &Db, comment_id: &str, now: i64) -> Result<(), EngineError> {
    let transaction = begin(db)?;
    let comment = TrackCommentRepo::get(&transaction, comment_id)?
        .ok_or_else(|| EngineError::CommentNotFound(comment_id.to_string()))?;
    open_event_in(&transaction, &comment.event_id, "redaction")?;
    if comment.redacted_at.is_some() {
        return Err(EngineError::CommentAlreadyRedacted);
    }
    if !TrackCommentRepo::redact(&transaction, comment_id, now)? {
        return Err(EngineError::CommentAlreadyRedacted);
    }
    AuditRepo::insert(
        &transaction,
        &comment.event_id,
        now,
        "Host",
        "Track comment redacted",
        None,
        &serde_json::json!({
            "trackCode": comment.track_code,
            "commentId": comment.id
        })
        .to_string(),
    )?;
    commit(transaction)?;
    Ok(())
}

pub fn request_extension(
    db: &Db,
    event_id: &str,
    user_id: &str,
    hours: i64,
    now: i64,
) -> Result<ExtensionOutcome, EngineError> {
    if !matches!(hours, 1 | 6 | 12) {
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
        AuditRepo::insert(
            &transaction,
            event_id,
            now,
            "worker",
            "Extension refused",
            None,
            &serde_json::json!({ "reason": "extension limit", "assignmentId": assignment.id })
                .to_string(),
        )?;
        enqueue_effect(
            &transaction,
            event_id,
            &Effect::Dm {
                user_id: user_id.to_string(),
                content: "This assignment has used its extensions. Submit, skip, or ask the host."
                    .to_string(),
            },
            now,
        )?;
        commit(transaction)?;
        return Ok(ExtensionOutcome {
            status: ExtensionStatus::Refused,
            due_at: None,
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
    if assignment.status == AssignmentStatus::Claimed && deadline.extension_count == 0 && hours <= 6
    {
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
            AuditRepo::insert(
                &transaction,
                &decision.event_id,
                now,
                "Host",
                "Extension refused",
                None,
                &serde_json::json!({ "reason": "extension limit", "assignmentId": assignment_id })
                    .to_string(),
            )?;
            enqueue_effect(
                &transaction,
                &decision.event_id,
                &Effect::Dm {
                    user_id: assignment.participant_id.clone(),
                    content:
                        "The assignment has used its extensions. Submit, skip, or ask the host."
                            .to_string(),
                },
                now,
            )?;
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
            if !matches!(hours, 1 | 6 | 12) {
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
            if assignment.status != AssignmentStatus::Overdue {
                return Err(EngineError::WrongAssignmentState(assignment.status));
            }
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
        let settings = parse_settings(&current)?;
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
                .is_some_and(|tz| {
                    quiet_hours(
                        now,
                        tz,
                        settings.quiet_hours_start,
                        settings.quiet_hours_end,
                    )
                });
            if quiet {
                continue;
            }
            let mut pending = Vec::new();
            for (step, column) in [
                (50_i64, "reminded_50"),
                (80, "reminded_80"),
                (95, "reminded_95"),
            ] {
                if percent >= step {
                    pending.push((step, column));
                }
            }
            let highest = pending.last().map(|(step, _)| *step);
            let mut marked = false;
            for (_, column) in &pending {
                marked |= DeadlineRepo::mark_reminder(&transaction, &assignment.id, column)?;
            }
            if marked && let Some(step) = highest {
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

fn quiet_hours(now: i64, timezone: Tz, start: u8, end: u8) -> bool {
    if start == end {
        return false;
    }
    Utc.timestamp_opt(now, 0)
        .single()
        .map(|time| u8::try_from(time.with_timezone(&timezone).hour()).unwrap_or(0))
        .is_some_and(|hour| {
            if start < end {
                (start..end).contains(&hour)
            } else {
                hour >= start || hour < end
            }
        })
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
            title: Some("A title".to_string()),
            seed_owner: "u1".to_string(),
            seed_owner_name: "Seed".to_string(),
            entries: vec![CreditEntry {
                round: 0,
                participant_id: "u2".to_string(),
                display_name: "x".repeat(3_000),
                status: AssignmentStatus::Submitted,
                fill_in: false,
            }],
            comments: Vec::new(),
        };
        let messages = reveal_messages("Long reveal", &[track]);
        assert!(messages.len() > 1);
        assert!(
            messages
                .iter()
                .all(|message| message.chars().count() <= 1_900)
        );
    }

    #[test]
    fn reveal_messages_include_titles_fill_ins_gaps_and_comments() {
        let track = TrackCredits {
            track_code: "SAAAA".to_string(),
            title: Some("Midnight Draft".to_string()),
            seed_owner: "u1".to_string(),
            seed_owner_name: "Seed".to_string(),
            entries: vec![
                CreditEntry {
                    round: 0,
                    participant_id: "u2".to_string(),
                    display_name: "Editor".to_string(),
                    status: AssignmentStatus::Submitted,
                    fill_in: true,
                },
                CreditEntry {
                    round: 1,
                    participant_id: String::new(),
                    display_name: "No edit".to_string(),
                    status: AssignmentStatus::SkippedAfterClaim,
                    fill_in: false,
                },
            ],
            comments: vec![RevealComment {
                id: "comment-1".to_string(),
                participant_id: "u2".to_string(),
                author_name: "Editor".to_string(),
                kind: "comment".to_string(),
                body: "A great bassline.".to_string(),
                created_at: 10,
            }],
        };
        let messages = reveal_messages("Reveal", &[track]);
        let text = messages.join("\n");
        assert!(text.contains("Midnight Draft (SAAAA)"));
        assert!(text.contains("Editor (fill-in)"));
        assert!(text.contains("gaps: 1"));
        assert!(text.contains("Editor: A great bassline."));
        assert!(!text.contains("No edit;"));
    }

    #[test]
    fn reveal_messages_do_not_allow_mentions_or_control_characters() {
        let track = TrackCredits {
            track_code: "SAAAA".to_string(),
            title: Some("@everyone\nReal".to_string()),
            seed_owner: "u1".to_string(),
            seed_owner_name: "<@123>".to_string(),
            entries: vec![CreditEntry {
                round: 0,
                participant_id: "u2".to_string(),
                display_name: "Editor".to_string(),
                status: AssignmentStatus::Submitted,
                fill_in: false,
            }],
            comments: vec![RevealComment {
                id: "comment-1".to_string(),
                participant_id: "u2".to_string(),
                author_name: "Editor".to_string(),
                kind: "comment".to_string(),
                body: "@here look".to_string(),
                created_at: 10,
            }],
        };
        let text = reveal_messages("Event @everyone", &[track]).join("\n");
        assert!(!text.contains("@everyone"));
        assert!(!text.contains("@here"));
        assert!(!text.contains("<@123>"));
        assert!(text.contains("@ everyone"));
        assert!(text.contains("@ here"));
        assert!(text.contains("< @123>"));
    }
}
