use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Lifecycle of one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventState {
    Draft,
    Signup,
    ScheduleReview,
    Active,
    Paused,
    Reveal,
    Archived,
}

impl EventState {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Signup)
                | (Self::Signup, Self::ScheduleReview)
                | (Self::ScheduleReview, Self::Signup)
                | (Self::ScheduleReview, Self::Active)
                | (Self::Active, Self::Paused)
                | (Self::Active, Self::Reveal)
                | (Self::Paused, Self::Active)
                | (Self::Paused, Self::Reveal)
                | (Self::Reveal, Self::Archived)
        )
    }

    pub fn transition_to(self, next: Self) -> Result<Self, StateError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(StateError::InvalidEventTransition {
                from: self,
                to: next,
            })
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Archived)
    }
}

/// One classic round is either the seed round or an exchange round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundKind {
    Seed,
    Exchange,
}

/// Lifecycle of one classic round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundStatus {
    Planned,
    Open,
    Closed,
    Cancelled,
}

/// Membership is about the roster, not about readiness or a single turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Membership {
    SignedUp,
    Seeded,
    Active,
    Left,
    Removed,
}

impl Membership {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::SignedUp, Self::Seeded)
                | (Self::SignedUp, Self::Left)
                | (Self::SignedUp, Self::Removed)
                | (Self::Seeded, Self::Active)
                | (Self::Seeded, Self::Left)
                | (Self::Seeded, Self::Removed)
                | (Self::Active, Self::Left)
                | (Self::Active, Self::Removed)
                | (Self::Left, Self::Active)
        )
    }

    pub fn transition_to(self, next: Self) -> Result<Self, StateError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(StateError::InvalidMembershipTransition {
                from: self,
                to: next,
            })
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Left | Self::Removed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Participant,
    FillIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Readiness {
    Ready,
    Paused,
}

/// A participant in the event. The ID is stored outside this struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub membership: Membership,
    pub role: Role,
    pub readiness: Readiness,
}

impl Participant {
    pub fn new(role: Role) -> Self {
        Self {
            membership: Membership::SignedUp,
            role,
            readiness: Readiness::Ready,
        }
    }

    pub fn can_receive_offer(&self) -> bool {
        self.membership == Membership::Active && self.readiness == Readiness::Ready
    }

    pub fn pause(&mut self) {
        self.readiness = Readiness::Paused;
    }

    pub fn resume(&mut self) {
        self.readiness = Readiness::Ready;
    }
}

/// The custody lifecycle of one track lineage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackState {
    Seeded,
    Pool,
    Offered,
    InCustody,
    Submitted,
    Final,
    Retired,
}

impl TrackState {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Seeded, Self::Pool)
                | (Self::Pool, Self::Offered)
                | (Self::Pool, Self::Retired)
                | (Self::Offered, Self::InCustody)
                | (Self::Offered, Self::Pool)
                | (Self::Offered, Self::Retired)
                | (Self::InCustody, Self::Submitted)
                | (Self::InCustody, Self::Pool)
                | (Self::InCustody, Self::Retired)
                | (Self::Submitted, Self::Pool)
                | (Self::Submitted, Self::Final)
                | (Self::Submitted, Self::Retired)
                | (Self::Final, Self::Retired)
        )
    }

    pub fn transition_to(self, next: Self) -> Result<Self, StateError> {
        if self.can_transition_to(next) {
            Ok(next)
        } else {
            Err(StateError::InvalidTrackTransition {
                from: self,
                to: next,
            })
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Retired)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub state: TrackState,
}

impl Track {
    pub fn new() -> Self {
        Self {
            state: TrackState::Seeded,
        }
    }

    pub fn transition_to(&mut self, next: TrackState) -> Result<(), StateError> {
        self.state = self.state.transition_to(next)?;
        Ok(())
    }
}

impl Default for Track {
    fn default() -> Self {
        Self::new()
    }
}

