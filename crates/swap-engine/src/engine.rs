use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Duration as ChronoDuration, NaiveDate, Offset, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use swap_core::{
    AssignmentStatus, AsyncCandidate, EventState, Membership, ParticipantId, Readiness, Role,
    RoundKind, RoundStatus, Schedule, StateError, TrackCode, TrackSpec, TrackState, album_order,
    choose_async_offers,
};
use swap_db::{
    AssignmentRecord, AssignmentRepo, AuditRepo, AvailabilityRecord, AvailabilityRepo, Connection,
    Db, DbError, DeadlineRepo, EventRecord, EventRepo, EventRoundRecord, ExpiredPairRecord,
    ExpiredPairRepo, ExpiryStateRecord, ExpiryStateRepo, FileRecord, FileRepo, FillInChainRepo,
    HostDecisionRecord, HostDecisionRepo, LateSeedRecord, LateSeedRepo, MetaRepo, PackRecord,
    PackRepo, ParticipantRecord, ParticipantRepo, PlanAssignmentRecord, PlanRecord, PlanRepo,
    ProcessedAttachmentRepo, RoundRepo, SeedOwnerRepo, SeenRepo, TrackCommentRecord,
    TrackCommentRepo, TrackRecord, TrackRepo, TrackTitleRecord, TrackTitleRepo, Transaction,
    TransactionBehavior,
};
use swap_files::{BlobStore, FileError, StoredBlob, ValidatedFile};
use thiserror::Error;

use crate::delivery::{DeliveryError, PendingDelivery, enqueue_effect, enqueue_pending};
use crate::effects::Effect;
use crate::fill_in::is_fill_in_turn;
use crate::ids::{
    CommitmentInputs, assignment_id, plan_commitment, plan_commitment_v2, random_hex_128,
    track_code, track_code_from_secret,
};
use crate::prepare::{DeliveryPreparer, PreparedBytes};
use crate::settings::{EventSettings, ExpiryProtection, Mode, ReofferAfterExpiry, SettingsError};

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
    #[error(
        "participant {participant} allows {allowed} handoffs but the strict event needs {required}"
    )]
    InsufficientHandoffs {
        participant: String,
        allowed: u32,
        required: u32,
    },
    #[error("the event has no active participants")]
    NoParticipants,
    #[error("the event has no seeded tracks")]
    NoSeededTracks,
    #[error("the seed round is not open")]
    SeedRoundNotOpen,
    #[error("the seed round is closed")]
    SeedRoundClosed,
    #[error("the event needs at least {minimum} seeds; it has {seeds}")]
    SeedMinimum { seeds: usize, minimum: u32 },
    #[error("the host did not upload the required pack")]
    PackRequired,
    #[error("the pack is locked and cannot be replaced")]
    PackLocked,
    #[error("round {round} is not open")]
    RoundNotOpen { round: u32 },
    #[error("round {round} has no deadline")]
    RoundDeadlineMissing { round: u32 },
    #[error("the late seed was not found: {0}")]
    LateSeedNotFound(String),
    #[error("the late seed is not pending")]
    LateSeedNotPending,
    #[error("the availability range is invalid")]
    InvalidAvailability,
    #[error("participant {participant} has already seen track {track}; the offer is refused")]
    SeenPairOffer { track: String, participant: String },
    #[error("participant {participant} owns track {track}; the offer is refused")]
    OwnTrackOffer { track: String, participant: String },
    #[error("a fill-in list needs at least one person")]
    FillInListEmpty,
    #[error("a fill-in list cannot name the same person twice")]
    FillInListDuplicate,
    #[error("the fill-in due time must be in the future")]
    FillInDueInPast,
    #[error("the fill-in due time is more than 14 days away")]
    FillInDueTooFar,
    #[error("the participant cannot take this fill-in: {0}")]
    FillInNotEligible(String),
    #[error("strict participation cannot honor {count} away ranges; use flexible participation")]
    AvailabilityBlocksStrict { count: usize },
    #[error("the event needs at least two active participants to schedule a round")]
    NoRounds,
    #[error("the event has no committed plan")]
    NoPlan,
    #[error("the plan commitment does not match the stored plan")]
    CommitmentMismatch,
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
    #[error("the edit deadline has passed")]
    DeadlinePassed,
    #[error("the offer is not due yet")]
    OfferNotDue,
    #[error("the participant has no active assignment")]
    NoActiveAssignment,
    #[error("the file format changed from .{current} to .{submitted}")]
    FormatChange { current: String, submitted: String },
    #[error("the reveal is blocked by {count} active assignments")]
    PendingAssignments { count: usize, ids: Vec<String> },
    #[error("the reveal package could not be built: {0}")]
    RevealZip(String),
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
    #[error("the re-offer setting does not allow a host re-offer")]
    ReofferNotAllowed,
    #[error("the participant cannot take a new offer: {0}")]
    ExpiryNotEligible(String),
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
    #[error("the event is not in async mode")]
    NotAsyncMode,
    #[error(
        "the async event has {tracks} tracks and {people} seeded participants; expected {expected:?} songs"
    )]
    AsyncTargetMismatch {
        people: usize,
        tracks: usize,
        expected: Option<u32>,
    },
    #[error("claims per participant {claims} exceeds the {tracks} available tracks")]
    AsyncClaimTarget { claims: u32, tracks: usize },
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
    #[error("the participant has not left the event")]
    ParticipantNotLeft,
    #[error("a restore request is already pending for this participant")]
    RestorePending,
    #[error("there is no pending restore request for this participant")]
    NoPendingRestore,
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
    pub held: bool,
    pub late_seed_id: Option<String>,
    pub replaced: bool,
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
    pub gaps: u32,
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
pub struct AsyncWaveOutcome {
    pub offers: usize,
    pub effects: Vec<Effect>,
    /// Ready people that the wave passed over, with the reason.
    pub skips: Vec<AsyncSkip>,
}

/// One ready participant that the queue did not offer a track this wave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncSkip {
    pub participant_id: String,
    pub reason: String,
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
    pub expired_offers: usize,
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
    pub total_rounds: u32,
    pub seed_round_open: bool,
    pub committed: bool,
    pub active_assignments: usize,
    pub submitted_assignments: usize,
    /// The fill-in turns that the next classic round waits on.
    pub waiting_fill_ins: Vec<crate::fill_in::WaitingFillIn>,
}

pub fn create_event(
    db: &Db,
    id: &str,
    name: &str,
    seed: u64,
    settings: &EventSettings,
    now: i64,
) -> Result<(), EngineError> {
    create_event_with_secret(db, id, name, seed, settings, None, now)
}

/// Create an event with a caller-supplied 128-bit secret. Tests use a fixed
/// secret to keep a plan reproducible. Production uses a random secret.
pub fn create_event_with_secret(
    db: &Db,
    id: &str,
    name: &str,
    seed: u64,
    settings: &EventSettings,
    secret_hex: Option<&str>,
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
            state: EventState::Draft,
            seed,
            settings_json: serde_json::to_string(settings)?,
            created_at: now,
            updated_at: now,
        },
    )?;
    EventRepo::set_secret_hex(db.conn(), id, secret_hex.unwrap_or(&random_hex_128()))?;
    EventRepo::set_round_model(db.conn(), id, 1)?;
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
    require_seed_state(&event, "signup")?;
    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    require_seed_state(&current, "signup")?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackOutcome {
    pub id: String,
    pub sha256: String,
    pub size: u64,
}

/// Upload or replace the host pack before Round 1 opens. The bytes stay in the
/// local content store. The UI receives the opaque pack id and the hash, never
/// the bytes or an absolute path.
pub fn upload_pack(
    db: &Db,
    store: &BlobStore,
    event_id: &str,
    filename: &str,
    bytes: &[u8],
    now: i64,
) -> Result<PackOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_state(&event, EventState::Draft, "pack upload", "draft")?;
    if bytes.is_empty() {
        return Err(EngineError::PackRequired);
    }
    if bytes.len() as u64 > swap_files::DEFAULT_MAX_SIZE {
        return Err(EngineError::File(FileError::TooLarge {
            size: bytes.len() as u64,
            max: swap_files::DEFAULT_MAX_SIZE,
        }));
    }
    if PackRepo::get_for_event(db.conn(), event_id)?.is_some_and(|pack| pack.locked_at.is_some()) {
        return Err(EngineError::PackLocked);
    }
    let blob = store.store(bytes)?;
    let id = format!("p{}", &blob.sha256[..16]);
    let transaction = begin(db)?;
    PackRepo::delete_for_event(&transaction, event_id)?;
    PackRepo::insert(
        &transaction,
        &PackRecord {
            id: id.clone(),
            event_id: event_id.to_string(),
            filename: filename.to_string(),
            sha256: blob.sha256.clone(),
            size: blob.size,
            storage_path: blob.relative_path.clone(),
            created_at: now,
            locked_at: None,
        },
    )?;
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Pack uploaded",
        Some(&id),
        &serde_json::json!({ "filename": filename, "sha256": blob.sha256, "size": blob.size })
            .to_string(),
    )?;
    commit(transaction)?;
    Ok(PackOutcome {
        id,
        sha256: blob.sha256,
        size: blob.size,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedOpenOutcome {
    pub deadline_at: i64,
    pub pack_id: Option<String>,
    pub effects: Vec<Effect>,
}

/// Open Round 1, the seed round. The host must upload the required pack and
/// set the seed deadline before this action. The event moves from Draft to
/// Signup.
pub fn open_seed_round(
    db: &Db,
    event_id: &str,
    deadline_at: i64,
    channel_id: Option<&str>,
    now: i64,
) -> Result<SeedOpenOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_state(&event, EventState::Draft, "seed round open", "draft")?;
    let settings = parse_settings(&event)?;
    if deadline_at <= now {
        return Err(EngineError::SeedRoundNotOpen);
    }
    let pack = PackRepo::get_for_event(db.conn(), event_id)?;
    if settings.pack_required.unwrap_or(true) && pack.is_none() {
        return Err(EngineError::PackRequired);
    }

    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if current.state != EventState::Draft {
        return Err(EngineError::WrongEventState {
            action: "seed round open",
            state: current.state,
            expected: "draft",
        });
    }
    if let Some(existing) = RoundRepo::get(&transaction, event_id, 1)?
        && existing.status == RoundStatus::Open
    {
        return Err(EngineError::SeedRoundNotOpen);
    }
    RoundRepo::insert(
        &transaction,
        &EventRoundRecord {
            event_id: event_id.to_string(),
            round_no: 1,
            kind: RoundKind::Seed,
            status: RoundStatus::Open,
            opened_at: Some(now),
            closed_at: None,
            deadline_at: Some(deadline_at),
            original_deadline_at: Some(deadline_at),
            final_round: false,
            deadline_moves: 0,
        },
    )?;
    if let Some(pack) = &pack {
        PackRepo::lock(&transaction, event_id, now)?;
        let participants = ParticipantRepo::list_for_event(&transaction, event_id)?;
        for participant in participants.iter().filter(|participant| {
            matches!(
                participant.membership,
                Membership::SignedUp | Membership::Seeded
            )
        }) {
            enqueue_pending(
                &transaction,
                event_id,
                &PendingDelivery::Pack {
                    user_id: participant.user_id.clone(),
                    filename: pack.filename.clone(),
                    sha256: pack.sha256.clone(),
                    caption: format!(
                        "Monsquaz Swap pack {}. Round 1 seed deadline: {deadline_at} UTC.",
                        pack.id
                    ),
                },
                now,
            )?;
        }
    }
    EventRepo::set_state(&transaction, event_id, EventState::Signup, now)?;
    EventRepo::set_seed_round_closed_at(&transaction, event_id, None)?;
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Seed round opened",
        None,
        &serde_json::json!({
            "round": 1,
            "deadlineAt": deadline_at,
            "packId": pack.as_ref().map(|pack| pack.id.clone()),
        })
        .to_string(),
    )?;
    let mut effects = Vec::new();
    if let Some(channel_id) = channel_id.filter(|channel| !channel.trim().is_empty()) {
        let pack_line = pack
            .as_ref()
            .map(|pack| format!(" Pack: {}.", pack.id))
            .unwrap_or_default();
        effects.push(Effect::Channel {
            channel_id: channel_id.to_string(),
            content: format!(
                "Round 1 is open. Create one track from the host pack and send it to the bot before {deadline_at} UTC.{pack_line}"
            ),
        });
        for effect in &effects {
            enqueue_effect(&transaction, event_id, effect, now)?;
        }
    }
    commit(transaction)?;
    Ok(SeedOpenOutcome {
        deadline_at,
        pack_id: pack.map(|pack| pack.id),
        effects,
    })
}

/// Extend the seed deadline while Round 1 is open.
pub fn extend_seed_deadline(
    db: &Db,
    event_id: &str,
    deadline_at: i64,
    now: i64,
) -> Result<i64, EngineError> {
    let event = load_event(db, event_id)?;
    require_state(&event, EventState::Signup, "seed deadline move", "signup")?;
    if deadline_at <= now {
        return Err(EngineError::SeedRoundNotOpen);
    }
    let transaction = begin(db)?;
    let round = RoundRepo::get(&transaction, event_id, 1)?.ok_or(EngineError::SeedRoundNotOpen)?;
    if round.status != RoundStatus::Open || round.kind != RoundKind::Seed {
        return Err(EngineError::SeedRoundClosed);
    }
    RoundRepo::set_deadline(&transaction, event_id, 1, deadline_at)?;
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Seed deadline moved",
        None,
        &serde_json::json!({ "deadlineAt": deadline_at }).to_string(),
    )?;
    commit(transaction)?;
    Ok(deadline_at)
}

