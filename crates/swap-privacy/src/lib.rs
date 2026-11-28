//! Privacy and data-control operations for Monsquaz Swap.
//!
//! The functions here are host-driven and local. They never contact Discord.

pub mod delete;
pub mod diagnostics;
pub mod export;

pub use delete::{DeletionReport, delete_participant};
pub use diagnostics::{RedactionSpec, redact_diagnostics};
pub use export::{
    ExportAssignment, ExportComment, ExportFileInfo, ExportManifest, ExportSeenPair, ExportTitle,
    ParticipantExport, ParticipantInfo, export_participant,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PrivacyError {
    #[error(transparent)]
    Db(#[from] swap_db::DbError),
    #[error(transparent)]
    Store(#[from] swap_files::StoreError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error("participant not found: {0}")]
    ParticipantMissing(String),
    #[error("the participant has an active assignment; resolve it before deletion")]
    ActiveAssignment,
    #[error("the participant has more than one seed track")]
    MultipleSeedTracks,
    #[error("random data is unavailable: {0}")]
    Random(String),
    #[error("stored file {0} does not match its hash")]
    HashMismatch(String),
}
