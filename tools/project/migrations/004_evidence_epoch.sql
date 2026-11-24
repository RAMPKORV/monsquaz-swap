-- Evidence invalidation epoch per issue (idempotent).
CREATE TABLE IF NOT EXISTS issue_evidence_epoch (
    issue_id INTEGER PRIMARY KEY REFERENCES issue(id) ON DELETE CASCADE,
    epoch        INTEGER NOT NULL DEFAULT 0,
    epoch_at     TEXT NOT NULL DEFAULT '1970-01-01 00:00:00',
    test_marker  INTEGER NOT NULL DEFAULT 0,
    review_marker INTEGER NOT NULL DEFAULT 0
);
