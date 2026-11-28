//! Leave the event with confirmation, and the host restore (D-051, MSW-109).
//!
//! A leave is immediate. A held track is skipped and returns unchanged, the
//! fill-in flow starts for the stranded slot, and the host gets an alert.
//! Future planned turns become gaps; the plan does not change. The seed stays
//! in the event, and past credits keep the name. Every unsent DM to the
//! leaver is cancelled, and the outbox refuses new DMs to the leaver. Only
//! the host can start a restore, and only the person's confirm completes it.

use swap_core::{AssignmentStatus, EventState, Membership};
use swap_db::{
    AssignmentRepo, AuditRepo, Db, DeliveryRepo, EventRepo, ParticipantRepo, PlanRepo, RestoreRepo,
};

use crate::delivery::{NOTICE_PREFIX, PendingDelivery, enqueue_effect, enqueue_pending};
use crate::effects::Effect;
use crate::engine::{EngineError, begin, commit, skip_in};

/// Discord expires an interaction token after 15 minutes. A leave
/// confirmation that is older than this does nothing.
pub const LEAVE_CONFIRM_SECS: i64 = 15 * 60;

/// A restore invitation expires after 7 days (D-068). An old invitation must
/// not return a person without a fresh host decision.
pub const RESTORE_REQUEST_SECS: i64 = 7 * 24 * 60 * 60;

/// The warning that every leave path shows before the confirm. It names no
/// other participant and no track owner.
pub const LEAVE_WARNING: &str = "Leave this swap? Read this first:\n\
- If you hold a track, the track is skipped now and returns unchanged.\n\
- A seed that you own stays in the event and keeps circulating.\n\
- Your past credits keep your name.\n\
- The bot sends you no DMs unless the host asks you to return.\n\
- Only the host can undo this, and you must confirm.\n\
Press Confirm in the next 15 minutes to leave. If you do nothing, nothing changes.";

/// Who started the leave. The audit log records the actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaveActor {
    Participant,
    Host,
}

