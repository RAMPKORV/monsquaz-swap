//! Pure domain primitives for Monsquaz Swap.
//!
//! This crate has no I/O, no async runtime, and no Discord or Tauri types. It
//! protects the invariants that must never regress:
//!
//! 1. A participant never works on the same track twice.
//! 2. A participant is never assigned their own track.
//!
//! The state machines here are intentionally small. The app layer composes
//! them; the UI and the bot do not invent their own states.

pub mod ids;
pub mod ledger;
pub mod schedule;
pub mod state;

pub use ids::{ParticipantId, TrackCode};
pub use ledger::{LedgerError, SeenLedger, Touch};
pub use schedule::{
    BalanceReport, Schedule, ScheduleError, ScheduleStrategy, ScheduledAssignment, TrackSpec,
};
pub use state::{
    Assignment, AssignmentError, AssignmentStatus, EventState, Membership, Participant, Readiness,
    Role, StateError, Track, TrackState,
};

/// Backwards-compatible name for the ledger error.
pub type InvariantError = LedgerError;

pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");
