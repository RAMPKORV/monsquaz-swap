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
    ClaimOutcome, CreditEntry, DeadlineDecisionOutcome, DeadlineReport, EngineError, EventStatus,
    ExpireOutcome, ExtensionOutcome, ExtensionStatus, JoinOutcome, JoinRequest, LockOutcome,
    OpenRoundOutcome, RevealComment, RevealOutcome, RevealPackage, RevealStats, RevealTrack,
    RevealVersion, RevealVersionRole, ScheduleOutcome, SkipOutcome, SubmissionSource,
    SubmitOutcome, SubmitRequest, TrackCredits, add_track_comment, claim, create_event,
    decide_deadline, decide_extension, event_status, expire_offer, generate_schedule,
    join_with_seed, lock_roster, open_decisions, open_round, redact_track_comment,
    request_extension, reveal, reveal_package, reveal_with_channel, set_track_title,
    set_track_title_for_user, signup, skip, start_event, submit, submit_attachment, tick_deadlines,
};
pub use ids::{assignment_id, plan_commitment, track_code};
pub use inbound::InboundService;
pub use prepare::{
    AB_SECONDS, DeliveryPreparer, OpenMptPreparer, PassthroughPreparer, PreparedBytes,
};
pub use settings::{EventSettings, Mode, SettingsError};
