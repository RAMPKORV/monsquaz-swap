//! Pure domain primitives for Monsquaz Swap.
//!
//! This crate deliberately has no I/O, no async runtime, and no Discord or
//! Tauri types. It is the place for invariants that must never regress:
//! a participant may never work on the same track twice and may never be
//! assigned their own track.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ParticipantId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TrackCode(pub String);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum InvariantError {
    #[error("participant cannot be assigned their own track")]
    OwnTrack,
    #[error("participant has already worked on this track")]
    AlreadySeen,
}

/// Append-only memory of every (track, participant) pair that has been seen.
///
/// A pair is recorded when a participant claims a file, submits an edit, or
/// skips after claiming. Offers that are never claimed do not count.
#[derive(Debug, Default, Clone)]
pub struct SeenLedger {
    pairs: HashSet<(TrackCode, ParticipantId)>,
}

impl SeenLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_seen(&self, track: &TrackCode, participant: &ParticipantId) -> bool {
        self.pairs.contains(&(track.clone(), participant.clone()))
    }

    /// Record a touch. Returns `AlreadySeen` if the pair exists; the ledger is
    /// never mutated on an error.
    pub fn mark(
        &mut self,
        track: &TrackCode,
        participant: &ParticipantId,
    ) -> Result<(), InvariantError> {
        let key = (track.clone(), participant.clone());
        if self.pairs.contains(&key) {
            return Err(InvariantError::AlreadySeen);
        }
        self.pairs.insert(key);
        Ok(())
    }

    /// Check both hard invariants before an assignment is delivered.
    pub fn validate_assignment(
        &self,
        track: &TrackCode,
        participant: &ParticipantId,
        owner: &ParticipantId,
    ) -> Result<(), InvariantError> {
        if participant == owner {
            return Err(InvariantError::OwnTrack);
        }
        if self.has_seen(track, participant) {
            return Err(InvariantError::AlreadySeen);
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: &str) -> ParticipantId {
        ParticipantId(id.to_string())
    }

    fn t(id: &str) -> TrackCode {
        TrackCode(id.to_string())
    }

    #[test]
    fn a_person_may_never_see_the_same_track_twice() {
        let mut ledger = SeenLedger::new();
        ledger.mark(&t("T-7F3A"), &p("alice")).unwrap();
        assert_eq!(
            ledger.mark(&t("T-7F3A"), &p("alice")),
            Err(InvariantError::AlreadySeen)
        );
    }

    #[test]
    fn a_person_may_never_be_assigned_their_own_track() {
        let ledger = SeenLedger::new();
        assert_eq!(
            ledger.validate_assignment(&t("T-7F3A"), &p("alice"), &p("alice")),
            Err(InvariantError::OwnTrack)
        );
    }

    #[test]
    fn different_people_can_share_a_track_and_one_person_can_hold_many_tracks() {
        let mut ledger = SeenLedger::new();
        ledger.mark(&t("T-1"), &p("alice")).unwrap();
        ledger.mark(&t("T-1"), &p("bob")).unwrap();
        ledger.mark(&t("T-2"), &p("alice")).unwrap();
        assert_eq!(ledger.len(), 3);
    }

    #[test]
    fn a_failed_mark_does_not_mutate_the_ledger() {
        let mut ledger = SeenLedger::new();
        ledger.mark(&t("T-1"), &p("alice")).unwrap();
        let before = ledger.len();
        let _ = ledger.mark(&t("T-1"), &p("alice"));
        assert_eq!(ledger.len(), before);
    }
}
