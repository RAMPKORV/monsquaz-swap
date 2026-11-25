use swap_core::{AssignmentStatus, Membership, Readiness};
use swap_db::{
    AssignmentRepo, AuditRepo, Db, DbError, MetaRepo, ParticipantRecord, ParticipantRepo,
    PrivacyRepo, TrackRepo, Transaction,
};
use swap_files::sha256_hex;

use crate::PrivacyError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletionReport {
    pub tombstone_id: String,
    pub display_name_removed: bool,
    pub timezone_removed: bool,
    pub seeded_tracks: usize,
    pub assignments_kept: usize,
    pub seen_pairs: usize,
    pub files_kept: usize,
    pub plan_assignments_removed: usize,
    pub audit_hash: String,
}

pub fn delete_participant(
    db: &Db,
    event_id: &str,
    user_id: &str,
    now: i64,
) -> Result<DeletionReport, PrivacyError> {
    let participant = ParticipantRepo::get(db.conn(), event_id, user_id)?
        .ok_or_else(|| PrivacyError::ParticipantMissing(user_id.to_string()))?;

    let tracks: Vec<_> = TrackRepo::list_for_event(db.conn(), event_id)?
        .into_iter()
        .filter(|track| track.owner_id == user_id)
        .collect();
    if tracks.len() > 1 {
        return Err(PrivacyError::MultipleSeedTracks);
    }

    let assignments: Vec<_> = AssignmentRepo::list_for_event(db.conn(), event_id)?
        .into_iter()
        .filter(|assignment| assignment.participant_id == user_id)
        .collect();
    if assignments.iter().any(|assignment| {
        matches!(
            assignment.status,
            AssignmentStatus::Offered | AssignmentStatus::Claimed | AssignmentStatus::Overdue
        )
    }) {
        return Err(PrivacyError::ActiveAssignment);
    }

    let last_opened_round = assignments
        .iter()
        .map(|assignment| assignment.round)
        .max()
        .map(i64::from)
        .unwrap_or(-1);

    let salt = deletion_salt(db, event_id)?;
    let tombstone_id = tombstone_id(db, event_id, user_id, &salt)?;

    let transaction = begin(db)?;
    ParticipantRepo::upsert(
        &transaction,
        &ParticipantRecord {
            event_id: event_id.to_string(),
            user_id: tombstone_id.clone(),
            display_name: "Removed participant".to_string(),
            role: participant.role,
            membership: Membership::Left,
            readiness: Readiness::Paused,
            timezone: "UTC".to_string(),
            max_handoffs: 0,
            created_at: now,
            updated_at: now,
        },
    )?;
    let plan_assignments_removed =
        PrivacyRepo::delete_plan_after(&transaction, event_id, user_id, last_opened_round)?;
    let counts = PrivacyRepo::replace_identity(&transaction, event_id, user_id, &tombstone_id)?;
    ParticipantRepo::delete(&transaction, event_id, user_id)?;

    let detail = serde_json::json!({
        "displayNameRemoved": !participant.display_name.is_empty(),
        "timezoneRemoved": !participant.timezone.is_empty(),
        "seededTracks": counts.tracks,
        "assignmentsKept": counts.assignments,
        "seenPairs": counts.seen_pairs,
        "filesKept": counts.files,
        "planAssignmentsRemoved": plan_assignments_removed,
        "reason": "participant request"
    })
    .to_string();
    let audit_hash = AuditRepo::insert(
        &transaction,
        event_id,
        now,
        "host",
        "participant.deleted",
        Some(&tombstone_id),
        &detail,
    )?;
    commit(transaction)?;

    Ok(DeletionReport {
        tombstone_id,
        display_name_removed: !participant.display_name.is_empty(),
        timezone_removed: !participant.timezone.is_empty(),
        seeded_tracks: counts.tracks,
        assignments_kept: counts.assignments,
        seen_pairs: counts.seen_pairs,
        files_kept: counts.files,
        plan_assignments_removed,
        audit_hash,
    })
}

fn deletion_salt(db: &Db, event_id: &str) -> Result<String, PrivacyError> {
    let key = format!("privacy.deletion_salt.{event_id}");
    if let Some(value) = MetaRepo::get(db.conn(), &key)? {
        return Ok(value);
    }
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| PrivacyError::Random(error.to_string()))?;
    let value = hex::encode(bytes);
    MetaRepo::set(db.conn(), &key, &value)?;
    Ok(value)
}

fn tombstone_id(
    db: &Db,
    event_id: &str,
    user_id: &str,
    salt: &str,
) -> Result<String, PrivacyError> {
    for attempt in 0..8u8 {
        let digest = sha256_hex(format!("{salt}:{event_id}:{user_id}:{attempt}").as_bytes());
        let candidate = format!("deleted-{}", &digest[..16]);
        if ParticipantRepo::get(db.conn(), event_id, &candidate)?.is_none() {
            return Ok(candidate);
        }
    }
    Err(PrivacyError::Random(
        "could not allocate a deletion tombstone".to_string(),
    ))
}

fn begin(db: &Db) -> Result<Transaction<'_>, PrivacyError> {
    Ok(db.conn().unchecked_transaction().map_err(DbError::from)?)
}

fn commit(transaction: Transaction<'_>) -> Result<(), PrivacyError> {
    transaction.commit().map_err(DbError::from)?;
    Ok(())
}