/// Reopen Round 1 before the schedule starts. Draft plans are void. Existing
/// seeds stay. The host sets a new deadline.
pub fn reopen_seed_round(
    db: &Db,
    event_id: &str,
    deadline_at: i64,
    now: i64,
) -> Result<(), EngineError> {
    let event = load_event(db, event_id)?;
    require_state(
        &event,
        EventState::ScheduleReview,
        "seed round reopen",
        "schedule review",
    )?;
    if deadline_at <= now {
        return Err(EngineError::SeedRoundNotOpen);
    }
    if !AssignmentRepo::list_active(db.conn(), event_id)?.is_empty() {
        return Err(EngineError::RoundNotResolved { round: 1 });
    }
    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if current.state != EventState::ScheduleReview {
        return Err(EngineError::WrongEventState {
            action: "seed round reopen",
            state: current.state,
            expected: "schedule review",
        });
    }
    if let Some(plan) = PlanRepo::latest(&transaction, event_id)? {
        PlanRepo::void(&transaction, event_id, plan.version, now)?;
    }
    let participants = ParticipantRepo::list_for_event(&transaction, event_id)?;
    for mut participant in participants {
        if participant.membership != Membership::Active {
            continue;
        }
        participant.membership = if participant.role == Role::FillIn {
            Membership::SignedUp
        } else {
            Membership::Seeded
        };
        participant.updated_at = now;
        ParticipantRepo::upsert(&transaction, &participant)?;
    }
    let round = RoundRepo::get(&transaction, event_id, 1)?.ok_or(EngineError::SeedRoundNotOpen)?;
    if round.status == RoundStatus::Closed {
        RoundRepo::set_status(&transaction, event_id, 1, RoundStatus::Open, now)?;
    }
    RoundRepo::set_deadline(&transaction, event_id, 1, deadline_at)?;
    EventRepo::set_state(&transaction, event_id, EventState::Signup, now)?;
    EventRepo::set_seed_round_closed_at(&transaction, event_id, None)?;
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Seed round reopened",
        None,
        &serde_json::json!({ "deadlineAt": deadline_at }).to_string(),
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
    let round =
        RoundRepo::get(db.conn(), request.event_id, 1)?.ok_or(EngineError::SeedRoundNotOpen)?;
    if round.status != RoundStatus::Open {
        return Err(EngineError::SeedRoundClosed);
    }
    let deadline_at = round.deadline_at.ok_or(EngineError::SeedRoundNotOpen)?;
    let late = now > deadline_at;
    let secret_hex = EventRepo::secret_hex(db.conn(), request.event_id)?;

    let existing = ParticipantRepo::get(db.conn(), request.event_id, request.user_id)?;
    if let Some(existing) = &existing
        && matches!(existing.membership, Membership::Left | Membership::Removed)
    {
        return Err(EngineError::ParticipantLeft);
    }

    let tracks = TrackRepo::list_for_event(db.conn(), request.event_id)?;
    let owned = tracks
        .iter()
        .find(|track| track.owner_id == request.user_id)
        .cloned();
    if owned.is_some() && late {
        return Err(EngineError::SeedRoundClosed);
    }

    let prepared = preparer.prepare(request.bytes, validated.kind, &validated.extension)?;
    let original_blob = store.store(request.bytes)?;
    let delivery_blob = if prepared.delivery == request.bytes {
        original_blob.clone()
    } else {
        store.store(&prepared.delivery)?
    };

    if let Some(track) = owned {
        let file = file_record(
            request.event_id,
            &track.code,
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
                "Seed replaced for track {}. SHA-256: {}. Size: {} bytes.",
                track.code, original_blob.sha256, original_blob.size
            ),
        };
        let transaction = begin(db)?;
        let current = EventRepo::get(&transaction, request.event_id)?
            .ok_or_else(|| EngineError::EventNotFound(request.event_id.to_string()))?;
        if current.state != EventState::Signup {
            return Err(EngineError::WrongEventState {
                action: "seed replacement",
                state: current.state,
                expected: "signup",
            });
        }
        let current_round = RoundRepo::get(&transaction, request.event_id, 1)?
            .ok_or(EngineError::SeedRoundNotOpen)?;
        if current_round.status != RoundStatus::Open
            || current_round
                .deadline_at
                .is_some_and(|deadline| now > deadline)
        {
            return Err(EngineError::SeedRoundClosed);
        }
        if !FileRepo::replace_v1(&transaction, request.event_id, &track.code, &file)? {
            return Err(EngineError::NoFile(track.code.clone()));
        }
        AuditRepo::insert(
            &transaction,
            request.event_id,
            now,
            "Participant",
            "Seed replaced",
            Some(&track.code),
            &serde_json::json!({ "userId": request.user_id, "sha256": original_blob.sha256 })
                .to_string(),
        )?;
        enqueue_effect(&transaction, request.event_id, &receipt, now)?;
        commit(transaction)?;
        return Ok(JoinOutcome {
            track_code: track.code,
            sha256: delivery_blob.sha256,
            original_sha256: original_blob.sha256,
            effects: vec![receipt],
            held: false,
            late_seed_id: None,
            replaced: true,
        });
    }

    let code = unique_track_code(&tracks, event.seed, secret_hex.as_deref());
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

    if late {
        let late_id = format!(
            "l{}",
            &swap_files::sha256_hex(format!("msw-late-seed:{now}:{}", request.user_id).as_bytes())
                [..16]
        );
        let receipt = Effect::Dm {
            user_id: request.user_id.to_string(),
            content: format!(
                "Your seed for track {code} arrived after the deadline. The host must approve it. You will get a message when the host decides."
            ),
        };
        let transaction = begin(db)?;
        let current = EventRepo::get(&transaction, request.event_id)?
            .ok_or_else(|| EngineError::EventNotFound(request.event_id.to_string()))?;
        if current.state != EventState::Signup {
            return Err(EngineError::WrongEventState {
                action: "late seed",
                state: current.state,
                expected: "signup",
            });
        }
        let current_round = RoundRepo::get(&transaction, request.event_id, 1)?
            .ok_or(EngineError::SeedRoundNotOpen)?;
        if current_round.status != RoundStatus::Open {
            return Err(EngineError::SeedRoundClosed);
        }
        LateSeedRepo::insert(
            &transaction,
            &LateSeedRecord {
                id: late_id.clone(),
                event_id: request.event_id.to_string(),
                user_id: request.user_id.to_string(),
                display_name: request.display_name.to_string(),
                timezone: request.timezone.to_string(),
                max_handoffs: request.max_handoffs,
                track_code: code.clone(),
                filename: request.filename.to_string(),
                sha256: delivery_blob.sha256.clone(),
                original_sha256: original_blob.sha256.clone(),
                size: delivery_blob.size,
                extension: validated.extension.clone(),
                storage_path: delivery_blob.relative_path.clone(),
                original_storage_path: Some(original_blob.relative_path.clone()),
                audio_verified: prepared.audio_verified,
                scrub_summary_json: prepared.scrub_summary_json.clone(),
                status: "pending".to_string(),
                created_at: now,
                decided_at: None,
            },
        )?;
        AuditRepo::insert(
            &transaction,
            request.event_id,
            now,
            "Participant",
            "Late seed held",
            Some(&code),
            &serde_json::json!({ "userId": request.user_id, "lateSeedId": late_id }).to_string(),
        )?;
        enqueue_effect(&transaction, request.event_id, &receipt, now)?;
        commit(transaction)?;
        return Ok(JoinOutcome {
            track_code: code,
            sha256: delivery_blob.sha256,
            original_sha256: original_blob.sha256,
            effects: vec![receipt],
            held: true,
            late_seed_id: Some(late_id),
            replaced: false,
        });
    }

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
    let current_round =
        RoundRepo::get(&transaction, request.event_id, 1)?.ok_or(EngineError::SeedRoundNotOpen)?;
    if current_round.status != RoundStatus::Open
        || current_round
            .deadline_at
            .is_some_and(|deadline| now > deadline)
    {
        return Err(EngineError::SeedRoundClosed);
    }
    ParticipantRepo::upsert(&transaction, &participant)?;
    TrackRepo::insert(&transaction, &track)?;
    FileRepo::insert(&transaction, &file)?;
    enqueue_effect(&transaction, request.event_id, &receipt, now)?;
    commit(transaction)?;

    Ok(JoinOutcome {
        track_code: code,
        sha256: delivery_blob.sha256,
        original_sha256: original_blob.sha256,
        effects: vec![receipt],
        held: false,
        late_seed_id: None,
        replaced: false,
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
    let settings = parse_settings(&current)?;
    let minimum = settings.seed_min.max(2);
    if (active as u32) < minimum {
        return Err(EngineError::SeedMinimum {
            seeds: active,
            minimum,
        });
    }
    if let Some(round) = RoundRepo::get(&transaction, event_id, 1)?
        && round.status == RoundStatus::Open
    {
        RoundRepo::set_status(&transaction, event_id, 1, RoundStatus::Closed, now)?;
    }
    EventRepo::set_seed_round_closed_at(&transaction, event_id, Some(now))?;
    EventRepo::set_state(&transaction, event_id, EventState::ScheduleReview, now)?;
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Seed round closed and roster locked",
        None,
        &serde_json::json!({ "active": active, "fillIns": fill_ins }).to_string(),
    )?;
    commit(transaction)?;
    Ok(LockOutcome { active, fill_ins })
}

pub fn pending_late_seeds(db: &Db, event_id: &str) -> Result<Vec<LateSeedRecord>, EngineError> {
    Ok(LateSeedRepo::list_pending(db.conn(), event_id)?)
}

/// Accept a held seed while Round 1 is open. The seed becomes a normal v1 seed.
pub fn accept_late_seed(
    db: &Db,
    event_id: &str,
    late_seed_id: &str,
    now: i64,
) -> Result<LateSeedRecord, EngineError> {
    let event = load_event(db, event_id)?;
    require_state(&event, EventState::Signup, "late seed accept", "signup")?;
    let transaction = begin(db)?;
    let round = RoundRepo::get(&transaction, event_id, 1)?.ok_or(EngineError::SeedRoundNotOpen)?;
    if round.status != RoundStatus::Open {
        return Err(EngineError::SeedRoundClosed);
    }
    let seed = LateSeedRepo::get(&transaction, event_id, late_seed_id)?
        .ok_or_else(|| EngineError::LateSeedNotFound(late_seed_id.to_string()))?;
    if seed.status != "pending" {
        return Err(EngineError::LateSeedNotPending);
    }
    let tracks = TrackRepo::list_for_event(&transaction, event_id)?;
    if tracks.iter().any(|track| track.owner_id == seed.user_id)
        || tracks.iter().any(|track| track.code == seed.track_code)
    {
        return Err(EngineError::AlreadySeeded);
    }
    ParticipantRepo::upsert(
        &transaction,
        &ParticipantRecord {
            event_id: event_id.to_string(),
            user_id: seed.user_id.clone(),
            display_name: seed.display_name.clone(),
            role: Role::Participant,
            membership: Membership::Seeded,
            readiness: Readiness::Ready,
            timezone: seed.timezone.clone(),
            max_handoffs: seed.max_handoffs,
            created_at: now,
            updated_at: now,
        },
    )?;
    TrackRepo::insert(
        &transaction,
        &TrackRecord {
            event_id: event_id.to_string(),
            code: seed.track_code.clone(),
            owner_id: seed.user_id.clone(),
            state: TrackState::Seeded,
            created_at: now,
        },
    )?;
    FileRepo::insert(
        &transaction,
        &FileRecord {
            id: format!("{event_id}:{}:1", seed.track_code),
            event_id: event_id.to_string(),
            track_code: seed.track_code.clone(),
            version: 1,
            holder_id: seed.user_id.clone(),
            sha256: seed.sha256.clone(),
            size: seed.size,
            extension: seed.extension.clone(),
            storage_path: seed.storage_path.clone(),
            original_sha256: Some(seed.original_sha256.clone()),
            original_storage_path: seed.original_storage_path.clone(),
            audio_verified: seed.audio_verified,
            scrub_summary_json: seed.scrub_summary_json.clone(),
            created_at: now,
        },
    )?;
    LateSeedRepo::set_status(&transaction, event_id, late_seed_id, "accepted", now)?;
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Late seed accepted",
        Some(&seed.track_code),
        &serde_json::json!({ "lateSeedId": late_seed_id, "userId": seed.user_id }).to_string(),
    )?;
    enqueue_effect(
        &transaction,
        event_id,
        &Effect::Dm {
            user_id: seed.user_id.clone(),
            content: format!(
                "The host approved your late seed for track {}. The seed deadline was extended for you. Send a new file before the host closes Round 1 if you want to replace it.",
                seed.track_code
            ),
        },
        now,
    )?;
    commit(transaction)?;
    Ok(seed)
}

pub fn reject_late_seed(
    db: &Db,
    event_id: &str,
    late_seed_id: &str,
    reason: &str,
    now: i64,
) -> Result<(), EngineError> {
    let event = load_event(db, event_id)?;
    require_state(&event, EventState::Signup, "late seed reject", "signup")?;
    let transaction = begin(db)?;
    let seed = LateSeedRepo::get(&transaction, event_id, late_seed_id)?
        .ok_or_else(|| EngineError::LateSeedNotFound(late_seed_id.to_string()))?;
    if seed.status != "pending" {
        return Err(EngineError::LateSeedNotPending);
    }
    LateSeedRepo::set_status(&transaction, event_id, late_seed_id, "rejected", now)?;
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Late seed rejected",
        Some(&seed.track_code),
        &serde_json::json!({ "lateSeedId": late_seed_id, "reason": reason }).to_string(),
    )?;
    enqueue_effect(
        &transaction,
        event_id,
        &Effect::Dm {
            user_id: seed.user_id.clone(),
            content: "The host did not accept your late seed. You can join as a fill-in if the host needs one.".to_string(),
        },
        now,
    )?;
    commit(transaction)?;
    Ok(())
}

pub fn list_availability(db: &Db, event_id: &str) -> Result<Vec<AvailabilityRecord>, EngineError> {
    Ok(AvailabilityRepo::list_for_event(db.conn(), event_id)?)
}

