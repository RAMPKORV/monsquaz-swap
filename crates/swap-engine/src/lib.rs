//! The event application service.
//!
//! The engine owns the classic event loop: create, join by seed, lock roster,
//! commit a schedule, open rounds, claim, submit, skip, expire, reveal. Every
//! command is synchronous and returns the Discord effects to deliver. The live
//! transport and the UI call the same commands.

pub mod clock;
pub mod delivery;
pub mod effects;
pub mod engine;
pub mod ids;
pub mod inbound;
pub mod prepare;
pub mod settings;

pub use clock::{Clock, FixedClock, SystemClock};
pub use delivery::{
    DeliveryError, DeliveryReport, DeliveryWorker, PendingDelivery, dispatch_pending,
};
pub use effects::{Effect, dispatch};
pub use engine::{
    AsyncWaveOutcome, ClaimOutcome, CreditEntry, DeadlineDecisionOutcome, DeadlineReport,
    EngineError, EventStatus, ExpireOutcome, ExtensionOutcome, ExtensionStatus, GroupRepairOutcome,
    JoinOutcome, JoinRequest, LockOutcome, OpenRoundOutcome, ParticipantCell,
    ParticipantScheduleView, ParticipantTurn, PlanEditCandidate, RevealComment, RevealOutcome,
    RevealPackage, RevealStats, RevealTrack, RevealVersion, RevealVersionRole, ScheduleOutcome,
    SeedOpenOutcome, SkipOutcome, StartOutcome, SubmissionSource, SubmitOutcome, SubmitRequest,
    TrackCredits, VerifyOutcome, accept_late_seed, add_availability, add_track_comment, claim,
    create_event, create_event_with_secret, decide_deadline, decide_extension, dispatch_async_wave,
    edit_plan_assignment, event_status, expire_offer, extend_seed_deadline, generate_schedule,
    join_with_seed, list_availability, lock_roster, move_round_deadline, open_decisions,
    open_round, open_seed_round, participant_schedule, pending_late_seeds, plan_edit_candidates,
    redact_track_comment, reject_late_seed, remove_availability, reopen_seed_round,
    repair_group_schedule, request_extension, reveal, reveal_package, reveal_with_channel,
    set_track_title, set_track_title_for_user, set_track_title_for_user_on_track, signup, skip,
    start_async_event, start_event, start_event_with_channel, submit, submit_attachment,
    tick_deadlines, upload_pack, verify_commitment,
};
pub use ids::{CommitmentInputs, assignment_id, plan_commitment, plan_commitment_v2, track_code};
pub use inbound::InboundService;
pub use prepare::{
    AB_SECONDS, DeliveryPreparer, OpenMptPreparer, PassthroughPreparer, PreparedBytes,
};
pub use settings::{EventSettings, Mode, SettingsError};
