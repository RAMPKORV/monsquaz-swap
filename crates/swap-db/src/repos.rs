use rusqlite::{Connection, OptionalExtension, Row, params};
use sha2::{Digest, Sha256};

use swap_core::{
    AssignmentStatus, EventState, Membership, Readiness, Role, RoundKind, RoundStatus, TrackState,
};

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

    pub fn signup_event(conn: &Connection) -> Result<Option<EventRecord>, DbError> {
        let raw = conn
            .query_row(
                "SELECT e.id, e.name, e.state, e.seed, e.settings_json, e.created_at,
                        e.updated_at
                 FROM events e
                 JOIN meta m ON m.value = e.id
                 WHERE m.key = 'host.current_event' AND e.state = 'signup'
                 LIMIT 1",
                [],
                row_to_raw_event,
            )
            .optional()?;
        raw.map(TryInto::try_into).transpose()
    }

    pub fn list_active(conn: &Connection) -> Result<Vec<EventRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT id, name, state, seed, settings_json, created_at, updated_at
             FROM events WHERE state = 'active' ORDER BY created_at, id",
        )?;
        let rows = statement
            .query_map([], row_to_raw_event)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub fn open_for_user(conn: &Connection, user_id: &str) -> Result<Option<EventRecord>, DbError> {
        let raw = conn
            .query_row(
                "SELECT e.id, e.name, e.state, e.seed, e.settings_json, e.created_at,
                        e.updated_at
                 FROM events e
                 JOIN participants p ON p.event_id = e.id
                 WHERE p.user_id = ?1
                   AND p.membership IN ('signed_up', 'seeded', 'active')
                   AND e.state NOT IN ('reveal', 'archived')
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

    pub fn round_model(conn: &Connection, id: &str) -> Result<u32, DbError> {
        Ok(conn.query_row(
            "SELECT round_model FROM events WHERE id = ?1",
            params![id],
            |row| row.get::<_, i64>(0),
        )? as u32)
    }

    pub fn set_round_model(conn: &Connection, id: &str, model: u32) -> Result<(), DbError> {
        conn.execute(
            "UPDATE events SET round_model = ?2 WHERE id = ?1",
            params![id, model as i64],
        )?;
        Ok(())
    }

    pub fn secret_hex(conn: &Connection, id: &str) -> Result<Option<String>, DbError> {
        Ok(conn
            .query_row(
                "SELECT secret_hex FROM events WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }

    pub fn set_secret_hex(conn: &Connection, id: &str, secret_hex: &str) -> Result<(), DbError> {
        conn.execute(
            "UPDATE events SET secret_hex = ?2 WHERE id = ?1",
            params![id, secret_hex],
        )?;
        Ok(())
    }

    pub fn seed_round_closed_at(conn: &Connection, id: &str) -> Result<Option<i64>, DbError> {
        Ok(conn
            .query_row(
                "SELECT seed_round_closed_at FROM events WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }

    pub fn set_seed_round_closed_at(
        conn: &Connection,
        id: &str,
        at: Option<i64>,
    ) -> Result<(), DbError> {
        conn.execute(
            "UPDATE events SET seed_round_closed_at = ?2 WHERE id = ?1",
            params![id, at],
        )?;
        Ok(())
    }

    pub fn reveal_at(conn: &Connection, id: &str) -> Result<Option<i64>, DbError> {
        Ok(conn
            .query_row(
                "SELECT reveal_at FROM events WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }

    pub fn set_reveal_at(conn: &Connection, id: &str, at: Option<i64>) -> Result<(), DbError> {
        conn.execute(
            "UPDATE events SET reveal_at = ?2 WHERE id = ?1",
            params![id, at],
        )?;
        Ok(())
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
    pub extension_secs: i64,
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
    extension_secs: i64,
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
            extension_secs: raw.extension_secs,
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
        extension_secs: row.get(12)?,
    })
}

pub struct AssignmentRepo;

impl AssignmentRepo {
    pub fn insert(conn: &Connection, assignment: &AssignmentRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO assignments
                (id, event_id, track_code, participant_id, round, status,
                 offered_at, claimed_at, due_at, submitted_at, created_at, updated_at, extension_secs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
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
                assignment.updated_at,
                assignment.extension_secs
            ],
        )?;
        Ok(())
    }

    pub fn get(conn: &Connection, id: &str) -> Result<Option<AssignmentRecord>, DbError> {
        let raw = conn
            .query_row(
                "SELECT id, event_id, track_code, participant_id, round, status,
                        offered_at, claimed_at, due_at, submitted_at, created_at, updated_at, extension_secs
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
                    offered_at, claimed_at, due_at, submitted_at, created_at, updated_at, extension_secs
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

    pub fn extend(
        conn: &Connection,
        id: &str,
        due_at: i64,
        updated_at: i64,
    ) -> Result<bool, DbError> {
        let changed = conn.execute(
            "UPDATE assignments
             SET status = 'claimed', due_at = ?1, updated_at = ?2
             WHERE id = ?3 AND status IN ('claimed', 'overdue')",
            params![due_at, updated_at, id],
        )?;
        Ok(changed > 0)
    }

    pub fn set_due(
        conn: &Connection,
        id: &str,
        due_at: i64,
        extension_secs: i64,
        updated_at: i64,
    ) -> Result<bool, DbError> {
        let changed = conn.execute(
            "UPDATE assignments
             SET due_at = ?1, extension_secs = ?2, updated_at = ?3
             WHERE id = ?4 AND status IN ('offered', 'claimed', 'overdue')",
            params![due_at, extension_secs, updated_at, id],
        )?;
        Ok(changed > 0)
    }

    pub fn revert_overdue(conn: &Connection, id: &str, updated_at: i64) -> Result<bool, DbError> {
        let changed = conn.execute(
            "UPDATE assignments SET status = 'claimed', updated_at = ?1
             WHERE id = ?2 AND status = 'overdue'",
            params![updated_at, id],
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
                        offered_at, claimed_at, due_at, submitted_at, created_at, updated_at, extension_secs
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
                    offered_at, claimed_at, due_at, submitted_at, created_at, updated_at, extension_secs
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanCommitmentMeta {
    pub nonce: Option<String>,
    pub pack_id: Option<String>,
    pub recipe: String,
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
                 FROM plan_versions WHERE event_id = ?1 AND voided_at IS NULL
                 ORDER BY version DESC LIMIT 1",
                params![event_id],
                row_to_raw_plan,
            )
            .optional()?;
        raw.map(PlanRecord::try_from).transpose()
    }

    pub fn set_commitment_meta(
        conn: &Connection,
        event_id: &str,
        version: u32,
        nonce: &str,
        pack_id: Option<&str>,
        recipe: &str,
    ) -> Result<bool, DbError> {
        Ok(conn.execute(
            "UPDATE plan_versions
             SET nonce = ?3, pack_id = ?4, commitment_recipe = ?5
             WHERE event_id = ?1 AND version = ?2",
            params![event_id, version as i64, nonce, pack_id, recipe],
        )? > 0)
    }

    pub fn commitment_meta(
        conn: &Connection,
        event_id: &str,
        version: u32,
    ) -> Result<Option<PlanCommitmentMeta>, DbError> {
        Ok(conn
            .query_row(
                "SELECT nonce, pack_id, commitment_recipe FROM plan_versions
                 WHERE event_id = ?1 AND version = ?2",
                params![event_id, version as i64],
                |row| {
                    Ok(PlanCommitmentMeta {
                        nonce: row.get(0)?,
                        pack_id: row.get(1)?,
                        recipe: row.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn void(
        conn: &Connection,
        event_id: &str,
        version: u32,
        now: i64,
    ) -> Result<bool, DbError> {
        Ok(conn.execute(
            "UPDATE plan_versions SET voided_at = ?3
             WHERE event_id = ?1 AND version = ?2 AND voided_at IS NULL",
            params![event_id, version as i64, now],
        )? > 0)
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
    /// Replace the v1 seed blob without creating a new version. The host
    /// retains the old blob in the content store.
    pub fn replace_v1(
        conn: &Connection,
        event_id: &str,
        track_code: &str,
        file: &FileRecord,
    ) -> Result<bool, DbError> {
        Ok(conn.execute(
            "UPDATE files
             SET sha256 = ?3, size = ?4, extension = ?5, storage_path = ?6,
                 original_sha256 = ?7, original_storage_path = ?8,
                 audio_verified = ?9, scrub_summary_json = ?10, created_at = ?11
             WHERE event_id = ?1 AND track_code = ?2 AND version = 1",
            params![
                event_id,
                track_code,
                &file.sha256,
                file.size as i64,
                &file.extension,
                &file.storage_path,
                &file.original_sha256,
                &file.original_storage_path,
                i64::from(file.audio_verified),
                &file.scrub_summary_json,
                file.created_at
            ],
        )? > 0)
    }

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

    pub fn get(conn: &Connection, id: &str) -> Result<Option<FileRecord>, DbError> {
        conn.query_row(
            "SELECT id, event_id, track_code, version, holder_id, sha256, size,
                    extension, storage_path, original_sha256, original_storage_path,
                    audio_verified, scrub_summary_json, created_at
             FROM files WHERE id = ?1",
            params![id],
            row_to_raw_file,
        )
        .optional()?
        .map(FileRecord::try_from)
        .transpose()
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
pub struct TrackTitleRecord {
    pub event_id: String,
    pub track_code: String,
    pub title: String,
    pub set_by: String,
    pub created_at: i64,
}

pub struct TrackTitleRepo;

impl TrackTitleRepo {
    pub fn get(
        conn: &Connection,
        event_id: &str,
        track_code: &str,
    ) -> Result<Option<TrackTitleRecord>, DbError> {
        Ok(conn
            .query_row(
                "SELECT event_id, track_code, title, set_by, created_at
                 FROM track_titles WHERE event_id = ?1 AND track_code = ?2",
                params![event_id, track_code],
                row_to_track_title,
            )
            .optional()?)
    }

    pub fn upsert(conn: &Connection, title: &TrackTitleRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO track_titles (event_id, track_code, title, set_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(event_id, track_code) DO UPDATE SET
                title = excluded.title,
                set_by = excluded.set_by,
                created_at = excluded.created_at",
            params![
                &title.event_id,
                &title.track_code,
                &title.title,
                &title.set_by,
                title.created_at
            ],
        )?;
        Ok(())
    }

    pub fn list_for_event(
        conn: &Connection,
        event_id: &str,
    ) -> Result<Vec<TrackTitleRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT event_id, track_code, title, set_by, created_at
             FROM track_titles WHERE event_id = ?1 ORDER BY track_code",
        )?;
        let rows = statement
            .query_map(params![event_id], row_to_track_title)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

fn row_to_track_title(row: &Row<'_>) -> rusqlite::Result<TrackTitleRecord> {
    Ok(TrackTitleRecord {
        event_id: row.get(0)?,
        track_code: row.get(1)?,
        title: row.get(2)?,
        set_by: row.get(3)?,
        created_at: row.get(4)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackCommentRecord {
    pub id: String,
    pub event_id: String,
    pub track_code: String,
    pub participant_id: String,
    pub kind: String,
    pub body: String,
    pub redacted_at: Option<i64>,
    pub created_at: i64,
}

pub struct TrackCommentRepo;

impl TrackCommentRepo {
    pub fn insert(conn: &Connection, comment: &TrackCommentRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO track_comments
                (id, event_id, track_code, participant_id, kind, body, redacted_at, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &comment.id,
                &comment.event_id,
                &comment.track_code,
                &comment.participant_id,
                &comment.kind,
                &comment.body,
                comment.redacted_at,
                comment.created_at
            ],
        )?;
        Ok(())
    }

    pub fn get(conn: &Connection, id: &str) -> Result<Option<TrackCommentRecord>, DbError> {
        Ok(conn
            .query_row(
                "SELECT id, event_id, track_code, participant_id, kind, body,
                        redacted_at, created_at
                 FROM track_comments WHERE id = ?1",
                params![id],
                row_to_track_comment,
            )
            .optional()?)
    }

    pub fn list_for_event(
        conn: &Connection,
        event_id: &str,
    ) -> Result<Vec<TrackCommentRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT id, event_id, track_code, participant_id, kind, body,
                    redacted_at, created_at
             FROM track_comments WHERE event_id = ?1 ORDER BY created_at, id",
        )?;
        let rows = statement
            .query_map(params![event_id], row_to_track_comment)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn redact(conn: &Connection, id: &str, now: i64) -> Result<bool, DbError> {
        Ok(conn.execute(
            "UPDATE track_comments SET redacted_at = ?2
             WHERE id = ?1 AND redacted_at IS NULL",
            params![id, now],
        )? > 0)
    }
}

fn row_to_track_comment(row: &Row<'_>) -> rusqlite::Result<TrackCommentRecord> {
    Ok(TrackCommentRecord {
        id: row.get(0)?,
        event_id: row.get(1)?,
        track_code: row.get(2)?,
        participant_id: row.get(3)?,
        kind: row.get(4)?,
        body: row.get(5)?,
        redacted_at: row.get(6)?,
        created_at: row.get(7)?,
    })
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
        conn.execute(
            "UPDATE track_comments SET participant_id = ?1
             WHERE event_id = ?2 AND participant_id = ?3",
            params![new_id, event_id, old_id],
        )?;
        conn.execute(
            "UPDATE track_titles SET set_by = ?1
             WHERE event_id = ?2 AND set_by = ?3",
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
    pub recipient_id: Option<String>,
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
        recipient_id: row.get(2)?,
        kind: row.get(3)?,
        payload: row.get(4)?,
        status: row.get(5)?,
        attempts: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        sent_at: row.get(9)?,
        last_error: row.get(10)?,
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
        recipient_id: Option<&str>,
        kind: &str,
        payload: &[u8],
        now: i64,
    ) -> Result<bool, DbError> {
        let changed = conn.execute(
            "INSERT INTO deliveries
                (id, event_id, recipient_id, kind, payload, status, attempts, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', 0, ?6, ?6)
             ON CONFLICT(id) DO NOTHING",
            params![id, event_id, recipient_id, kind, payload, now],
        )?;
        Ok(changed > 0)
    }

    pub fn get(conn: &Connection, id: &str) -> Result<Option<DeliveryRecord>, DbError> {
        let row = conn
            .query_row(
                "SELECT id, event_id, recipient_id, kind, payload, status, attempts, created_at,
                        updated_at, sent_at, last_error
                 FROM deliveries WHERE id = ?1",
                params![id],
                row_to_delivery,
            )
            .optional()?;
        Ok(row)
    }

    pub fn pending(conn: &Connection, limit: usize) -> Result<Vec<DeliveryRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT id, event_id, recipient_id, kind, payload, status, attempts, created_at,
                    updated_at, sent_at, last_error
             FROM deliveries WHERE status = 'pending'
             ORDER BY updated_at, id LIMIT ?1",
        )?;
        let rows = statement
            .query_map(params![limit as i64], row_to_delivery)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn delete_for_recipient(conn: &Connection, recipient_id: &str) -> Result<usize, DbError> {
        Ok(conn.execute(
            "DELETE FROM deliveries WHERE recipient_id = ?1",
            params![recipient_id],
        )?)
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
             SET status = 'failed', updated_at = ?2, last_error = ?3
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

        assert!(
            DeliveryRepo::enqueue(db.conn(), "key-1", "evt-1", None, "dm", b"payload", 10).unwrap()
        );
        assert!(
            !DeliveryRepo::enqueue(db.conn(), "key-1", "evt-1", None, "dm", b"payload", 11)
                .unwrap()
        );
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
            DeliveryRepo::enqueue(&tx, "rollback", "evt-1", None, "dm", b"payload", 10).unwrap();
            tx.rollback().unwrap();
        }
        assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 0);

        {
            let tx = db.conn_mut().transaction().unwrap();
            DeliveryRepo::enqueue(&tx, "commit", "evt-1", None, "dm", b"payload", 11).unwrap();
            tx.commit().unwrap();
        }
        assert_eq!(DeliveryRepo::count_pending(db.conn()).unwrap(), 1);
    }

    #[test]
    fn a_failed_attempt_stays_pending_until_the_terminal_failure() {
        let db = Db::open_in_memory().unwrap();
        EventRepo::insert(db.conn(), &event()).unwrap();
        DeliveryRepo::enqueue(db.conn(), "key-2", "evt-1", None, "file", b"file", 10).unwrap();

        assert!(DeliveryRepo::mark_retry(db.conn(), "key-2", 20, "temporary").unwrap());
        let row = DeliveryRepo::get(db.conn(), "key-2").unwrap().unwrap();
        assert_eq!(row.status, "pending");
        assert_eq!(row.attempts, 1);
        assert_eq!(row.last_error.as_deref(), Some("temporary"));

        assert!(DeliveryRepo::mark_failed(db.conn(), "key-2", 30, "dm closed").unwrap());
        let row = DeliveryRepo::get(db.conn(), "key-2").unwrap().unwrap();
        assert_eq!(row.status, "failed");
        assert_eq!(row.attempts, 1);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmCursorRecord {
    pub user_id: String,
    pub channel_id: String,
    pub last_seen_message_id: Option<String>,
    pub last_sent_message_id: Option<String>,
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
            "INSERT INTO dm_cursors
                (user_id, channel_id, last_seen_message_id, last_sent_message_id, updated_at)
             VALUES (?1, ?2, ?3, NULL, ?4)
             ON CONFLICT(user_id) DO UPDATE SET
                 channel_id = excluded.channel_id,
                 last_seen_message_id = CASE
                     WHEN dm_cursors.last_seen_message_id IS NULL THEN excluded.last_seen_message_id
                     WHEN excluded.last_seen_message_id IS NULL THEN dm_cursors.last_seen_message_id
                     WHEN CAST(excluded.last_seen_message_id AS INTEGER)
                          > CAST(dm_cursors.last_seen_message_id AS INTEGER)
                         THEN excluded.last_seen_message_id
                     ELSE dm_cursors.last_seen_message_id
                 END,
                 updated_at = excluded.updated_at",
            params![user_id, channel_id, last_seen_message_id, now],
        )?;
        Ok(())
    }

    pub fn get(conn: &Connection, user_id: &str) -> Result<Option<DmCursorRecord>, DbError> {
        Ok(conn
            .query_row(
                "SELECT user_id, channel_id, last_seen_message_id, last_sent_message_id, updated_at
                 FROM dm_cursors WHERE user_id = ?1",
                params![user_id],
                |row| {
                    Ok(DmCursorRecord {
                        user_id: row.get(0)?,
                        channel_id: row.get(1)?,
                        last_seen_message_id: row.get(2)?,
                        last_sent_message_id: row.get(3)?,
                        updated_at: row.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    /// Record an outbound message. An offer also seeds the inbound cursor to
    /// the offer message, so backfill cannot return older history.
    pub fn set_sent(
        conn: &Connection,
        user_id: &str,
        channel_id: &str,
        message_id: &str,
        seed_seen: bool,
        now: i64,
    ) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO dm_cursors
                (user_id, channel_id, last_seen_message_id, last_sent_message_id, updated_at)
             VALUES (?1, ?2, CASE WHEN ?5 THEN ?3 ELSE NULL END, ?3, ?4)
             ON CONFLICT(user_id) DO UPDATE SET
                 channel_id = COALESCE(NULLIF(dm_cursors.channel_id, ''), excluded.channel_id),
                 last_seen_message_id = COALESCE(
                     dm_cursors.last_seen_message_id,
                     excluded.last_seen_message_id
                 ),
                 last_sent_message_id = excluded.last_sent_message_id,
                 updated_at = excluded.updated_at",
            params![user_id, channel_id, message_id, now, seed_seen],
        )?;
        Ok(())
    }

    pub fn delete(conn: &Connection, user_id: &str) -> Result<usize, DbError> {
        Ok(conn.execute(
            "DELETE FROM dm_cursors WHERE user_id = ?1",
            params![user_id],
        )?)
    }

    pub fn list(conn: &Connection) -> Result<Vec<DmCursorRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT user_id, channel_id, last_seen_message_id, last_sent_message_id, updated_at
             FROM dm_cursors ORDER BY user_id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(DmCursorRecord {
                    user_id: row.get(0)?,
                    channel_id: row.get(1)?,
                    last_seen_message_id: row.get(2)?,
                    last_sent_message_id: row.get(3)?,
                    updated_at: row.get(4)?,
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
            "INSERT INTO processed_attachments
                (user_id, message_id, attachment_id, processed_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(user_id, message_id, attachment_id) DO NOTHING",
            params![user_id, message_id, attachment_id, now],
        )?;
        Ok(changed > 0)
    }

    pub fn delete_for_user(conn: &Connection, user_id: &str) -> Result<usize, DbError> {
        Ok(conn.execute(
            "DELETE FROM processed_attachments WHERE user_id = ?1",
            params![user_id],
        )?)
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

#[cfg(test)]
mod dm_cursor_tests {
    use super::*;
    use crate::db::Db;

    #[test]
    fn an_offer_seeds_the_inbound_cursor_and_later_sends_do_not() {
        let db = Db::open_in_memory().unwrap();
        DmCursorRepo::set_sent(db.conn(), "u1", "dm", "100", true, 10).unwrap();
        let cursor = DmCursorRepo::get(db.conn(), "u1").unwrap().unwrap();
        assert_eq!(cursor.last_seen_message_id.as_deref(), Some("100"));
        assert_eq!(cursor.last_sent_message_id.as_deref(), Some("100"));

        DmCursorRepo::set_sent(db.conn(), "u1", "dm", "200", false, 20).unwrap();
        let cursor = DmCursorRepo::get(db.conn(), "u1").unwrap().unwrap();
        assert_eq!(cursor.last_seen_message_id.as_deref(), Some("100"));
        assert_eq!(cursor.last_sent_message_id.as_deref(), Some("200"));

        DmCursorRepo::set_sent(db.conn(), "u2", "dm", "300", false, 30).unwrap();
        let cursor = DmCursorRepo::get(db.conn(), "u2").unwrap().unwrap();
        assert!(cursor.last_seen_message_id.is_none());
        assert_eq!(cursor.last_sent_message_id.as_deref(), Some("300"));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentDeadlineRecord {
    pub assignment_id: String,
    pub extension_count: i64,
    pub reminded_50: bool,
    pub reminded_80: bool,
    pub reminded_95: bool,
    pub deadline_reminded: bool,
}

pub struct DeadlineRepo;

impl DeadlineRepo {
    pub fn ensure(conn: &Connection, assignment_id: &str) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO assignment_deadlines (assignment_id)
             VALUES (?1) ON CONFLICT(assignment_id) DO NOTHING",
            params![assignment_id],
        )?;
        Ok(())
    }

    pub fn get(
        conn: &Connection,
        assignment_id: &str,
    ) -> Result<Option<AssignmentDeadlineRecord>, DbError> {
        Ok(conn
            .query_row(
                "SELECT assignment_id, extension_count, reminded_50, reminded_80,
                        reminded_95, deadline_reminded
                 FROM assignment_deadlines WHERE assignment_id = ?1",
                params![assignment_id],
                |row| {
                    Ok(AssignmentDeadlineRecord {
                        assignment_id: row.get(0)?,
                        extension_count: row.get(1)?,
                        reminded_50: row.get::<_, i64>(2)? != 0,
                        reminded_80: row.get::<_, i64>(3)? != 0,
                        reminded_95: row.get::<_, i64>(4)? != 0,
                        deadline_reminded: row.get::<_, i64>(5)? != 0,
                    })
                },
            )
            .optional()?)
    }

    pub fn mark_reminder(
        conn: &Connection,
        assignment_id: &str,
        column: &str,
    ) -> Result<bool, DbError> {
        let sql = match column {
            "reminded_50" => {
                "UPDATE assignment_deadlines SET reminded_50 = 1
                 WHERE assignment_id = ?1 AND reminded_50 = 0"
            }
            "reminded_80" => {
                "UPDATE assignment_deadlines SET reminded_80 = 1
                 WHERE assignment_id = ?1 AND reminded_80 = 0"
            }
            "reminded_95" => {
                "UPDATE assignment_deadlines SET reminded_95 = 1
                 WHERE assignment_id = ?1 AND reminded_95 = 0"
            }
            "deadline_reminded" => {
                "UPDATE assignment_deadlines SET deadline_reminded = 1
                 WHERE assignment_id = ?1 AND deadline_reminded = 0"
            }
            _ => {
                return Err(DbError::Sqlite(rusqlite::Error::InvalidParameterName(
                    column.to_string(),
                )));
            }
        };
        Ok(conn.execute(sql, params![assignment_id])? > 0)
    }

    pub fn increment_extension(conn: &Connection, assignment_id: &str) -> Result<bool, DbError> {
        Ok(conn.execute(
            "UPDATE assignment_deadlines SET extension_count = extension_count + 1
             WHERE assignment_id = ?1",
            params![assignment_id],
        )? > 0)
    }

    pub fn reset_reminders(conn: &Connection, assignment_id: &str) -> Result<bool, DbError> {
        Ok(conn.execute(
            "UPDATE assignment_deadlines
             SET reminded_50 = 0, reminded_80 = 0, reminded_95 = 0,
                 deadline_reminded = 0, reminder_epoch = reminder_epoch + 1
             WHERE assignment_id = ?1",
            params![assignment_id],
        )? > 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostDecisionRecord {
    pub id: String,
    pub event_id: String,
    pub assignment_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub requested_hours: Option<i64>,
    pub created_at: i64,
    pub decided_at: Option<i64>,
    pub reason: Option<String>,
}

pub struct HostDecisionRepo;

impl HostDecisionRepo {
    pub fn insert(conn: &Connection, decision: &HostDecisionRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO host_decisions
                (id, event_id, assignment_id, kind, status, requested_hours,
                 created_at, decided_at, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO NOTHING",
            params![
                &decision.id,
                &decision.event_id,
                &decision.assignment_id,
                &decision.kind,
                &decision.status,
                decision.requested_hours,
                decision.created_at,
                decision.decided_at,
                &decision.reason,
            ],
        )?;
        Ok(())
    }

    pub fn get(conn: &Connection, id: &str) -> Result<Option<HostDecisionRecord>, DbError> {
        Ok(conn
            .query_row(
                "SELECT id, event_id, assignment_id, kind, status, requested_hours,
                        created_at, decided_at, reason
                 FROM host_decisions WHERE id = ?1",
                params![id],
                row_to_host_decision,
            )
            .optional()?)
    }

    pub fn open_for_assignment(
        conn: &Connection,
        assignment_id: &str,
        kind: &str,
    ) -> Result<Option<HostDecisionRecord>, DbError> {
        Ok(conn
            .query_row(
                "SELECT id, event_id, assignment_id, kind, status, requested_hours,
                        created_at, decided_at, reason
                 FROM host_decisions
                 WHERE assignment_id = ?1 AND kind = ?2 AND status = 'open'
                 ORDER BY created_at DESC LIMIT 1",
                params![assignment_id, kind],
                row_to_host_decision,
            )
            .optional()?)
    }

    pub fn open_for_key(
        conn: &Connection,
        event_id: &str,
        key: &str,
    ) -> Result<Option<HostDecisionRecord>, DbError> {
        Ok(conn
            .query_row(
                "SELECT id, event_id, assignment_id, kind, status, requested_hours,
                        created_at, decided_at, reason
                 FROM host_decisions
                 WHERE event_id = ?1 AND (id = ?2 OR assignment_id = ?2) AND status = 'open'
                 ORDER BY created_at DESC LIMIT 1",
                params![event_id, key],
                row_to_host_decision,
            )
            .optional()?)
    }

    pub fn list_open(
        conn: &Connection,
        event_id: &str,
    ) -> Result<Vec<HostDecisionRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT id, event_id, assignment_id, kind, status, requested_hours,
                    created_at, decided_at, reason
             FROM host_decisions
             WHERE event_id = ?1 AND status = 'open'
             ORDER BY created_at, id",
        )?;
        let rows = statement
            .query_map(params![event_id], row_to_host_decision)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn close(
        conn: &Connection,
        id: &str,
        status: &str,
        now: i64,
        reason: &str,
    ) -> Result<bool, DbError> {
        Ok(conn.execute(
            "UPDATE host_decisions SET status = ?2, decided_at = ?3, reason = ?4
             WHERE id = ?1 AND status = 'open'",
            params![id, status, now, reason],
        )? > 0)
    }
}

fn row_to_host_decision(row: &Row<'_>) -> rusqlite::Result<HostDecisionRecord> {
    Ok(HostDecisionRecord {
        id: row.get(0)?,
        event_id: row.get(1)?,
        assignment_id: row.get(2)?,
        kind: row.get(3)?,
        status: row.get(4)?,
        requested_hours: row.get(5)?,
        created_at: row.get(6)?,
        decided_at: row.get(7)?,
        reason: row.get(8)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRoundRecord {
    pub event_id: String,
    pub round_no: u32,
    pub kind: RoundKind,
    pub status: RoundStatus,
    pub opened_at: Option<i64>,
    pub closed_at: Option<i64>,
    pub deadline_at: Option<i64>,
    pub original_deadline_at: Option<i64>,
    pub final_round: bool,
    pub deadline_moves: u32,
}

struct RawEventRound {
    event_id: String,
    round_no: i64,
    kind: String,
    status: String,
    opened_at: Option<i64>,
    closed_at: Option<i64>,
    deadline_at: Option<i64>,
    original_deadline_at: Option<i64>,
    final_round: i64,
    deadline_moves: i64,
}

impl TryFrom<RawEventRound> for EventRoundRecord {
    type Error = DbError;

    fn try_from(raw: RawEventRound) -> Result<Self, DbError> {
        Ok(Self {
            event_id: raw.event_id,
            round_no: raw.round_no as u32,
            kind: from_db(&raw.kind)?,
            status: from_db(&raw.status)?,
            opened_at: raw.opened_at,
            closed_at: raw.closed_at,
            deadline_at: raw.deadline_at,
            original_deadline_at: raw.original_deadline_at,
            final_round: raw.final_round != 0,
            deadline_moves: raw.deadline_moves as u32,
        })
    }
}

fn row_to_raw_event_round(row: &Row<'_>) -> rusqlite::Result<RawEventRound> {
    Ok(RawEventRound {
        event_id: row.get(0)?,
        round_no: row.get(1)?,
        kind: row.get(2)?,
        status: row.get(3)?,
        opened_at: row.get(4)?,
        closed_at: row.get(5)?,
        deadline_at: row.get(6)?,
        original_deadline_at: row.get(7)?,
        final_round: row.get(8)?,
        deadline_moves: row.get(9)?,
    })
}

pub struct RoundRepo;

impl RoundRepo {
    pub fn insert(conn: &Connection, round: &EventRoundRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO event_rounds
                (event_id, round_no, kind, status, opened_at, closed_at, deadline_at,
                 original_deadline_at, final, deadline_moves)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                &round.event_id,
                round.round_no as i64,
                to_db(&round.kind)?,
                to_db(&round.status)?,
                round.opened_at,
                round.closed_at,
                round.deadline_at,
                round.original_deadline_at,
                i64::from(round.final_round),
                round.deadline_moves as i64
            ],
        )?;
        Ok(())
    }

    pub fn get(
        conn: &Connection,
        event_id: &str,
        round_no: u32,
    ) -> Result<Option<EventRoundRecord>, DbError> {
        let raw = conn
            .query_row(
                "SELECT event_id, round_no, kind, status, opened_at, closed_at,
                        deadline_at, original_deadline_at, final, deadline_moves
                 FROM event_rounds WHERE event_id = ?1 AND round_no = ?2",
                params![event_id, round_no as i64],
                row_to_raw_event_round,
            )
            .optional()?;
        raw.map(TryInto::try_into).transpose()
    }

    pub fn open_for_event(
        conn: &Connection,
        event_id: &str,
    ) -> Result<Option<EventRoundRecord>, DbError> {
        let raw = conn
            .query_row(
                "SELECT event_id, round_no, kind, status, opened_at, closed_at,
                        deadline_at, original_deadline_at, final, deadline_moves
                 FROM event_rounds WHERE event_id = ?1 AND status = 'open'
                 LIMIT 1",
                params![event_id],
                row_to_raw_event_round,
            )
            .optional()?;
        raw.map(TryInto::try_into).transpose()
    }

    pub fn list_for_event(
        conn: &Connection,
        event_id: &str,
    ) -> Result<Vec<EventRoundRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT event_id, round_no, kind, status, opened_at, closed_at,
                    deadline_at, original_deadline_at, final, deadline_moves
             FROM event_rounds WHERE event_id = ?1 ORDER BY round_no",
        )?;
        let rows = statement
            .query_map(params![event_id], row_to_raw_event_round)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub fn set_status(
        conn: &Connection,
        event_id: &str,
        round_no: u32,
        status: RoundStatus,
        now: i64,
    ) -> Result<bool, DbError> {
        Ok(conn.execute(
            "UPDATE event_rounds SET status = ?3,
                    closed_at = CASE WHEN ?3 = 'closed' THEN ?4 ELSE closed_at END
             WHERE event_id = ?1 AND round_no = ?2",
            params![event_id, round_no as i64, to_db(&status)?, now],
        )? > 0)
    }

    pub fn set_deadline(
        conn: &Connection,
        event_id: &str,
        round_no: u32,
        deadline_at: i64,
    ) -> Result<bool, DbError> {
        Ok(conn.execute(
            "UPDATE event_rounds
             SET deadline_at = ?3,
                 original_deadline_at = COALESCE(original_deadline_at, deadline_at),
                 deadline_moves = deadline_moves + 1
             WHERE event_id = ?1 AND round_no = ?2 AND status = 'open'",
            params![event_id, round_no as i64, deadline_at],
        )? > 0)
    }

    pub fn close_open_rounds(
        conn: &Connection,
        event_id: &str,
        now: i64,
    ) -> Result<usize, DbError> {
        Ok(conn.execute(
            "UPDATE event_rounds SET status = 'closed', closed_at = ?2
             WHERE event_id = ?1 AND status = 'open'",
            params![event_id, now],
        )?)
    }

    pub fn plan_to_open(
        conn: &Connection,
        event_id: &str,
        round_no: u32,
        now: i64,
        deadline_at: i64,
    ) -> Result<bool, DbError> {
        Ok(conn.execute(
            "UPDATE event_rounds
             SET status = 'open', opened_at = ?3, deadline_at = ?4,
                 original_deadline_at = COALESCE(original_deadline_at, ?4)
             WHERE event_id = ?1 AND round_no = ?2 AND status = 'planned'",
            params![event_id, round_no as i64, now, deadline_at],
        )? > 0)
    }

    pub fn shift_planned(
        conn: &Connection,
        event_id: &str,
        after_round: u32,
        delta: i64,
    ) -> Result<usize, DbError> {
        Ok(conn.execute(
            "UPDATE event_rounds
             SET deadline_at = deadline_at + ?3,
                 original_deadline_at = original_deadline_at + ?3
             WHERE event_id = ?1 AND round_no > ?2 AND status = 'planned'
               AND deadline_at IS NOT NULL",
            params![event_id, after_round as i64, delta],
        )?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackRecord {
    pub id: String,
    pub event_id: String,
    pub filename: String,
    pub sha256: String,
    pub size: u64,
    pub storage_path: String,
    pub created_at: i64,
    pub locked_at: Option<i64>,
}

pub struct PackRepo;

impl PackRepo {
    pub fn insert(conn: &Connection, pack: &PackRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO packs
                (id, event_id, filename, sha256, size, storage_path, created_at, locked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &pack.id,
                &pack.event_id,
                &pack.filename,
                &pack.sha256,
                pack.size as i64,
                &pack.storage_path,
                pack.created_at,
                pack.locked_at
            ],
        )?;
        Ok(())
    }

    pub fn get_for_event(conn: &Connection, event_id: &str) -> Result<Option<PackRecord>, DbError> {
        Ok(conn
            .query_row(
                "SELECT id, event_id, filename, sha256, size, storage_path, created_at, locked_at
                 FROM packs WHERE event_id = ?1",
                params![event_id],
                |row| {
                    Ok(PackRecord {
                        id: row.get(0)?,
                        event_id: row.get(1)?,
                        filename: row.get(2)?,
                        sha256: row.get(3)?,
                        size: row.get::<_, i64>(4)? as u64,
                        storage_path: row.get(5)?,
                        created_at: row.get(6)?,
                        locked_at: row.get(7)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn lock(conn: &Connection, event_id: &str, now: i64) -> Result<bool, DbError> {
        Ok(conn.execute(
            "UPDATE packs SET locked_at = ?2 WHERE event_id = ?1 AND locked_at IS NULL",
            params![event_id, now],
        )? > 0)
    }

    pub fn delete_for_event(conn: &Connection, event_id: &str) -> Result<usize, DbError> {
        Ok(conn.execute("DELETE FROM packs WHERE event_id = ?1", params![event_id])?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LateSeedRecord {
    pub id: String,
    pub event_id: String,
    pub user_id: String,
    pub display_name: String,
    pub timezone: String,
    pub max_handoffs: u32,
    pub track_code: String,
    pub filename: String,
    pub sha256: String,
    pub original_sha256: String,
    pub size: u64,
    pub extension: String,
    pub storage_path: String,
    pub original_storage_path: Option<String>,
    pub audio_verified: bool,
    pub scrub_summary_json: String,
    pub status: String,
    pub created_at: i64,
    pub decided_at: Option<i64>,
}

pub struct LateSeedRepo;

impl LateSeedRepo {
    pub fn insert(conn: &Connection, seed: &LateSeedRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO late_seeds
                (id, event_id, user_id, display_name, timezone, max_handoffs, track_code,
                 filename, sha256, original_sha256, size, extension, storage_path,
                 original_storage_path, audio_verified, scrub_summary_json, status,
                 created_at, decided_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                     ?16, ?17, ?18, ?19)
             ON CONFLICT(event_id, user_id) DO UPDATE SET
                 id = excluded.id,
                 display_name = excluded.display_name,
                 timezone = excluded.timezone,
                 max_handoffs = excluded.max_handoffs,
                 track_code = excluded.track_code,
                 filename = excluded.filename,
                 sha256 = excluded.sha256,
                 original_sha256 = excluded.original_sha256,
                 size = excluded.size,
                 extension = excluded.extension,
                 storage_path = excluded.storage_path,
                 original_storage_path = excluded.original_storage_path,
                 audio_verified = excluded.audio_verified,
                 scrub_summary_json = excluded.scrub_summary_json,
                 status = 'pending',
                 created_at = excluded.created_at,
                 decided_at = NULL",
            params![
                &seed.id,
                &seed.event_id,
                &seed.user_id,
                &seed.display_name,
                &seed.timezone,
                seed.max_handoffs as i64,
                &seed.track_code,
                &seed.filename,
                &seed.sha256,
                &seed.original_sha256,
                seed.size as i64,
                &seed.extension,
                &seed.storage_path,
                &seed.original_storage_path,
                i64::from(seed.audio_verified),
                &seed.scrub_summary_json,
                &seed.status,
                seed.created_at,
                seed.decided_at
            ],
        )?;
        Ok(())
    }

    pub fn get(
        conn: &Connection,
        event_id: &str,
        id: &str,
    ) -> Result<Option<LateSeedRecord>, DbError> {
        Ok(conn
            .query_row(
                "SELECT id, event_id, user_id, display_name, timezone, max_handoffs, track_code,
                        filename, sha256, original_sha256, size, extension, storage_path,
                        original_storage_path, audio_verified, scrub_summary_json, status,
                        created_at, decided_at
                 FROM late_seeds WHERE event_id = ?1 AND id = ?2",
                params![event_id, id],
                row_to_late_seed,
            )
            .optional()?)
    }

    pub fn list_pending(conn: &Connection, event_id: &str) -> Result<Vec<LateSeedRecord>, DbError> {
        let mut statement = conn.prepare(
            "SELECT id, event_id, user_id, display_name, timezone, max_handoffs, track_code,
                    filename, sha256, original_sha256, size, extension, storage_path,
                    original_storage_path, audio_verified, scrub_summary_json, status,
                    created_at, decided_at
             FROM late_seeds WHERE event_id = ?1 AND status = 'pending'
             ORDER BY created_at, id",
        )?;
        let rows = statement
            .query_map(params![event_id], row_to_late_seed)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn set_status(
        conn: &Connection,
        event_id: &str,
        id: &str,
        status: &str,
        now: i64,
    ) -> Result<bool, DbError> {
        Ok(conn.execute(
            "UPDATE late_seeds SET status = ?3, decided_at = ?4
             WHERE event_id = ?1 AND id = ?2 AND status = 'pending'",
            params![event_id, id, status, now],
        )? > 0)
    }
}

fn row_to_late_seed(row: &Row<'_>) -> rusqlite::Result<LateSeedRecord> {
    Ok(LateSeedRecord {
        id: row.get(0)?,
        event_id: row.get(1)?,
        user_id: row.get(2)?,
        display_name: row.get(3)?,
        timezone: row.get(4)?,
        max_handoffs: row.get::<_, i64>(5)? as u32,
        track_code: row.get(6)?,
        filename: row.get(7)?,
        sha256: row.get(8)?,
        original_sha256: row.get(9)?,
        size: row.get::<_, i64>(10)? as u64,
        extension: row.get(11)?,
        storage_path: row.get(12)?,
        original_storage_path: row.get(13)?,
        audio_verified: row.get::<_, i64>(14)? != 0,
        scrub_summary_json: row.get(15)?,
        status: row.get(16)?,
        created_at: row.get(17)?,
        decided_at: row.get(18)?,
    })
}

pub struct SeedOwnerRepo;

impl SeedOwnerRepo {
    pub fn insert(
        conn: &Connection,
        event_id: &str,
        plan_version: u32,
        track_code: &str,
        owner_id: &str,
    ) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO plan_seed_owners (event_id, plan_version, track_code, owner_id)
             VALUES (?1, ?2, ?3, ?4)",
            params![event_id, plan_version as i64, track_code, owner_id],
        )?;
        Ok(())
    }

    pub fn list(
        conn: &Connection,
        event_id: &str,
        plan_version: u32,
    ) -> Result<Vec<(String, String)>, DbError> {
        let mut statement = conn.prepare(
            "SELECT track_code, owner_id FROM plan_seed_owners
             WHERE event_id = ?1 AND plan_version = ?2 ORDER BY track_code",
        )?;
        let rows = statement.query_map(params![event_id, plan_version as i64], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}
