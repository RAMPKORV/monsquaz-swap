-- Add structured test-run → issue links (idempotent for fresh databases).
CREATE TABLE IF NOT EXISTS test_run_issue (
    run_id   INTEGER NOT NULL REFERENCES test_run(id) ON DELETE CASCADE,
    issue_id INTEGER NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    PRIMARY KEY (run_id, issue_id)
);