/// One participant's turn with one track.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    Offered,
    Claimed,
    Submitted,
    SkippedBeforeClaim,
    SkippedAfterClaim,
    Expired,
    Overdue,
    Revoked,
    Superseded,
}

impl AssignmentStatus {
    /// A claim, a submission, and a claim-then-skip count as seen.
    /// A pre-claim skip and an expired offer do not.
    pub fn records_seen(self) -> bool {
        matches!(
            self,
            Self::Claimed | Self::Submitted | Self::SkippedAfterClaim
        )
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Submitted
                | Self::SkippedBeforeClaim
                | Self::SkippedAfterClaim
                | Self::Expired
                | Self::Revoked
                | Self::Superseded
        )
    }
}

/// A single assignment. The IDs live in the persistence layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    pub status: AssignmentStatus,
}

impl Assignment {
    pub fn offered() -> Self {
        Self {
            status: AssignmentStatus::Offered,
        }
    }

    pub fn claim(&mut self) -> Result<(), AssignmentError> {
        if self.status == AssignmentStatus::Offered {
            self.status = AssignmentStatus::Claimed;
            Ok(())
        } else {
            Err(self.invalid("claim"))
        }
    }

    pub fn submit(&mut self) -> Result<(), AssignmentError> {
        if matches!(
            self.status,
            AssignmentStatus::Claimed | AssignmentStatus::Overdue
        ) {
            self.status = AssignmentStatus::Submitted;
            Ok(())
        } else {
            Err(self.invalid("submit"))
        }
    }

    pub fn skip(&mut self) -> Result<(), AssignmentError> {
        let next = match self.status {
            AssignmentStatus::Offered => AssignmentStatus::SkippedBeforeClaim,
            AssignmentStatus::Claimed | AssignmentStatus::Overdue => {
                AssignmentStatus::SkippedAfterClaim
            }
            _ => return Err(self.invalid("skip")),
        };
        self.status = next;
        Ok(())
    }

    pub fn expire(&mut self) -> Result<(), AssignmentError> {
        if self.status == AssignmentStatus::Offered {
            self.status = AssignmentStatus::Expired;
            Ok(())
        } else {
            Err(self.invalid("expire"))
        }
    }

    pub fn mark_overdue(&mut self) -> Result<(), AssignmentError> {
        if self.status == AssignmentStatus::Claimed {
            self.status = AssignmentStatus::Overdue;
            Ok(())
        } else {
            Err(self.invalid("mark_overdue"))
        }
    }

    /// Return an overdue turn to claimed after the host moves the round deadline.
    pub fn restore_claimed(&mut self) -> Result<(), AssignmentError> {
        if self.status == AssignmentStatus::Overdue {
            self.status = AssignmentStatus::Claimed;
            Ok(())
        } else {
            Err(self.invalid("restore_claimed"))
        }
    }

    pub fn revoke(&mut self) -> Result<(), AssignmentError> {
        if matches!(
            self.status,
            AssignmentStatus::Offered | AssignmentStatus::Claimed | AssignmentStatus::Overdue
        ) {
            self.status = AssignmentStatus::Revoked;
            Ok(())
        } else {
            Err(self.invalid("revoke"))
        }
    }

    pub fn supersede(&mut self) -> Result<(), AssignmentError> {
        if matches!(
            self.status,
            AssignmentStatus::Offered | AssignmentStatus::Claimed | AssignmentStatus::Overdue
        ) {
            self.status = AssignmentStatus::Superseded;
            Ok(())
        } else {
            Err(self.invalid("supersede"))
        }
    }

    pub fn records_seen(&self) -> bool {
        self.status.records_seen()
    }

