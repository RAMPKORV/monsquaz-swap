use rusqlite::{Connection, OptionalExtension, Row, params};

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
        created_at: row.get(9)?,
    })
}

pub struct FileRepo;

impl FileRepo {
    pub fn insert(conn: &Connection, file: &FileRecord) -> Result<(), DbError> {
        conn.execute(
            "INSERT INTO files
                (id, event_id, track_code, version, holder_id, sha256, size,
                 extension, storage_path, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
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
                    extension, storage_path, created_at
             FROM files WHERE event_id = ?1 AND track_code = ?2 ORDER BY version",
        )?;
        let rows = statement
            .query_map(params![event_id, track_code], row_to_raw_file)?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(FileRecord::try_from)
            .collect::<Result<Vec<_>, _>>()
    }
}
