//! Model-based test for the assignment lifecycle and the seen ledger.
//!
//! The reference model states the rules in plain Rust. The concrete system
//! uses the production `Assignment` and `SeenLedger` types.

use std::collections::HashSet;

use proptest::prelude::*;
use proptest_state_machine::{ReferenceStateMachine, StateMachineTest, prop_state_machine};
use swap_core::{Assignment, AssignmentStatus, ParticipantId, SeenLedger, Touch, TrackCode};

#[derive(Debug, Clone, Copy)]
enum Transition {
    Claim,
    Submit,
    Skip,
    Expire,
    MarkOverdue,
    Revoke,
    Supersede,
    Noop,
}

#[derive(Debug, Clone)]
struct Reference {
    status: AssignmentStatus,
    seen: HashSet<(TrackCode, ParticipantId)>,
    track: TrackCode,
    participant: ParticipantId,
}

impl ReferenceStateMachine for Reference {
    type State = Reference;
    type Transition = Transition;

    fn init_state() -> BoxedStrategy<Self::State> {
        Just(Reference {
            status: AssignmentStatus::Offered,
            seen: HashSet::new(),
            track: TrackCode::new("T1"),
            participant: ParticipantId::new("p1"),
        })
        .boxed()
    }

    fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        if state.status.is_terminal() {
            return Just(Transition::Noop).boxed();
        }
        let mut options = vec![Transition::Noop];
        match state.status {
            AssignmentStatus::Offered => {
                options.extend([Transition::Claim, Transition::Skip, Transition::Expire]);
                options.extend([Transition::Revoke, Transition::Supersede]);
            }
            AssignmentStatus::Claimed => {
                options.extend([
                    Transition::Submit,
                    Transition::Skip,
                    Transition::MarkOverdue,
                ]);
                options.extend([Transition::Revoke, Transition::Supersede]);
            }
            AssignmentStatus::Overdue => {
                options.extend([Transition::Submit, Transition::Skip]);
                options.extend([Transition::Revoke, Transition::Supersede]);
            }
            _ => {}
        }
        proptest::sample::select(options).boxed()
    }

    fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
        match transition {
            Transition::Claim => {
                state.status = AssignmentStatus::Claimed;
                state
                    .seen
                    .insert((state.track.clone(), state.participant.clone()));
            }
            Transition::Submit => {
                state.status = AssignmentStatus::Submitted;
                state
                    .seen
                    .insert((state.track.clone(), state.participant.clone()));
            }
            Transition::Skip => {
                let after_claim = matches!(
                    state.status,
                    AssignmentStatus::Claimed | AssignmentStatus::Overdue
                );
                state.status = if after_claim {
                    AssignmentStatus::SkippedAfterClaim
                } else {
                    AssignmentStatus::SkippedBeforeClaim
                };
                if after_claim {
                    state
                        .seen
                        .insert((state.track.clone(), state.participant.clone()));
                }
            }
            Transition::Expire => state.status = AssignmentStatus::Expired,
            Transition::MarkOverdue => state.status = AssignmentStatus::Overdue,
            Transition::Revoke => state.status = AssignmentStatus::Revoked,
            Transition::Supersede => state.status = AssignmentStatus::Superseded,
            Transition::Noop => {}
        }
        state
    }
}

struct Concrete {
    assignment: Assignment,
    ledger: SeenLedger,
    track: TrackCode,
    participant: ParticipantId,
}

impl StateMachineTest for Concrete {
    type SystemUnderTest = Concrete;
    type Reference = Reference;

    fn init_test(_ref_state: &Reference) -> Self::SystemUnderTest {
        Concrete {
            assignment: Assignment::offered(),
            ledger: SeenLedger::new(),
            track: TrackCode::new("T1"),
            participant: ParticipantId::new("p1"),
        }
    }

    fn apply(
        mut state: Self::SystemUnderTest,
        _ref_state: &Reference,
        transition: Transition,
    ) -> Self::SystemUnderTest {
        match transition {
            Transition::Claim => {
                state.assignment.claim().unwrap();
                state
                    .ledger
                    .apply(&state.track, &state.participant, Touch::Claim);
            }
            Transition::Submit => {
                state.assignment.submit().unwrap();
                state
                    .ledger
                    .apply(&state.track, &state.participant, Touch::Submit);
            }
            Transition::Skip => {
                let after_claim = matches!(
                    state.assignment.status,
                    AssignmentStatus::Claimed | AssignmentStatus::Overdue
                );
                state.assignment.skip().unwrap();
                state.ledger.apply(
                    &state.track,
                    &state.participant,
                    if after_claim {
                        Touch::SkipAfterClaim
                    } else {
                        Touch::SkipBeforeClaim
                    },
                );
            }
            Transition::Expire => {
                state.assignment.expire().unwrap();
                state
                    .ledger
                    .apply(&state.track, &state.participant, Touch::OfferExpired);
            }
            Transition::MarkOverdue => state.assignment.mark_overdue().unwrap(),
            Transition::Revoke => state.assignment.revoke().unwrap(),
            Transition::Supersede => state.assignment.supersede().unwrap(),
            Transition::Noop => {}
        }
        state
    }

    fn check_invariants(state: &Self::SystemUnderTest, ref_state: &Reference) {
        assert_eq!(state.participant, ref_state.participant);
        assert_eq!(state.assignment.status, ref_state.status);
        assert_eq!(state.ledger.len(), ref_state.seen.len());
        assert_eq!(
            state.ledger.has_seen(&state.track, &state.participant),
            ref_state
                .seen
                .contains(&(state.track.clone(), state.participant.clone()))
        );
        if state.assignment.status.is_terminal() {
            assert!(
                state.assignment.records_seen()
                    == state.ledger.has_seen(&state.track, &state.participant)
                    || matches!(
                        state.assignment.status,
                        AssignmentStatus::Expired
                            | AssignmentStatus::Revoked
                            | AssignmentStatus::Superseded
                    )
            );
        }
    }
}

prop_state_machine! {
    #[test]
    fn assignment_and_ledger_follow_the_rules(sequential 1..40 => Concrete);
}

#[test]
fn every_terminal_assignment_refuses_further_transitions() {
    let terminal = [
        AssignmentStatus::Submitted,
        AssignmentStatus::SkippedBeforeClaim,
        AssignmentStatus::SkippedAfterClaim,
        AssignmentStatus::Expired,
        AssignmentStatus::Revoked,
        AssignmentStatus::Superseded,
    ];
    for status in terminal {
        let mut assignment = Assignment { status };
        assert!(assignment.claim().is_err(), "{status:?} accepted claim");
        assert!(assignment.submit().is_err(), "{status:?} accepted submit");
        assert!(assignment.skip().is_err(), "{status:?} accepted skip");
        assert!(assignment.expire().is_err(), "{status:?} accepted expire");
        assert!(
            assignment.mark_overdue().is_err(),
            "{status:?} accepted overdue"
        );
        assert!(assignment.revoke().is_err(), "{status:?} accepted revoke");
        assert!(
            assignment.supersede().is_err(),
            "{status:?} accepted supersede"
        );
    }
}