    fn invalid(&self, action: &'static str) -> AssignmentError {
        StateError::InvalidAssignmentTransition {
            from: self.status,
            action,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StateError {
    #[error("invalid event transition from {from:?} to {to:?}")]
    InvalidEventTransition { from: EventState, to: EventState },
    #[error("invalid membership transition from {from:?} to {to:?}")]
    InvalidMembershipTransition { from: Membership, to: Membership },
    #[error("invalid track transition from {from:?} to {to:?}")]
    InvalidTrackTransition { from: TrackState, to: TrackState },
    #[error("invalid assignment action '{action}' from {from:?}")]
    InvalidAssignmentTransition {
        from: AssignmentStatus,
        action: &'static str,
    },
}

pub type AssignmentError = StateError;

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn active(role: Role) -> Participant {
        Participant {
            membership: Membership::Active,
            role,
            readiness: Readiness::Ready,
        }
    }

    #[test]
    fn event_follows_the_happy_path() {
        let mut state = EventState::Draft;
        for next in [
            EventState::Signup,
            EventState::ScheduleReview,
            EventState::Active,
            EventState::Paused,
            EventState::Active,
            EventState::Reveal,
            EventState::Archived,
        ] {
            state = state.transition_to(next).unwrap();
        }
        assert!(state.is_terminal());
    }

    #[test]
    fn event_rejects_a_shortcut() {
        assert_eq!(
            EventState::Draft.transition_to(EventState::Active),
            Err(StateError::InvalidEventTransition {
                from: EventState::Draft,
                to: EventState::Active
            })
        );
    }

    #[test]
    fn membership_can_go_from_signed_up_to_active() {
        let mut membership = Membership::SignedUp;
        membership = membership.transition_to(Membership::Seeded).unwrap();
        membership = membership.transition_to(Membership::Active).unwrap();
        assert_eq!(membership, Membership::Active);
    }

    /// D-051: only a host restore with the person's consent returns a leaver.
    #[test]
    fn a_leaver_can_return_to_active_but_not_to_seeded() {
        assert_eq!(
            Membership::Left.transition_to(Membership::Active),
            Ok(Membership::Active)
        );
        assert!(Membership::Left.transition_to(Membership::Seeded).is_err());
        assert!(
            Membership::Removed
                .transition_to(Membership::Active)
                .is_err()
        );
    }

    #[test]
    fn readiness_controls_offers() {
        let mut person = active(Role::Participant);
        assert!(person.can_receive_offer());
        person.pause();
        assert!(!person.can_receive_offer());
        person.resume();
        assert!(person.can_receive_offer());
    }

    #[test]
    fn left_or_removed_people_cannot_receive_offers() {
        let mut person = active(Role::Participant);
        person.membership = Membership::Left;
        assert!(!person.can_receive_offer());
        person.membership = Membership::Removed;
        assert!(!person.can_receive_offer());
    }

    #[test]
    fn track_follows_custody() {
        let mut track = Track::new();
        for next in [
            TrackState::Pool,
            TrackState::Offered,
            TrackState::InCustody,
            TrackState::Submitted,
            TrackState::Final,
            TrackState::Retired,
        ] {
            track.transition_to(next).unwrap();
        }
        assert!(track.state.is_terminal());
    }

    #[test]
    fn assignment_claim_then_submit() {
        let mut assignment = Assignment::offered();
        assignment.claim().unwrap();
        assert!(assignment.records_seen());
        assignment.submit().unwrap();
        assert!(assignment.status.is_terminal());
    }

    #[test]
    fn assignment_skip_before_claim_does_not_record_seen() {
        let mut assignment = Assignment::offered();
        assignment.skip().unwrap();
        assert_eq!(assignment.status, AssignmentStatus::SkippedBeforeClaim);
        assert!(!assignment.records_seen());
    }

    #[test]
    fn assignment_skip_after_claim_records_seen() {
        let mut assignment = Assignment::offered();
        assignment.claim().unwrap();
        assignment.skip().unwrap();
        assert_eq!(assignment.status, AssignmentStatus::SkippedAfterClaim);
        assert!(assignment.records_seen());
    }

    #[test]
    fn assignment_overdue_can_submit_or_skip() {
        let mut submit = Assignment::offered();
        submit.claim().unwrap();
        submit.mark_overdue().unwrap();
        submit.submit().unwrap();
        assert_eq!(submit.status, AssignmentStatus::Submitted);

        let mut skip = Assignment::offered();
        skip.claim().unwrap();
        skip.mark_overdue().unwrap();
        skip.skip().unwrap();
        assert_eq!(skip.status, AssignmentStatus::SkippedAfterClaim);
    }

    #[test]
    fn a_terminal_assignment_cannot_change() {
        let mut assignment = Assignment::offered();
        assignment.expire().unwrap();
        assert_eq!(assignment.claim(), Err(assignment.invalid("claim")));
        assert_eq!(assignment.status, AssignmentStatus::Expired);
    }

    #[test]
    fn overdue_restore_returns_to_claimed() {
        let mut assignment = Assignment::offered();
        assignment.claim().unwrap();
        assignment.mark_overdue().unwrap();
        assert_eq!(assignment.restore_claimed(), Ok(()));
        assert_eq!(assignment.status, AssignmentStatus::Claimed);
        assert_eq!(assignment.submit(), Ok(()));
        assert_eq!(assignment.status, AssignmentStatus::Submitted);

        let mut offered = Assignment::offered();
        assert_eq!(
            offered.restore_claimed(),
            Err(offered.invalid("restore_claimed"))
        );
        assert_eq!(offered.status, AssignmentStatus::Offered);
    }

    #[test]
    fn every_terminal_state_and_invalid_transition_is_explicit() {
        let event_states = [
            EventState::Draft,
            EventState::Signup,
            EventState::ScheduleReview,
            EventState::Active,
            EventState::Paused,
            EventState::Reveal,
            EventState::Archived,
        ];
        for state in event_states {
            assert_eq!(state.is_terminal(), state == EventState::Archived);
        }
        assert!(EventState::Draft.can_transition_to(EventState::Signup));
        assert!(!EventState::Draft.can_transition_to(EventState::Archived));

        let memberships = [
            Membership::SignedUp,
            Membership::Seeded,
            Membership::Active,
            Membership::Left,
            Membership::Removed,
        ];
        for state in memberships {
            assert_eq!(
                state.is_terminal(),
                matches!(state, Membership::Left | Membership::Removed)
            );
        }
        assert!(Membership::SignedUp.can_transition_to(Membership::Seeded));
        assert!(!Membership::SignedUp.can_transition_to(Membership::Active));
        assert!(!Membership::Left.can_transition_to(Membership::Active));
        assert!(!Membership::Removed.can_transition_to(Membership::SignedUp));

        let track_states = [
            TrackState::Seeded,
            TrackState::Pool,
            TrackState::Offered,
            TrackState::InCustody,
            TrackState::Submitted,
            TrackState::Final,
            TrackState::Retired,
        ];
        for state in track_states {
            assert_eq!(state.is_terminal(), state == TrackState::Retired);
        }
        assert!(TrackState::Seeded.can_transition_to(TrackState::Pool));
        assert!(!TrackState::Seeded.can_transition_to(TrackState::Final));
        assert!(!TrackState::Retired.can_transition_to(TrackState::Pool));
    }

    fn any_action() -> impl Strategy<Value = u8> {
        0u8..7
    }

    proptest! {
        #[test]
        fn terminal_assignments_never_change(actions in prop::collection::vec(any_action(), 0..80)) {
            let mut assignment = Assignment::offered();
            for action in actions {
                let before = assignment.status;
                match action {
                    0 => { let _ = assignment.claim(); }
                    1 => { let _ = assignment.submit(); }
                    2 => { let _ = assignment.skip(); }
                    3 => { let _ = assignment.expire(); }
                    4 => { let _ = assignment.mark_overdue(); }
                    5 => { let _ = assignment.revoke(); }
                    _ => { let _ = assignment.supersede(); }
                }
                if before.is_terminal() {
                    prop_assert_eq!(assignment.status, before);
                }
            }
        }
    }
}
