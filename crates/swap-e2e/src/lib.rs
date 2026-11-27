//! Deterministic end-to-end harness for Monsquaz Swap.
//!
//! The crate is test-only. It holds the file clock, audio fixtures, temporary
//! workspace, and run report. The local Discord simulator and the journeys use
//! the same foundation.

pub mod app;
#[cfg(feature = "bridge")]
pub mod bridge;
pub mod clock;
pub mod fixtures;
pub mod report;
pub mod sim;
pub mod workspace;

pub use app::{BASE_TIME, HeadlessApp};
pub use clock::{FileClock, clock_from_env, system_now};
pub use fixtures::{invalid_module, mod_track, pack_zip, wav_tone};
pub use report::{CaseResult, CaseStatus, RunReport};
pub use sim::{
    DiscordSim, InjectInteractionRequest, InjectMessageRequest, SimConfig, SimInteraction, SimState,
};
pub use workspace::TempWorkspace;
