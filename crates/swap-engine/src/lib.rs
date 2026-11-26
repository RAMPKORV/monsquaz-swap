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
    ClaimOutcome, CreditEntry, EngineError, EventStatus, ExpireOutcome, JoinOutcome, JoinRequest,
    LockOutcome, OpenRoundOutcome, RevealOutcome, ScheduleOutcome, SkipOutcome, SubmissionSource,
    SubmitOutcome, SubmitRequest, TrackCredits, claim, create_event, event_status, expire_offer,
    generate_schedule, join_with_seed, lock_roster, open_round, reveal, signup, skip, start_event,
    submit, submit_attachment,
};
pub use ids::{assignment_id, plan_commitment, track_code};
pub use inbound::InboundService;
pub use prepare::{
    AB_SECONDS, DeliveryPreparer, OpenMptPreparer, PassthroughPreparer, PreparedBytes,
};
pub use settings::{EventSettings, Mode, SettingsError};
