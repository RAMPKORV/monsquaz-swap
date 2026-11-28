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
    r#"
-- Migration 006: retried rows move behind untouched rows in one pass.
CREATE INDEX deliveries_pending_updated ON deliveries(status, updated_at);
"#,
    r#"
-- Migration 007: soft deadlines, reminders, extensions, host decisions.
CREATE TABLE assignment_deadlines (
    assignment_id TEXT PRIMARY KEY REFERENCES assignments(id) ON DELETE CASCADE,
    extension_count INTEGER NOT NULL DEFAULT 0,
    reminded_50 INTEGER NOT NULL DEFAULT 0,
    reminded_80 INTEGER NOT NULL DEFAULT 0,
    reminded_95 INTEGER NOT NULL DEFAULT 0,
    deadline_reminded INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE host_decisions (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    assignment_id TEXT REFERENCES assignments(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    requested_hours INTEGER,
    created_at INTEGER NOT NULL,
    decided_at INTEGER,
    reason TEXT
) STRICT;

CREATE INDEX host_decisions_open ON host_decisions(event_id, status);
"#,
    r#"
CREATE TABLE track_titles (
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    track_code TEXT NOT NULL,
    title TEXT NOT NULL,
    set_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (event_id, track_code)
) STRICT;

CREATE TABLE track_comments (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    track_code TEXT NOT NULL,
    participant_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    body TEXT NOT NULL,
    redacted_at INTEGER,
    created_at INTEGER NOT NULL
) STRICT;

CREATE INDEX track_comments_track ON track_comments(event_id, track_code, created_at);
"#,
    r#"
-- Migration 008: classic round model. R1 is the seed round and exchange
-- assignments keep their zero-based exchange index. The round table uses the
-- display number: R1 seed, R2..RN exchanges.
ALTER TABLE events ADD COLUMN round_model INTEGER NOT NULL DEFAULT 0;
ALTER TABLE events ADD COLUMN secret_hex TEXT;
ALTER TABLE events ADD COLUMN seed_round_closed_at INTEGER;
ALTER TABLE plan_versions ADD COLUMN voided_at INTEGER;
ALTER TABLE plan_versions ADD COLUMN nonce TEXT;
ALTER TABLE plan_versions ADD COLUMN pack_id TEXT;
ALTER TABLE plan_versions ADD COLUMN commitment_recipe TEXT NOT NULL DEFAULT 'v1';
ALTER TABLE assignments ADD COLUMN extension_secs INTEGER NOT NULL DEFAULT 0;
ALTER TABLE assignment_deadlines ADD COLUMN reminder_epoch INTEGER NOT NULL DEFAULT 0;

CREATE TABLE event_rounds (
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    round_no INTEGER NOT NULL CHECK (round_no >= 1),
    kind TEXT NOT NULL CHECK (kind IN ('seed', 'exchange')),
    status TEXT NOT NULL CHECK (status IN ('planned', 'open', 'closed', 'cancelled')),
    opened_at INTEGER,
    closed_at INTEGER,
    deadline_at INTEGER,
    original_deadline_at INTEGER,
    final INTEGER NOT NULL DEFAULT 0,
    deadline_moves INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (event_id, round_no)
) STRICT;

CREATE UNIQUE INDEX one_open_round_per_event
    ON event_rounds(event_id) WHERE status = 'open';
CREATE INDEX event_rounds_by_status ON event_rounds(event_id, status);

CREATE TABLE packs (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    size INTEGER NOT NULL,
    storage_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    locked_at INTEGER
) STRICT;

CREATE UNIQUE INDEX one_pack_per_event ON packs(event_id);

CREATE TABLE plan_seed_owners (
    event_id TEXT NOT NULL,
    plan_version INTEGER NOT NULL,
    track_code TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    PRIMARY KEY (event_id, plan_version, track_code),
    FOREIGN KEY (event_id, plan_version)
        REFERENCES plan_versions(event_id, version) ON DELETE CASCADE
) STRICT;

CREATE TABLE late_seeds (
    id TEXT NOT NULL,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    timezone TEXT NOT NULL,
    max_handoffs INTEGER NOT NULL,
    track_code TEXT NOT NULL,
    filename TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    original_sha256 TEXT NOT NULL,
    size INTEGER NOT NULL,
    extension TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    original_storage_path TEXT,
    audio_verified INTEGER NOT NULL DEFAULT 0,
    scrub_summary_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL CHECK (status IN ('pending', 'accepted', 'rejected')),
    created_at INTEGER NOT NULL,
    decided_at INTEGER,
    PRIMARY KEY (event_id, id),
    UNIQUE (event_id, user_id)
) STRICT;

CREATE INDEX late_seeds_pending ON late_seeds(event_id, status, created_at);
"#,
    r#"
-- Migration 009: projected reveal time. Planned round rows shift with a
-- forward deadline move.
ALTER TABLE events ADD COLUMN reveal_at INTEGER;
"#,
    r#"
-- Migration 010: participant away ranges. Times are UTC seconds.
CREATE TABLE participant_availability (
    id TEXT NOT NULL,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    participant_id TEXT NOT NULL,
    starts_at INTEGER NOT NULL,
    ends_at INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('away', 'paused')),
    note TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (event_id, id),
    FOREIGN KEY (event_id, participant_id)
        REFERENCES participants(event_id, user_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX availability_by_window
    ON participant_availability(event_id, starts_at, ends_at);
"#,
    r#"
-- Migration 011: database guards for the scheduling invariants (MSW-081).
-- Migration 001 already keeps one active assignment per track and per person
-- with partial unique indexes over 'offered', 'claimed', and 'overdue'.
-- A seen pair now remembers the assignment that made it seen, so a retry of
-- the same assignment is a no-op and a second assignment fails loudly.
ALTER TABLE seen_pairs ADD COLUMN assignment_id TEXT;

UPDATE seen_pairs SET assignment_id = (
    SELECT a.id FROM assignments a
    WHERE a.event_id = seen_pairs.event_id
      AND a.track_code = seen_pairs.track_code
      AND a.participant_id = seen_pairs.participant_id
      AND (a.claimed_at IS NOT NULL
           OR a.status IN ('claimed', 'overdue', 'submitted', 'skipped_after_claim'))
    ORDER BY a.offered_at, a.id
    LIMIT 1
);

CREATE TRIGGER assignments_no_own_track_insert
BEFORE INSERT ON assignments
WHEN EXISTS (
    SELECT 1 FROM tracks
    WHERE event_id = NEW.event_id AND code = NEW.track_code AND owner_id = NEW.participant_id
)
BEGIN
    SELECT RAISE(ABORT, 'own track assignment refused');
END;

CREATE TRIGGER assignments_no_own_track_update
BEFORE UPDATE OF participant_id, track_code ON assignments
WHEN EXISTS (
    SELECT 1 FROM tracks
    WHERE event_id = NEW.event_id AND code = NEW.track_code AND owner_id = NEW.participant_id
)
BEGIN
    SELECT RAISE(ABORT, 'own track assignment refused');
END;

CREATE TRIGGER plan_assignments_no_own_track_insert
BEFORE INSERT ON plan_assignments
WHEN EXISTS (
    SELECT 1 FROM tracks
    WHERE event_id = NEW.event_id AND code = NEW.track_code AND owner_id = NEW.participant_id
)
BEGIN
    SELECT RAISE(ABORT, 'own track plan row refused');
END;

CREATE TRIGGER plan_assignments_no_own_track_update
BEFORE UPDATE OF participant_id, track_code ON plan_assignments
WHEN EXISTS (
    SELECT 1 FROM tracks
    WHERE event_id = NEW.event_id AND code = NEW.track_code AND owner_id = NEW.participant_id
)
BEGIN
    SELECT RAISE(ABORT, 'own track plan row refused');
END;

-- A fill-in chain offers one stranded classic slot to one person at a time
-- (MSW-083). The row survives a restart. The round is the exchange index.
CREATE TABLE fill_in_chains (
    id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    track_code TEXT NOT NULL,
    round INTEGER NOT NULL,
    flow TEXT NOT NULL CHECK (flow IN ('auto_chain', 'host_list')),
    host_list_json TEXT NOT NULL DEFAULT '[]',
    tried_json TEXT NOT NULL DEFAULT '[]',
    offers INTEGER NOT NULL DEFAULT 0,
    current_assignment_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('active', 'filled', 'exhausted', 'cancelled')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (event_id, track_code) REFERENCES tracks(event_id, code) ON DELETE CASCADE
) STRICT;

CREATE UNIQUE INDEX one_active_chain_per_slot
    ON fill_in_chains(event_id, track_code, round) WHERE status = 'active';
"#,
    r#"
-- Migration 012: the host can restore a participant who left (D-051).
-- The bot asks the person once. Only a confirm makes the person active again.
-- A cancelled delivery is a row that the leave stopped before the send.
CREATE TABLE restore_requests (
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    requested_at INTEGER NOT NULL,
    status TEXT NOT NULL
        CHECK (status IN ('pending', 'accepted', 'declined', 'cancelled')),
    answered_at INTEGER,
    PRIMARY KEY (event_id, user_id, requested_at)
) STRICT;

CREATE UNIQUE INDEX one_pending_restore
    ON restore_requests(event_id, user_id) WHERE status = 'pending';
"#,
    r#"
-- Migration 013: separate claim and edit deadlines for fill-ins (D-069), a
-- database guard for the seen pair (MSW-082), and restore requests that can be
-- asked again in the same second (MSW-109).
ALTER TABLE assignments ADD COLUMN claim_deadline_at INTEGER;

UPDATE assignments SET claim_deadline_at = due_at;

-- A seen pair is never assigned again. A retry of the same assignment id is
-- allowed: the pair points at that assignment, or the engine rolls the whole
-- transaction back.
CREATE TRIGGER assignments_no_seen_pair_insert
BEFORE INSERT ON assignments
WHEN EXISTS (
    SELECT 1 FROM seen_pairs
    WHERE event_id = NEW.event_id
      AND track_code = NEW.track_code
      AND participant_id = NEW.participant_id
      AND (assignment_id IS NULL OR assignment_id <> NEW.id)
)
BEGIN
    SELECT RAISE(ABORT, 'seen pair assignment refused');
END;

CREATE TRIGGER assignments_no_seen_pair_update
BEFORE UPDATE OF participant_id, track_code, status ON assignments
WHEN EXISTS (
    SELECT 1 FROM seen_pairs
    WHERE event_id = NEW.event_id
      AND track_code = NEW.track_code
      AND participant_id = NEW.participant_id
      AND (assignment_id IS NULL OR assignment_id <> NEW.id)
)
BEGIN
    SELECT RAISE(ABORT, 'seen pair assignment refused');
END;

-- The reverse direction is scoped to a new active or claimable assignment.
-- Historical skipped-before-claim, expired, and submitted rows may legally
-- share a pair with a later seen row.
CREATE TRIGGER seen_pairs_no_active_assignment_insert
BEFORE INSERT ON seen_pairs
WHEN EXISTS (
    SELECT 1 FROM assignments
    WHERE event_id = NEW.event_id
      AND track_code = NEW.track_code
      AND participant_id = NEW.participant_id
      AND status IN ('offered', 'claimed', 'overdue')
      AND (NEW.assignment_id IS NULL OR id <> NEW.assignment_id)
)
BEGIN
    SELECT RAISE(ABORT, 'seen pair for an active assignment refused');
END;

CREATE TRIGGER seen_pairs_no_active_assignment_update
BEFORE UPDATE OF participant_id, track_code, assignment_id ON seen_pairs
WHEN EXISTS (
    SELECT 1 FROM assignments
    WHERE event_id = NEW.event_id
      AND track_code = NEW.track_code
      AND participant_id = NEW.participant_id
      AND status IN ('offered', 'claimed', 'overdue')
      AND (NEW.assignment_id IS NULL OR id <> NEW.assignment_id)
)
BEGIN
    SELECT RAISE(ABORT, 'seen pair for an active assignment refused');
END;

-- The old primary key (event, user, requested_at) refused a second request in
-- the same second. The row id is the key now; one pending request per person
-- remains.
CREATE TABLE restore_requests_v2 (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    requested_at INTEGER NOT NULL,
    status TEXT NOT NULL
        CHECK (status IN ('pending', 'accepted', 'declined', 'cancelled')),
    answered_at INTEGER
) STRICT;

INSERT INTO restore_requests_v2 (event_id, user_id, requested_at, status, answered_at)
SELECT event_id, user_id, requested_at, status, answered_at FROM restore_requests;

DROP TABLE restore_requests;
ALTER TABLE restore_requests_v2 RENAME TO restore_requests;

CREATE UNIQUE INDEX one_pending_restore
    ON restore_requests(event_id, user_id) WHERE status = 'pending';
CREATE INDEX restore_requests_by_user ON restore_requests(event_id, user_id);
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
