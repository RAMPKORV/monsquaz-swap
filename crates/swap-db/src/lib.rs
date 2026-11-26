//! SQLite persistence for Monsquaz Swap.
//!
//! The database is a single-writer projection. This crate stays synchronous;
//! callers use `spawn_blocking` when they build the Tauri service layer.

pub mod codec;
pub mod db;
pub mod migrations;
pub mod repos;

pub use db::{Db, DbError};
pub use repos::{
    AssignmentDeadlineRecord, AssignmentRecord, AssignmentRepo, AuditRecord, AuditRepo,
    DeadlineRepo, DeliveryRecord, DeliveryRepo, DmCursorRecord, DmCursorRepo, EventRecord,
    EventRepo, FileRecord, FileRepo, HostDecisionRecord, HostDecisionRepo, IdentityCounts,
    MetaRepo, ParticipantRecord, ParticipantRepo, PlanAssignmentRecord, PlanRecord, PlanRepo,
    PrivacyRepo, ProcessedAttachmentRepo, SeenPairRecord, SeenRepo, TrackCommentRecord,
    TrackCommentRepo, TrackRecord, TrackRepo, TrackTitleRecord, TrackTitleRepo,
};
pub use rusqlite::{Connection, Transaction, TransactionBehavior};