pub fn add_availability(
    db: &Db,
    event_id: &str,
    participant_id: &str,
    starts_at: i64,
    ends_at: i64,
    note: Option<&str>,
    now: i64,
) -> Result<AvailabilityRecord, EngineError> {
    let event = load_event(db, event_id)?;
    if !matches!(
        event.state,
        EventState::Draft | EventState::Signup | EventState::ScheduleReview
    ) {
        return Err(EngineError::WrongEventState {
            action: "availability",
            state: event.state,
            expected: "draft, signup, or schedule review",
        });
    }
    if starts_at >= ends_at {
        return Err(EngineError::InvalidAvailability);
    }
    if ParticipantRepo::get(db.conn(), event_id, participant_id)?.is_none() {
        return Err(EngineError::ParticipantNotFound(participant_id.to_string()));
    }
    let id = format!(
        "av{}",
        &swap_files::sha256_hex(
            format!("msw-availability:{event_id}:{participant_id}:{starts_at}:{ends_at}:{now}")
                .as_bytes()
        )[..16]
    );
    let range = AvailabilityRecord {
        id,
        event_id: event_id.to_string(),
        participant_id: participant_id.to_string(),
        starts_at,
        ends_at,
        kind: "away".to_string(),
        note: note.map(str::to_string),
        created_at: now,
    };
    let transaction = begin(db)?;
    AvailabilityRepo::insert(&transaction, &range)?;
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Availability added",
        Some(participant_id),
        &serde_json::json!({
            "startsAt": starts_at,
            "endsAt": ends_at,
            "note": note,
        })
        .to_string(),
    )?;
    commit(transaction)?;
    Ok(range)
}

