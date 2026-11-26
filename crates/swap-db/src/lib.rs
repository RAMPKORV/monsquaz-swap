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
    AssignmentRecord, AssignmentRepo, AuditRecord, AuditRepo, DeliveryRecord, DeliveryRepo,
    DmCursorRecord, DmCursorRepo, EventRecord, EventRepo, FileRecord, FileRepo, IdentityCounts,
    MetaRepo, ParticipantRecord, ParticipantRepo, PlanAssignmentRecord, PlanRecord, PlanRepo,
    PrivacyRepo, ProcessedAttachmentRepo, SeenPairRecord, SeenRepo, TrackRecord, TrackRepo,
};
pub use rusqlite::{Connection, Transaction};
