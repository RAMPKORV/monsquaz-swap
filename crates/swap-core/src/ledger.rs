use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ids::{ParticipantId, TrackCode};

/// The event that caused a participant to interact with a track.
///
/// The hard rule is "received means seen". A claim, a submission, and a skip
/// after a claim all count. A skip before a claim and an expired offer do not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Touch {
    Claim,
    Submit,
    SkipAfterClaim,
    SkipBeforeClaim,
    OfferExpired,
}

impl Touch {
    pub fn counts_as_seen(self) -> bool {
        matches!(self, Self::Claim | Self::Submit | Self::SkipAfterClaim)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LedgerError {
    #[error("participant cannot be assigned their own track")]
    OwnTrack,
    #[error("participant has already worked on this track")]
    AlreadySeen,
}

/// Append-only set of every `(track, participant)` pair that counts as seen.
///
/// The ledger is a set. A normal claim → submit/skip flow is idempotent. The
/// no-repeat invariant is enforced by `validate_assignment`, not by rejecting
/// repeat events.
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

    /// Apply one touch. Returns `true` when this touch marked a new pair and
    /// `false` when the touch does not count as seen or the pair is already
    /// recorded.
    pub fn apply(&mut self, track: &TrackCode, participant: &ParticipantId, touch: Touch) -> bool {
        if !touch.counts_as_seen() {
            return false;
        }
        self.pairs.insert((track.clone(), participant.clone()))
    }

    /// Convenience: a claim is the canonical seen event.
    pub fn mark(&mut self, track: &TrackCode, participant: &ParticipantId) -> bool {
        self.apply(track, participant, Touch::Claim)
    }

    /// Check both hard invariants before an assignment is delivered.
    pub fn validate_assignment(
        &self,
        track: &TrackCode,
        participant: &ParticipantId,
        owner: &ParticipantId,
    ) -> Result<(), LedgerError> {
        if participant == owner {
            return Err(LedgerError::OwnTrack);
        }
        if self.has_seen(track, participant) {
            return Err(LedgerError::AlreadySeen);
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
        ParticipantId::new(id)
    }

    fn t(id: &str) -> TrackCode {
        TrackCode::new(id)
    }

    #[test]
    fn a_claim_marks_the_pair_seen() {
        let mut ledger = SeenLedger::new();
        assert!(ledger.apply(&t("S7F3A"), &p("alice"), Touch::Claim));
        assert!(ledger.has_seen(&t("S7F3A"), &p("alice")));
    }

    #[test]
    fn a_submission_marks_the_pair_seen() {
        let mut ledger = SeenLedger::new();
        assert!(ledger.apply(&t("S7F3A"), &p("alice"), Touch::Submit));
        assert!(ledger.has_seen(&t("S7F3A"), &p("alice")));
    }

    #[test]
    fn a_skip_after_claim_marks_the_pair_seen() {
        let mut ledger = SeenLedger::new();
        assert!(ledger.apply(&t("S7F3A"), &p("alice"), Touch::SkipAfterClaim));
        assert!(ledger.has_seen(&t("S7F3A"), &p("alice")));
    }

    #[test]
    fn a_skip_before_claim_does_not_mark_the_pair() {
        let mut ledger = SeenLedger::new();
        assert!(!ledger.apply(&t("S7F3A"), &p("alice"), Touch::SkipBeforeClaim));
        assert!(!ledger.has_seen(&t("S7F3A"), &p("alice")));
        assert!(ledger.is_empty());
    }

    #[test]
    fn an_expired_offer_does_not_mark_the_pair() {
        let mut ledger = SeenLedger::new();
        assert!(!ledger.apply(&t("S7F3A"), &p("alice"), Touch::OfferExpired));
        assert!(!ledger.has_seen(&t("S7F3A"), &p("alice")));
    }

    #[test]
    fn a_seen_pair_is_recorded_once() {
        let mut ledger = SeenLedger::new();
        assert!(ledger.apply(&t("S7F3A"), &p("alice"), Touch::Claim));
        assert!(!ledger.apply(&t("S7F3A"), &p("alice"), Touch::Submit));
        assert!(!ledger.apply(&t("S7F3A"), &p("alice"), Touch::SkipAfterClaim));
        assert_eq!(ledger.len(), 1);
        assert_eq!(
            ledger.validate_assignment(&t("S7F3A"), &p("alice"), &p("owner")),
            Err(LedgerError::AlreadySeen)
        );
    }

    #[test]
    fn a_non_seen_touch_after_a_seen_touch_is_harmless() {
        let mut ledger = SeenLedger::new();
        assert!(ledger.apply(&t("S7F3A"), &p("alice"), Touch::Claim));
        assert!(!ledger.apply(&t("S7F3A"), &p("alice"), Touch::OfferExpired));
        assert_eq!(ledger.len(), 1);
    }

    #[test]
    fn a_person_may_never_be_assigned_their_own_track() {
        let ledger = SeenLedger::new();
        assert_eq!(
            ledger.validate_assignment(&t("S7F3A"), &p("alice"), &p("alice")),
            Err(LedgerError::OwnTrack)
        );
    }

    #[test]
    fn different_people_can_share_a_track_and_one_person_can_hold_many_tracks() {
        let mut ledger = SeenLedger::new();
        assert!(ledger.apply(&t("T-1"), &p("alice"), Touch::Claim));
        assert!(ledger.apply(&t("T-1"), &p("bob"), Touch::Claim));
        assert!(ledger.apply(&t("T-2"), &p("alice"), Touch::Claim));
        assert_eq!(ledger.len(), 3);
    }

    #[test]
    fn a_repeated_touch_does_not_change_the_ledger_length() {
        let mut ledger = SeenLedger::new();
        assert!(ledger.apply(&t("T-1"), &p("alice"), Touch::Claim));
        let before = ledger.len();
        assert!(!ledger.apply(&t("T-1"), &p("alice"), Touch::Submit));
        assert_eq!(ledger.len(), before);
    }

    use proptest::prelude::*;

    fn any_touch() -> impl Strategy<Value = Touch> {
        prop_oneof![
            Just(Touch::Claim),
            Just(Touch::Submit),
            Just(Touch::SkipAfterClaim),
            Just(Touch::SkipBeforeClaim),
            Just(Touch::OfferExpired),
        ]
    }

    proptest! {
        #[test]
        fn seen_pairs_never_duplicate(
            events in prop::collection::vec(
                (0u8..4, 0u8..4, any_touch()),
                0..200,
            )
        ) {
            let mut ledger = SeenLedger::new();
            let mut seen = HashSet::new();
            for (track_id, person_id, touch) in events {
                let track = TrackCode::new(format!("T{track_id}"));
                let person = ParticipantId::new(format!("P{person_id}"));
                let marked = ledger.apply(&track, &person, touch);
                let key = (track.clone(), person.clone());
                if touch.counts_as_seen() {
                    if seen.contains(&key) {
                        prop_assert!(!marked);
                    } else {
                        prop_assert!(marked);
                        seen.insert(key);
                    }
                } else {
                    prop_assert!(!marked);
                }
                prop_assert_eq!(
                    ledger.validate_assignment(&track, &person, &ParticipantId::new("owner")),
                    if seen.contains(&(track.clone(), person.clone())) {
                        Err(LedgerError::AlreadySeen)
                    } else {
                        Ok(())
                    }
                );
            }
            prop_assert_eq!(ledger.len(), seen.len());
        }
    }
}