pub fn remove_availability(db: &Db, event_id: &str, id: &str, now: i64) -> Result<(), EngineError> {
    let event = load_event(db, event_id)?;
    if !matches!(
        event.state,
        EventState::Draft | EventState::Signup | EventState::ScheduleReview
    ) {
        return Err(EngineError::WrongEventState {
            action: "availability",
            state: event.state,
            expected: "draft, signup, or schedule review",
        });
    }
    let transaction = begin(db)?;
    if !AvailabilityRepo::delete(&transaction, event_id, id)? {
        return Err(EngineError::InvalidAvailability);
    }
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Availability removed",
        Some(id),
        "{}",
    )?;
    commit(transaction)?;
    Ok(())
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
    // D-051: the seed of a person who left before the roster lock keeps
    // circulating. That track has no owner in the roster.
    let departed_ids: Vec<&str> = participants
        .iter()
        .filter(|participant| {
            participant.membership == Membership::Left && participant.role == Role::Participant
        })
        .map(|participant| participant.user_id.as_str())
        .collect();
    let tracks: Vec<TrackRecord> = TrackRepo::list_for_event(db.conn(), event_id)?
        .into_iter()
        .filter(|track| {
            active_ids.contains(&track.owner_id.as_str())
                || departed_ids.contains(&track.owner_id.as_str())
        })
        .collect();
    let free_tracks: Vec<TrackCode> = tracks
        .iter()
        .filter(|track| departed_ids.contains(&track.owner_id.as_str()))
        .map(|track| TrackCode::new(track.code.clone()))
        .collect();
    if tracks.len() - free_tracks.len() != active.len() {
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
    let flexible = settings.handoff_policy.as_deref() == Some("Flexible");
    if !flexible {
        for participant in &active {
            if participant.max_handoffs > 0 && participant.max_handoffs < rounds {
                return Err(EngineError::InsufficientHandoffs {
                    participant: participant.display_name.clone(),
                    allowed: participant.max_handoffs,
                    required: rounds,
                });
            }
        }
    }
    let availability = AvailabilityRepo::list_for_event(db.conn(), event_id)?;
    let mut blocked = HashSet::new();
    let mut cursor = now;
    for index in 0..rounds {
        let duration = if index + 1 == rounds {
            settings.final_round_window_secs
        } else {
            settings.round_window_secs
        };
        let window_start = cursor;
        let window_end = cursor.saturating_add(duration);
        cursor = window_end;
        for participant in &active {
            let away = availability.iter().any(|range| {
                range.participant_id == participant.user_id
                    && range.starts_at < window_end
                    && range.ends_at > window_start
            });
            if away {
                blocked.insert((participant.user_id.clone(), index));
            }
        }
    }
    if !flexible && !blocked.is_empty() {
        return Err(EngineError::AvailabilityBlocksStrict {
            count: blocked.len(),
        });
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
    let version = PlanRepo::next_version(db.conn(), event_id)?;
    let round_model = EventRepo::round_model(db.conn(), event_id)?;
    let secret_hex = EventRepo::secret_hex(db.conn(), event_id)?;
    let plan_seed = if round_model == 1 {
        seed_from_secret(secret_hex.as_deref(), version)
    } else {
        event.seed.wrapping_add(u64::from(version - 1))
    };
    let grouped_plan = settings.mode == Mode::Classic && settings.group_count.unwrap_or(1) > 1;
    if grouped_plan && !free_tracks.is_empty() {
        return Err(EngineError::PlanInvalid(
            "a group plan cannot carry a seed whose owner left; use a single group".to_string(),
        ));
    }
    let (mut assignments, fallback) = if grouped_plan {
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
        let schedule = Schedule::generate_with_free_tracks(
            &people,
            &specs,
            &free_tracks,
            rounds,
            plan_seed,
            strategy,
        )?;
        (schedule.assignments, schedule.fallback)
    };

    let mut gaps = 0u32;
    if flexible {
        let caps: HashMap<String, u32> = active
            .iter()
            .map(|participant| {
                (
                    participant.user_id.clone(),
                    if participant.max_handoffs == 0 {
                        rounds
                    } else {
                        participant.max_handoffs.min(rounds)
                    },
                )
            })
            .collect();
        let mut used: HashMap<String, u32> = HashMap::new();
        assignments.retain(|assignment| {
            let used_count = used.entry(assignment.participant.0.clone()).or_insert(0);
            let cap = caps
                .get(&assignment.participant.0)
                .copied()
                .unwrap_or(rounds);
            if *used_count >= cap {
                false
            } else {
                *used_count += 1;
                true
            }
        });
        gaps = (active.len() as u32)
            .saturating_mul(rounds)
            .saturating_sub(assignments.len() as u32);
    }
    if !blocked.is_empty() {
        let before = assignments.len();
        assignments.retain(|assignment| {
            !blocked.contains(&(assignment.participant.0.clone(), assignment.round))
        });
        gaps = gaps.saturating_add((before - assignments.len()) as u32);
    }

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
    let pack = PackRepo::get_for_event(db.conn(), event_id)?;
    let nonce = random_hex_128();
    let seed_owners: Vec<(String, String)> = tracks
        .iter()
        .map(|track| (track.code.clone(), track.owner_id.clone()))
        .collect();
    let (commitment, recipe) = if round_model == 1 {
        let secret = secret_hex.as_deref().unwrap_or("legacy");
        (
            plan_commitment_v2(&CommitmentInputs {
                event_id,
                plan_version: version,
                total_rounds: rounds + 1,
                exchange_rounds: rounds,
                secret_hex: secret,
                nonce: &nonce,
                pack_id: pack.as_ref().map(|pack| pack.id.as_str()),
                settings_json: &settings_json,
                seed_owners: &seed_owners,
                assignments: &planned,
            }),
            "v2",
        )
    } else {
        let plan = PlanRecord {
            event_id: event_id.to_string(),
            version,
            seed: plan_seed,
            rounds,
            settings_json: settings_json.clone(),
            commitment: String::new(),
            committed_at: now,
            reason: plan_reason.map(str::to_string),
        };
        (plan_commitment(&plan, &planned), "v1")
    };
    let plan = PlanRecord {
        event_id: event_id.to_string(),
        version,
        seed: plan_seed,
        rounds,
        settings_json,
        commitment: commitment.clone(),
        committed_at: now,
        reason: plan_reason.map(str::to_string),
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
    PlanRepo::set_commitment_meta(
        &transaction,
        event_id,
        version,
        &nonce,
        pack.as_ref().map(|pack| pack.id.as_str()),
        recipe,
    )?;
    if round_model == 1 {
        for (track_code, owner_id) in &seed_owners {
            SeedOwnerRepo::insert(&transaction, event_id, version, track_code, owner_id)?;
        }
    }
    for assignment in &planned {
        PlanRepo::insert_assignment(&transaction, assignment)?;
    }
    commit(transaction)?;

    Ok(ScheduleOutcome {
        version,
        rounds,
        commitment,
        fallback,
        gaps,
        assignments: planned,
    })
}

type PlanRoster = (
    std::collections::BTreeSet<String>,
    std::collections::BTreeMap<String, String>,
    std::collections::BTreeMap<String, String>,
    std::collections::BTreeSet<String>,
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
        .iter()
        .map(|participant| {
            (
                participant.user_id.clone(),
                participant.display_name.clone(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    // D-051: a person who left keeps their planned rows as gaps, and their
    // seed keeps circulating. The plan can still name them.
    let departed = participants
        .iter()
        .filter(|participant| participant.membership == Membership::Left)
        .map(|participant| participant.user_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let owners = TrackRepo::list_for_event(conn, event_id)?
        .into_iter()
        .map(|track| (track.code, track.owner_id))
        .collect::<std::collections::BTreeMap<_, _>>();
    Ok((active, owners, names, departed))
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
    departed: &std::collections::BTreeSet<String>,
    allow_gaps: bool,
) -> Result<(), EngineError> {
    let known = |user_id: &String| active.contains(user_id) || departed.contains(user_id);
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
        if !known(&assignment.participant_id) {
            return Err(EngineError::PlanInvalid(format!(
                "{} is not active and ready",
                assignment.participant_id
            )));
        }
        if !known(owner) {
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
    if !allow_gaps && !departed.is_empty() {
        // A leave (D-051) leaves gaps and can leave one track more than
        // people. Every active person still works once in each round.
        for round in 0..rounds {
            let people = assignments
                .iter()
                .filter(|assignment| assignment.round == round)
                .map(|assignment| assignment.participant_id.as_str())
                .collect::<Vec<_>>();
            let distinct = people
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            if distinct.len() != people.len() {
                return Err(EngineError::PlanInvalid(format!(
                    "round {round} gives one person two tracks"
                )));
            }
            if !active
                .iter()
                .all(|person| distinct.contains(person.as_str()))
            {
                return Err(EngineError::PlanInvalid(format!(
                    "round {round} is missing an active person"
                )));
            }
        }
    } else if !allow_gaps {
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
    let (active, owners, names, departed) = plan_roster(db.conn(), event_id)?;
    let allow_gaps = parse_settings(&event)?.handoff_policy.as_deref() == Some("Flexible");
    validate_plan_assignments(
        &assignments,
        plan.rounds,
        &active,
        &owners,
        &departed,
        allow_gaps,
    )?;
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
        if validate_plan_assignments(
            &swapped,
            plan.rounds,
            &active,
            &owners,
            &departed,
            allow_gaps,
        )
        .is_ok()
        {
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
    let (active, owners, _, departed) = plan_roster(db.conn(), event_id)?;
    let allow_gaps = parse_settings(&event)?.handoff_policy.as_deref() == Some("Flexible");
    validate_plan_assignments(
        &assignments,
        plan.rounds,
        &active,
        &owners,
        &departed,
        allow_gaps,
    )?;
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
    validate_plan_assignments(
        &assignments,
        plan.rounds,
        &active,
        &owners,
        &departed,
        allow_gaps,
    )?;

    let version = plan.version + 1;
    for assignment in &mut assignments {
        assignment.plan_version = version;
    }
    let round_model = EventRepo::round_model(db.conn(), event_id)?;
    let secret_hex = EventRepo::secret_hex(db.conn(), event_id)?;
    let pack = PackRepo::get_for_event(db.conn(), event_id)?;
    let nonce = random_hex_128();
    let seed_owners = if round_model == 1 {
        SeedOwnerRepo::list(db.conn(), event_id, plan.version)?
    } else {
        Vec::new()
    };
    let (commitment, recipe) = if round_model == 1 {
        (
            plan_commitment_v2(&CommitmentInputs {
                event_id,
                plan_version: version,
                total_rounds: plan.rounds + 1,
                exchange_rounds: plan.rounds,
                secret_hex: secret_hex.as_deref().unwrap_or("legacy"),
                nonce: &nonce,
                pack_id: pack.as_ref().map(|pack| pack.id.as_str()),
                settings_json: &plan.settings_json,
                seed_owners: &seed_owners,
                assignments: &assignments,
            }),
            "v2",
        )
    } else {
        let next = PlanRecord {
            event_id: event_id.to_string(),
            version,
            seed: plan.seed,
            rounds: plan.rounds,
            settings_json: plan.settings_json.clone(),
            commitment: String::new(),
            committed_at: now,
            reason: Some("manual edit".to_string()),
        };
        (plan_commitment(&next, &assignments), "v1")
    };
    let mut next = PlanRecord {
        event_id: event_id.to_string(),
        version,
        seed: plan.seed,
        rounds: plan.rounds,
        settings_json: plan.settings_json.clone(),
        commitment: commitment.clone(),
        committed_at: now,
        reason: Some("manual edit".to_string()),
    };
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
    PlanRepo::set_commitment_meta(
        &transaction,
        event_id,
        version,
        &nonce,
        pack.as_ref().map(|pack| pack.id.as_str()),
        recipe,
    )?;
    if round_model == 1 {
        for (track_code, owner_id) in &seed_owners {
            SeedOwnerRepo::insert(&transaction, event_id, version, track_code, owner_id)?;
        }
    }
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
        gaps: 0,
        assignments,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyOutcome {
    pub recipe: String,
    pub commitment: String,
    pub verified: bool,
}

fn verify_plan_commitment(
    conn: &Connection,
    event_id: &str,
    plan: &PlanRecord,
    assignments: &[PlanAssignmentRecord],
) -> Result<(), EngineError> {
    let meta =
        PlanRepo::commitment_meta(conn, event_id, plan.version)?.ok_or(EngineError::NoPlan)?;
    let expected = if meta.recipe == "v2" {
        let secret = EventRepo::secret_hex(conn, event_id)?.unwrap_or_default();
        let owners = SeedOwnerRepo::list(conn, event_id, plan.version)?;
        plan_commitment_v2(&CommitmentInputs {
            event_id,
            plan_version: plan.version,
            total_rounds: plan.rounds + 1,
            exchange_rounds: plan.rounds,
            secret_hex: &secret,
            nonce: meta.nonce.as_deref().unwrap_or_default(),
            pack_id: meta.pack_id.as_deref(),
            settings_json: &plan.settings_json,
            seed_owners: &owners,
            assignments,
        })
    } else {
        plan_commitment(plan, assignments)
    };
    if expected != plan.commitment {
        return Err(EngineError::CommitmentMismatch);
    }
    Ok(())
}

/// Recompute the commitment from stored rows. The host and any participant can
/// run this after the reveal.
pub fn verify_commitment(db: &Db, event_id: &str) -> Result<VerifyOutcome, EngineError> {
    let plan = PlanRepo::latest(db.conn(), event_id)?.ok_or(EngineError::NoPlan)?;
    let assignments = PlanRepo::list_assignments(db.conn(), event_id, plan.version)?;
    let meta =
        PlanRepo::commitment_meta(db.conn(), event_id, plan.version)?.ok_or(EngineError::NoPlan)?;
    let verified = verify_plan_commitment(db.conn(), event_id, &plan, &assignments).is_ok();
    Ok(VerifyOutcome {
        recipe: meta.recipe,
        commitment: plan.commitment,
        verified,
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
    if EventRepo::round_model(&transaction, event_id)? == 1 {
        let seed_round =
            RoundRepo::get(&transaction, event_id, 1)?.ok_or(EngineError::SeedRoundNotOpen)?;
        if seed_round.status != RoundStatus::Closed {
            return Err(EngineError::SeedRoundNotOpen);
        }
    }
    let assignments = PlanRepo::list_assignments(&transaction, event_id, plan.version)?;
    let (active, owners, _, departed) = plan_roster(&transaction, event_id)?;
    let settings = parse_settings(&event)?;
    validate_plan_assignments(
        &assignments,
        plan.rounds,
        &active,
        &owners,
        &departed,
        settings.handoff_policy.as_deref() == Some("Flexible"),
    )?;
    verify_plan_commitment(&transaction, event_id, &plan, &assignments)?;
    let mut cursor = now;
    for index in 0..plan.rounds {
        let final_round = index + 1 == plan.rounds;
        cursor += if final_round {
            settings.final_round_window_secs
        } else {
            settings.round_window_secs
        };
        let round_no = index + 2;
        if RoundRepo::get(&transaction, event_id, round_no)?.is_none() {
            RoundRepo::insert(
                &transaction,
                &EventRoundRecord {
                    event_id: event_id.to_string(),
                    round_no,
                    kind: RoundKind::Exchange,
                    status: RoundStatus::Planned,
                    opened_at: None,
                    closed_at: None,
                    deadline_at: Some(cursor),
                    original_deadline_at: Some(cursor),
                    final_round,
                    deadline_moves: 0,
                },
            )?;
        }
    }
    EventRepo::set_reveal_at(&transaction, event_id, Some(cursor))?;
    let next = current.state.transition_to(EventState::Active)?;
    EventRepo::set_state(&transaction, event_id, next, now)?;
    let effect = channel_id
        .filter(|channel_id| !channel_id.trim().is_empty())
        .map(|channel_id| Effect::Channel {
            channel_id: channel_id.to_string(),
            content: format!(
                "Schedule locked for {}. Commitment: {}. You can verify this after the reveal.",
                event.name, plan.commitment
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

pub fn start_async_event(db: &Db, event_id: &str, now: i64) -> Result<(), EngineError> {
    let event = load_event(db, event_id)?;
    require_state(
        &event,
        EventState::ScheduleReview,
        "async event start",
        "schedule review",
    )?;
    if parse_settings(&event)?.mode != Mode::Async {
        return Err(EngineError::NotAsyncMode);
    }
    let settings = parse_settings(&event)?;
    let participants = ParticipantRepo::list_for_event(db.conn(), event_id)?;
    let people = participants
        .iter()
        .filter(|participant| {
            participant.membership == Membership::Active && participant.role == Role::Participant
        })
        .count();
    let tracks = TrackRepo::list_for_event(db.conn(), event_id)?.len();
    if people != tracks
        || settings
            .async_expected_songs
            .is_some_and(|expected| expected as usize != tracks)
    {
        return Err(EngineError::AsyncTargetMismatch {
            people,
            tracks,
            expected: settings.async_expected_songs,
        });
    }
    if let Some(claims) = settings.async_claims_per_participant
        && claims as usize >= tracks
    {
        return Err(EngineError::AsyncClaimTarget { claims, tracks });
    }
    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if current.state != EventState::ScheduleReview {
        return Err(EngineError::WrongEventState {
            action: "async event start",
            state: current.state,
            expected: "schedule review",
        });
    }
    EventRepo::set_state(&transaction, event_id, EventState::Active, now)?;
    commit(transaction)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncGap {
    pub track_code: String,
    pub participant_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsyncCoverage {
    pub tracks: usize,
    pub participants: usize,
    pub exposed_pairs: usize,
    pub possible_pairs: usize,
    pub claimed_handoffs: usize,
    pub target_handoffs: usize,
    pub gaps: Vec<AsyncGap>,
}

/// Coverage and gaps for the async queue. A pair is exposed when the seen
/// ledger has a claim or a claimed miss. Expired offers and pre-claim skips
/// are gaps. Full pair coverage is T*(P-1) when nobody touches their own
/// track.
pub fn set_participant_ready(
    db: &Db,
    event_id: &str,
    user_id: &str,
    ready: bool,
    now: i64,
) -> Result<(), EngineError> {
    let event = load_event(db, event_id)?;
    require_open_event(&event, "ready")?;
    let transaction = begin(db)?;
    let mut participant = ParticipantRepo::get(&transaction, event_id, user_id)?
        .ok_or_else(|| EngineError::ParticipantNotFound(user_id.to_string()))?;
    participant.readiness = if ready {
        Readiness::Ready
    } else {
        Readiness::Paused
    };
    participant.updated_at = now;
    ParticipantRepo::upsert(&transaction, &participant)?;
    if ready {
        let reset = ExpiryStateRepo::reset(&transaction, event_id, user_id)?;
        AuditRepo::insert(
            &transaction,
            event_id,
            now,
            "Participant",
            "Participant ready",
            Some(user_id),
            &serde_json::json!({ "expiryStrikesReset": reset }).to_string(),
        )?;
        if reset {
            AuditRepo::insert(
                &transaction,
                event_id,
                now,
                "Participant",
                "Expiry state reset",
                Some(user_id),
                "{}",
            )?;
        }
    } else {
        AuditRepo::insert(
            &transaction,
            event_id,
            now,
            "Participant",
            "Participant paused",
            Some(user_id),
            "{}",
        )?;
    }
    commit(transaction)?;
    Ok(())
}

pub fn async_coverage(db: &Db, event_id: &str) -> Result<AsyncCoverage, EngineError> {
    let event = load_event(db, event_id)?;
    if parse_settings(&event)?.mode != Mode::Async {
        return Err(EngineError::NotAsyncMode);
    }
    let settings = parse_settings(&event)?;
    let participants: Vec<ParticipantRecord> =
        ParticipantRepo::list_for_event(db.conn(), event_id)?
            .into_iter()
            .filter(|participant| {
                participant.membership == Membership::Active
                    && participant.role == Role::Participant
            })
            .collect();
    let tracks = TrackRepo::list_for_event(db.conn(), event_id)?;
    let seen = SeenRepo::list_for_event(db.conn(), event_id)?;
    let assignments = AssignmentRepo::list_for_event(db.conn(), event_id)?;
    let target_claims = settings
        .async_claims_per_participant
        .map(|claims| claims as usize)
        .unwrap_or_else(|| tracks.len().saturating_sub(1));
    let mut seen_per_person: HashMap<&str, usize> = HashMap::new();
    for pair in &seen {
        *seen_per_person
            .entry(pair.participant_id.as_str())
            .or_default() += 1;
    }
    let mut gaps = Vec::new();
    for track in &tracks {
        for participant in &participants {
            if track.owner_id == participant.user_id {
                continue;
            }
            if seen.iter().any(|pair| {
                pair.track_code == track.code && pair.participant_id == participant.user_id
            }) {
                continue;
            }
            let latest = assignments
                .iter()
                .filter(|assignment| {
                    assignment.track_code == track.code
                        && assignment.participant_id == participant.user_id
                })
                .max_by_key(|assignment| assignment.updated_at);
            if latest.is_some_and(|assignment| {
                matches!(
                    assignment.status,
                    AssignmentStatus::Offered
                        | AssignmentStatus::Claimed
                        | AssignmentStatus::Overdue
                )
            }) {
                continue;
            }
            let reason = match latest.map(|assignment| assignment.status) {
                Some(AssignmentStatus::Expired) => "expired",
                Some(AssignmentStatus::SkippedBeforeClaim) => "skipped before claim",
                _ => {
                    if seen_per_person
                        .get(participant.user_id.as_str())
                        .copied()
                        .unwrap_or(0)
                        >= target_claims
                    {
                        "handoff cap"
                    } else {
                        "not offered"
                    }
                }
            };
            gaps.push(AsyncGap {
                track_code: track.code.clone(),
                participant_id: participant.user_id.clone(),
                reason: reason.to_string(),
            });
        }
    }
    let possible_pairs = tracks
        .len()
        .saturating_mul(participants.len().saturating_sub(1));
    Ok(AsyncCoverage {
        tracks: tracks.len(),
        participants: participants.len(),
        exposed_pairs: seen.len(),
        possible_pairs,
        claimed_handoffs: seen.len(),
        target_handoffs: participants.len().saturating_mul(target_claims),
        gaps,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsyncRelease {
    pub next_wave: u32,
    pub next_at: i64,
}

fn async_release_parts(settings: &EventSettings) -> (u32, u32) {
    settings
        .async_release_time
        .as_deref()
        .and_then(|value| {
            let hour = value.get(..2)?.parse::<u32>().ok()?;
            let minute = value.get(3..)?.parse::<u32>().ok()?;
            Some((hour, minute))
        })
        .unwrap_or((18, 0))
}

fn release_zone(settings: &EventSettings) -> Tz {
    settings
        .host_timezone
        .as_deref()
        .and_then(|zone| zone.parse::<Tz>().ok())
        .unwrap_or(chrono_tz::UTC)
}

/// The UTC instant of one local wall-clock time (D-042).
///
/// A time in a spring-forward gap moves forward to the first valid local time.
/// A time in a fall-back overlap uses the earlier offset.
fn local_release_instant(date: NaiveDate, hour: u32, minute: u32, zone: Tz) -> i64 {
    let naive = date
        .and_hms_opt(hour, minute, 0)
        .expect("the release time is validated as HH:MM");
    match zone.from_local_datetime(&naive) {
        chrono::LocalResult::Single(value) => value.timestamp(),
        chrono::LocalResult::Ambiguous(earliest, _) => earliest.timestamp(),
        chrono::LocalResult::None => {
            // The wall-clock time is inside a spring-forward gap. Move the
            // time forward by the width of the gap, which keeps the minute.
            for step in 1..=180 {
                let shifted = naive + ChronoDuration::minutes(step);
                match zone.from_local_datetime(&shifted) {
                    chrono::LocalResult::Single(first_valid) => {
                        let before = first_valid - ChronoDuration::minutes(1);
                        let width = i64::from(
                            first_valid.offset().fix().local_minus_utc()
                                - before.offset().fix().local_minus_utc(),
                        );
                        let adjusted = naive + ChronoDuration::seconds(width);
                        return match zone.from_local_datetime(&adjusted) {
                            chrono::LocalResult::Single(value) => value.timestamp(),
                            chrono::LocalResult::Ambiguous(earliest, _) => earliest.timestamp(),
                            chrono::LocalResult::None => first_valid.timestamp(),
                        };
                    }
                    chrono::LocalResult::Ambiguous(earliest, _) => return earliest.timestamp(),
                    chrono::LocalResult::None => {}
                }
            }
            naive.and_utc().timestamp()
        }
    }
}

/// The first wave of the release day that contains `now`, and that local day.
fn async_release_anchor(settings: &EventSettings, now: i64) -> (i64, NaiveDate) {
    let (hour, minute) = async_release_parts(settings);
    let zone = release_zone(settings);
    let local_now = Utc
        .timestamp_opt(now, 0)
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).unwrap())
        .with_timezone(&zone);
    let date = local_now.date_naive();
    (local_release_instant(date, hour, minute, zone), date)
}

pub fn next_async_release(db: &Db, event_id: &str, now: i64) -> Result<AsyncRelease, EngineError> {
    let event = load_event(db, event_id)?;
    let settings = parse_settings(&event)?;
    if settings.mode != Mode::Async {
        return Err(EngineError::NotAsyncMode);
    }
    let (first, date) = async_release_anchor(&settings, now);
    let second = first + settings.async_wave_offset_secs;
    if now < first {
        Ok(AsyncRelease {
            next_wave: 1,
            next_at: first,
        })
    } else if now < second {
        Ok(AsyncRelease {
            next_wave: 2,
            next_at: second,
        })
    } else {
        let (hour, minute) = async_release_parts(&settings);
        let tomorrow = date.succ_opt().unwrap_or(date);
        Ok(AsyncRelease {
            next_wave: 1,
            next_at: local_release_instant(tomorrow, hour, minute, release_zone(&settings)),
        })
    }
}

/// Dispatch the release wave that is due now. The host can call this from the
/// periodic worker. It never sends the same wave twice for one local day.
pub fn dispatch_async_release(
    db: &Db,
    event_id: &str,
    now: i64,
) -> Result<AsyncWaveOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    let settings = parse_settings(&event)?;
    if settings.mode != Mode::Async {
        return Err(EngineError::NotAsyncMode);
    }
    let (first, date) = async_release_anchor(&settings, now);
    let second = first + settings.async_wave_offset_secs;
    let wave = if now >= second {
        2
    } else if now >= first {
        1
    } else {
        return Ok(AsyncWaveOutcome {
            offers: 0,
            effects: Vec::new(),
            skips: Vec::new(),
        });
    };
    let key = format!(
        "async-release:{event_id}:{}:{wave}",
        date.format("%Y-%m-%d")
    );
    if MetaRepo::get(db.conn(), &key)?.is_some() {
        return Ok(AsyncWaveOutcome {
            offers: 0,
            effects: Vec::new(),
            skips: Vec::new(),
        });
    }
    let outcome = dispatch_async_wave(db, event_id, now)?;
    MetaRepo::set(db.conn(), &key, "1")?;
    AuditRepo::insert(
        db.conn(),
        event_id,
        now,
        "worker",
        "Async release wave",
        Some(&format!("Wave {wave}")),
        &serde_json::json!({ "wave": wave, "offers": outcome.offers }).to_string(),
    )?;
    Ok(outcome)
}

pub fn dispatch_async_wave(
    db: &Db,
    event_id: &str,
    now: i64,
) -> Result<AsyncWaveOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_open_event(&event, "async dispatch")?;
    let settings = parse_settings(&event)?;
    if settings.mode != Mode::Async {
        return Err(EngineError::NotAsyncMode);
    }
    let participants = ParticipantRepo::list_for_event(db.conn(), event_id)?;
    let expiry_states = ExpiryStateRepo::list_for_event(db.conn(), event_id)?;
    let state_by_person: HashMap<&str, &ExpiryStateRecord> = expiry_states
        .iter()
        .map(|state| (state.participant_id.as_str(), state))
        .collect();
    let mut ready: Vec<&ParticipantRecord> = Vec::new();
    let mut skips: Vec<AsyncSkip> = Vec::new();
    for participant in &participants {
        if participant.membership != Membership::Active
            || participant.role != Role::Participant
            || participant.readiness != Readiness::Ready
        {
            continue;
        }
        if let Some(until) = state_by_person
            .get(participant.user_id.as_str())
            .and_then(|state| state.cooldown_until)
            && until > now
        {
            skips.push(AsyncSkip {
                participant_id: participant.user_id.clone(),
                reason: format!(
                    "the expiry cooldown runs until {until}; the queue waits {} hours",
                    (until - now + 3_599) / 3_600
                ),
            });
            continue;
        }
        ready.push(participant);
    }
    if ready.is_empty() {
        return Ok(AsyncWaveOutcome {
            offers: 0,
            effects: Vec::new(),
            skips,
        });
    }
    let assignments = AssignmentRepo::list_for_event(db.conn(), event_id)?;
    let active = AssignmentRepo::list_active(db.conn(), event_id)?;
    let active_people: HashSet<&str> = active
        .iter()
        .map(|assignment| assignment.participant_id.as_str())
        .collect();
    let active_tracks: HashSet<&str> = active
        .iter()
        .map(|assignment| assignment.track_code.as_str())
        .collect();
    let seen_pairs = SeenRepo::list_for_event(db.conn(), event_id)?;
    let seen: HashSet<(&str, &str)> = seen_pairs
        .iter()
        .map(|pair| (pair.track_code.as_str(), pair.participant_id.as_str()))
        .collect();
    let expired_pairs = ExpiredPairRepo::list_for_event(db.conn(), event_id)?;
    let expired: HashMap<(&str, &str), &ExpiredPairRecord> = expired_pairs
        .iter()
        .map(|pair| {
            (
                (pair.track_code.as_str(), pair.participant_id.as_str()),
                pair,
            )
        })
        .collect();
    let tracks: Vec<TrackRecord> = TrackRepo::list_for_event(db.conn(), event_id)?
        .into_iter()
        .filter(|track| {
            !matches!(track.state, TrackState::Final | TrackState::Retired)
                && !active_tracks.contains(track.code.as_str())
        })
        .collect();

    let mut holders: HashMap<&str, HashSet<&str>> = HashMap::new();
    for track in &tracks {
        holders
            .entry(track.code.as_str())
            .or_default()
            .insert(track.owner_id.as_str());
    }
    for assignment in &assignments {
        if assignment.claimed_at.is_some()
            || matches!(
                assignment.status,
                AssignmentStatus::Claimed | AssignmentStatus::Submitted | AssignmentStatus::Overdue
            )
        {
            holders
                .entry(assignment.track_code.as_str())
                .or_default()
                .insert(assignment.participant_id.as_str());
        }
    }
    let mut pair_counts: HashMap<(&str, &str), u32> = HashMap::new();
    for set in holders.values() {
        let mut people: Vec<&str> = set.iter().copied().collect();
        people.sort_unstable();
        for left in 0..people.len() {
            for right in (left + 1)..people.len() {
                *pair_counts
                    .entry(ordered_pair(people[left], people[right]))
                    .or_default() += 1;
            }
        }
    }

    let ready_people: Vec<ParticipantId> = ready
        .iter()
        .filter(|participant| !active_people.contains(participant.user_id.as_str()))
        .map(|participant| ParticipantId::new(participant.user_id.clone()))
        .collect();
    let pair_meetings = |participant: &str, track: &str| -> u32 {
        let mut meetings = 0u32;
        if let Some(people) = holders.get(track) {
            for other in people {
                if *other == participant {
                    continue;
                }
                meetings += pair_counts
                    .get(&ordered_pair(participant, other))
                    .copied()
                    .unwrap_or(0);
            }
        }
        meetings
    };
    // The base options exclude every expired pair. The re-offer setting can
    // add a pair back: after the pair cooldown, or as a last resort when no
    // other person can take the track (MSW-085).
    let mut base: BTreeMap<ParticipantId, Vec<AsyncCandidate>> = BTreeMap::new();
    for participant in &ready_people {
        let mut options = Vec::new();
        for track in &tracks {
            if track.owner_id == participant.0
                || seen.contains(&(track.code.as_str(), participant.0.as_str()))
                || expired.contains_key(&(track.code.as_str(), participant.0.as_str()))
            {
                continue;
            }
            options.push(AsyncCandidate {
                track: TrackCode::new(track.code.clone()),
                pair_meetings: pair_meetings(&participant.0, track.code.as_str()),
            });
        }
        if !options.is_empty() {
            base.insert(participant.clone(), options);
        }
    }
    let tracks_with_candidate: HashSet<String> = base
        .values()
        .flat_map(|options| options.iter().map(|candidate| candidate.track.0.clone()))
        .collect();
    let mut offer_reasons: HashMap<(String, String), &'static str> = HashMap::new();
    let mut candidates = base;
    for participant in &ready_people {
        let mut options = candidates.get(participant).cloned().unwrap_or_default();
        for track in &tracks {
            if track.owner_id == participant.0
                || seen.contains(&(track.code.as_str(), participant.0.as_str()))
            {
                continue;
            }
            let Some(pair) = expired.get(&(track.code.as_str(), participant.0.as_str())) else {
                continue;
            };
            let (allowed, reason) = match settings.reoffer_after_expiry {
                ReofferAfterExpiry::Never => (false, "the re-offer rule is never"),
                ReofferAfterExpiry::AfterCooldown => {
                    let host_waiting = pair.last_assignment_id.as_deref().is_some_and(|id| {
                        HostDecisionRepo::open_for_assignment(db.conn(), id, "expiry")
                            .ok()
                            .flatten()
                            .is_some()
                    });
                    (
                        !host_waiting && pair.cooldown_until.is_none_or(|until| until <= now),
                        "re-offer after the cooldown",
                    )
                }
                ReofferAfterExpiry::LastResort => (
                    !tracks_with_candidate.contains(&track.code),
                    "re-offer as a last resort",
                ),
                ReofferAfterExpiry::HostDecides => (false, "the host decides the re-offer"),
            };
            if !allowed
                || options
                    .iter()
                    .any(|candidate| candidate.track.0 == track.code)
            {
                continue;
            }
            options.push(AsyncCandidate {
                track: TrackCode::new(track.code.clone()),
                pair_meetings: pair_meetings(&participant.0, track.code.as_str()),
            });
            offer_reasons.insert((participant.0.clone(), track.code.clone()), reason);
        }
        if !options.is_empty() {
            candidates.insert(participant.clone(), options);
        }
    }

    let choices = choose_async_offers(
        &ready_people,
        &candidates,
        event.seed.wrapping_add(now as u64),
    );
    let chosen: HashSet<&str> = choices
        .iter()
        .map(|choice| choice.participant.0.as_str())
        .collect();
    for participant in &ready_people {
        if !chosen.contains(participant.0.as_str()) {
            skips.push(AsyncSkip {
                participant_id: participant.0.clone(),
                reason: format!(
                    "no eligible track under the re-offer rule {:?}",
                    settings.reoffer_after_expiry
                ),
            });
        }
    }
    if choices.is_empty() {
        return Ok(AsyncWaveOutcome {
            offers: 0,
            effects: Vec::new(),
            skips,
        });
    }
    let round = assignments
        .iter()
        .map(|assignment| assignment.round)
        .max()
        .map(|round| round + 1)
        .unwrap_or(0);
    let transaction = begin(db)?;
    let current = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if current.state != EventState::Active {
        return Err(EngineError::WrongEventState {
            action: "async dispatch",
            state: current.state,
            expected: "active",
        });
    }
    let mut effects = Vec::with_capacity(choices.len());
    for choice in &choices {
        let id = assignment_id(event_id, round, &choice.track.0);
        let due_at = now + settings.claim_window_secs;
        if let Err(error) = insert_offer(
            &transaction,
            &AssignmentRecord {
                id: id.clone(),
                event_id: event_id.to_string(),
                track_code: choice.track.0.clone(),
                participant_id: choice.participant.0.clone(),
                round,
                status: AssignmentStatus::Offered,
                offered_at: now,
                claimed_at: None,
                due_at: Some(due_at),
                submitted_at: None,
                created_at: now,
                updated_at: now,
                extension_secs: 0,
                claim_deadline_at: Some(due_at),
            },
        ) {
            if matches!(error, EngineError::SeenPairOffer { .. }) {
                AuditRepo::insert(
                    &transaction,
                    event_id,
                    now,
                    "worker",
                    "Async gap skipped",
                    Some(&choice.track.0),
                    &serde_json::json!({
                        "round": round,
                        "trackCode": choice.track.0,
                        "participantId": choice.participant.0,
                        "reason": "the seen ledger already has this pair",
                    })
                    .to_string(),
                )?;
                continue;
            }
            return Err(error);
        }
        let reason = offer_reasons
            .get(&(choice.participant.0.clone(), choice.track.0.clone()))
            .copied()
            .unwrap_or("eligible track");
        AuditRepo::insert(
            &transaction,
            event_id,
            now,
            "worker",
            "Async offer",
            Some(&choice.track.0),
            &serde_json::json!({
                "round": round,
                "trackCode": choice.track.0,
                "participantId": choice.participant.0,
                "reason": reason,
            })
            .to_string(),
        )?;
        move_track(&transaction, event_id, &choice.track.0, TrackState::Offered)?;
        let effect = Effect::Offer {
            user_id: choice.participant.0.clone(),
            event_id: event_id.to_string(),
            assignment_id: id,
            track_code: choice.track.0.clone(),
            due_at,
            claim_window_secs: settings.claim_window_secs,
        };
        enqueue_effect(&transaction, event_id, &effect, now)?;
        effects.push(effect);
    }
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "worker",
        "Async wave dispatched",
        None,
        &serde_json::json!({
            "offers": effects.len(),
            "round": round,
            "skips": skips.iter().map(|skip| serde_json::json!({
                "participantId": skip.participant_id,
                "reason": skip.reason,
            })).collect::<Vec<_>>(),
        })
        .to_string(),
    )?;
    commit(transaction)?;
    Ok(AsyncWaveOutcome {
        offers: effects.len(),
        effects,
        skips,
    })
}

fn ordered_pair<'a>(left: &'a str, right: &'a str) -> (&'a str, &'a str) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
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
        || RoundRepo::get(db.conn(), event_id, round + 2)?
            .is_some_and(|row| row.opened_at.is_some())
    {
        return Err(EngineError::RoundAlreadyOpened { round });
    }
    for previous in 0..round {
        // A round can open with only gaps after a leave (D-051). The round
        // row then shows that the round opened.
        let opened = RoundRepo::get(db.conn(), event_id, previous + 2)?
            .is_some_and(|row| row.opened_at.is_some());
        if !opened
            && !assignments
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

    let round_no = round + 2;
    let final_round = round + 1 == plan.rounds;
    let deadline_at = if final_round {
        now + settings.final_round_window_secs
    } else {
        now + settings.round_window_secs
    };
    let remaining_claim = (deadline_at - now).max(1);

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
    if EventRepo::round_model(&transaction, event_id)? == 1 {
        let seed_round =
            RoundRepo::get(&transaction, event_id, 1)?.ok_or(EngineError::SeedRoundNotOpen)?;
        if seed_round.status != RoundStatus::Closed {
            return Err(EngineError::SeedRoundNotOpen);
        }
    }
    RoundRepo::close_open_rounds(&transaction, event_id, now)?;
    crate::fill_in_chain::on_round_open(&transaction, event_id, now)?;
    if !RoundRepo::plan_to_open(&transaction, event_id, round_no, now, deadline_at)? {
        RoundRepo::insert(
            &transaction,
            &EventRoundRecord {
                event_id: event_id.to_string(),
                round_no,
                kind: RoundKind::Exchange,
                status: RoundStatus::Open,
                opened_at: Some(now),
                closed_at: None,
                deadline_at: Some(deadline_at),
                original_deadline_at: Some(deadline_at),
                final_round,
                deadline_moves: 0,
            },
        )?;
    }
    let participants = ParticipantRepo::list_for_event(&transaction, event_id)?;
    for planned_assignment in &for_round {
        let participant = participants
            .iter()
            .find(|participant| participant.user_id == planned_assignment.participant_id)
            .ok_or_else(|| {
                EngineError::ParticipantNotFound(planned_assignment.participant_id.clone())
            })?;
        if participant.membership == Membership::Left {
            // D-051: the planned turn of a person who left is a gap. The
            // track passes unchanged; nobody gets an offer for this cell.
            AuditRepo::insert(
                &transaction,
                event_id,
                now,
                "Host",
                "Planned turn skipped",
                Some(&format!("R{round_no}")),
                &serde_json::json!({
                    "round": round_no,
                    "trackCode": planned_assignment.track_code,
                    "reason": "the participant left",
                })
                .to_string(),
            )?;
            continue;
        }
        if participant.membership != Membership::Active || participant.readiness != Readiness::Ready
        {
            return Err(EngineError::ParticipantNotReady(
                planned_assignment.participant_id.clone(),
            ));
        }
        if SeenRepo::has(
            &transaction,
            event_id,
            &planned_assignment.track_code,
            &planned_assignment.participant_id,
        )? {
            // D-051/MSW-080: an upgraded event can already have this pair in
            // the seen ledger. The cell becomes a gap; the round still opens.
            AuditRepo::insert(
                &transaction,
                event_id,
                now,
                "Host",
                "Planned turn skipped",
                Some(&format!("R{round_no}")),
                &serde_json::json!({
                    "round": round_no,
                    "trackCode": planned_assignment.track_code,
                    "reason": "the seen ledger already has this pair",
                })
                .to_string(),
            )?;
            enqueue_effect(
                &transaction,
                event_id,
                &Effect::HostAlert {
                    key: format!(
                        "notice:plan-gap:{}:{}:{}",
                        event_id, round_no, planned_assignment.track_code
                    ),
                    content: "A planned turn is a gap because the person already saw the track. Open the host app to fill the slot.".to_string(),
                },
                now,
            )?;
            continue;
        }
        let id = assignment_id(event_id, round, &planned_assignment.track_code);
        insert_offer(
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
                due_at: Some(deadline_at),
                submitted_at: None,
                created_at: now,
                updated_at: now,
                extension_secs: 0,
                claim_deadline_at: Some(deadline_at),
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
            due_at: deadline_at,
            claim_window_secs: remaining_claim,
        };
        enqueue_effect(&transaction, event_id, &effect, now)?;
        effects.push(effect);
    }
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Round opened",
        Some(&format!("R{round_no}")),
        &serde_json::json!({
            "round": round_no,
            "final": final_round,
            "deadlineAt": deadline_at,
            "offers": effects.len(),
        })
        .to_string(),
    )?;
    commit(transaction)?;

    Ok(OpenRoundOutcome {
        round,
        offers: effects.len(),
        effects,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveDeadlineOutcome {
    pub round_no: u32,
    pub new_deadline: i64,
    pub moved: usize,
    pub effects: Vec<Effect>,
}

/// Move the whole deadline of the open classic round. Every offered, claimed,
/// and overdue planned assignment gets the new deadline plus its own extension,
/// and its claim deadline moves to the new round deadline. A fill-in assignment
/// keeps its own claim and edit deadlines (D-069). Overdue work returns to
/// claimed. Submitted work keeps its final status. One audit entry records the
/// move.
pub fn move_round_deadline(
    db: &Db,
    event_id: &str,
    round_no: u32,
    new_deadline: i64,
    reason: &str,
    now: i64,
) -> Result<MoveDeadlineOutcome, EngineError> {
    let event = load_event(db, event_id)?;
    require_state(&event, EventState::Active, "round deadline move", "active")?;
    if round_no < 2 {
        return Err(EngineError::RoundNotOpen { round: round_no });
    }
    let transaction = begin(db)?;
    let round = RoundRepo::get(&transaction, event_id, round_no)?
        .ok_or(EngineError::RoundNotOpen { round: round_no })?;
    if round.status != RoundStatus::Open {
        return Err(EngineError::RoundNotOpen { round: round_no });
    }
    let old_deadline = round
        .deadline_at
        .ok_or(EngineError::RoundDeadlineMissing { round: round_no })?;
    if new_deadline <= now {
        return Err(EngineError::RoundDeadlineMissing { round: round_no });
    }
    if new_deadline <= old_deadline {
        return Err(EngineError::RoundDeadlineMissing { round: round_no });
    }
    RoundRepo::set_deadline(&transaction, event_id, round_no, new_deadline)?;
    let delta = new_deadline - old_deadline;
    RoundRepo::shift_planned(&transaction, event_id, round_no, delta)?;
    if let Some(reveal_at) = EventRepo::reveal_at(&transaction, event_id)? {
        EventRepo::set_reveal_at(&transaction, event_id, Some(reveal_at + delta))?;
    }
    let index = round_no - 2;
    let assignments = AssignmentRepo::list_for_event(&transaction, event_id)?;
    let mut moved = 0usize;
    let mut effects = Vec::new();
    for assignment in assignments
        .iter()
        .filter(|assignment| assignment.round == index)
    {
        if !matches!(
            assignment.status,
            AssignmentStatus::Offered | AssignmentStatus::Claimed | AssignmentStatus::Overdue
        ) {
            continue;
        }
        // A fill-in has its own claim and edit deadlines (D-069). A round
        // move never shortens or extends them.
        if is_fill_in_turn(event_id, assignment) {
            continue;
        }
        let due_at = (new_deadline + assignment.extension_secs).max(assignment.due_at.unwrap_or(0));
        AssignmentRepo::set_due_and_claim(
            &transaction,
            &assignment.id,
            due_at,
            assignment.extension_secs,
            new_deadline,
            now,
        )?;
        if assignment.status == AssignmentStatus::Overdue {
            AssignmentRepo::revert_overdue(&transaction, &assignment.id, now)?;
            if let Some(decision) =
                HostDecisionRepo::open_for_assignment(&transaction, &assignment.id, "overdue")?
            {
                HostDecisionRepo::close(&transaction, &decision.id, "addressed", now, reason)?;
            }
        }
        DeadlineRepo::ensure(&transaction, &assignment.id)?;
        DeadlineRepo::reset_reminders(&transaction, &assignment.id)?;
        moved += 1;
        effects.push(Effect::Dm {
            user_id: assignment.participant_id.clone(),
            content: format!(
                "The host moved the deadline for Round {round_no} to {new_deadline} UTC. Your track {} is due at {due_at} UTC.",
                assignment.track_code
            ),
        });
    }
    for effect in &effects {
        enqueue_effect(&transaction, event_id, effect, now)?;
    }
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Round deadline moved",
        Some(&format!("R{round_no}")),
        &serde_json::json!({
            "round": round_no,
            "oldDeadlineAt": old_deadline,
            "newDeadlineAt": new_deadline,
            "moved": moved,
            "reason": reason,
        })
        .to_string(),
    )?;
    commit(transaction)?;
    Ok(MoveDeadlineOutcome {
        round_no,
        new_deadline,
        moved,
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
    let claim_deadline_at = assignment.claim_deadline_at.unwrap_or(due_at);
    if now > claim_deadline_at {
        return Err(EngineError::OfferExpired);
    }
    let chained = FillInChainRepo::active_for_assignment(db.conn(), &assignment.id)?.is_some();
    let next_due = if EventRepo::round_model(db.conn(), event_id)? == 1 {
        if is_fill_in_turn(event_id, &assignment) {
            if chained {
                // D-069: the edit window of a chain or host-list offer starts
                // at the claim.
                now + settings.fill_in_window_secs
            } else {
                // The host set this due time. A claim keeps it, even when it
                // is before the round deadline (MSW-080).
                due_at
            }
        } else {
            // A planned offer has the shared round deadline.
            RoundRepo::get(db.conn(), event_id, assignment.round + 2)?
                .and_then(|round| round.deadline_at)
                .unwrap_or(now + settings.round_window_secs)
                .max(due_at)
        }
    } else {
        now + settings.round_window_secs
    };
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
    crate::fill_in_chain::on_claimed(&transaction, &assignment, now)?;
    SeenRepo::record(
        &transaction,
        event_id,
        &assignment.track_code,
        user_id,
        &assignment.id,
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
    if EventRepo::round_model(db.conn(), request.event_id)? == 1
        && assignment.due_at.is_some_and(|due_at| now > due_at)
    {
        return Err(EngineError::DeadlinePassed);
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
        &assignment.id,
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
    if !matches!(
        assignment.status,
        AssignmentStatus::Offered | AssignmentStatus::Claimed | AssignmentStatus::Overdue
    ) {
        return Err(EngineError::WrongAssignmentState(assignment.status));
    }

    let receipt = Effect::Dm {
        user_id: user_id.to_string(),
        content: format!(
            "You skipped track {}. The track returns to the pool unchanged.",
            assignment.track_code
        ),
    };
    let transaction = begin(db)?;
    open_event_in(&transaction, event_id, "skip")?;
    enqueue_effect(&transaction, event_id, &receipt, now)?;
    let status = skip_in(&transaction, &assignment, "the participant skipped", now)?;
    commit(transaction)?;

    let effects = vec![receipt];
    Ok(SkipOutcome { status, effects })
}

/// Skip one active assignment inside the caller's transaction. An offer that
/// is not claimed is not seen. A claimed or overdue track is seen, with the
/// assignment id. The track returns to the pool unchanged, and the fill-in
/// flow starts for the stranded slot. A skip and a leave (D-051) share this.
pub(crate) fn skip_in(
    conn: &Connection,
    assignment: &AssignmentRecord,
    reason: &str,
    now: i64,
) -> Result<AssignmentStatus, EngineError> {
    let (status, seen_reason) = match assignment.status {
        AssignmentStatus::Offered => (AssignmentStatus::SkippedBeforeClaim, None),
        AssignmentStatus::Claimed | AssignmentStatus::Overdue => (
            AssignmentStatus::SkippedAfterClaim,
            Some("skip_after_claim"),
        ),
        other => return Err(EngineError::WrongAssignmentState(other)),
    };
    let event_id = assignment.event_id.as_str();
    for kind in ["extension", "overdue"] {
        if let Some(open) = HostDecisionRepo::open_for_assignment(conn, &assignment.id, kind)? {
            HostDecisionRepo::close(conn, &open.id, "addressed", now, reason)?;
        }
    }
    AssignmentRepo::update_status(conn, &assignment.id, status, now)?;
    if let Some(seen_reason) = seen_reason {
        SeenRepo::record(
            conn,
            event_id,
            &assignment.track_code,
            &assignment.participant_id,
            &assignment.id,
            seen_reason,
            now,
        )?;
    }
    move_track(conn, event_id, &assignment.track_code, TrackState::Pool)?;
    crate::fill_in_chain::on_slot_stranded(conn, assignment, now)?;
    Ok(status)
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
    let claim_deadline_at = assignment
        .claim_deadline_at
        .or(assignment.due_at)
        .ok_or(EngineError::OfferExpired)?;
    if now < claim_deadline_at {
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
    let settings = parse_settings(&event)?;
    apply_expiry_policy(&transaction, &assignment, &settings, now)?;
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "worker",
        "Offer expired",
        None,
        &serde_json::json!({
            "assignmentId": assignment.id,
            "trackCode": assignment.track_code,
            "participantId": assignment.participant_id,
        })
        .to_string(),
    )?;
    crate::fill_in_chain::on_slot_stranded(&transaction, &assignment, now)?;
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
            let opened = RoundRepo::get(&transaction, event_id, round + 2)?
                .is_some_and(|row| row.opened_at.is_some());
            if !opened
                && !assignments
                    .iter()
                    .any(|assignment| assignment.round == round)
            {
                return Err(EngineError::PlanNotFinished { round });
            }
        }
        let planned = PlanRepo::list_assignments(&transaction, event_id, plan.version)?;
        verify_plan_commitment(&transaction, event_id, &plan, &planned)?;
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
                fill_in: crate::fill_in::is_fill_in_turn(event_id, assignment)
                    || participants
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
                    let fill_in = crate::fill_in::is_fill_in_turn(event_id, assignment)
                        || participants
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

/// Build the reveal ZIP: one final audio file per track plus a credits JSON.
pub fn reveal_zip(db: &Db, store: &BlobStore, event_id: &str) -> Result<Vec<u8>, EngineError> {
    let package = reveal_package(db, event_id)?;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut manifest = Vec::new();
    for track in &package.album {
        let final_version = track
            .versions
            .iter()
            .find(|version| version.final_version)
            .or_else(|| track.versions.last());
        let Some(version) = final_version else {
            continue;
        };
        let file = FileRepo::get(db.conn(), &version.file_id)?
            .ok_or_else(|| EngineError::NoFile(track.track_code.clone()))?;
        let bytes = store.read(&file.sha256)?;
        let name = format!(
            "{:02}_{}.{}",
            track.position, track.track_code, version.extension
        );
        writer
            .start_file(&name, options)
            .map_err(|error| EngineError::RevealZip(error.to_string()))?;
        std::io::Write::write_all(&mut writer, &bytes)
            .map_err(|error| EngineError::RevealZip(error.to_string()))?;
        manifest.push(serde_json::json!({
            "position": track.position,
            "trackCode": track.track_code,
            "title": track.title,
            "seedOwner": track.seed_owner_name,
            "file": name,
            "credits": track.credits.iter().map(|credit| serde_json::json!({
                "round": credit.round,
                "name": credit.display_name,
                "status": format!("{:?}", credit.status),
                "fillIn": credit.fill_in,
            })).collect::<Vec<_>>(),
        }));
    }
    writer
        .start_file("README.txt", options)
        .map_err(|error| EngineError::RevealZip(error.to_string()))?;
    let readme = format!(
        "{}\n\nFinal tracks and credits from Monsquaz Swap.\n",
        package.event_name
    );
    std::io::Write::write_all(&mut writer, readme.as_bytes())
        .map_err(|error| EngineError::RevealZip(error.to_string()))?;
    writer
        .start_file("credits.json", options)
        .map_err(|error| EngineError::RevealZip(error.to_string()))?;
    let body = serde_json::to_vec_pretty(&manifest)?;
    std::io::Write::write_all(&mut writer, &body)
        .map_err(|error| EngineError::RevealZip(error.to_string()))?;
    writer
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|error| EngineError::RevealZip(error.to_string()))
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
            let round_model = EventRepo::round_model(&transaction, &assignment.event_id)?;
            let extra = hours * 3_600;
            let extension_secs = assignment.extension_secs + extra;
            let new_due = if round_model == 1 && !is_fill_in_turn(&assignment.event_id, &assignment)
            {
                RoundRepo::get(&transaction, &assignment.event_id, assignment.round + 2)?
                    .and_then(|round| round.deadline_at)
                    .map(|deadline_at| (deadline_at + extension_secs).max(due_at + extra))
                    .unwrap_or(due_at.max(now) + extra)
            } else {
                due_at.max(now) + extra
            };
            if !AssignmentRepo::set_due(&transaction, assignment_id, new_due, extension_secs, now)?
            {
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
                &assignment.id,
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
            crate::fill_in_chain::on_slot_stranded(&transaction, &assignment, now)?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpiryDecisionOutcome {
    pub action: String,
    pub assignment_id: Option<String>,
}

/// Decide one expired offer (MSW-084, MSW-085). The host can pause the
/// participant, start a cooldown, ignore the expiry, or re-offer the same
/// track when the re-offer setting is `host_decides`.
pub fn decide_expiry(
    db: &Db,
    decision_id: &str,
    action: &str,
    now: i64,
) -> Result<ExpiryDecisionOutcome, EngineError> {
    let transaction = begin(db)?;
    let decision = HostDecisionRepo::get(&transaction, decision_id)?
        .ok_or_else(|| EngineError::DecisionNotFound(decision_id.to_string()))?;
    if decision.kind != "expiry" {
        return Err(EngineError::WrongDecisionKind(decision.kind));
    }
    if decision.status != "open" {
        return Err(EngineError::DecisionNotOpen);
    }
    let assignment_id = decision
        .assignment_id
        .clone()
        .ok_or_else(|| EngineError::DecisionNotFound(decision_id.to_string()))?;
    let assignment = AssignmentRepo::get(&transaction, &assignment_id)?
        .ok_or_else(|| EngineError::AssignmentNotFound(assignment_id.clone()))?;
    let event = EventRepo::get(&transaction, &assignment.event_id)?
        .ok_or_else(|| EngineError::EventNotFound(assignment.event_id.clone()))?;
    require_open_event(&event, "expiry decision")?;
    let settings = parse_settings(&event)?;
    let mut participant = ParticipantRepo::get(
        &transaction,
        &assignment.event_id,
        &assignment.participant_id,
    )?
    .ok_or_else(|| EngineError::ParticipantNotFound(assignment.participant_id.clone()))?;
    match action {
        "ignore" => {
            HostDecisionRepo::close(&transaction, &decision.id, "addressed", now, action)?;
        }
        "cooldown" => {
            let until = now + EXPIRY_COOLDOWN_SECS;
            let mut state = ExpiryStateRepo::get(
                &transaction,
                &assignment.event_id,
                &assignment.participant_id,
            )?
            .unwrap_or_else(|| ExpiryStateRecord {
                event_id: assignment.event_id.clone(),
                participant_id: assignment.participant_id.clone(),
                updated_at: now,
                ..ExpiryStateRecord::default()
            });
            state.cooldown_until = Some(until);
            state.updated_at = now;
            ExpiryStateRepo::upsert(&transaction, &state)?;
            ExpiredPairRepo::set_cooldown(
                &transaction,
                &assignment.event_id,
                &assignment.track_code,
                &assignment.participant_id,
                Some(until),
            )?;
            HostDecisionRepo::close(&transaction, &decision.id, "addressed", now, action)?;
            enqueue_effect(
                &transaction,
                &assignment.event_id,
                &Effect::Dm {
                    user_id: assignment.participant_id.clone(),
                    content: "The host set a 24-hour cooldown on your next offer.".to_string(),
                },
                now,
            )?;
        }
        "pause" => {
            participant.readiness = Readiness::Paused;
            participant.updated_at = now;
            ParticipantRepo::upsert(&transaction, &participant)?;
            let mut state = ExpiryStateRepo::get(
                &transaction,
                &assignment.event_id,
                &assignment.participant_id,
            )?
            .unwrap_or_else(|| ExpiryStateRecord {
                event_id: assignment.event_id.clone(),
                participant_id: assignment.participant_id.clone(),
                updated_at: now,
                ..ExpiryStateRecord::default()
            });
            state.paused_at = Some(now);
            state.updated_at = now;
            ExpiryStateRepo::upsert(&transaction, &state)?;
            HostDecisionRepo::close(&transaction, &decision.id, "addressed", now, action)?;
            enqueue_effect(
                &transaction,
                &assignment.event_id,
                &Effect::Dm {
                    user_id: assignment.participant_id.clone(),
                    content: "The host paused your offers. Use /swap resume when you are ready."
                        .to_string(),
                },
                now,
            )?;
        }
        "reoffer" => {
            if settings.reoffer_after_expiry != ReofferAfterExpiry::HostDecides {
                return Err(EngineError::ReofferNotAllowed);
            }
            if participant.readiness != Readiness::Ready
                || participant.membership != Membership::Active
            {
                return Err(EngineError::ExpiryNotEligible(
                    "the participant is not ready".to_string(),
                ));
            }
            if AssignmentRepo::list_active(&transaction, &assignment.event_id)?
                .iter()
                .any(|active| active.participant_id == assignment.participant_id)
            {
                return Err(EngineError::ExpiryNotEligible(
                    "the participant already holds a track".to_string(),
                ));
            }
            if SeenRepo::has(
                &transaction,
                &assignment.event_id,
                &assignment.track_code,
                &assignment.participant_id,
            )? {
                return Err(EngineError::SeenPairOffer {
                    track: assignment.track_code.clone(),
                    participant: assignment.participant_id.clone(),
                });
            }
            let track = TrackRepo::get(&transaction, &assignment.event_id, &assignment.track_code)?
                .ok_or_else(|| EngineError::TrackNotFound(assignment.track_code.clone()))?;
            if matches!(track.state, TrackState::Final | TrackState::Retired) {
                return Err(EngineError::TrackPath(assignment.track_code.clone()));
            }
            if AssignmentRepo::list_active(&transaction, &assignment.event_id)?
                .iter()
                .any(|active| active.track_code == assignment.track_code)
            {
                return Err(EngineError::ExpiryNotEligible(
                    "the track has an active holder".to_string(),
                ));
            }
            let round = AssignmentRepo::list_for_event(&transaction, &assignment.event_id)?
                .iter()
                .map(|row| row.round)
                .max()
                .map(|round| round + 1)
                .unwrap_or(0);
            let id = crate::ids::assignment_id(&assignment.event_id, round, &assignment.track_code);
            let due_at = now + settings.claim_window_secs;
            insert_offer(
                &transaction,
                &AssignmentRecord {
                    id: id.clone(),
                    event_id: assignment.event_id.clone(),
                    track_code: assignment.track_code.clone(),
                    participant_id: assignment.participant_id.clone(),
                    round,
                    status: AssignmentStatus::Offered,
                    offered_at: now,
                    claimed_at: None,
                    due_at: Some(due_at),
                    submitted_at: None,
                    created_at: now,
                    updated_at: now,
                    extension_secs: 0,
                    claim_deadline_at: Some(due_at),
                },
            )?;
            move_track(
                &transaction,
                &assignment.event_id,
                &assignment.track_code,
                TrackState::Offered,
            )?;
            HostDecisionRepo::close(&transaction, &decision.id, "addressed", now, action)?;
            enqueue_effect(
                &transaction,
                &assignment.event_id,
                &Effect::Offer {
                    user_id: assignment.participant_id.clone(),
                    event_id: assignment.event_id.clone(),
                    assignment_id: id.clone(),
                    track_code: assignment.track_code.clone(),
                    due_at,
                    claim_window_secs: settings.claim_window_secs,
                },
                now,
            )?;
            AuditRepo::insert(
                &transaction,
                &assignment.event_id,
                now,
                "Host",
                "Expired pair re-offered",
                Some(&assignment.track_code),
                &serde_json::json!({
                    "participantId": assignment.participant_id,
                    "assignmentId": id,
                    "round": round,
                })
                .to_string(),
            )?;
            AuditRepo::insert(
                &transaction,
                &assignment.event_id,
                now,
                "Host",
                "Expiry decided",
                Some(&assignment.participant_id),
                &serde_json::json!({
                    "action": action,
                    "assignmentId": assignment_id,
                })
                .to_string(),
            )?;
            commit(transaction)?;
            return Ok(ExpiryDecisionOutcome {
                action: action.to_string(),
                assignment_id: Some(id),
            });
        }
        _ => {
            return Err(EngineError::WrongDecisionKind(format!(
                "unknown expiry action {action}"
            )));
        }
    }
    AuditRepo::insert(
        &transaction,
        &assignment.event_id,
        now,
        "Host",
        "Expiry decided",
        Some(&assignment.participant_id),
        &serde_json::json!({
            "action": action,
            "assignmentId": assignment.id,
        })
        .to_string(),
    )?;
    commit(transaction)?;
    Ok(ExpiryDecisionOutcome {
        action: action.to_string(),
        assignment_id: None,
    })
}

pub fn open_decisions(db: &Db, event_id: &str) -> Result<Vec<HostDecisionRecord>, EngineError> {
    Ok(HostDecisionRepo::list_open(db.conn(), event_id)?)
}

/// The length of the automatic cooldown after an expired offer (D-041).
pub const EXPIRY_COOLDOWN_SECS: i64 = 24 * 60 * 60;

fn record_expired_pair(
    conn: &Connection,
    assignment: &AssignmentRecord,
    now: i64,
    cooldown_until: Option<i64>,
) -> Result<(), EngineError> {
    ExpiredPairRepo::record(
        conn,
        &ExpiredPairRecord {
            event_id: assignment.event_id.clone(),
            track_code: assignment.track_code.clone(),
            participant_id: assignment.participant_id.clone(),
            expiries: 1,
            last_expired_at: now,
            cooldown_until,
            last_assignment_id: Some(assignment.id.clone()),
        },
    )?;
    Ok(())
}

/// Apply the expiry protection preset (MSW-084). A classic expiry only
/// records the pair: classic has no async cooldown or pause.
pub(crate) fn apply_expiry_policy(
    conn: &Connection,
    assignment: &AssignmentRecord,
    settings: &EventSettings,
    now: i64,
) -> Result<(), EngineError> {
    let event_id = assignment.event_id.as_str();
    if settings.mode == Mode::Classic {
        record_expired_pair(conn, assignment, now, Some(now + EXPIRY_COOLDOWN_SECS))?;
        return Ok(());
    }
    record_expired_pair(conn, assignment, now, None)?;
    let Some(mut participant) = ParticipantRepo::get(conn, event_id, &assignment.participant_id)?
    else {
        return Ok(());
    };
    let mut state = ExpiryStateRepo::get(conn, event_id, &assignment.participant_id)?
        .unwrap_or_else(|| ExpiryStateRecord {
            event_id: event_id.to_string(),
            participant_id: assignment.participant_id.clone(),
            updated_at: now,
            ..ExpiryStateRecord::default()
        });
    state.strikes = state.strikes.saturating_add(1);
    state.updated_at = now;
    let mut cooldown_until = None;
    let mut pause = false;
    let mut host_review = false;
    match settings.expiry_protection {
        ExpiryProtection::StrikeCooldown => {
            if state.strikes >= 2 {
                pause = true;
                state.paused_at = Some(now);
            } else {
                cooldown_until = Some(now + EXPIRY_COOLDOWN_SECS);
            }
        }
        ExpiryProtection::CooldownOnly => {
            cooldown_until = Some(now + EXPIRY_COOLDOWN_SECS);
        }
        ExpiryProtection::HostReview => host_review = true,
        ExpiryProtection::None => {}
    }
    state.cooldown_until = cooldown_until;
    ExpiryStateRepo::upsert(conn, &state)?;
    if let Some(until) = cooldown_until {
        ExpiredPairRepo::set_cooldown(
            conn,
            event_id,
            &assignment.track_code,
            &assignment.participant_id,
            Some(until),
        )?;
    }
    if pause {
        participant.readiness = Readiness::Paused;
        participant.updated_at = now;
        ParticipantRepo::upsert(conn, &participant)?;
        enqueue_effect(
            conn,
            event_id,
            &Effect::Dm {
                user_id: assignment.participant_id.clone(),
                content: "A second offer expired. The queue paused your offers until you use /swap resume or the host resumes you.".to_string(),
            },
            now,
        )?;
        enqueue_effect(
            conn,
            event_id,
            &Effect::HostAlert {
                key: format!("expiry-pause:{}", assignment.id),
                content: format!(
                    "{} had a second expired offer. The queue paused {} until a resume or a host action.",
                    participant.display_name, participant.display_name
                ),
            },
            now,
        )?;
        AuditRepo::insert(
            conn,
            event_id,
            now,
            "worker",
            "Participant auto-paused",
            Some(&assignment.participant_id),
            &serde_json::json!({
                "reason": "second expired offer",
                "assignmentId": assignment.id,
                "trackCode": assignment.track_code,
                "strikes": state.strikes,
            })
            .to_string(),
        )?;
    } else if let Some(until) = cooldown_until {
        let hours = (until - now).max(0) / 3_600;
        enqueue_effect(
            conn,
            event_id,
            &Effect::Dm {
                user_id: assignment.participant_id.clone(),
                content: format!(
                    "Your offer expired. The queue waits {hours} hours before the next offer."
                ),
            },
            now,
        )?;
    }
    if host_review || settings.reoffer_after_expiry == ReofferAfterExpiry::HostDecides {
        if HostDecisionRepo::open_for_assignment(conn, &assignment.id, "expiry")?.is_none() {
            HostDecisionRepo::insert(
                conn,
                &HostDecisionRecord {
                    id: format!("{}:expiry", assignment.id),
                    event_id: event_id.to_string(),
                    assignment_id: Some(assignment.id.clone()),
                    kind: "expiry".to_string(),
                    status: "open".to_string(),
                    requested_hours: None,
                    created_at: now,
                    decided_at: None,
                    reason: Some(format!(
                        "expiry protection {:?}; re-offer {:?}",
                        settings.expiry_protection, settings.reoffer_after_expiry
                    )),
                },
            )?;
            enqueue_effect(
                conn,
                event_id,
                &Effect::HostAlert {
                    key: format!("expiry-review:{}", assignment.id),
                    content: format!(
                        "An offer for track {} expired. The host decides the next step.",
                        assignment.track_code
                    ),
                },
                now,
            )?;
        }
        enqueue_effect(
            conn,
            event_id,
            &Effect::Dm {
                user_id: assignment.participant_id.clone(),
                content: "Your offer expired. The host decides the next step.".to_string(),
            },
            now,
        )?;
    } else if !pause && cooldown_until.is_none() {
        enqueue_effect(
            conn,
            event_id,
            &Effect::Dm {
                user_id: assignment.participant_id.clone(),
                content: "Your offer expired. The track returns to the pool.".to_string(),
            },
            now,
        )?;
    }
    Ok(())
}

pub fn tick_deadlines(db: &Db, now: i64) -> Result<DeadlineReport, EngineError> {
    let events = EventRepo::list_active(db.conn())?;
    let mut report = DeadlineReport::default();
    for event in events {
        if parse_settings(&event)?.mode == Mode::Async {
            let _ = dispatch_async_release(db, &event.id, now);
        }
        let transaction = begin(db)?;
        let current = EventRepo::get(&transaction, &event.id)?
            .ok_or_else(|| EngineError::EventNotFound(event.id.clone()))?;
        if current.state != EventState::Active {
            continue;
        }
        let settings = parse_settings(&current)?;
        let round_model = EventRepo::round_model(&transaction, &event.id)?;
        let participants = ParticipantRepo::list_for_event(&transaction, &event.id)?;
        let assignments = AssignmentRepo::list_active(&transaction, &event.id)?;
        if settings.mode == Mode::Classic && round_model == 1 {
            for assignment in assignments
                .iter()
                .filter(|assignment| assignment.status == AssignmentStatus::Offered)
            {
                let Some(claim_deadline_at) = assignment.claim_deadline_at.or(assignment.due_at)
                else {
                    continue;
                };
                if now >= claim_deadline_at {
                    AssignmentRepo::update_status(
                        &transaction,
                        &assignment.id,
                        AssignmentStatus::Expired,
                        now,
                    )?;
                    move_track(
                        &transaction,
                        &event.id,
                        &assignment.track_code,
                        TrackState::Pool,
                    )?;
                    AuditRepo::insert(
                        &transaction,
                        &event.id,
                        now,
                        "worker",
                        "Offer expired",
                        None,
                        &serde_json::json!({
                            "assignmentId": assignment.id,
                            "trackCode": assignment.track_code,
                            "participantId": assignment.participant_id,
                            "round": assignment.round + 2,
                        })
                        .to_string(),
                    )?;
                    report.expired_offers += 1;
                    apply_expiry_policy(&transaction, assignment, &settings, now)?;
                    crate::fill_in_chain::on_slot_stranded(&transaction, assignment, now)?;
                }
            }
        }
        if settings.mode == Mode::Async {
            for assignment in assignments
                .iter()
                .filter(|assignment| assignment.status == AssignmentStatus::Offered)
            {
                let Some(claim_deadline_at) = assignment.claim_deadline_at.or(assignment.due_at)
                else {
                    continue;
                };
                if now >= claim_deadline_at {
                    AssignmentRepo::update_status(
                        &transaction,
                        &assignment.id,
                        AssignmentStatus::Expired,
                        now,
                    )?;
                    move_track(
                        &transaction,
                        &event.id,
                        &assignment.track_code,
                        TrackState::Pool,
                    )?;
                    apply_expiry_policy(&transaction, assignment, &settings, now)?;
                    AuditRepo::insert(
                        &transaction,
                        &event.id,
                        now,
                        "worker",
                        "Offer expired",
                        None,
                        &serde_json::json!({
                            "assignmentId": assignment.id,
                            "trackCode": assignment.track_code,
                            "participantId": assignment.participant_id
                        })
                        .to_string(),
                    )?;
                    report.expired_offers += 1;
                } else if let Some(reminder_secs) = settings.claim_reminder_secs
                    && claim_deadline_at - now <= reminder_secs
                    && AssignmentRepo::mark_claim_reminder(&transaction, &assignment.id, now)?
                {
                    let remaining = claim_deadline_at - now;
                    enqueue_effect(
                        &transaction,
                        &event.id,
                        &Effect::Prompt {
                            user_id: assignment.participant_id.clone(),
                            event_id: event.id.clone(),
                            content: format!(
                                "Track {} is still unclaimed. The claim window ends in {} at <t:{}:f> (<t:{}:R>).",
                                assignment.track_code,
                                human_duration(remaining),
                                claim_deadline_at,
                                claim_deadline_at
                            ),
                        },
                        now,
                    )?;
                    report.reminders += 1;
                }
            }
        }
        for assignment in assignments {
            if assignment.status == AssignmentStatus::Offered {
                continue;
            }
            let Some(due_at) = assignment.due_at else {
                continue;
            };
            DeadlineRepo::ensure(&transaction, &assignment.id)?;
            let claimed_at = if round_model == 1 && settings.mode == Mode::Classic {
                assignment.offered_at
            } else {
                assignment.claimed_at.unwrap_or(assignment.offered_at)
            };
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
                        &Effect::Prompt {
                            user_id: assignment.participant_id.clone(),
                            event_id: event.id.clone(),
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
                    &Effect::Prompt {
                        user_id: assignment.participant_id.clone(),
                        event_id: event.id.clone(),
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
        if settings.mode == Mode::Classic && round_model == 1 {
            crate::fill_in_chain::resume_waiting(&transaction, &event.id, now)?;
        }
        commit(transaction)?;
    }
    Ok(report)
}

pub(crate) fn participant_timezone(
    participants: &[ParticipantRecord],
    user_id: &str,
) -> Option<Tz> {
    participants
        .iter()
        .find(|participant| participant.user_id == user_id)
        .and_then(|participant| participant.timezone.parse().ok())
}

pub(crate) fn human_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    match (hours, minutes) {
        (0, 0) => "less than a minute".to_string(),
        (0, 1) => "1 minute".to_string(),
        (0, minutes) => format!("{minutes} minutes"),
        (1, 0) => "1 hour".to_string(),
        (hours, 0) => format!("{hours} hours"),
        (1, 1) => "1 hour and 1 minute".to_string(),
        (1, minutes) => format!("1 hour and {minutes} minutes"),
        (hours, 1) => format!("{hours} hours and 1 minute"),
        (hours, minutes) => format!("{hours} hours and {minutes} minutes"),
    }
}

pub(crate) fn quiet_hours(now: i64, timezone: Tz, start: u8, end: u8) -> bool {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantTurn {
    pub round_no: u32,
    pub track_code: String,
    pub status: AssignmentStatus,
    pub due_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantCell {
    pub round_no: u32,
    pub track_code: String,
    pub participant_name: String,
    pub owner_name: String,
    pub title: Option<String>,
    pub comments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantScheduleView {
    pub open: bool,
    pub own: Vec<ParticipantTurn>,
    pub full: Option<Vec<ParticipantCell>>,
    pub seen: usize,
    pub edited: usize,
    pub days_remaining: i64,
    pub current_offer: Option<ParticipantTurn>,
}

/// The participant schedule surface. With the default visibility it returns
/// only the caller's turns. With the open schedule it returns the full matrix,
/// seed owners, names, titles, and comments.
pub fn participant_schedule(
    db: &Db,
    event_id: &str,
    user_id: &str,
    now: i64,
) -> Result<ParticipantScheduleView, EngineError> {
    let event = load_event(db, event_id)?;
    if ParticipantRepo::get(db.conn(), event_id, user_id)?.is_none() {
        return Err(EngineError::ParticipantNotFound(user_id.to_string()));
    }
    let settings = parse_settings(&event)?;
    let assignments = AssignmentRepo::list_for_event(db.conn(), event_id)?;
    let own = assignments
        .iter()
        .filter(|assignment| assignment.participant_id == user_id)
        .map(|assignment| ParticipantTurn {
            round_no: assignment.round + 2,
            track_code: assignment.track_code.clone(),
            status: assignment.status,
            due_at: assignment.due_at,
        })
        .collect();
    let open = settings.schedule_visibility.as_deref() == Some("Open");
    let full = if open {
        let plan = PlanRepo::latest(db.conn(), event_id)?.ok_or(EngineError::NoPlan)?;
        let participants = ParticipantRepo::list_for_event(db.conn(), event_id)?;
        let tracks = TrackRepo::list_for_event(db.conn(), event_id)?;
        let titles = TrackTitleRepo::list_for_event(db.conn(), event_id)?;
        let comments = TrackCommentRepo::list_for_event(db.conn(), event_id)?;
        let name_for = |id: &str| {
            participants
                .iter()
                .find(|participant| participant.user_id == id)
                .map(|participant| participant.display_name.clone())
                .unwrap_or_else(|| id.to_string())
        };
        let mut cells = Vec::new();
        for assignment in PlanRepo::list_assignments(db.conn(), event_id, plan.version)? {
            let owner = tracks
                .iter()
                .find(|track| track.code == assignment.track_code)
                .map(|track| track.owner_id.as_str())
                .unwrap_or("");
            cells.push(ParticipantCell {
                round_no: assignment.round + 2,
                track_code: assignment.track_code.clone(),
                participant_name: name_for(&assignment.participant_id),
                owner_name: name_for(owner),
                title: titles
                    .iter()
                    .find(|title| title.track_code == assignment.track_code)
                    .map(|title| title.title.clone()),
                comments: comments
                    .iter()
                    .filter(|comment| {
                        comment.track_code == assignment.track_code && comment.redacted_at.is_none()
                    })
                    .map(|comment| comment.body.clone())
                    .collect(),
            });
        }
        Some(cells)
    } else {
        None
    };
    let seen = SeenRepo::list_for_event(db.conn(), event_id)?
        .into_iter()
        .filter(|pair| pair.participant_id == user_id)
        .count();
    let edited = assignments
        .iter()
        .filter(|assignment| {
            assignment.participant_id == user_id && assignment.status == AssignmentStatus::Submitted
        })
        .count();
    let current_offer = assignments
        .iter()
        .find(|assignment| {
            assignment.participant_id == user_id && assignment.status == AssignmentStatus::Offered
        })
        .map(|assignment| ParticipantTurn {
            round_no: assignment.round + 2,
            track_code: assignment.track_code.clone(),
            status: assignment.status,
            due_at: assignment.due_at,
        });
    let days_remaining = EventRepo::reveal_at(db.conn(), event_id)?
        .map(|at| (at - now).max(0) / 86_400)
        .unwrap_or(0);
    Ok(ParticipantScheduleView {
        open,
        own,
        full,
        seen,
        edited,
        days_remaining,
        current_offer,
    })
}

pub fn event_status(db: &Db, event_id: &str) -> Result<EventStatus, EngineError> {
    let event = load_event(db, event_id)?;
    let classic = parse_settings(&event)?.mode == Mode::Classic;
    let plan = PlanRepo::latest(db.conn(), event_id)?;
    let assignments = AssignmentRepo::list_for_event(db.conn(), event_id)?;
    let seed_round = RoundRepo::get(db.conn(), event_id, 1)?;
    let rounds = plan.as_ref().map(|plan| plan.rounds).unwrap_or(0);
    Ok(EventStatus {
        event_id: event.id,
        state: event.state,
        rounds,
        total_rounds: if rounds > 0 { rounds + 1 } else { 0 },
        seed_round_open: seed_round
            .as_ref()
            .is_some_and(|round| round.status == RoundStatus::Open),
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
        waiting_fill_ins: if classic {
            crate::fill_in::waiting_fill_ins(db.conn(), event_id)?
        } else {
            Vec::new()
        },
    })
}

pub(crate) fn begin(db: &Db) -> Result<Transaction<'_>, EngineError> {
    Ok(
        Transaction::new_unchecked(db.conn(), TransactionBehavior::Immediate)
            .map_err(DbError::from)?,
    )
}

pub(crate) fn commit(transaction: Transaction<'_>) -> Result<(), EngineError> {
    transaction.commit().map_err(DbError::from)?;
    Ok(())
}

pub(crate) fn load_event(db: &Db, event_id: &str) -> Result<EventRecord, EngineError> {
    EventRepo::get(db.conn(), event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))
}

pub(crate) fn parse_settings(event: &EventRecord) -> Result<EventSettings, EngineError> {
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

fn require_seed_state(event: &EventRecord, action: &'static str) -> Result<(), EngineError> {
    if matches!(event.state, EventState::Draft | EventState::Signup) {
        Ok(())
    } else {
        Err(EngineError::WrongEventState {
            action,
            state: event.state,
            expected: "draft or signup",
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

pub(crate) fn require_open_event(
    event: &EventRecord,
    action: &'static str,
) -> Result<(), EngineError> {
    match event.state {
        EventState::Active | EventState::Paused => Ok(()),
        state => Err(EngineError::WrongEventState {
            action,
            state,
            expected: "active or paused",
        }),
    }
}

fn seed_from_secret(secret_hex: Option<&str>, version: u32) -> u64 {
    let secret = secret_hex.unwrap_or("legacy");
    let digest = swap_files::sha256_hex(format!("msw-plan-seed-v2:{secret}:{version}").as_bytes());
    u64::from_str_radix(&digest[..16], 16).unwrap_or(0)
}

fn unique_track_code(tracks: &[TrackRecord], seed: u64, secret_hex: Option<&str>) -> String {
    let mut salt = 0u32;
    loop {
        let candidate = match secret_hex {
            Some(secret) => track_code_from_secret(secret, salt),
            None => track_code(seed, salt),
        };
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

/// Insert one offer. Every offer path uses this guard: a seen pair or an own
/// track is refused with a typed error before the database sees the row.
pub(crate) fn insert_offer(conn: &Connection, offer: &AssignmentRecord) -> Result<(), EngineError> {
    if SeenRepo::has(
        conn,
        &offer.event_id,
        &offer.track_code,
        &offer.participant_id,
    )? {
        return Err(EngineError::SeenPairOffer {
            track: offer.track_code.clone(),
            participant: offer.participant_id.clone(),
        });
    }
    if TrackRepo::get(conn, &offer.event_id, &offer.track_code)?
        .is_some_and(|track| track.owner_id == offer.participant_id)
    {
        return Err(EngineError::OwnTrackOffer {
            track: offer.track_code.clone(),
            participant: offer.participant_id.clone(),
        });
    }
    AssignmentRepo::insert(conn, offer)?;
    Ok(())
}

pub(crate) fn move_track(
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

    // scenario: Comments are published with attribution
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
