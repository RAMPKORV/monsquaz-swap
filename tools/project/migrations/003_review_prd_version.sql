-- Per-review PRD version binding (idempotent for fresh databases).
CREATE TABLE IF NOT EXISTS review_prd_version (
    review_id   INTEGER PRIMARY KEY REFERENCES review(id) ON DELETE CASCADE,
    prd_version INTEGER NOT NULL
);
