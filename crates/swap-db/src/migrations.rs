use rusqlite::Connection;

use crate::db::DbError;

/// Migration 001: the initial event, roster, track, assignment, file, and audit
/// schema. Enums are stored as snake_case strings and validated by the codec.
pub const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE events (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    state TEXT NOT NULL,
    seed INTEGER NOT NULL,
    settings_json TEXT NOT NULL DEFAULT '{}',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE participants (
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL,
    membership TEXT NOT NULL,
    readiness TEXT NOT NULL,
    timezone TEXT NOT NULL,
    max_handoffs INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (event_id, user_id)
) STRICT;

CREATE TABLE tracks (
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    code TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (event_id, code)
) STRICT;

CREATE TABLE assignments (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    track_code TEXT NOT NULL,
    participant_id TEXT NOT NULL,
    round INTEGER NOT NULL,
    status TEXT NOT NULL,
    offered_at INTEGER NOT NULL,
    claimed_at INTEGER,
    due_at INTEGER,
    submitted_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (event_id, track_code) REFERENCES tracks(event_id, code) ON DELETE CASCADE,
    FOREIGN KEY (event_id, participant_id) REFERENCES participants(event_id, user_id) ON DELETE CASCADE
) STRICT;

CREATE UNIQUE INDEX one_active_assignment_per_track
    ON assignments(event_id, track_code)
    WHERE status IN ('offered', 'claimed', 'overdue');

CREATE UNIQUE INDEX one_active_assignment_per_person
    ON assignments(event_id, participant_id)
    WHERE status IN ('offered', 'claimed', 'overdue');

CREATE TABLE seen_pairs (
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    track_code TEXT NOT NULL,
    participant_id TEXT NOT NULL,
    reason TEXT NOT NULL,
    first_seen_at INTEGER NOT NULL,
    PRIMARY KEY (event_id, track_code, participant_id)
) STRICT;

CREATE TABLE files (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    track_code TEXT NOT NULL,
    version INTEGER NOT NULL,
    holder_id TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    size INTEGER NOT NULL,
    extension TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (event_id, track_code, version),
    FOREIGN KEY (event_id, track_code) REFERENCES tracks(event_id, code) ON DELETE CASCADE
) STRICT;

CREATE TABLE audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    at INTEGER NOT NULL,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    subject TEXT,
    detail_json TEXT NOT NULL DEFAULT '{}',
    prev_hash TEXT,
    hash TEXT NOT NULL
) STRICT;

CREATE TABLE dm_cursors (
    user_id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL,
    last_seen_message_id TEXT,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT;

CREATE INDEX participants_by_event ON participants(event_id);
CREATE INDEX tracks_by_event ON tracks(event_id);
CREATE INDEX assignments_by_event_round ON assignments(event_id, round);
CREATE INDEX seen_pairs_by_event ON seen_pairs(event_id);
CREATE INDEX files_by_track ON files(event_id, track_code, version);
CREATE INDEX audit_by_event ON audit_log(event_id, at);
"#,
    r#"
-- Migration 002: committed schedule plans and one seed track per owner.
CREATE UNIQUE INDEX one_track_per_owner ON tracks(event_id, owner_id);

CREATE TABLE plan_versions (
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    seed INTEGER NOT NULL,
    rounds INTEGER NOT NULL,
    settings_json TEXT NOT NULL,
    commitment TEXT NOT NULL,
    committed_at INTEGER NOT NULL,
    reason TEXT,
    PRIMARY KEY (event_id, version)
) STRICT;

CREATE TABLE plan_assignments (
    event_id TEXT NOT NULL,
    plan_version INTEGER NOT NULL,
    round INTEGER NOT NULL,
    track_code TEXT NOT NULL,
    participant_id TEXT NOT NULL,
    PRIMARY KEY (event_id, plan_version, round, track_code),
    FOREIGN KEY (event_id, plan_version)
        REFERENCES plan_versions(event_id, version) ON DELETE CASCADE,
    FOREIGN KEY (event_id, track_code)
        REFERENCES tracks(event_id, code) ON DELETE CASCADE,
    FOREIGN KEY (event_id, participant_id)
        REFERENCES participants(event_id, user_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX plan_assignments_by_round
    ON plan_assignments(event_id, plan_version, round);
"#,
    r#"
-- Migration 003: keep the untouched original and the delivery blob.
ALTER TABLE files ADD COLUMN original_sha256 TEXT;
ALTER TABLE files ADD COLUMN original_storage_path TEXT;
ALTER TABLE files ADD COLUMN audio_verified INTEGER NOT NULL DEFAULT 0;
ALTER TABLE files ADD COLUMN scrub_summary_json TEXT NOT NULL DEFAULT '{}';
"#,
    r#"
-- Migration 004: durable delivery outbox. The id is the dedupe key.
CREATE TABLE deliveries (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    payload BLOB NOT NULL,
    status TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    sent_at INTEGER,
    last_error TEXT
) STRICT;

CREATE INDEX deliveries_pending ON deliveries(status, created_at);

CREATE TABLE processed_attachments (
    user_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    attachment_id TEXT NOT NULL,
    processed_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, message_id, attachment_id)
) STRICT;
"#,
    r#"
-- Migration 005: separate recipient index for privacy deletion, and an
-- outbound DM cursor that cannot consume inbound attachments.
ALTER TABLE deliveries ADD COLUMN recipient_id TEXT;
CREATE INDEX deliveries_by_recipient ON deliveries(recipient_id);
ALTER TABLE dm_cursors ADD COLUMN last_sent_message_id TEXT;
"#,
];

pub const LATEST_VERSION: i64 = MIGRATIONS.len() as i64;

pub fn migrate(conn: &mut Connection) -> Result<(), DbError> {
    let found = schema_version(conn)?;
    if found > LATEST_VERSION {
        return Err(DbError::SchemaTooNew {
            found,
            supported: LATEST_VERSION,
        });
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(found as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (index + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

pub fn schema_version(conn: &Connection) -> Result<i64, DbError> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}
