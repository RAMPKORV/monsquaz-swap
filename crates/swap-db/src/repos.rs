use rusqlite::{Connection, OptionalExtension, Row, params};
use sha2::{Digest, Sha256};

use swap_core::{AssignmentStatus, EventState, Membership, Readiness, Role, TrackState};

use crate::codec::{from_db, to_db};
use crate::db::DbError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    pub id: String,
    pub name: String,
    pub state: EventState,
    pub seed: u64,
    pub settings_json: String,
    pub created_at: i64,
    pub updated_at: i64,
}

struct RawEvent {
    id: String,
    name: String,
    state: String,
    seed: i64,
    settings_json: String,
    created_at: i64,
    updated_at: i64,
}

impl TryFrom<RawEvent> for EventRecord {
    type Error = DbError;

    fn try_from(raw: RawEvent) -> Result<Self, DbError> {
        Ok(Self {
            id: raw.id,
            name: raw.name,
            state: from_db(&raw.state)?,
            seed: raw.seed as u64,
            settings_json: raw.settings_json,
            created_at: raw.created_at,
            updated_at: raw.updated_at,
        })
    }
}

fn row_to_raw_event(row: &Row<'_>) -> rusqlite::Result<RawEvent> {
    Ok(RawEvent {
        id: row.get(0)?,
        name: row.get(1)?,
        state: row.get(2)?,
        seed: row.get(3)?,
        settings_json: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

pub struct EventRepo;

impl EventRepo {
    pub fn insert(conn: &Connection, event: &EventRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO events
                (id, name, state, seed, settings_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                &event.id,
                &event.name,
                to_db(&event.state)?,
                event.seed as i64,
                &event.settings_json,
                event.created_at,
                event.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn active_for_user(
        conn: &Connection,
        user_id: &str,
    ) -> Result<Option<EventRecord>, DbError> {
        let raw = conn
            .query_row(
                "SELECT e.id, e.name, e.state, e.seed, e.settings_json, e.created_at, e.updated_at
                 FROM events e
                 JOIN participants p ON p.event_id = e.id
                 WHERE p.user_id = ?1 AND p.membership = 'active'
                   AND e.state IN ('active', 'paused')
                 ORDER BY e.updated_at DESC LIMIT 1",
                params![user_id],
                row_to_raw_event,
            )
            .optional()?;
        raw.map(TryInto::try_into).transpose()
    }

    pub fn get(conn: &Connection, id: &str) -> Result<Option<EventRecord>, DbError> {
        let raw = conn
            .query_row(
                "SELECT id, name, state, seed, settings_json, created_at, updated_at
                 FROM events WHERE id = ?1",
                params![id],
                row_to_raw_event,
            )
            .optional()?;
        raw.map(EventRecord::try_from).transpose()
    }

    pub fn set_state(
        conn: &Connection,
        id: &str,
        state: EventState,
        updated_at: i64,
    ) -> Result<bool, DbError> {
        let changed = conn.execute(
            "UPDATE events SET state = ?1, updated_at = ?2 WHERE id = ?3",
            params![to_db(&state)?, updated_at, id],
        )?;
        Ok(changed > 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantRecord {
    pub event_id: String,
    pub user_id: String,
    pub display_name: String,
    pub role: Role,
    pub membership: Membership,
    pub readiness: Readiness,
    pub timezone: String,
    pub max_handoffs: u32,
    pub created_at: i64,
    pub updated_at: i64,
}

struct RawParticipant {
    event_id: String,
    user_id: String,
    display_name: String,
    role: String,
    membership: String,
    readiness: String,
    timezone: String,
    max_handoffs: i64,
    created_at: i64,
    updated_at: i64,
}

impl TryFrom<RawParticipant> for ParticipantRecord {
    type Error = DbError;

    fn try_from(raw: RawParticipant) -> Result<Self, DbError> {
        Ok(Self {
            event_id: raw.event_id,
            user_id: raw.user_id,
            display_name: raw.display_name,
            role: from_db(&raw.role)?,
            membership: from_db(&raw.membership)?,
            readiness: from_db(&raw.readiness)?,
            timezone: raw.timezone,
            max_handoffs: raw.max_handoffs as u32,
            created_at: raw.created_at,
            updated_at: raw.updated_at,
        })
    }
}

fn row_to_raw_participant(row: &Row<'_>) -> rusqlite::Result<RawParticipant> {
    Ok(RawParticipant {
        event_id: row.get(0)?,
        user_id: row.get(1)?,
        display_name: row.get(2)?,
        role: row.get(3)?,
        membership: row.get(4)?,
        readiness: row.get(5)?,
        timezone: row.get(6)?,
        max_handoffs: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

pub struct ParticipantRepo;

impl ParticipantRepo {
    pub fn upsert(conn: &Connection, participant: &ParticipantRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO participants
                (event_id, user_id, display_name, role, membership, readiness,
                 timezone, max_handoffs, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(event_id, user_id) DO UPDATE SET
                display_name = excluded.display_name,
                role = excluded.role,
                membership = excluded.membership,
                readiness = excluded.readiness,
                timezone = excluded.timezone,
                max_handoffs = excluded.max_handoffs,
                updated_at = excluded.updated_at",
            params![
                &participant.event_id,
                &participant.user_id,
                &participant.display_name,
                to_db(&participant.role)?,
                to_db(&participant.membership)?,
                to_db(&participant.readiness)?,
                &participant.timezone,
                participant.max_handoffs as i64,
                participant.created_at,
                participant.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get(
        conn: &Connection,
        event_id: &str,
        user_id: &str,
    ) -> Result<Option<ParticipantRecord>, DbError> {
        let raw = conn
            .query_row(
                "SELECT event_id, user_id, display_name, role, membership, readiness,
                        timezone, max_handoffs, created_at, updated_at
                 FROM participants WHERE event_id = ?1 AND user_id = ?2",
                params![event_id, user_id],
                row_to_raw_participant,
            )
            .optional()?;
        raw.map(ParticipantRecord::try_from).transpose()
    }

    pub fn list_for_event(
        conn: &Connection,
        event_id: &str,
    ) -> Result<Vec<ParticipantRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT event_id, user_id, display_name, role, membership, readiness,
                    timezone, max_handoffs, created_at, updated_at
             FROM participants WHERE event_id = ?1 ORDER BY user_id",
        )?;
        let rows = statement
            .query_map(params![event_id], row_to_raw_participant)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(ParticipantRecord::try_from)
            .collect::<Result<Vec<_>, _>>()
    }

    pub fn delete(conn: &Connection, event_id: &str, user_id: &str) -> Result<bool, DbError> {
        let changed = conn.execute(
            "DELETE FROM participants WHERE event_id = ?1 AND user_id = ?2",
            params![event_id, user_id],
        )?;
        Ok(changed > 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRecord {
    pub event_id: String,
    pub code: String,
    pub owner_id: String,
    pub state: TrackState,
    pub created_at: i64,
}

struct RawTrack {
    event_id: String,
    code: String,
    owner_id: String,
    state: String,
    created_at: i64,
}

impl TryFrom<RawTrack> for TrackRecord {
    type Error = DbError;

    fn try_from(raw: RawTrack) -> Result<Self, DbError> {
        Ok(Self {
            event_id: raw.event_id,
            code: raw.code,
            owner_id: raw.owner_id,
            state: from_db(&raw.state)?,
            created_at: raw.created_at,
        })
    }
}

fn row_to_raw_track(row: &Row<'_>) -> rusqlite::Result<RawTrack> {
    Ok(RawTrack {
        event_id: row.get(0)?,
        code: row.get(1)?,
        owner_id: row.get(2)?,
        state: row.get(3)?,
        created_at: row.get(4)?,
    })
}

pub struct TrackRepo;

impl TrackRepo {
    pub fn insert(conn: &Connection, track: &TrackRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO tracks (event_id, code, owner_id, state, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                &track.event_id,
                &track.code,
                &track.owner_id,
                to_db(&track.state)?,
                track.created_at
            ],
        )?;
        Ok(())
    }

    pub fn get(
        conn: &Connection,
        event_id: &str,
        code: &str,
    ) -> Result<Option<TrackRecord>, DbError> {
        let raw = conn
            .query_row(
                "SELECT event_id, code, owner_id, state, created_at
                 FROM tracks WHERE event_id = ?1 AND code = ?2",
                params![event_id, code],
                row_to_raw_track,
            )
            .optional()?;
        raw.map(TrackRecord::try_from).transpose()
    }

    pub fn list_for_event(conn: &Connection, event_id: &str) -> Result<Vec<TrackRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT event_id, code, owner_id, state, created_at
             FROM tracks WHERE event_id = ?1 ORDER BY code",
        )?;
        let rows = statement
            .query_map(params![event_id], row_to_raw_track)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(TrackRecord::try_from)
            .collect::<Result<Vec<_>, _>>()
    }

    pub fn set_state(
        conn: &Connection,
        event_id: &str,
        code: &str,
        state: TrackState,
    ) -> Result<bool, DbError> {
        let changed = conn.execute(
            "UPDATE tracks SET state = ?1 WHERE event_id = ?2 AND code = ?3",
            params![to_db(&state)?, event_id, code],
        )?;
        Ok(changed > 0)
    }
}

pub struct SeenRepo;

impl SeenRepo {
    /// Insert a seen pair. Returns true when the pair is new.
    pub fn record(
        conn: &Connection,
        event_id: &str,
        track_code: &str,
        participant_id: &str,
        reason: &str,
        at: i64,
    ) -> Result<bool, DbError> {
        let changed = conn.execute(
            "INSERT OR IGNORE INTO seen_pairs
                (event_id, track_code, participant_id, reason, first_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![event_id, track_code, participant_id, reason, at],
        )?;
        Ok(changed > 0)
    }

    pub fn list_for_participant(
        conn: &Connection,
        event_id: &str,
        participant_id: &str,
    ) -> Result<Vec<SeenPairRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT track_code, participant_id, reason, first_seen_at
             FROM seen_pairs WHERE event_id = ?1 AND participant_id = ?2
             ORDER BY first_seen_at, track_code",
        )?;
        let rows = statement
            .query_map(params![event_id, participant_id], |row| {
                Ok(SeenPairRecord {
                    track_code: row.get(0)?,
                    participant_id: row.get(1)?,
                    reason: row.get(2)?,
                    first_seen_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn has(
        conn: &Connection,
        event_id: &str,
        track_code: &str,
        participant_id: &str,
    ) -> Result<bool, DbError> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM seen_pairs
             WHERE event_id = ?1 AND track_code = ?2 AND participant_id = ?3",
            params![event_id, track_code, participant_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn list_for_event(
        conn: &Connection,
        event_id: &str,
    ) -> Result<Vec<SeenPairRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT track_code, participant_id, reason, first_seen_at
             FROM seen_pairs WHERE event_id = ?1 ORDER BY first_seen_at, track_code",
        )?;
        let rows = statement
            .query_map(params![event_id], |row| {
                Ok(SeenPairRecord {
                    track_code: row.get(0)?,
                    participant_id: row.get(1)?,
                    reason: row.get(2)?,
                    first_seen_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeenPairRecord {
    pub track_code: String,
    pub participant_id: String,
    pub reason: String,
    pub first_seen_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentRecord {
    pub id: String,
    pub event_id: String,
    pub track_code: String,
    pub participant_id: String,
    pub round: u32,
    pub status: AssignmentStatus,
    pub offered_at: i64,
    pub claimed_at: Option<i64>,
    pub due_at: Option<i64>,
    pub submitted_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

struct RawAssignment {
    id: String,
    event_id: String,
    track_code: String,
    participant_id: String,
    round: i64,
    status: String,
    offered_at: i64,
    claimed_at: Option<i64>,
    due_at: Option<i64>,
    submitted_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

impl TryFrom<RawAssignment> for AssignmentRecord {
    type Error = DbError;

    fn try_from(raw: RawAssignment) -> Result<Self, DbError> {
        Ok(Self {
            id: raw.id,
            event_id: raw.event_id,
            track_code: raw.track_code,
            participant_id: raw.participant_id,
            round: raw.round as u32,
            status: from_db(&raw.status)?,
            offered_at: raw.offered_at,
            claimed_at: raw.claimed_at,
            due_at: raw.due_at,
            submitted_at: raw.submitted_at,
            created_at: raw.created_at,
            updated_at: raw.updated_at,
        })
    }
}

fn row_to_raw_assignment(row: &Row<'_>) -> rusqlite::Result<RawAssignment> {
    Ok(RawAssignment {
        id: row.get(0)?,
        event_id: row.get(1)?,
        track_code: row.get(2)?,
        participant_id: row.get(3)?,
        round: row.get(4)?,
        status: row.get(5)?,
        offered_at: row.get(6)?,
        claimed_at: row.get(7)?,
        due_at: row.get(8)?,
        submitted_at: row.get(9)?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
    })
}

pub struct AssignmentRepo;

impl AssignmentRepo {
    pub fn insert(conn: &Connection, assignment: &AssignmentRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO assignments
                (id, event_id, track_code, participant_id, round, status,
                 offered_at, claimed_at, due_at, submitted_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                &assignment.id,
                &assignment.event_id,
                &assignment.track_code,
                &assignment.participant_id,
                assignment.round as i64,
                to_db(&assignment.status)?,
                assignment.offered_at,
                assignment.claimed_at,
                assignment.due_at,
                assignment.submitted_at,
                assignment.created_at,
                assignment.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn get(conn: &Connection, id: &str) -> Result<Option<AssignmentRecord>, DbError> {
        let raw = conn
            .query_row(
                "SELECT id, event_id, track_code, participant_id, round, status,
                        offered_at, claimed_at, due_at, submitted_at, created_at, updated_at
                 FROM assignments WHERE id = ?1",
                params![id],
                row_to_raw_assignment,
            )
            .optional()?;
        raw.map(AssignmentRecord::try_from).transpose()
    }

    pub fn list_for_event(
        conn: &Connection,
        event_id: &str,
    ) -> Result<Vec<AssignmentRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT id, event_id, track_code, participant_id, round, status,
                    offered_at, claimed_at, due_at, submitted_at, created_at, updated_at
             FROM assignments WHERE event_id = ?1 ORDER BY round, track_code",
        )?;
        let rows = statement
            .query_map(params![event_id], row_to_raw_assignment)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(AssignmentRecord::try_from)
            .collect::<Result<Vec<_>, _>>()
    }

    pub fn update_status(
        conn: &Connection,
        id: &str,
        status: AssignmentStatus,
        updated_at: i64,
    ) -> Result<bool, DbError> {
        let changed = conn.execute(
            "UPDATE assignments SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![to_db(&status)?, updated_at, id],
        )?;
        Ok(changed > 0)
    }

    pub fn claim(
        conn: &Connection,
        id: &str,
        claimed_at: i64,
        due_at: i64,
        updated_at: i64,
    ) -> Result<bool, DbError> {
        let changed = conn.execute(
            "UPDATE assignments
             SET status = 'claimed', claimed_at = ?1, due_at = ?2, updated_at = ?3
             WHERE id = ?4 AND status = 'offered'",
            params![claimed_at, due_at, updated_at, id],
        )?;
        Ok(changed > 0)
    }

    pub fn submit(
        conn: &Connection,
        id: &str,
        submitted_at: i64,
        updated_at: i64,
    ) -> Result<bool, DbError> {
        let changed = conn.execute(
            "UPDATE assignments
             SET status = 'submitted', submitted_at = ?1, updated_at = ?2
             WHERE id = ?3 AND status IN ('claimed', 'overdue', 'submitted')",
            params![submitted_at, updated_at, id],
        )?;
        Ok(changed > 0)
    }

    pub fn latest_for_participant(
        conn: &Connection,
        event_id: &str,
        participant_id: &str,
    ) -> Result<Option<AssignmentRecord>, DbError> {
        let raw = conn
            .query_row(
                "SELECT id, event_id, track_code, participant_id, round, status,
                        offered_at, claimed_at, due_at, submitted_at, created_at, updated_at
                 FROM assignments
                 WHERE event_id = ?1 AND participant_id = ?2
                 ORDER BY round DESC, created_at DESC LIMIT 1",
                params![event_id, participant_id],
                row_to_raw_assignment,
            )
            .optional()?;
        raw.map(AssignmentRecord::try_from).transpose()
    }

    pub fn list_active(
        conn: &Connection,
        event_id: &str,
    ) -> Result<Vec<AssignmentRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT id, event_id, track_code, participant_id, round, status,
                    offered_at, claimed_at, due_at, submitted_at, created_at, updated_at
             FROM assignments
             WHERE event_id = ?1 AND status IN ('offered', 'claimed', 'overdue')
             ORDER BY round, track_code",
        )?;
        let rows = statement
            .query_map(params![event_id], row_to_raw_assignment)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(AssignmentRecord::try_from)
            .collect::<Result<Vec<_>, _>>()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRecord {
    pub event_id: String,
    pub version: u32,
    pub seed: u64,
    pub rounds: u32,
    pub settings_json: String,
    pub commitment: String,
    pub committed_at: i64,
    pub reason: Option<String>,
}

struct RawPlan {
    event_id: String,
    version: i64,
    seed: i64,
    rounds: i64,
    settings_json: String,
    commitment: String,
    committed_at: i64,
    reason: Option<String>,
}

impl TryFrom<RawPlan> for PlanRecord {
    type Error = DbError;

    fn try_from(raw: RawPlan) -> Result<Self, DbError> {
        Ok(Self {
            event_id: raw.event_id,
            version: raw.version as u32,
            seed: raw.seed as u64,
            rounds: raw.rounds as u32,
            settings_json: raw.settings_json,
            commitment: raw.commitment,
            committed_at: raw.committed_at,
            reason: raw.reason,
        })
    }
}

fn row_to_raw_plan(row: &Row<'_>) -> rusqlite::Result<RawPlan> {
    Ok(RawPlan {
        event_id: row.get(0)?,
        version: row.get(1)?,
        seed: row.get(2)?,
        rounds: row.get(3)?,
        settings_json: row.get(4)?,
        commitment: row.get(5)?,
        committed_at: row.get(6)?,
        reason: row.get(7)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanAssignmentRecord {
    pub event_id: String,
    pub plan_version: u32,
    pub round: u32,
    pub track_code: String,
    pub participant_id: String,
}

struct RawPlanAssignment {
    event_id: String,
    plan_version: i64,
    round: i64,
    track_code: String,
    participant_id: String,
}

impl TryFrom<RawPlanAssignment> for PlanAssignmentRecord {
    type Error = DbError;

    fn try_from(raw: RawPlanAssignment) -> Result<Self, DbError> {
        Ok(Self {
            event_id: raw.event_id,
            plan_version: raw.plan_version as u32,
            round: raw.round as u32,
            track_code: raw.track_code,
            participant_id: raw.participant_id,
        })
    }
}

fn row_to_raw_plan_assignment(row: &Row<'_>) -> rusqlite::Result<RawPlanAssignment> {
    Ok(RawPlanAssignment {
        event_id: row.get(0)?,
        plan_version: row.get(1)?,
        round: row.get(2)?,
        track_code: row.get(3)?,
        participant_id: row.get(4)?,
    })
}

pub struct PlanRepo;

impl PlanRepo {
    pub fn insert(conn: &Connection, plan: &PlanRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO plan_versions
                (event_id, version, seed, rounds, settings_json, commitment,
                 committed_at, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &plan.event_id,
                plan.version as i64,
                plan.seed as i64,
                plan.rounds as i64,
                &plan.settings_json,
                &plan.commitment,
                plan.committed_at,
                &plan.reason
            ],
        )?;
        Ok(())
    }

    pub fn latest(conn: &Connection, event_id: &str) -> Result<Option<PlanRecord>, DbError> {
        let raw = conn
            .query_row(
                "SELECT event_id, version, seed, rounds, settings_json, commitment,
                        committed_at, reason
                 FROM plan_versions WHERE event_id = ?1
                 ORDER BY version DESC LIMIT 1",
                params![event_id],
                row_to_raw_plan,
            )
            .optional()?;
        raw.map(PlanRecord::try_from).transpose()
    }

    pub fn insert_assignment(
        conn: &Connection,
        assignment: &PlanAssignmentRecord,
    ) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO plan_assignments
                (event_id, plan_version, round, track_code, participant_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                &assignment.event_id,
                assignment.plan_version as i64,
                assignment.round as i64,
                &assignment.track_code,
                &assignment.participant_id
            ],
        )?;
        Ok(())
    }

    pub fn list_assignments(
        conn: &Connection,
        event_id: &str,
        plan_version: u32,
    ) -> Result<Vec<PlanAssignmentRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT event_id, plan_version, round, track_code, participant_id
             FROM plan_assignments
             WHERE event_id = ?1 AND plan_version = ?2
             ORDER BY round, track_code",
        )?;
        let rows = statement
            .query_map(
                params![event_id, plan_version as i64],
                row_to_raw_plan_assignment,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(PlanAssignmentRecord::try_from)
            .collect::<Result<Vec<_>, _>>()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRecord {
    pub id: String,
    pub event_id: String,
    pub track_code: String,
    pub version: u32,
    pub holder_id: String,
    pub sha256: String,
    pub size: u64,
    pub extension: String,
    pub storage_path: String,
    pub original_sha256: Option<String>,
    pub original_storage_path: Option<String>,
    pub audio_verified: bool,
    pub scrub_summary_json: String,
    pub created_at: i64,
}

struct RawFile {
    id: String,
    event_id: String,
    track_code: String,
    version: i64,
    holder_id: String,
    sha256: String,
    size: i64,
    extension: String,
    storage_path: String,
    original_sha256: Option<String>,
    original_storage_path: Option<String>,
    audio_verified: i64,
    scrub_summary_json: String,
    created_at: i64,
}

impl TryFrom<RawFile> for FileRecord {
    type Error = DbError;

    fn try_from(raw: RawFile) -> Result<Self, DbError> {
        Ok(Self {
            id: raw.id,
            event_id: raw.event_id,
            track_code: raw.track_code,
            version: raw.version as u32,
            holder_id: raw.holder_id,
            sha256: raw.sha256,
            size: raw.size as u64,
            extension: raw.extension,
            storage_path: raw.storage_path,
            original_sha256: raw.original_sha256,
            original_storage_path: raw.original_storage_path,
            audio_verified: raw.audio_verified != 0,
            scrub_summary_json: raw.scrub_summary_json,
            created_at: raw.created_at,
        })
    }
}

fn row_to_raw_file(row: &Row<'_>) -> rusqlite::Result<RawFile> {
    Ok(RawFile {
        id: row.get(0)?,
        event_id: row.get(1)?,
        track_code: row.get(2)?,
        version: row.get(3)?,
        holder_id: row.get(4)?,
        sha256: row.get(5)?,
        size: row.get(6)?,
        extension: row.get(7)?,
        storage_path: row.get(8)?,
        original_sha256: row.get(9)?,
        original_storage_path: row.get(10)?,
        audio_verified: row.get(11)?,
        scrub_summary_json: row.get(12)?,
        created_at: row.get(13)?,
    })
}

pub struct FileRepo;

impl FileRepo {
    pub fn insert(conn: &Connection, file: &FileRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO files
                (id, event_id, track_code, version, holder_id, sha256, size,
                 extension, storage_path, original_sha256, original_storage_path,
                 audio_verified, scrub_summary_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                &file.id,
                &file.event_id,
                &file.track_code,
                file.version as i64,
                &file.holder_id,
                &file.sha256,
                file.size as i64,
                &file.extension,
                &file.storage_path,
                &file.original_sha256,
                &file.original_storage_path,
                file.audio_verified as i64,
                &file.scrub_summary_json,
                file.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_for_track(
        conn: &Connection,
        event_id: &str,
        track_code: &str,
    ) -> Result<Vec<FileRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT id, event_id, track_code, version, holder_id, sha256, size,
                    extension, storage_path, original_sha256, original_storage_path,
                    audio_verified, scrub_summary_json, created_at
             FROM files WHERE event_id = ?1 AND track_code = ?2 ORDER BY version",
        )?;
        let rows = statement
            .query_map(params![event_id, track_code], row_to_raw_file)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(FileRecord::try_from)
            .collect::<Result<Vec<_>, _>>()
    }

    pub fn list_for_holder(
        conn: &Connection,
        event_id: &str,
        holder_id: &str,
    ) -> Result<Vec<FileRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT id, event_id, track_code, version, holder_id, sha256, size,
                    extension, storage_path, original_sha256, original_storage_path,
                    audio_verified, scrub_summary_json, created_at
             FROM files WHERE event_id = ?1 AND holder_id = ?2
             ORDER BY track_code, version",
        )?;
        let rows = statement
            .query_map(params![event_id, holder_id], row_to_raw_file)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(FileRecord::try_from)
            .collect::<Result<Vec<_>, _>>()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub id: i64,
    pub event_id: String,
    pub at: i64,
    pub actor: String,
    pub action: String,
    pub subject: Option<String>,
    pub detail_json: String,
    pub prev_hash: Option<String>,
    pub hash: String,
}

struct RawAudit {
    id: i64,
    event_id: String,
    at: i64,
    actor: String,
    action: String,
    subject: Option<String>,
    detail_json: String,
    prev_hash: Option<String>,
    hash: String,
}

impl TryFrom<RawAudit> for AuditRecord {
    type Error = DbError;

    fn try_from(raw: RawAudit) -> Result<Self, DbError> {
        Ok(Self {
            id: raw.id,
            event_id: raw.event_id,
            at: raw.at,
            actor: raw.actor,
            action: raw.action,
            subject: raw.subject,
            detail_json: raw.detail_json,
            prev_hash: raw.prev_hash,
            hash: raw.hash,
        })
    }
}

fn row_to_raw_audit(row: &Row<'_>) -> rusqlite::Result<RawAudit> {
    Ok(RawAudit {
        id: row.get(0)?,
        event_id: row.get(1)?,
        at: row.get(2)?,
        actor: row.get(3)?,
        action: row.get(4)?,
        subject: row.get(5)?,
        detail_json: row.get(6)?,
        prev_hash: row.get(7)?,
        hash: row.get(8)?,
    })
}

pub struct AuditRepo;

impl AuditRepo {
    /// Append an audit entry and return its hash. The hash covers the previous
    /// hash, so entries cannot be reordered or edited without detection.
    pub fn insert(
        conn: &Connection,
        event_id: &str,
        at: i64,
        actor: &str,
        action: &str,
        subject: Option<&str>,
        detail_json: &str,
    ) -> Result<String, DbError> {
        let previous = Self::last_hash(conn, event_id)?;
        let payload = serde_json::json!([
            previous.as_deref().unwrap_or(""),
            event_id,
            at,
            actor,
            action,
            subject.unwrap_or(""),
            detail_json
        ])
        .to_string();
        let hash = hex::encode(Sha256::digest(payload.as_bytes()));
        conn.execute(
            "INSERT INTO audit_log
                (event_id, at, actor, action, subject, detail_json, prev_hash, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event_id,
                at,
                actor,
                action,
                subject,
                detail_json,
                &previous,
                &hash
            ],
        )?;
        Ok(hash)
    }

    pub fn list_for_event(conn: &Connection, event_id: &str) -> Result<Vec<AuditRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT id, event_id, at, actor, action, subject, detail_json,
                    prev_hash, hash
             FROM audit_log WHERE event_id = ?1 ORDER BY id DESC",
        )?;
        let rows = statement
            .query_map(params![event_id], row_to_raw_audit)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(AuditRecord::try_from).collect()
    }

    pub fn last_hash(conn: &Connection, event_id: &str) -> Result<Option<String>, DbError> {
        Ok(conn
            .query_row(
                "SELECT hash FROM audit_log WHERE event_id = ?1 ORDER BY id DESC LIMIT 1",
                params![event_id],
                |row| row.get(0),
            )
            .optional()?)
    }
}

pub struct MetaRepo;

impl MetaRepo {
    pub fn get(conn: &Connection, key: &str) -> Result<Option<String>, DbError> {
        Ok(conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set(conn: &Connection, key: &str, value: &str) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

pub struct PrivacyRepo;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct IdentityCounts {
    pub tracks: usize,
    pub assignments: usize,
    pub seen_pairs: usize,
    pub files: usize,
    pub plan_assignments: usize,
}

impl PrivacyRepo {
    /// Rewrite every reference to one participant. The caller inserts the
    /// replacement participant first.
    pub fn replace_identity(
        conn: &Connection,
        event_id: &str,
        old_id: &str,
        new_id: &str,
    ) -> Result<IdentityCounts, DbError> {
        let tracks = conn.execute(
            "UPDATE tracks SET owner_id = ?1 WHERE event_id = ?2 AND owner_id = ?3",
            params![new_id, event_id, old_id],
        )?;
        let assignments = conn.execute(
            "UPDATE assignments SET participant_id = ?1
             WHERE event_id = ?2 AND participant_id = ?3",
            params![new_id, event_id, old_id],
        )?;
        let seen_pairs = conn.execute(
            "UPDATE seen_pairs SET participant_id = ?1
             WHERE event_id = ?2 AND participant_id = ?3",
            params![new_id, event_id, old_id],
        )?;
        let files = conn.execute(
            "UPDATE files SET holder_id = ?1 WHERE event_id = ?2 AND holder_id = ?3",
            params![new_id, event_id, old_id],
        )?;
        let plan_assignments = conn.execute(
            "UPDATE plan_assignments SET participant_id = ?1
             WHERE event_id = ?2 AND participant_id = ?3",
            params![new_id, event_id, old_id],
        )?;
        Ok(IdentityCounts {
            tracks,
            assignments,
            seen_pairs,
            files,
            plan_assignments,
        })
    }

    /// Remove plan rows after the last opened round. Pass -1 to remove all.
    pub fn delete_plan_after(
        conn: &Connection,
        event_id: &str,
        participant_id: &str,
        round: i64,
    ) -> Result<usize, DbError> {
        let changed = conn.execute(
            "DELETE FROM plan_assignments
             WHERE event_id = ?1 AND participant_id = ?2 AND round > ?3",
            params![event_id, participant_id, round],
        )?;
        Ok(changed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryRecord {
    pub id: String,
    pub event_id: String,
    pub kind: String,
    pub payload: Vec<u8>,
    pub status: String,
    pub attempts: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub sent_at: Option<i64>,
    pub last_error: Option<String>,
}

fn row_to_delivery(row: &Row<'_>) -> rusqlite::Result<DeliveryRecord> {
    Ok(DeliveryRecord {
        id: row.get(0)?,
        event_id: row.get(1)?,
        kind: row.get(2)?,
        payload: row.get(3)?,
        status: row.get(4)?,
        attempts: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        sent_at: row.get(8)?,
        last_error: row.get(9)?,
    })
}

pub struct DeliveryRepo;

impl DeliveryRepo {
    /// Insert a delivery. The id is the dedupe key. Returns true when the row
    /// is new.
    pub fn enqueue(
        conn: &Connection,
        id: &str,
        event_id: &str,
        kind: &str,
        payload: &[u8],
        now: i64,
    ) -> Result<bool, DbError> {
        let changed = conn.execute(
            "INSERT OR IGNORE INTO deliveries
                (id, event_id, kind, payload, status, attempts, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', 0, ?5, ?5)",
            params![id, event_id, kind, payload, now],
        )?;
        Ok(changed > 0)
    }

    pub fn get(conn: &Connection, id: &str) -> Result<Option<DeliveryRecord>, DbError> {
        let row = conn
            .query_row(
                "SELECT id, event_id, kind, payload, status, attempts, created_at, updated_at,
                        sent_at, last_error
                 FROM deliveries WHERE id = ?1",
                params![id],
                row_to_delivery,
            )
            .optional()?;
        Ok(row)
    }

    pub fn pending(conn: &Connection, limit: usize) -> Result<Vec<DeliveryRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT id, event_id, kind, payload, status, attempts, created_at, updated_at,
                    sent_at, last_error
             FROM deliveries WHERE status = 'pending'
             ORDER BY created_at, id LIMIT ?1",
        )?;
        let rows = statement
            .query_map(params![limit as i64], row_to_delivery)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn count_pending(conn: &Connection) -> Result<i64, DbError> {
        Ok(conn.query_row(
            "SELECT COUNT(*) FROM deliveries WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn mark_sent(conn: &Connection, id: &str, now: i64) -> Result<bool, DbError> {
        let changed = conn.execute(
            "UPDATE deliveries
             SET status = 'sent', sent_at = ?2, updated_at = ?2, last_error = NULL
             WHERE id = ?1 AND status = 'pending'",
            params![id, now],
        )?;
        Ok(changed > 0)
    }

    pub fn mark_retry(conn: &Connection, id: &str, now: i64, error: &str) -> Result<bool, DbError> {
        let changed = conn.execute(
            "UPDATE deliveries
             SET attempts = attempts + 1, updated_at = ?2, last_error = ?3
             WHERE id = ?1 AND status = 'pending'",
            params![id, now, error],
        )?;
        Ok(changed > 0)
    }

    pub fn mark_failed(
        conn: &Connection,
        id: &str,
        now: i64,
        error: &str,
    ) -> Result<bool, DbError> {
        let changed = conn.execute(
            "UPDATE deliveries
             SET status = 'failed', attempts = attempts + 1, updated_at = ?2, last_error = ?3
             WHERE id = ?1 AND status = 'pending'",
            params![id, now, error],
        )?;
        Ok(changed > 0)
    }
}

#[cfg(test)]
mod delivery_tests {
    use super::*;
    use crate::db::Db;
    use swap_core::EventState;

    fn event() -> EventRecord {
        EventRecord {
            id: "evt-1".to_string(),
            name: "Test Swap".to_string(),
            state: EventState::Draft,
            seed: 42,
            settings_json: "{}".to_string(),
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn enqueue_is_idempotent_and_marks_sent() {
        let db = Db::open_in_memory().unwrap();
        EventRepo::insert(db.conn(), &event()).unwrap();

        assert!(DeliveryRepo::enqueue(db.conn(), "key-1", "evt-1", "dm", b"payload", 10).unwrap());
        assert!(!DeliveryRepo::enqueue(db.conn(), "key-1", "evt-1", "dm", b"payload", 11).unwrap());
        assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 1);

        let pending = DeliveryRepo::pending(db.conn(), 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "key-1");
        assert_eq!(pending[0].payload, b"payload");

        assert!(DeliveryRepo::mark_sent(db.conn(), "key-1", 20).unwrap());
        assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 0);
        let sent = DeliveryRepo::get(db.conn(), "key-1").unwrap().unwrap();
        assert_eq!(sent.status, "sent");
        assert_eq!(sent.sent_at, Some(20));
    }

    #[test]
    fn a_rolled_back_enqueue_is_not_visible() {
        let mut db = Db::open_in_memory().unwrap();
        EventRepo::insert(db.conn(), &event()).unwrap();
        {
            let tx = db.conn_mut().transaction().unwrap();
            DeliveryRepo::enqueue(&tx, "rollback", "evt-1", "dm", b"payload", 10).unwrap();
            tx.rollback().unwrap();
        }
        assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 0);

        {
            let tx = db.conn_mut().transaction().unwrap();
            DeliveryRepo::enqueue(&tx, "commit", "evt-1", "dm", b"payload", 11).unwrap();
            tx.commit().unwrap();
        }
        assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 1);
    }

    #[test]
    fn a_failed_attempt_stays_pending_until_the_terminal_failure() {
        let db = Db::open_in_memory().unwrap();
        EventRepo::insert(db.conn(), &event()).unwrap();
        DeliveryRepo::enqueue(db.conn(), "key-2", "evt-1", "file", b"file", 10).unwrap();

        assert!(DeliveryRepo::mark_retry(db.conn(), "key-2", 20, "temporary").unwrap());
        let row = DeliveryRepo::get(db.conn(), "key-2").unwrap().unwrap();
        assert_eq!(row.status, "pending");
        assert_eq!(row.attempts, 1);
        assert_eq!(row.last_error.as_deref(), Some("temporary"));

        assert!(DeliveryRepo::mark_failed(db.conn(), "key-2", 30, "dm closed").unwrap());
        let row = DeliveryRepo::get(db.conn(), "key-2").unwrap().unwrap();
        assert_eq!(row.status, "failed");
        assert_eq!(row.attempts, 2);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmCursorRecord {
    pub user_id: String,
    pub channel_id: String,
    pub last_seen_message_id: Option<String>,
    pub updated_at: i64,
}

pub struct DmCursorRepo;

impl DmCursorRepo {
    pub fn upsert(
        conn: &Connection,
        user_id: &str,
        channel_id: &str,
        last_seen_message_id: Option<&str>,
        now: i64,
    ) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO dm_cursors (user_id, channel_id, last_seen_message_id, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(user_id) DO UPDATE SET
                 channel_id = excluded.channel_id,
                 last_seen_message_id = COALESCE(excluded.last_seen_message_id, dm_cursors.last_seen_message_id),
                 updated_at = excluded.updated_at",
            params![user_id, channel_id, last_seen_message_id, now],
        )?;
        Ok(())
    }

    pub fn get(conn: &Connection, user_id: &str) -> Result<Option<DmCursorRecord>, DbError> {
        Ok(conn
            .query_row(
                "SELECT user_id, channel_id, last_seen_message_id, updated_at
                 FROM dm_cursors WHERE user_id = ?1",
                params![user_id],
                |row| {
                    Ok(DmCursorRecord {
                        user_id: row.get(0)?,
                        channel_id: row.get(1)?,
                        last_seen_message_id: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn list(conn: &Connection) -> Result<Vec<DmCursorRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT user_id, channel_id, last_seen_message_id, updated_at
             FROM dm_cursors ORDER BY user_id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(DmCursorRecord {
                    user_id: row.get(0)?,
                    channel_id: row.get(1)?,
                    last_seen_message_id: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

pub struct ProcessedAttachmentRepo;

impl ProcessedAttachmentRepo {
    /// Mark an attachment as processed. Returns true when the row is new.
    pub fn mark(
        conn: &Connection,
        user_id: &str,
        message_id: &str,
        attachment_id: &str,
        now: i64,
    ) -> Result<bool, DbError> {
        let changed = conn.execute(
            "INSERT OR IGNORE INTO processed_attachments
                (user_id, message_id, attachment_id, processed_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![user_id, message_id, attachment_id, now],
        )?;
        Ok(changed > 0)
    }

    pub fn is_processed(
        conn: &Connection,
        user_id: &str,
        message_id: &str,
        attachment_id: &str,
    ) -> Result<bool, DbError> {
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM processed_attachments
             WHERE user_id = ?1 AND message_id = ?2 AND attachment_id = ?3",
            params![user_id, message_id, attachment_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }
}
