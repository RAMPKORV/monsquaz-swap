-- project-scaffold schema v1
-- Canonical process state for one project. Product intent (PRD) lives in the
-- prd_* / requirement / scenario tables; work lives in issue; every mutation is
-- recorded in the append-only event ledger.
--
-- Rules enforced by the CLI and this schema:
--   * one issue table, unlimited hierarchy via parent_id
--   * blocked is a flag, not a status
--   * status rows have a stable category; names may be renamed
--   * events are append-only (triggers below)
--   * work_queue is derived and may be rebuilt at any time

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS config (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS status (
    id              INTEGER PRIMARY KEY,
    key             TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    category        TEXT NOT NULL CHECK (category IN
                    ('triage','discovery','ready','active','done','cancelled','duplicate')),
    position        INTEGER NOT NULL,
    is_terminal     INTEGER NOT NULL DEFAULT 0 CHECK (is_terminal IN (0,1)),
    requires_reason INTEGER NOT NULL DEFAULT 0 CHECK (requires_reason IN (0,1)),
    description     TEXT NOT NULL DEFAULT ''
);

INSERT OR IGNORE INTO status (id,key,name,category,position,is_terminal,requires_reason,description) VALUES
 (1,'triage','Triage','triage',10,0,0,'Qualify: valid as stated, needs change, or not worth pursuing'),
 (2,'research','Research','discovery',20,0,0,'Raise confidence with codebase and external evidence'),
 (3,'spec','Specification','discovery',30,0,0,'Map behaviour, edge cases and Gherkin acceptance scenarios'),
 (4,'ready','Ready for implementation','ready',40,0,0,'Unblocked, specified, and verifiable'),
 (5,'implementing','Implementation','active',50,0,0,'Branch/worktree claimed; code and tests in progress'),
 (6,'review','Reviewing','active',60,0,0,'Adversarial review of artifacts and implementation'),
 (7,'testing','Testing','active',70,0,0,'Deliberate adversarial testing beyond TDD'),
 (8,'merging','Merging','active',80,0,0,'Integration, conflict resolution and squash merge'),
 (9,'done','Done','done',90,1,0,'Merged, verified and recorded'),
 (10,'cancelled','Cancelled','cancelled',100,1,1,'Deliberately not pursued'),
 (11,'duplicate','Duplicate','duplicate',110,1,1,'Superseded by another issue');

CREATE TABLE IF NOT EXISTS milestone (
    id           INTEGER PRIMARY KEY,
    key          TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    target_date  TEXT,
    status       TEXT NOT NULL DEFAULT 'planned'
                 CHECK (status IN ('planned','in_progress','blocked','completed','cancelled')),
    definition_of_done TEXT NOT NULL DEFAULT '',
    evidence     TEXT NOT NULL DEFAULT '',
    review_bar   INTEGER NOT NULL DEFAULT 9 CHECK (review_bar BETWEEN 1 AND 10),
    position     INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS prd_document (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    title         TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'draft'
                  CHECK (status IN ('draft','review','signed_off','superseded')),
    version       INTEGER NOT NULL DEFAULT 1,
    summary       TEXT NOT NULL DEFAULT '',
    signed_off_at TEXT,
    signed_off_by TEXT NOT NULL DEFAULT '',
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT OR IGNORE INTO prd_document (id,title,status) VALUES (1,'Untitled project','draft');

CREATE TABLE IF NOT EXISTS prd_section (
    id          INTEGER PRIMARY KEY,
    parent_id   INTEGER REFERENCES prd_section(id),
    key         TEXT NOT NULL UNIQUE,
    title       TEXT NOT NULL,
    body_md     TEXT NOT NULL DEFAULT '',
    position    INTEGER NOT NULL DEFAULT 0,
    status      TEXT NOT NULL DEFAULT 'draft'
                CHECK (status IN ('draft','review','locked','deprecated')),
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (parent_id IS NULL OR parent_id <> id)
);
CREATE INDEX IF NOT EXISTS idx_prd_section_parent ON prd_section(parent_id);

CREATE TABLE IF NOT EXISTS requirement (
    id          INTEGER PRIMARY KEY,
    key         TEXT NOT NULL UNIQUE,
    area        TEXT NOT NULL DEFAULT '',
    title       TEXT NOT NULL,
    statement   TEXT NOT NULL,
    kind        TEXT NOT NULL DEFAULT 'functional'
                CHECK (kind IN ('functional','nonfunctional','constraint')),
    priority    TEXT NOT NULL DEFAULT 'should'
                CHECK (priority IN ('must','should','could','wont')),
    status      TEXT NOT NULL DEFAULT 'proposed'
                CHECK (status IN ('proposed','active','deferred','deprecated')),
    rationale   TEXT NOT NULL DEFAULT '',
    section_id  INTEGER REFERENCES prd_section(id),
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_requirement_section ON requirement(section_id);
CREATE INDEX IF NOT EXISTS idx_requirement_status ON requirement(status);

CREATE TABLE IF NOT EXISTS scenario (
    id             INTEGER PRIMARY KEY,
    key            TEXT NOT NULL UNIQUE,
    requirement_id INTEGER NOT NULL REFERENCES requirement(id) ON DELETE CASCADE,
    title          TEXT NOT NULL,
    given_md       TEXT NOT NULL DEFAULT '',
    when_md        TEXT NOT NULL DEFAULT '',
    then_md        TEXT NOT NULL DEFAULT '',
    tags           TEXT NOT NULL DEFAULT '',
    status         TEXT NOT NULL DEFAULT 'draft'
                   CHECK (status IN ('draft','active','deprecated')),
    position       INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_scenario_requirement ON scenario(requirement_id);

CREATE TABLE IF NOT EXISTS prd_question (
    id            INTEGER PRIMARY KEY,
    section_key   TEXT NOT NULL DEFAULT '',
    question      TEXT NOT NULL,
    why           TEXT NOT NULL DEFAULT '',
    impact        TEXT NOT NULL DEFAULT 'medium' CHECK (impact IN ('high','medium','low')),
    status        TEXT NOT NULL DEFAULT 'open'
                  CHECK (status IN ('open','answered','deferred','dropped')),
    answer        TEXT NOT NULL DEFAULT '',
    assumption_id INTEGER,
    decision_id   INTEGER,
    asked_at      TEXT NOT NULL DEFAULT (datetime('now')),
    answered_at   TEXT
);
CREATE INDEX IF NOT EXISTS idx_prd_question_status ON prd_question(status,impact);

CREATE TABLE IF NOT EXISTS assumption (
    id              INTEGER PRIMARY KEY,
    key             TEXT NOT NULL UNIQUE,
    statement       TEXT NOT NULL,
    confidence      REAL NOT NULL DEFAULT 0.5 CHECK (confidence BETWEEN 0 AND 1),
    impact          TEXT NOT NULL DEFAULT 'medium' CHECK (impact IN ('high','medium','low')),
    status          TEXT NOT NULL DEFAULT 'open'
                    CHECK (status IN ('open','validated','invalidated','deferred')),
    owner           TEXT NOT NULL DEFAULT '',
    revisit_trigger TEXT NOT NULL DEFAULT '',
    evidence        TEXT NOT NULL DEFAULT '',
    resolved_by     INTEGER REFERENCES decision(id),
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS risk (
    id          INTEGER PRIMARY KEY,
    key         TEXT NOT NULL UNIQUE,
    title       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    likelihood  INTEGER NOT NULL DEFAULT 3 CHECK (likelihood BETWEEN 1 AND 5),
    impact      INTEGER NOT NULL DEFAULT 3 CHECK (impact BETWEEN 1 AND 5),
    mitigation  TEXT NOT NULL DEFAULT '',
    trigger     TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'open'
                CHECK (status IN ('open','mitigated','accepted','realized','closed')),
    owner       TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS decision (
    id               INTEGER PRIMARY KEY,
    code             TEXT NOT NULL UNIQUE,
    title            TEXT NOT NULL,
    context_md       TEXT NOT NULL DEFAULT '',
    decision_md      TEXT NOT NULL DEFAULT '',
    consequences_md  TEXT NOT NULL DEFAULT '',
    status           TEXT NOT NULL DEFAULT 'accepted'
                     CHECK (status IN ('proposed','accepted','superseded','rejected')),
    supersedes_id    INTEGER REFERENCES decision(id),
    superseded_by_id INTEGER REFERENCES decision(id),
    prd_section_key  TEXT NOT NULL DEFAULT '',
    issue_id         INTEGER,
    requirement_id   INTEGER REFERENCES requirement(id),
    decided_by       TEXT NOT NULL DEFAULT '',
    decided_at       TEXT NOT NULL DEFAULT (datetime('now')),
    revisit_trigger  TEXT NOT NULL DEFAULT '',
    last_reviewed_at TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_decision_status ON decision(status);

CREATE TABLE IF NOT EXISTS source (
    id            INTEGER PRIMARY KEY,
    kind          TEXT NOT NULL DEFAULT 'web'
                  CHECK (kind IN ('web','paper','book','docs','video','local','experiment','other')),
    title         TEXT NOT NULL,
    url           TEXT NOT NULL DEFAULT '',
    artifact_path TEXT NOT NULL DEFAULT '',
    reliability   TEXT NOT NULL DEFAULT 'medium'
                  CHECK (reliability IN ('high','medium','low','unknown')),
    summary       TEXT NOT NULL DEFAULT '',
    tags          TEXT NOT NULL DEFAULT '',
    retrieved_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS finding (
    id             INTEGER PRIMARY KEY,
    source_id      INTEGER REFERENCES source(id),
    issue_id       INTEGER,
    requirement_id INTEGER REFERENCES requirement(id),
    category       TEXT NOT NULL DEFAULT '',
    claim          TEXT NOT NULL,
    quote          TEXT NOT NULL DEFAULT '',
    confidence     TEXT NOT NULL DEFAULT 'medium'
                   CHECK (confidence IN ('high','medium','low')),
    created_at     TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_finding_issue ON finding(issue_id);

CREATE TABLE IF NOT EXISTS issue_seq (
    id   INTEGER PRIMARY KEY CHECK (id = 1),
    next INTEGER NOT NULL DEFAULT 1
);
INSERT OR IGNORE INTO issue_seq (id,next) VALUES (1,1);

CREATE TABLE IF NOT EXISTS issue (
    id                INTEGER PRIMARY KEY,
    key               TEXT NOT NULL UNIQUE,
    parent_id         INTEGER REFERENCES issue(id),
    kind              TEXT NOT NULL DEFAULT 'work'
                      CHECK (kind IN ('work','bug','spike','chore')),
    title             TEXT NOT NULL,
    description_md    TEXT NOT NULL DEFAULT '',
    purpose_md        TEXT NOT NULL DEFAULT '',
    reason_md         TEXT NOT NULL DEFAULT '',
    solution_idea_md  TEXT NOT NULL DEFAULT '',
    status_id         INTEGER NOT NULL REFERENCES status(id),
    blocked           INTEGER NOT NULL DEFAULT 0 CHECK (blocked IN (0,1)),
    blocked_reason    TEXT NOT NULL DEFAULT '',
    priority          INTEGER CHECK (priority IS NULL OR priority BETWEEN 0 AND 4),
    rank              REAL,
    confidence        REAL CHECK (confidence IS NULL OR confidence BETWEEN 0 AND 1),
    impact            INTEGER CHECK (impact IS NULL OR impact BETWEEN 0 AND 5),
    importance        INTEGER CHECK (importance IS NULL OR importance BETWEEN 0 AND 5),
    foundational      INTEGER CHECK (foundational IS NULL OR foundational BETWEEN 0 AND 5),
    complexity        INTEGER CHECK (complexity IS NULL OR complexity IN (1,2,3,5,8,13,21)),
    mechanical        INTEGER CHECK (mechanical IS NULL OR mechanical BETWEEN 0 AND 5),
    research_needed   INTEGER CHECK (research_needed IS NULL OR research_needed BETWEEN 0 AND 5),
    time_criticality  INTEGER CHECK (time_criticality IS NULL OR time_criticality BETWEEN 0 AND 5),
    score             REAL,
    score_bucket      INTEGER CHECK (score_bucket IS NULL OR score_bucket BETWEEN 1 AND 10),
    score_rationale   TEXT NOT NULL DEFAULT '',
    scored_by         TEXT NOT NULL DEFAULT '',
    scored_at         TEXT,
    ceremony          TEXT NOT NULL DEFAULT 'standard'
                      CHECK (ceremony IN ('light','standard','full')),
    milestone_id      INTEGER REFERENCES milestone(id),
    assignee          TEXT NOT NULL DEFAULT '',
    branch_name       TEXT NOT NULL DEFAULT '',
    worktree_path     TEXT NOT NULL DEFAULT '',
    details_json      TEXT NOT NULL DEFAULT '{}',
    rework_count      INTEGER NOT NULL DEFAULT 0,
    status_entered_at TEXT NOT NULL DEFAULT (datetime('now')),
    first_started_at  TEXT,
    completed_at      TEXT,
    canceled_at       TEXT,
    archived_at       TEXT,
    created_at        TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at        TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (parent_id IS NULL OR parent_id <> id)
);
CREATE INDEX IF NOT EXISTS idx_issue_parent ON issue(parent_id);
CREATE INDEX IF NOT EXISTS idx_issue_status ON issue(status_id);
CREATE INDEX IF NOT EXISTS idx_issue_milestone ON issue(milestone_id);
CREATE INDEX IF NOT EXISTS idx_issue_assignee ON issue(assignee);
CREATE UNIQUE INDEX IF NOT EXISTS idx_issue_worktree_unique
    ON issue(worktree_path) WHERE worktree_path <> '';

CREATE TABLE IF NOT EXISTS issue_requirement (
    issue_id       INTEGER NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    requirement_id INTEGER NOT NULL REFERENCES requirement(id) ON DELETE CASCADE,
    relation       TEXT NOT NULL DEFAULT 'implements'
                   CHECK (relation IN ('implements','verifies','relates','blocks')),
    PRIMARY KEY (issue_id, requirement_id, relation)
);

CREATE TABLE IF NOT EXISTS issue_link (
    id         INTEGER PRIMARY KEY,
    src_id     INTEGER NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    dst_id     INTEGER NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    type       TEXT NOT NULL
               CHECK (type IN ('blocks','relates','duplicates','caused_by','discovered_from','validates','supersedes')),
    created_by TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (src_id <> dst_id),
    UNIQUE (src_id, dst_id, type)
);
CREATE INDEX IF NOT EXISTS idx_issue_link_src ON issue_link(src_id,type);
CREATE INDEX IF NOT EXISTS idx_issue_link_dst ON issue_link(dst_id,type);

CREATE TABLE IF NOT EXISTS issue_evidence_epoch (
    issue_id INTEGER PRIMARY KEY REFERENCES issue(id) ON DELETE CASCADE,
    epoch        INTEGER NOT NULL DEFAULT 0,
    epoch_at     TEXT NOT NULL DEFAULT '1970-01-01 00:00:00',
    test_marker  INTEGER NOT NULL DEFAULT 0,
    review_marker INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS issue_note (
    id           INTEGER PRIMARY KEY,
    issue_id     INTEGER NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    kind         TEXT NOT NULL DEFAULT 'comment'
                 CHECK (kind IN ('comment','progress','finding','decision','risk','question','handoff','review','retro')),
    body_md      TEXT NOT NULL,
    confidence   TEXT NOT NULL DEFAULT 'medium'
                 CHECK (confidence IN ('high','medium','low')),
    supersedes_id INTEGER REFERENCES issue_note(id),
    author       TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_issue_note_issue ON issue_note(issue_id,kind);

CREATE TABLE IF NOT EXISTS label (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE,
    color TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS issue_label (
    issue_id INTEGER NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    label_id INTEGER NOT NULL REFERENCES label(id) ON DELETE CASCADE,
    PRIMARY KEY (issue_id,label_id)
);

CREATE TABLE IF NOT EXISTS component (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS issue_component (
    issue_id     INTEGER NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    component_id INTEGER NOT NULL REFERENCES component(id) ON DELETE CASCADE,
    PRIMARY KEY (issue_id,component_id)
);

CREATE TABLE IF NOT EXISTS review_prd_version (
    review_id   INTEGER PRIMARY KEY REFERENCES review(id) ON DELETE CASCADE,
    prd_version INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS review (
    id           INTEGER PRIMARY KEY,
    scope        TEXT NOT NULL DEFAULT 'issue'
                 CHECK (scope IN ('issue','milestone','prd','design','code','test','security','other')),
    subject      TEXT NOT NULL,
    round        INTEGER NOT NULL DEFAULT 1,
    score        INTEGER CHECK (score IS NULL OR score BETWEEN 0 AND 10),
    bar          INTEGER NOT NULL DEFAULT 8 CHECK (bar BETWEEN 1 AND 10),
    rubric_json  TEXT NOT NULL DEFAULT '{}',
    summary      TEXT NOT NULL DEFAULT '',
    must_fix     TEXT NOT NULL DEFAULT '',
    status       TEXT NOT NULL DEFAULT 'open'
                 CHECK (status IN ('open','addressed','wontfix')),
    reviewer     TEXT NOT NULL DEFAULT '',
    commit_sha   TEXT NOT NULL DEFAULT '',
    issue_id     INTEGER REFERENCES issue(id),
    milestone_id INTEGER REFERENCES milestone(id),
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at  TEXT
);
CREATE INDEX IF NOT EXISTS idx_review_subject ON review(scope,subject);

CREATE TABLE IF NOT EXISTS human_request (
    id           INTEGER PRIMARY KEY,
    kind         TEXT NOT NULL DEFAULT 'question'
                 CHECK (kind IN ('question','review','taste','approval','playtest','research')),
    issue_id     INTEGER REFERENCES issue(id),
    title        TEXT NOT NULL,
    context_md   TEXT NOT NULL DEFAULT '',
    how_to       TEXT NOT NULL DEFAULT '',
    options_json TEXT NOT NULL DEFAULT '[]',
    status       TEXT NOT NULL DEFAULT 'open'
                 CHECK (status IN ('open','asked','answered','deferred','cancelled')),
    answer       TEXT NOT NULL DEFAULT '',
    asked_at     TEXT,
    answered_at  TEXT,
    defer_until  TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_human_status ON human_request(status);

CREATE TABLE IF NOT EXISTS session (
    id            INTEGER PRIMARY KEY,
    agent         TEXT NOT NULL DEFAULT 'agent',
    started_at    TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at      TEXT,
    summary       TEXT NOT NULL DEFAULT '',
    handoff       TEXT NOT NULL DEFAULT '',
    next_issue_id INTEGER REFERENCES issue(id),
    commit_sha    TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS agent_run (
    id         INTEGER PRIMARY KEY,
    agent      TEXT NOT NULL,
    purpose    TEXT NOT NULL,
    session_id INTEGER REFERENCES session(id),
    model      TEXT NOT NULL DEFAULT '',
    issue_id   INTEGER REFERENCES issue(id),
    status     TEXT NOT NULL DEFAULT 'running'
               CHECK (status IN ('running','done','failed','cancelled')),
    outcome    TEXT NOT NULL DEFAULT '',
    artifact   TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    ended_at   TEXT
);

CREATE TABLE IF NOT EXISTS test_run (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL DEFAULT 'test'
                CHECK (kind IN ('unit','integration','e2e','acceptance','mutation','fuzz','manual','ci','smoke','test')),
    command     TEXT NOT NULL DEFAULT '',
    commit_sha  TEXT NOT NULL DEFAULT '',
    branch      TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL CHECK (status IN ('pass','fail','error','skipped')),
    checks      INTEGER NOT NULL DEFAULT 0,
    failed      INTEGER NOT NULL DEFAULT 0,
    output_path TEXT NOT NULL DEFAULT '',
    actor       TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS test_run_issue (
    run_id   INTEGER NOT NULL REFERENCES test_run(id) ON DELETE CASCADE,
    issue_id INTEGER NOT NULL REFERENCES issue(id) ON DELETE CASCADE,
    PRIMARY KEY (run_id, issue_id)
);

CREATE TABLE IF NOT EXISTS test_result (
    id          INTEGER PRIMARY KEY,
    run_id      INTEGER NOT NULL REFERENCES test_run(id) ON DELETE CASCADE,
    scenario_id INTEGER REFERENCES scenario(id),
    issue_id    INTEGER REFERENCES issue(id),
    status      TEXT NOT NULL CHECK (status IN ('pass','fail','error','skipped')),
    duration_ms INTEGER,
    error_md    TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS work_queue (
    issue_id     INTEGER PRIMARY KEY REFERENCES issue(id) ON DELETE CASCADE,
    topo_rank    INTEGER NOT NULL,
    effective_bucket INTEGER NOT NULL,
    score        REAL NOT NULL DEFAULT 0,
    ready        INTEGER NOT NULL DEFAULT 0 CHECK (ready IN (0,1)),
    actionable   INTEGER NOT NULL DEFAULT 1 CHECK (actionable IN (0,1)),
    graph_version INTEGER NOT NULL,
    computed_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS event (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type TEXT NOT NULL,
    entity_id   INTEGER NOT NULL,
    txn_id      TEXT NOT NULL,
    actor       TEXT NOT NULL DEFAULT 'agent',
    kind        TEXT NOT NULL,
    field       TEXT NOT NULL DEFAULT '',
    old_json    TEXT,
    new_json    TEXT,
    reason_code TEXT NOT NULL DEFAULT '',
    reason      TEXT NOT NULL DEFAULT '',
    at          TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_event_entity ON event(entity_type,entity_id,id);
CREATE INDEX IF NOT EXISTS idx_event_txn ON event(txn_id);

CREATE TRIGGER IF NOT EXISTS event_no_update BEFORE UPDATE ON event
BEGIN SELECT RAISE(ABORT,'event ledger is append-only: update rejected'); END;
CREATE TRIGGER IF NOT EXISTS event_no_delete BEFORE DELETE ON event
BEGIN SELECT RAISE(ABORT,'event ledger is append-only: delete rejected'); END;

-- ------------------------------------------------------------------ views --
CREATE VIEW IF NOT EXISTS v_issue AS
SELECT i.*, s.key AS status_key, s.name AS status_name, s.category AS status_category,
       s.position AS status_position,
       (SELECT COUNT(*) FROM issue c WHERE c.parent_id = i.id) AS child_count,
       (SELECT COUNT(*) FROM issue c WHERE c.parent_id = i.id
          AND c.archived_at IS NULL
          AND c.status_id NOT IN (SELECT id FROM status WHERE is_terminal = 1)) AS open_child_count,
       CASE WHEN i.blocked = 1 THEN 1
            WHEN EXISTS (
              SELECT 1 FROM issue_link l JOIN issue b ON b.id = l.src_id
              WHERE l.dst_id = i.id AND l.type = 'blocks'
                AND b.archived_at IS NULL
                AND b.status_id NOT IN (SELECT id FROM status WHERE is_terminal = 1)
            ) THEN 1 ELSE 0 END AS is_blocked
FROM issue i JOIN status s ON s.id = i.status_id;

CREATE VIEW IF NOT EXISTS v_ready AS
SELECT * FROM v_issue
WHERE archived_at IS NULL AND is_blocked = 0 AND status_key = 'ready';

CREATE VIEW IF NOT EXISTS v_blocked AS
SELECT * FROM v_issue WHERE archived_at IS NULL AND is_blocked = 1;

CREATE VIEW IF NOT EXISTS v_open_prd_questions AS
SELECT * FROM prd_question WHERE status IN ('open','deferred') ORDER BY
  CASE impact WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END, id;

CREATE VIEW IF NOT EXISTS v_requirement_coverage AS
SELECT r.id, r.key, r.title, r.priority, r.status,
       (SELECT COUNT(*) FROM scenario s WHERE s.requirement_id = r.id
          AND s.status='active' AND length(trim(s.when_md))>0 AND length(trim(s.then_md))>0) AS scenario_count,
       (SELECT COUNT(*) FROM issue_requirement ir WHERE ir.requirement_id = r.id) AS issue_count,
       CASE WHEN NOT EXISTS (SELECT 1 FROM scenario s WHERE s.requirement_id = r.id
          AND s.status='active' AND length(trim(s.when_md))>0 AND length(trim(s.then_md))>0) THEN 1 ELSE 0 END AS missing_scenario,
       CASE WHEN NOT EXISTS (SELECT 1 FROM issue_requirement ir WHERE ir.requirement_id = r.id) THEN 1 ELSE 0 END AS missing_issue
FROM requirement r;

CREATE VIEW IF NOT EXISTS v_stale_scores AS
SELECT i.id, i.key, i.title, i.scored_at, i.score_bucket, i.impact, i.importance,
       i.foundational, i.complexity, i.mechanical, i.research_needed,
       i.time_criticality, i.confidence
FROM issue i
WHERE i.archived_at IS NULL
  AND i.status_id NOT IN (SELECT id FROM status WHERE is_terminal = 1)
  AND (i.scored_at IS NULL OR i.scored_at < datetime('now', '-' ||
       COALESCE((SELECT value FROM config WHERE key='workflow.score_ttl_days'), '14') || ' days'));

CREATE VIEW IF NOT EXISTS v_open_reviews AS
SELECT * FROM review WHERE status = 'open' ORDER BY created_at, id;

CREATE VIEW IF NOT EXISTS v_open_human AS
SELECT * FROM human_request WHERE status IN ('open','asked') ORDER BY id;

CREATE VIEW IF NOT EXISTS v_last_session AS
SELECT * FROM session ORDER BY id DESC LIMIT 1;

CREATE VIEW IF NOT EXISTS v_plan AS
SELECT q.topo_rank, q.effective_bucket, q.ready, q.actionable,
       i.key, i.title, i.kind, s.key AS status_key, i.is_blocked,
       i.impact, i.importance, i.confidence, i.complexity, i.priority
FROM work_queue q
JOIN v_issue i ON i.id = q.issue_id
JOIN status s ON s.id = i.status_id
ORDER BY q.topo_rank;