impl LeaveActor {
    fn audit_actor(self) -> &'static str {
        match self {
            Self::Participant => "Participant",
            Self::Host => "Host",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedTurn {
    pub assignment_id: String,
    pub track_code: String,
    pub status: AssignmentStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaveOutcome {
    /// The held or offered turn that the leave skipped, if any.
    pub skipped: Option<SkippedTurn>,
    /// The unsent DMs that the leave cancelled.
    pub cancelled_deliveries: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreAnswer {
    Restored,
    Declined,
}

/// The token for a leave confirm button: the issue time in UTC seconds.
pub fn leave_confirm_token(now: i64) -> String {
    now.to_string()
}

/// True when the confirm button is older than 15 minutes, from the future,
/// or not readable.
pub fn leave_confirm_expired(token: Option<&str>, now: i64) -> bool {
    match token.and_then(|token| token.parse::<i64>().ok()) {
        Some(issued_at) => issued_at > now || now - issued_at > LEAVE_CONFIRM_SECS,
        None => true,
    }
}

/// Make a person leave the event now. The participant path calls this only
/// after the ephemeral confirm; the host path after a two-step dialog.
pub fn leave_event(
    db: &Db,
    event_id: &str,
    user_id: &str,
    actor: LeaveActor,
    now: i64,
) -> Result<LeaveOutcome, EngineError> {
    let transaction = begin(db)?;
    let event = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if matches!(event.state, EventState::Reveal | EventState::Archived) {
        return Err(EngineError::WrongEventState {
            action: "leave",
            state: event.state,
            expected: "draft, signup, schedule review, active, or paused",
        });
    }
    let mut participant = ParticipantRepo::get(&transaction, event_id, user_id)?
        .ok_or_else(|| EngineError::ParticipantNotFound(user_id.to_string()))?;
    if matches!(
        participant.membership,
        Membership::Left | Membership::Removed
    ) {
        return Err(EngineError::ParticipantLeft);
    }
    participant.membership = participant.membership.transition_to(Membership::Left)?;
    participant.updated_at = now;
    ParticipantRepo::upsert(&transaction, &participant)?;

    let mut skipped = None;
    if matches!(event.state, EventState::Active | EventState::Paused)
        && let Some(assignment) =
            AssignmentRepo::latest_for_participant(&transaction, event_id, user_id)?
        && matches!(
            assignment.status,
            AssignmentStatus::Offered | AssignmentStatus::Claimed | AssignmentStatus::Overdue
        )
    {
        let status = skip_in(&transaction, &assignment, "the participant left", now)?;
        skipped = Some(SkippedTurn {
            assignment_id: assignment.id.clone(),
            track_code: assignment.track_code.clone(),
            status,
        });
    }

    let reason = "the recipient left the event";
    let cancelled =
        DeliveryRepo::cancel_pending_for_recipient(&transaction, event_id, user_id, now, reason)?;
    let audit_actor = actor.audit_actor();
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        audit_actor,
        "Participant left",
        Some(&participant.display_name),
        &serde_json::json!({
            "participantId": user_id,
            "skippedAssignment": skipped.as_ref().map(|turn| turn.assignment_id.clone()),
            "skippedTrack": skipped.as_ref().map(|turn| turn.track_code.clone()),
            "seenAfterSkip": skipped
                .as_ref()
                .is_some_and(|turn| turn.status == AssignmentStatus::SkippedAfterClaim),
        })
        .to_string(),
    )?;
    if !cancelled.is_empty() {
        AuditRepo::insert(
            &transaction,
            event_id,
            now,
            audit_actor,
            "Deliveries cancelled",
            Some(&participant.display_name),
            &serde_json::json!({
                "count": cancelled.len(),
                "deliveryIds": cancelled,
                "reason": reason,
            })
            .to_string(),
        )?;
    }
    enqueue_effect(
        &transaction,
        event_id,
        &Effect::HostAlert {
            key: format!("{NOTICE_PREFIX}leave:{user_id}:{now}"),
            content: "A participant left the event. A held track returned to the pool unchanged. Open the host app for the details.".to_string(),
        },
        now,
    )?;
    commit(transaction)?;
    Ok(LeaveOutcome {
        skipped,
        cancelled_deliveries: cancelled.len(),
    })
}

/// The host asks a person who left to come back. The bot sends one DM with
/// Confirm and Decline buttons. The person stays out until a confirm.
pub fn request_restore(
    db: &Db,
    event_id: &str,
    user_id: &str,
    now: i64,
) -> Result<(), EngineError> {
    let transaction = begin(db)?;
    let event = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    if matches!(event.state, EventState::Reveal | EventState::Archived) {
        return Err(EngineError::WrongEventState {
            action: "restore",
            state: event.state,
            expected: "draft, signup, schedule review, active, or paused",
        });
    }
    let participant = ParticipantRepo::get(&transaction, event_id, user_id)?
        .ok_or_else(|| EngineError::ParticipantNotFound(user_id.to_string()))?;
    if participant.membership != Membership::Left {
        return Err(EngineError::ParticipantNotLeft);
    }
    if let Some(pending) = RestoreRepo::pending(&transaction, event_id, user_id)? {
        if now <= pending.requested_at + RESTORE_REQUEST_SECS {
            return Err(EngineError::RestorePending);
        }
        RestoreRepo::answer(&transaction, event_id, user_id, "cancelled", now)?;
        AuditRepo::insert(
            &transaction,
            event_id,
            now,
            "Host",
            "Restore expired",
            Some(&participant.display_name),
            &serde_json::json!({
                "participantId": user_id,
                "requestedAt": pending.requested_at,
            })
            .to_string(),
        )?;
    }
    RestoreRepo::insert(&transaction, event_id, user_id, now)?;
    enqueue_pending(
        &transaction,
        event_id,
        &PendingDelivery::Restore {
            user_id: user_id.to_string(),
            event_id: event_id.to_string(),
            issued_at: now,
        },
        now,
    )?;
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Host",
        "Restore requested",
        Some(&participant.display_name),
        &serde_json::json!({ "participantId": user_id }).to_string(),
    )?;
    commit(transaction)?;
    Ok(())
}

/// The person answers the restore DM. A confirm makes the person active and
/// keeps their previous readiness and role; the next round that opens gives
/// them their planned turns. A confirm in schedule review voids the draft
/// plan, because the roster changed (MSW-109). A decline keeps them out, and
/// the bot sends nothing more.
pub fn answer_restore(
    db: &Db,
    event_id: &str,
    user_id: &str,
    accept: bool,
    now: i64,
) -> Result<RestoreAnswer, EngineError> {
    let transaction = begin(db)?;
    let event = EventRepo::get(&transaction, event_id)?
        .ok_or_else(|| EngineError::EventNotFound(event_id.to_string()))?;
    let mut participant = ParticipantRepo::get(&transaction, event_id, user_id)?
        .ok_or_else(|| EngineError::ParticipantNotFound(user_id.to_string()))?;
    let Some(pending) = RestoreRepo::pending(&transaction, event_id, user_id)? else {
        return Err(EngineError::NoPendingRestore);
    };
    if now > pending.requested_at + RESTORE_REQUEST_SECS {
        RestoreRepo::answer(&transaction, event_id, user_id, "cancelled", now)?;
        AuditRepo::insert(
            &transaction,
            event_id,
            now,
            "Participant",
            "Restore expired",
            Some(&participant.display_name),
            &serde_json::json!({
                "participantId": user_id,
                "requestedAt": pending.requested_at,
            })
            .to_string(),
        )?;
        commit(transaction)?;
        return Err(EngineError::NoPendingRestore);
    }
    if matches!(event.state, EventState::Reveal | EventState::Archived) {
        RestoreRepo::answer(&transaction, event_id, user_id, "cancelled", now)?;
        commit(transaction)?;
        return Err(EngineError::WrongEventState {
            action: "restore",
            state: event.state,
            expected: "draft, signup, schedule review, active, or paused",
        });
    }
    let answer = if accept {
        participant.membership = participant.membership.transition_to(Membership::Active)?;
        participant.updated_at = now;
        ParticipantRepo::upsert(&transaction, &participant)?;
        RestoreRepo::answer(&transaction, event_id, user_id, "accepted", now)?;
        if event.state == EventState::ScheduleReview {
            let voided = match PlanRepo::latest(&transaction, event_id)? {
                Some(plan) => PlanRepo::void(&transaction, event_id, plan.version, now)?,
                None => false,
            };
            if voided {
                AuditRepo::insert(
                    &transaction,
                    event_id,
                    now,
                    "Host",
                    "Draft schedule invalidated",
                    Some(&participant.display_name),
                    &serde_json::json!({
                        "participantId": user_id,
                        "reason": "a participant returned to the roster",
                    })
                    .to_string(),
                )?;
                enqueue_effect(
                    &transaction,
                    event_id,
                    &Effect::HostAlert {
                        key: format!("{NOTICE_PREFIX}restore-plan:{event_id}:{user_id}:{now}"),
                        content: "A participant returned and the draft schedule no longer covers the roster. Generate the schedule again before you start the event.".to_string(),
                    },
                    now,
                )?;
            }
        }
        RestoreAnswer::Restored
    } else {
        RestoreRepo::answer(&transaction, event_id, user_id, "declined", now)?;
        RestoreAnswer::Declined
    };
    AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "Participant",
        match answer {
            RestoreAnswer::Restored => "Participant restored",
            RestoreAnswer::Declined => "Restore declined",
        },
        Some(&participant.display_name),
        &serde_json::json!({ "participantId": user_id }).to_string(),
    )?;
    commit(transaction)?;
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_confirm_expires_after_15_minutes() {
        assert!(!leave_confirm_expired(Some("1000"), 1000));
        assert!(!leave_confirm_expired(
            Some("1000"),
            1000 + LEAVE_CONFIRM_SECS
        ));
        assert!(leave_confirm_expired(
            Some("1000"),
            1001 + LEAVE_CONFIRM_SECS
        ));
        assert!(leave_confirm_expired(Some("2000"), 1000));
        assert!(leave_confirm_expired(Some("soon"), 1000));
        assert!(leave_confirm_expired(None, 1000));
    }

    // scenario: The warning states every consequence
    #[test]
    fn the_warning_states_every_consequence() {
        for phrase in [
            "skipped now and returns unchanged",
            "A seed that you own stays",
            "past credits keep your name",
            "no DMs unless the host asks you to return",
            "Only the host can undo this",
        ] {
            assert!(LEAVE_WARNING.contains(phrase), "missing: {phrase}");
        }
    }
}
