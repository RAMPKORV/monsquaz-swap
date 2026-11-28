#!/usr/bin/env python3
"""project — canonical project state for a solo developer + AI agents.

One SQLite database holds product intent (PRD, requirements, scenarios,
decisions), work (hierarchical issues, dependencies, reviews, evidence) and an
append-only event ledger. Every mutation goes through this CLI; the database is
never edited by hand.

Stdlib only. Run with Python 3.10+.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import time
import uuid
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path

# ----------------------------------------------------------------- constants --
SCHEMA_VERSION = "4"
TXN_ID = uuid.uuid4().hex
SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent.parent
CONFIG_PATH = REPO_ROOT / "project.config.json"
SCHEMA_PATH = SCRIPT_DIR / "schema.sql"
MIGRATIONS_DIR = SCRIPT_DIR / "migrations"

ISSUE_FIELDS = {
    "title": str, "description_md": str, "purpose_md": str, "reason_md": str,
    "solution_idea_md": str, "kind": str, "priority": int, "rank": float,
    "confidence": float, "impact": int, "importance": int, "foundational": int,
    "complexity": int, "mechanical": int, "research_needed": int,
    "time_criticality": int, "ceremony": str, "assignee": str,
    "branch_name": str, "worktree_path": str, "blocked": int, "blocked_reason": str,
    "score_rationale": str, "milestone_id": int, "parent_id": int,
}
TERMINAL = {"done", "cancelled", "duplicate"}
STATUS_ORDER = ["triage", "research", "spec", "ready", "implementing", "review",
                "testing", "merging", "done"]
DEFAULT_SECTIONS = [
    ("problem", "Problem & evidence", 10),
    ("users", "Users & jobs-to-be-done", 20),
    ("goals", "Goals, success metrics & kill criteria", 30),
    ("non_goals", "Non-goals", 40),
    ("scope", "Scope & first slice", 50),
    ("requirements", "Requirements", 60),
    ("constraints", "Constraints", 70),
    ("risks", "Risks & mitigations", 80),
    ("decisions", "Decision summary", 90),
    ("milestones", "Milestones & releases", 100),
    ("open_questions", "Open questions & assumptions", 110),
    ("glossary", "Glossary", 120),
]
REQUIRED_SECTIONS = ["problem", "users", "goals", "non_goals", "scope",
                     "requirements", "constraints", "risks", "milestones", "open_questions"]
GENERATED_MARKER = "<!-- GENERATED FILE — edit project.db via tools/project/cli.py; do not edit by hand. -->"


class ProjectError(RuntimeError):
    pass


# --------------------------------------------------------------- path/config --
_workspace_override: str | None = None


def _db_project_name(db: Path) -> str:
    try:
        con = sqlite3.connect(db)
        row = con.execute("SELECT value FROM config WHERE key='project.name'").fetchone()
        con.close()
        return row[0] if row else ""
    except sqlite3.Error:
        return ""


def _workspace_owned_by(ws: Path) -> bool:
    """True if ws belongs to this repo/worktree; does not raise."""
    cfg_path = ws / ".project" / "config.json"
    if cfg_path.exists():
        try:
            cfg = json.loads(cfg_path.read_text())
        except (OSError, ValueError):
            return False
        repo = Path(cfg.get("repo", "")).resolve() if cfg.get("repo") else None
        wts = Path(cfg.get("worktrees_dir", "")).resolve() if cfg.get("worktrees_dir") else None
        if repo == REPO_ROOT:
            return True
        if wts and (REPO_ROOT == wts or wts in REPO_ROOT.parents):
            return True
        return False
    db = ws / ".project" / "project.db"
    if db.exists():
        name = _db_project_name(db)
        return bool(name) and name == repo_config().get("project", {}).get("name", "")
    return True  # new/empty workspace


def _require_owned_workspace(ws: Path) -> Path:
    if not _workspace_owned_by(ws):
        raise ProjectError(f"workspace {ws} belongs to another project; use a separate workspace")
    return ws


def workspace_root() -> Path:
    if _workspace_override:
        return _require_owned_workspace(Path(_workspace_override).expanduser().resolve())
    env = os.environ.get("PROJECT_WORKSPACE")
    if env:
        return _require_owned_workspace(Path(env).expanduser().resolve())
    # Walk up from the repo root. Works for both the main checkout and a
    # worktree under <workspace>/worktrees/<issue>.
    candidates = [REPO_ROOT, *REPO_ROOT.parents[:5]]
    for cand in candidates:
        if _workspace_owned_by(cand):
            if (cand / ".project" / "config.json").exists() or (cand / ".project" / "project.db").exists():
                return cand
    # No matching workspace: a clone or standalone checkout owns its own .project.
    return REPO_ROOT


def repo_config() -> dict:
    if CONFIG_PATH.exists():
        return json.loads(CONFIG_PATH.read_text())
    name = REPO_ROOT.name
    prefix = re.sub(r"[^A-Za-z0-9]", "", name.upper())[:4] or "PROJ"
    return {
        "version": 1,
        "project": {"name": name, "slug": name.lower(), "prefix": prefix,
                    "kind": "generic", "description": ""},
        "paths": {
            "prd": "docs/PRD.md", "status": "docs/STATUS.md",
            "features": "tests/features",
            "dump": "project/ledger/current.jsonl",
        },
        "workflow": {"review_bar": 8, "ceremony_default": "standard",
                     "wip_limit": 6, "score_ttl_days": 14},
    }


def save_repo_config(cfg: dict) -> None:
    CONFIG_PATH.write_text(json.dumps(cfg, indent=2, sort_keys=True) + "\n")


def ws_config_path() -> Path:
    return workspace_root() / ".project" / "config.json"


def ws_config() -> dict:
    p = ws_config_path()
    if p.exists():
        return json.loads(p.read_text())
    return {}


def save_ws_config(cfg: dict) -> None:
    p = ws_config_path()
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(cfg, indent=2, sort_keys=True) + "\n")


def db_path() -> Path:
    cfg = ws_config()
    if cfg.get("db_path"):
        return Path(cfg["db_path"])
    return workspace_root() / ".project" / "project.db"


def repo_path(rel: str) -> Path:
    return REPO_ROOT / rel


def get_config_row(con: sqlite3.Connection, key: str, default: str = "") -> str:
    row = con.execute("SELECT value FROM config WHERE key=?", (key,)).fetchone()
    return row["value"] if row else default


def set_config_row(con: sqlite3.Connection, key: str, value) -> None:
    con.execute(
        "INSERT INTO config(key,value,updated_at) VALUES(?,?,datetime('now')) "
        "ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=datetime('now')",
        (key, str(value)),
    )


def project_name() -> str:
    return repo_config().get("project", {}).get("name", REPO_ROOT.name)


def project_prefix() -> str:
    return repo_config().get("project", {}).get("prefix", "PROJ")


# ------------------------------------------------------------------- database --
def connect() -> sqlite3.Connection:
    path = db_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    con = sqlite3.connect(path, timeout=10.0)
    con.row_factory = sqlite3.Row
    con.execute("PRAGMA journal_mode=WAL")
    con.execute("PRAGMA synchronous=NORMAL")
    con.execute("PRAGMA foreign_keys=ON")
    con.execute("PRAGMA busy_timeout=5000")
    return con


def write(fn):
    """Run fn(con) in a short BEGIN IMMEDIATE transaction with busy retries."""
    global TXN_ID
    TXN_ID = uuid.uuid4().hex
    con = connect()
    try:
        for attempt in range(6):
            try:
                con.execute("BEGIN IMMEDIATE")
                result = fn(con)
                con.commit()
                return result
            except sqlite3.OperationalError as exc:
                con.rollback()
                msg = str(exc).lower()
                if ("locked" in msg or "busy" in msg) and attempt < 5:
                    time.sleep(0.15 * (attempt + 1))
                    continue
                raise
    finally:
        con.close()


def audit(con, entity_type: str, entity_id: int, kind: str, field: str = "",
          old=None, new=None, reason_code: str = "", reason: str = "",
          actor: str = "agent") -> None:
    con.execute(
        "INSERT INTO event(entity_type,entity_id,txn_id,actor,kind,field,old_json,new_json,"
        "reason_code,reason) VALUES(?,?,?,?,?,?,?,?,?,?)",
        (entity_type, entity_id, TXN_ID, actor, kind, field,
         json.dumps(old, ensure_ascii=False) if old is not None else None,
         json.dumps(new, ensure_ascii=False) if new is not None else None,
         reason_code, reason),
    )


def slugify(text: str, limit: int = 48) -> str:
    s = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")
    return (s[:limit].rstrip("-") or "issue")


def rowdict(row) -> dict:
    return dict(row) if row is not None else {}


def one(con, sql, args=()):
    return con.execute(sql, args).fetchone()


def all_rows(con, sql, args=()):
    return [dict(r) for r in con.execute(sql, args).fetchall()]


def parse_kv(items, field_map: dict):
    out = {}
    for item in items or []:
        if "=" not in item:
            raise ProjectError(f"--set expects FIELD=VALUE, got {item!r}")
        key, raw = item.split("=", 1)
        key = key.strip()
        if key not in field_map:
            raise ProjectError(f"unknown field {key!r}; allowed: {', '.join(sorted(field_map))}")
        typ = field_map[key]
        if raw == "" or raw.lower() in ("null", "none"):
            out[key] = None
        elif typ is int:
            out[key] = int(raw)
        elif typ is float:
            out[key] = float(raw)
        else:
            out[key] = raw
    return out


def emit(data, as_json: bool = False) -> None:
    if as_json:
        print(json.dumps(data, indent=2, ensure_ascii=False, default=str))
        return
    if isinstance(data, dict):
        for k, v in data.items():
            if isinstance(v, (dict, list)):
                print(f"{k}: {json.dumps(v, ensure_ascii=False)}")
            else:
                print(f"{k}: {v}")
    elif isinstance(data, list):
        for item in data:
            print(item)
    else:
        print(data)


def table(rows: list[dict], columns: list[str], empty: str = "nothing found") -> None:
    if not rows:
        print(empty)
        return
    widths = {c: len(c) for c in columns}
    for r in rows:
        for c in columns:
            widths[c] = max(widths[c], len(str(r.get(c, ""))))
    print("  ".join(c.upper().ljust(widths[c]) for c in columns))
    print("  ".join("-" * widths[c] for c in columns))
    for r in rows:
        print("  ".join(str(r.get(c, "")).ljust(widths[c]) for c in columns))


# -------------------------------------------------------------------- schema --
def apply_schema(con: sqlite3.Connection) -> None:
    if not SCHEMA_PATH.exists():
        raise ProjectError(f"schema not found: {SCHEMA_PATH}")
    con.executescript(SCHEMA_PATH.read_text())
    # Apply numbered migrations newer than the recorded version.
    if MIGRATIONS_DIR.exists():
        current = int(get_config_row(con, "schema.version", "1") or "1")
        for path in sorted(MIGRATIONS_DIR.glob("*.sql")):
            try:
                num = int(path.name.split("_", 1)[0])
            except ValueError:
                continue
            if num > current:
                con.executescript(path.read_text())
                current = num
        set_config_row(con, "schema.version", current)
    set_config_row(con, "schema.version", max(int(get_config_row(con, "schema.version", "1") or "1"), 1))


def require_db() -> None:
    if not db_path().exists():
        raise ProjectError(f"database not found at {db_path()}; run `project rebuild --yes` to "
                           "restore from the dump (use init only for a truly new project)")


def get_issue(con, key_or_id: str) -> sqlite3.Row:
    if isinstance(key_or_id, int) or str(key_or_id).isdigit():
        row = one(con, "SELECT * FROM v_issue WHERE id=?", (int(key_or_id),))
    else:
        row = one(con, "SELECT * FROM v_issue WHERE key=?", (str(key_or_id).upper(),))
    if not row:
        raise ProjectError(f"issue not found: {key_or_id}")
    return row


def get_status(con, key: str) -> sqlite3.Row:
    row = one(con, "SELECT * FROM status WHERE key=?", (key,))
    if not row:
        raise ProjectError(f"unknown status {key!r}; use: {', '.join(STATUS_ORDER + ['cancelled', 'duplicate'])}")
    return row


def next_issue_key(con) -> str:
    row = one(con, "SELECT next FROM issue_seq WHERE id=1")
    seq = int(row["next"])
    con.execute("UPDATE issue_seq SET next=? WHERE id=1", (seq + 1,))
    return f"{project_prefix()}-{seq:03d}"


def branch_for(kind: str, key: str, title: str) -> str:
    prefix = {"bug": "fix", "spike": "spike", "chore": "chore"}.get(kind, "feat")
    return f"{prefix}/{key}-{slugify(title)}"


def bump_graph(con) -> int:
    current = int(get_config_row(con, "plan.graph_version", "0") or "0") + 1
    set_config_row(con, "plan.graph_version", current)
    return current


def audit_fields(con, issue_id: int, old: dict, new: dict, reason: str = "",
                 reason_code: str = "", actor: str = "agent") -> None:
    for field, value in new.items():
        if old.get(field) != value:
            audit(con, "issue", issue_id, "update", field, old.get(field), value,
                  reason_code=reason_code, reason=reason, actor=actor)


# ---------------------------------------------------------------------- plan --
def compute_plan(con) -> int:
    """Recompute work_queue. Call inside a write transaction."""
    issues = all_rows(con, "SELECT * FROM v_issue WHERE archived_at IS NULL")
    open_issues = [i for i in issues if i["status_key"] not in TERMINAL]
    by_id = {i["id"]: i for i in open_issues}
    edges: dict[int, set[int]] = {i["id"]: set() for i in open_issues}
    indeg: dict[int, int] = {i["id"]: 0 for i in open_issues}
    for link in all_rows(con, "SELECT src_id,dst_id FROM issue_link WHERE type='blocks'"):
        a, b = link["src_id"], link["dst_id"]
        if a in by_id and b in by_id and b not in edges[a]:
            edges[a].add(b)
            indeg[b] += 1

    def descendants(start: int) -> set[int]:
        seen, stack = set(), [start]
        while stack:
            n = stack.pop()
            for m in edges.get(n, ()):
                if m not in seen:
                    seen.add(m)
                    stack.append(m)
        return seen

    unblocks = {iid: len(descendants(iid)) for iid in by_id}
    scores = {}
    for iid, i in by_id.items():
        impact = i["impact"] or 0
        importance = i["importance"] or 0
        tc = i["time_criticality"] or 0
        foundational = i["foundational"] or 0
        cod = 3 * impact + 2 * importance + 1 * tc + 2 * foundational + 2 * math.log2(1 + unblocks[iid])
        complexity = i["complexity"] or 3
        research = i["research_needed"] or 0
        mechanical = i["mechanical"] or 0
        size = max(1.0, complexity * (1 + 0.30 * research) * (1 - 0.08 * mechanical))
        confidence = i["confidence"] if i["confidence"] is not None else 0.7
        score = round(confidence * cod / size, 3)
        bucket = max(1, min(10, int(round(score))))
        if i["priority"] is not None:
            bucket = min(10, bucket + (4 - int(i["priority"])))
        scores[iid] = (score, bucket)

    # Kahn topological sort, priority queue by bucket then rank then key.
    ready_ids = [iid for iid in by_id if indeg[iid] == 0]
    ready_ids.sort(key=lambda iid: (-scores[iid][1],
                                     by_id[iid]["rank"] if by_id[iid]["rank"] is not None else 1e9,
                                     by_id[iid]["key"]))
    order, seen = [], set()
    while ready_ids:
        iid = ready_ids.pop(0)
        if iid in seen:
            continue
        seen.add(iid)
        order.append(iid)
        for nxt in sorted(edges[iid], key=lambda x: by_id[x]["key"]):
            indeg[nxt] -= 1
            if indeg[nxt] == 0:
                ready_ids.append(nxt)
        ready_ids.sort(key=lambda iid: (-scores[iid][1],
                                         by_id[iid]["rank"] if by_id[iid]["rank"] is not None else 1e9,
                                         by_id[iid]["key"]))
    # Cycles: append remaining in score order, flagged not ready.
    remaining = [iid for iid in by_id if iid not in seen]
    remaining.sort(key=lambda iid: (-scores[iid][1], by_id[iid]["key"]))
    order.extend(remaining)

    version = int(get_config_row(con, "plan.graph_version", "0") or "0")
    con.execute("DELETE FROM work_queue")
    for rank, iid in enumerate(order, start=1):
        i = by_id[iid]
        score, bucket = scores[iid]
        is_ready = (i["status_key"] == "ready" and not i["is_blocked"] and indeg.get(iid, 0) == 0)
        con.execute("UPDATE issue SET score=?, score_bucket=?, updated_at=updated_at WHERE id=?",
                    (score, bucket, iid))
        con.execute(
            "INSERT INTO work_queue(issue_id,topo_rank,effective_bucket,score,ready,actionable,graph_version) "
            "VALUES(?,?,?,?,?,?,?)",
            (iid, rank, bucket, score, 1 if is_ready else 0,
             1 if (i["child_count"] == 0) else 0, version),
        )
    set_config_row(con, "plan.computed_version", version)
    set_config_row(con, "plan.computed_at", datetime.now(timezone.utc).isoformat(timespec="seconds"))
    return len(order)


def ensure_plan(con) -> None:
    current = int(get_config_row(con, "plan.graph_version", "0") or "0")
    computed = int(get_config_row(con, "plan.computed_version", "-1") or "-1")
    if current != computed:
        write(lambda c: compute_plan(c))


def plan_rows(con, limit: int | None = None, ready_only: bool = False) -> list[dict]:
    sql = ("SELECT q.topo_rank, i.key, i.title, i.kind, i.status_key, i.is_blocked, "
           "q.effective_bucket, q.score, q.ready, q.actionable, i.impact, i.importance, "
           "i.confidence, i.complexity, i.priority "
           "FROM work_queue q JOIN v_issue i ON i.id=q.issue_id ORDER BY q.topo_rank")
    rows = all_rows(con, sql)
    if ready_only:
        rows = [r for r in rows if r["ready"] and not r["is_blocked"] and r["actionable"]]
    return rows[:limit] if limit else rows


def _details(issue) -> dict:
    try:
        return json.loads(issue["details_json"] or "{}")
    except (TypeError, ValueError):
        return {}


def evidence_markers(con, issue_id: int) -> tuple[int, int]:
    row = one(con, "SELECT test_marker,review_marker FROM issue_evidence_epoch WHERE issue_id=?",
              (issue_id,))
    return (int(row["test_marker"]), int(row["review_marker"])) if row else (0, 0)


def bump_evidence_epoch(con, issue_id: int, actor: str = "agent") -> None:
    test_marker = one(con, "SELECT COALESCE(MAX(id),0) m FROM test_run")["m"]
    review_marker = one(con, "SELECT COALESCE(MAX(id),0) m FROM review")["m"]
    con.execute("""INSERT INTO issue_evidence_epoch(issue_id,epoch,epoch_at,test_marker,review_marker)
        VALUES(?,1,datetime('now'),?,?)
        ON CONFLICT(issue_id) DO UPDATE SET epoch=epoch+1, epoch_at=datetime('now'),
          test_marker=excluded.test_marker, review_marker=excluded.review_marker""",
                (issue_id, test_marker, review_marker))
    audit(con, "issue", issue_id, "evidence_epoch", "epoch", None, "bumped", actor=actor)


def _passing_test(con, issue) -> bool:
    details = _details(issue)
    mc = details.get("merge_commit", "")
    test_marker, _ = evidence_markers(con, issue["id"])
    row = one(con, """SELECT 1 FROM test_run t
        LEFT JOIN test_run_issue ti ON ti.run_id=t.id
        WHERE t.status='pass' AND t.id > ?
          AND (ti.issue_id=?
               OR (t.branch<>'' AND t.branch=?)
               OR (t.commit_sha<>'' AND t.commit_sha=?))
        LIMIT 1""", (test_marker, issue["id"], issue["branch_name"], mc))
    if row:
        return True
    # Scenario-level Gherkin results also count when they belong to this issue or one
    # of its linked requirements.
    row = one(con, """SELECT 1 FROM test_result tr JOIN test_run t ON t.id=tr.run_id
        WHERE t.status='pass' AND tr.status='pass' AND t.id > ?
          AND (tr.issue_id=? OR tr.scenario_id IN (
              SELECT s.id FROM scenario s JOIN issue_requirement ir ON ir.requirement_id=s.requirement_id
              WHERE ir.issue_id=?))
        LIMIT 1""", (test_marker, issue["id"], issue["id"]))
    return bool(row)


def _active_scenarios(con, issue_id: int) -> int:
    return one(con, """SELECT COUNT(*) n FROM issue_requirement ir
        JOIN scenario s ON s.requirement_id=ir.requirement_id
        WHERE ir.issue_id=? AND s.status='active'
          AND length(trim(s.when_md))>0 AND length(trim(s.then_md))>0""", (issue_id,))["n"]


def latest_review(con, scope: str, subject: str) -> dict | None:
    row = one(con, """SELECT * FROM review WHERE scope=? AND subject=?
        ORDER BY round DESC, id DESC LIMIT 1""", (scope, subject))
    return dict(row) if row else None


def review_is_good(review: dict | None) -> bool:
    return bool(review and review.get("status") == "addressed"
                and review.get("score") is not None
                and review["score"] >= review["bar"]
                and not (review.get("must_fix") or "").strip())


def _latest_review(con, issue) -> dict | None:
    _, review_marker = evidence_markers(con, issue["id"])
    row = one(con, """SELECT * FROM review WHERE
        (issue_id=? OR (scope='issue' AND subject=?)) AND id > ?
        ORDER BY round DESC, id DESC LIMIT 1""", (issue["id"], issue["key"], review_marker))
    return dict(row) if row else None


def _review_is_good(review: dict | None) -> bool:
    return review_is_good(review)


def _resolved_review(con, issue) -> bool:
    return _review_is_good(_latest_review(con, issue))


def _open_bad_review(con, issue) -> dict | None:
    review = _latest_review(con, issue)
    if review and not _review_is_good(review):
        return review
    return None


def assert_done_allowed(con, issue, evidence: str = "", force: bool = False,
                        reason: str = "", require_merge: bool = True,
                        waive_review: bool = True) -> None:
    if issue["status_key"] in TERMINAL:
        raise ProjectError("issue is already terminal")
    details = _details(issue)
    has_evidence = bool((evidence or "").strip() or details.get("merge_commit"))
    if not (_passing_test(con, issue) or has_evidence):
        raise ProjectError("done always requires a recorded passing test run, merge commit, "
                           "or --evidence; --force cannot waive evidence")
    pm = details.get("post_merge_check", "")
    if require_merge:
        if pm in ("pending", "failed"):
            raise ProjectError("done is blocked by a pending/failed main post-merge check; "
                               "rerun `worktree finish` after fixing main, or use "
                               "`issue acknowledge-check KEY --reason ...` for an explicit exception")
        if issue["worktree_path"]:
            raise ProjectError("done requires `worktree finish` (or `worktree recover` to clear a lost "
                               "worktree); a recorded worktree cannot be closed manually")
        if details.get("merge_commit") and pm not in ("passed", "skipped"):
            raise ProjectError("done requires the main post-merge check to pass "
                               "(or an explicit --no-check skip); rerun `worktree finish`")
    # `worktree finish` always passes --force for the PRD gate (D-021). That
    # flag must not waive the review gate, so finish calls with waive_review
    # false. The generic `issue done --force` path keeps its explicit override.
    if force and waive_review:
        return
    # Review gate: every standard and full issue (ceremony != light) needs a
    # resolved review.
    if issue["ceremony"] != "light":
        if not _resolved_review(con, issue):
            raise ProjectError("done requires a resolved review for a standard or full issue")
        bad = _open_bad_review(con, issue)
        if bad:
            raise ProjectError(f"open review #{bad['id']} is below bar or has must-fix items; "
                               "resolve it before done")
        if require_merge and not details.get("merge_commit") and not (evidence or "").strip():
            raise ProjectError("done requires the merge commit (or --evidence naming it)")


def assert_stage_gates(con, issue, target_key: str, force: bool = False,
                       reason: str = "", evidence: str = "") -> None:
    if force or issue["ceremony"] == "light":
        return
    details = _details(issue)
    if target_key == "ready":
        if _active_scenarios(con, issue["id"]) == 0 and not (details.get("verification") or details.get("verification_command")):
            raise ProjectError("ready requires an active Gherkin scenario or details.verification "
                               "(or --force with a reason)")
    elif target_key == "review":
        if not _passing_test(con, issue):
            raise ProjectError("review requires a recorded passing test run (or --force with a reason)")
    elif target_key == "testing":
        if not _resolved_review(con, issue):
            raise ProjectError("testing requires a resolved review at/above its bar with no must-fix "
                               "(or --force with a reason)")
    elif target_key == "merging":
        if not _passing_test(con, issue):
            raise ProjectError("merging requires a recorded passing test run (or --force with a reason)")
        if not _resolved_review(con, issue):
            raise ProjectError("merging requires a latest review at/above bar with no must-fix "
                               "(or --force with a reason)")
    elif target_key == "done":
        assert_done_allowed(con, issue, evidence=evidence, force=force, reason=reason)


PLACEHOLDER_RE = re.compile(r"\b(TODO|TBD|PLACEHOLDER|FIXME|NEEDS CLARIFICATION)\b", re.I)
EARS_RE = re.compile(r"\b(SHALL|MUST|SHOULD|MAY)\b", re.I)


def validate_requirement_statement(statement: str, kind: str = "functional") -> str:
    text = (statement or "").strip()
    if len(text) < 10:
        raise ProjectError("requirement statement is too short to be testable")
    if PLACEHOLDER_RE.search(text):
        raise ProjectError("requirement statement still contains a placeholder")
    if kind in ("functional", "nonfunctional") and not EARS_RE.search(text):
        raise ProjectError("functional/nonfunctional requirement should use EARS/RFC 2119 "
                           "(SHALL/MUST/SHOULD/MAY)")
    return text


def prd_validation(con) -> tuple[str, str]:
    value = get_config_row(con, "prd.validation", "validated") or "validated"
    note = get_config_row(con, "prd.validation_note", "")
    return value, note


def ensure_prd_mutable(con, reason: str, actor: str = "agent") -> None:
    doc = one(con, "SELECT * FROM prd_document WHERE id=1")
    if doc["status"] != "signed_off":
        reviewed = one(con, """SELECT 1 FROM review_prd_version rv
            JOIN review r ON r.id=rv.review_id
            WHERE r.scope='prd' AND rv.prd_version=? LIMIT 1""", (doc["version"],))
        if reviewed:
            con.execute("UPDATE prd_document SET version=version+1, updated_at=datetime('now') WHERE id=1")
            audit(con, "prd_document", 1, "review_stale", "version", doc["version"],
                  int(doc["version"]) + 1, reason=reason or "PRD edited after review", actor=actor)
        return
    if not reason:
        raise ProjectError("PRD is signed off; pass --reason to revise it (records a new version)")
    con.execute("UPDATE prd_document SET status='draft', version=version+1, "
                "signed_off_at=NULL, signed_off_by='', updated_at=datetime('now') WHERE id=1")
    con.execute("UPDATE prd_section SET status='draft' WHERE status IN ('review','locked')")
    audit(con, "prd_document", 1, "revise", "status", "signed_off", "draft",
          reason=reason, actor=actor)


def prd_blockers(con) -> tuple[list[str], list[str]]:
    blockers, warnings = [], []
    doc = one(con, "SELECT * FROM prd_document WHERE id=1")
    sections = {r["key"]: dict(r) for r in con.execute("SELECT * FROM prd_section")}
    for key in REQUIRED_SECTIONS:
        s = sections.get(key)
        if not s:
            blockers.append(f"missing required PRD section: {key}")
        else:
            body = (s["body_md"] or "").strip()
            if len(body) < 5 or not body:
                blockers.append(f"PRD section is empty: {key}")
            elif PLACEHOLDER_RE.search(body):
                blockers.append(f"PRD section still has a placeholder: {key}")
    for q in all_rows(con, "SELECT id,question FROM prd_question WHERE status='open' AND impact='high'"):
        blockers.append(f"open high-impact question #{q['id']}: {q['question']}")
    for r in all_rows(con, """SELECT r.key,r.title FROM requirement r
            WHERE r.status IN ('active','proposed') AND r.priority='must'
            AND NOT EXISTS (SELECT 1 FROM scenario s WHERE s.requirement_id=r.id
              AND s.status='active' AND length(trim(s.when_md))>0 AND length(trim(s.then_md))>0)"""):
        blockers.append(f"must requirement without scenario: {r['key']} {r['title']}")
    for r in all_rows(con, """SELECT r.key FROM requirement r
            WHERE r.status IN ('active','proposed') AND r.priority='should'
            AND NOT EXISTS (SELECT 1 FROM scenario s WHERE s.requirement_id=r.id
              AND s.status='active' AND length(trim(s.when_md))>0 AND length(trim(s.then_md))>0)"""):
        warnings.append(f"should requirement without scenario: {r['key']}")
    for r in all_rows(con, """SELECT key,statement,kind FROM requirement
            WHERE status IN ('active','proposed') AND priority='must'"""):
        try:
            validate_requirement_statement(r["statement"], r["kind"])
        except ProjectError as exc:
            blockers.append(f"invalid must requirement {r['key']}: {exc}")
    for r in all_rows(con, """SELECT r.key FROM requirement r
            WHERE r.status IN ('active','proposed') AND r.priority='must'
            AND NOT EXISTS (SELECT 1 FROM issue_requirement ir WHERE ir.requirement_id=r.id)"""):
        warnings.append(f"must requirement without linked issue: {r['key']}")
    for r in all_rows(con, "SELECT key FROM assumption WHERE status='open' AND impact='high'"):
        blockers.append(f"open high-impact assumption: {r['key']}")
    skeleton = get_config_row(con, "project.walking_skeleton_issue", "")
    if skeleton:
        row = one(con, "SELECT key,status_key FROM v_issue WHERE key=?", (skeleton,))
        if not row:
            blockers.append(f"walking skeleton issue not found: {skeleton}")
        elif row["status_key"] in TERMINAL:
            warnings.append(f"walking skeleton issue is already {row['status_key']}: {skeleton}")
    else:
        warnings.append("no walking skeleton issue recorded (project.walking_skeleton_issue)")
    prd_reviews = all_rows(con, """SELECT id,subject,round,status,score,bar,must_fix
        FROM review WHERE scope='prd' ORDER BY round DESC, id DESC""")
    latest_prd = {}
    for r in prd_reviews:
        latest_prd.setdefault(r["subject"], r)
    if not latest_prd:
        blockers.append("no PRD review recorded; run `review add --scope prd ...` then resolve it")
    for r in latest_prd.values():
        if r["status"] != "addressed" or r["score"] is None or r["score"] < r["bar"] or (r["must_fix"] or "").strip():
            blockers.append(f"latest PRD review not passing (#{r['id']} {r['subject']}): "
                            f"{r['status']} {r['score'] or '?'}/{r['bar']} {r['must_fix']}; "
                            "resolve it with `review resolve <id> --status addressed --commit <sha>`")
        bound = one(con, "SELECT prd_version FROM review_prd_version WHERE review_id=?", (r["id"],))
        if not bound or int(bound["prd_version"]) != int(doc["version"]):
            blockers.append(f"PRD review #{r['id']} ({r['subject']}) predates the current PRD "
                            f"version {doc['version']}; add and resolve a fresh review")
    for r in all_rows(con, "SELECT key,impact,revisit_trigger FROM assumption WHERE status='deferred'"):
        if not (r["revisit_trigger"] or "").strip():
            if r["impact"] == "high":
                blockers.append(f"high-impact deferred assumption without revisit trigger: {r['key']}")
            else:
                warnings.append(f"deferred assumption without revisit trigger: {r['key']}")
    for r in all_rows(con, "SELECT id,question,why FROM prd_question WHERE status='deferred' AND impact='high'"):
        if not (r["why"] or "").strip():
            blockers.append(f"high-impact question deferred without a reason/revisit note: #{r['id']} {r['question']}")
    validation, note = prd_validation(con)
    if validation == "unvalidated":
        warnings.append("PRD is v0.1 unvalidated" + (f": {note}" if note else ""))
    if doc["status"] == "signed_off":
        draft = all_rows(con, "SELECT key FROM prd_section WHERE status='draft'")
        if draft:
            warnings.append("signed-off PRD has draft sections: " + ", ".join(r["key"] for r in draft))
    return blockers, warnings


# -------------------------------------------------------------------- export --
def render_prd(con) -> str:
    doc = rowdict(one(con, "SELECT * FROM prd_document WHERE id=1"))
    sections = all_rows(con, "SELECT * FROM prd_section ORDER BY position,id")
    reqs = all_rows(con, "SELECT * FROM requirement ORDER BY key")
    by_parent: dict[int | None, list[dict]] = {}
    for s in sections:
        by_parent.setdefault(s["parent_id"], []).append(s)
    for items in by_parent.values():
        items.sort(key=lambda s: (s["position"], s["id"]))
    req_by_section: dict[int | None, list[dict]] = {}
    for r in reqs:
        req_by_section.setdefault(r["section_id"], []).append(r)

    validation, vnote = prd_validation(con)
    validation_line = ""
    if validation == "unvalidated":
        validation_line = " · **Validation:** v0.1 UNVALIDATED" + (f" — {vnote}" if vnote else "")
    out = [f"# {doc['title']}", "",
           f"> **Status:** {doc['status']} · **Version:** {doc['version']}"
           + (f" · **Signed off:** {doc['signed_off_at']} by {doc['signed_off_by']}"
              if doc["status"] == "signed_off" and doc.get("signed_off_at") else "")
           + validation_line,
           ">", f"> {GENERATED_MARKER}", ""]
    if doc.get("summary"):
        out += [doc["summary"], ""]

    # Table of contents (all sections, flat with indentation).
    def walk(parent, depth):
        for s in by_parent.get(parent, []):
            out.append("  " * depth + f"- [{s['title']}](#{s['key']})")
            walk(s["id"], depth + 1)
    out.append("## Contents")
    walk(None, 0)
    out.append("")

    counter = [0]

    def section_block(s, depth):
        nonlocal out
        counter[0] += 1
        out.extend(["", f"<a id=\"{s['key']}\"></a>", "",
                    "#" * min(2 + depth, 6) + f" {s['title']}", ""])
        body = (s["body_md"] or "").strip()
        out.append(body if body else "_Not yet written._")
        section_reqs = req_by_section.get(s["id"], [])
        if section_reqs:
            out += ["", f"**Requirements:** {len(section_reqs)}", ""]
            out += ["| ID | Priority | Status | Statement |", "|---|---|---|---|"]
            for r in section_reqs:
                stmt = (r["statement"] or "").replace("|", "\\|").replace("\n", " ")
                out.append(f"| `{r['key']}` | {r['priority']} | {r['status']} | {stmt} |")
        for child in by_parent.get(s["id"], []):
            section_block(child, depth + 1)

    for s in by_parent.get(None, []):
        section_block(s, 0)

    # Requirements without a section and the full requirement detail.
    loose = req_by_section.get(None, [])
    if loose:
        out += ["", "## Requirements without a section", ""]
        for r in loose:
            out.append(f"- `{r['key']}` — {r['title']}")
    out += ["", "## Requirement details", ""]
    features_dir = repo_config().get("paths", {}).get("features", "tests/features")
    for r in reqs:
        out += [f"### `{r['key']}` — {r['title']}", "",
                f"- **Kind:** {r['kind']} · **Priority:** {r['priority']} · **Status:** {r['status']}",
                f"- **Statement:** {r['statement']}"]
        if r.get("rationale"):
            out += [f"- **Rationale:** {r['rationale']}"]
        issues = all_rows(con, "SELECT i.key,i.title FROM issue_requirement ir JOIN issue i ON i.id=ir.issue_id WHERE ir.requirement_id=? ORDER BY i.key", (r["id"],))
        if issues:
            out.append("- **Issues:** " + ", ".join(f"`{i['key']}`" for i in issues))
        scenarios = all_rows(con, "SELECT key,title,status FROM scenario WHERE requirement_id=? ORDER BY position,id", (r["id"],))
        if scenarios:
            out.append(f"- **Acceptance scenarios:** {len(scenarios)} in `{features_dir}/{r['key']}.feature`")
        else:
            out.append("- **Acceptance scenarios:** _none yet_")
        out.append("")
    risks = all_rows(con, "SELECT key,title,likelihood,impact,status,mitigation,trigger,owner FROM risk ORDER BY key")
    if risks:
        out += ["", "## Risk register", "", "| ID | L | I | Status | Risk | Mitigation | Trigger | Owner |",
                "|---|---|---|---|---|---|---|---|"]
        for r in risks:
            out.append(f"| `{r['key']}` | {r['likelihood']} | {r['impact']} | {r['status']} | "
                       f"{r['title']} | {r['mitigation']} | {r['trigger']} | {r['owner']} |")
    assumptions = all_rows(con, "SELECT key,statement,confidence,impact,status,revisit_trigger FROM assumption ORDER BY key")
    if assumptions:
        out += ["", "## Assumptions", "", "| ID | Impact | Confidence | Status | Statement | Revisit trigger |",
                "|---|---|---|---|---|---|"]
        for a in assumptions:
            out.append(f"| `{a['key']}` | {a['impact']} | {a['confidence']} | {a['status']} | "
                       f"{a['statement']} | {a['revisit_trigger']} |")
    questions = all_rows(con, "SELECT id,impact,status,question,answer FROM prd_question ORDER BY id")
    if questions:
        out += ["", "## Question log", "", "| ID | Impact | Status | Question | Answer |", "|---|---|---|---|---|"]
        for q in questions:
            out.append(f"| {q['id']} | {q['impact']} | {q['status']} | {q['question']} | {q['answer']} |")
    return "\n".join(out).rstrip() + "\n"


def render_features(con) -> dict[str, str]:
    reqs = all_rows(con, "SELECT * FROM requirement WHERE status IN ('active','proposed') ORDER BY key")
    out = {}
    for r in reqs:
        scenarios = all_rows(con, "SELECT * FROM scenario WHERE requirement_id=? AND status='active' ORDER BY position,id", (r["id"],))
        if not scenarios:
            continue
        lines = [f"@{r['key']} @{r['priority']}",
                 f"Feature: {r['title']}",
                 f"  # {r['statement']}",
                 ""]
        for s in scenarios:
            tags = [f"@SCENARIO-{s['key']}"]
            if s.get("tags"):
                tags += [t if t.startswith("@") else "@" + t for t in re.split(r"[,\s]+", s["tags"]) if t]
            lines.append("  " + " ".join(tags))
            lines.append(f"  Scenario: {s['title']}")
            for keyword, field in (("Given", "given_md"), ("When", "when_md"), ("Then", "then_md")):
                body = (s.get(field) or "").strip()
                if not body:
                    continue
                parts = body.splitlines()
                first = re.sub(r"^(given|when|then|and|but)\s+", "", parts[0].strip(), count=1, flags=re.I)
                lines.append(f"    {keyword} {first}")
                for extra in parts[1:]:
                    lines.append(f"      {extra}" if extra.strip() else "")
            lines.append("")
        out[f"{r['key']}.feature"] = "\n".join(lines).rstrip() + "\n"
    return out


def render_status(con) -> str:
    counts = all_rows(con, "SELECT s.key, s.name, COUNT(i.id) n FROM status s "
                           "LEFT JOIN issue i ON i.status_id=s.id AND i.archived_at IS NULL "
                           "GROUP BY s.id ORDER BY s.position")
    blocked = one(con, "SELECT COUNT(*) n FROM v_blocked")["n"]
    ready = one(con, "SELECT COUNT(*) n FROM v_ready WHERE child_count=0")["n"]
    open_q = one(con, "SELECT COUNT(*) n FROM prd_question WHERE status='open'")["n"]
    high_q = one(con, "SELECT COUNT(*) n FROM prd_question WHERE status='open' AND impact='high'")["n"]
    open_rev = one(con, "SELECT COUNT(*) n FROM review WHERE status='open'")["n"]
    open_h = one(con, "SELECT COUNT(*) n FROM human_request WHERE status IN ('open','asked')")["n"]
    stale = one(con, "SELECT COUNT(*) n FROM v_stale_scores")["n"]
    doc = rowdict(one(con, "SELECT * FROM prd_document WHERE id=1"))
    last = rowdict(one(con, "SELECT * FROM v_last_session"))
    validation, vnote = prd_validation(con)
    vline = " · **UNVALIDATED v0.1**" + (f" — {vnote}" if vnote else "") if validation == "unvalidated" else ""
    out = [f"# Status — {project_name()}", "", f"> {GENERATED_MARKER}", "",
           f"**PRD:** {doc.get('status','?')} v{doc.get('version','?')}{vline}  ",
           f"**Blocked issues:** {blocked} · **Ready leaves:** {ready}  ",
           f"**Open PRD questions:** {open_q} ({high_q} high) · **Open reviews:** {open_rev} · **Human queue:** {open_h}  ",
           f"**Stale scores:** {stale}", ""]
    out += ["## Issues by status", "", "| Status | Count |", "|---|---|"]
    for c in counts:
        out.append(f"| {c['name']} | {c['n']} |")
    out += ["", "## Next actionable", "", "| Rank | Key | Title | Status | Bucket |", "|---|---|---|---|---|"]
    for r in plan_rows(con, limit=10, ready_only=True):
        out.append(f"| {r['topo_rank']} | `{r['key']}` | {r['title']} | {r['status_key']} | {r['effective_bucket']} |")
    if not plan_rows(con, limit=1, ready_only=True):
        out.append("| — | — | _nothing ready; run triage/research/spec_ | — | — |")
    qs = all_rows(con, "SELECT id,impact,question FROM prd_question WHERE status='open' ORDER BY CASE impact WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END,id LIMIT 10")
    if qs:
        out += ["", "## Open PRD questions", "", "| ID | Impact | Question |", "|---|---|---|"]
        for q in qs:
            out.append(f"| {q['id']} | {q['impact']} | {q['question']} |")
    revs = all_rows(con, "SELECT id,scope,subject,round,score,bar FROM review WHERE status='open' ORDER BY id LIMIT 10")
    if revs:
        out += ["", "## Open reviews", "", "| ID | Scope | Subject | Round | Score |", "|---|---|---|---|---|"]
        for r in revs:
            out.append(f"| {r['id']} | {r['scope']} | {r['subject']} | {r['round']} | {r['score'] or '?'}/{r['bar']} |")
    if last:
        out += ["", "## Last session", "",
                f"- Agent: {last.get('agent','')}",
                f"- Started: {last.get('started_at','')} · Ended: {last.get('ended_at','') or 'open'}",
                f"- Summary: {last.get('summary','') or '_none_'}",
                f"- Handoff: {last.get('handoff','') or '_none_'}"]
    return "\n".join(out).rstrip() + "\n"


def dump_tables(con) -> list[str]:
    rows = con.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetchall()
    return [r["name"] for r in rows if r["name"] != "work_queue"]


def render_dump(con) -> tuple[str, str]:
    tables = dump_tables(con)
    lines = []
    meta = {"schema_version": int(get_config_row(con, "schema.version", SCHEMA_VERSION) or SCHEMA_VERSION),
            "project": project_name(), "tables": tables}
    lines.append(json.dumps({"_meta": meta}, sort_keys=True, ensure_ascii=False))
    for t in tables:
        cols = [r["name"] for r in con.execute(f"PRAGMA table_info('{t}')").fetchall()]
        if not cols:
            continue
        try:
            rows = con.execute(f"SELECT * FROM '{t}' ORDER BY rowid").fetchall()
        except sqlite3.OperationalError:
            rows = con.execute(f"SELECT * FROM '{t}' ORDER BY {cols[0]}").fetchall()
        for r in rows:
            d = {c: r[c] for c in cols}
            if t == "config" and d.get("key", "") in (
                    "export.dump_sha256", "plan.computed_at", "plan.computed_version"):
                continue
            if t == "config":
                d.pop("updated_at", None)
            lines.append(json.dumps({"_table": t, "row": d}, sort_keys=True, ensure_ascii=False))
    text = "\n".join(lines) + "\n"
    return text, hashlib.sha256(text.encode()).hexdigest()


def atomic_write(path: Path, text: str, read_only: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(text)
    tmp.replace(path)
    if read_only:
        path.chmod(0o444)


@contextmanager
def generated_files(paths: list[Path], manifest: Path):
    """Remove stale generated files listed in the manifest after a successful write."""
    old = []
    if manifest.exists():
        try:
            old = json.loads(manifest.read_text()).get("files", [])
        except json.JSONDecodeError:
            old = []
    yield
    new = [str(p.relative_to(REPO_ROOT)) for p in paths]
    for rel in old:
        if rel not in new:
            p = REPO_ROOT / rel
            if p.exists():
                p.unlink()
    manifest.write_text(json.dumps({"files": new}, indent=2) + "\n")


def do_export(write_status: bool = True) -> dict:
    paths_cfg = repo_config().get("paths", {})
    prd_path = repo_path(paths_cfg.get("prd", "docs/PRD.md"))
    status_path = repo_path(paths_cfg.get("status", "docs/STATUS.md"))
    feat_dir = repo_path(paths_cfg.get("features", "tests/features"))
    dump_path = repo_path(paths_cfg.get("dump", "project/ledger/current.jsonl"))
    manifest = feat_dir / ".generated.json"
    plan_con = connect()
    try:
        ensure_plan(plan_con)
    finally:
        plan_con.close()
    con = connect()
    try:
        con.execute("BEGIN")
        prd = render_prd(con)
        features = render_features(con)
        status = render_status(con)
        atomic_write(prd_path, prd, read_only=True)
        if write_status:
            atomic_write(status_path, status, read_only=True)
        feat_dir.mkdir(parents=True, exist_ok=True)
        written = []
        for name, text in features.items():
            p = (feat_dir / name).resolve()
            if feat_dir.resolve() not in p.parents:
                raise ProjectError(f"unsafe generated feature path: {name}")
            atomic_write(p, text, read_only=True)
            written.append(p)
        # Remove stale generated features and manifest.
        old = []
        if manifest.exists():
            try:
                old = json.loads(manifest.read_text()).get("files", [])
            except json.JSONDecodeError:
                old = []
        new_rel = [str(p.relative_to(REPO_ROOT)) for p in written]
        for rel in old:
            if rel in new_rel:
                continue
            p = (REPO_ROOT / rel).resolve()
            if feat_dir.resolve() not in p.parents or p.suffix != ".feature":
                continue
            if p.exists():
                p.unlink()
        atomic_write(manifest, json.dumps({"files": new_rel}, indent=2) + "\n", read_only=True)
        dump_text, dump_hash = render_dump(con)
        atomic_write(dump_path, dump_text, read_only=True)
        atomic_write(dump_path.with_suffix(dump_path.suffix + ".sha256"), dump_hash + "\n", read_only=True)
        con.rollback()
        return {"prd": str(prd_path), "status": str(status_path),
                "features": new_rel, "dump": str(dump_path), "dump_sha256": dump_hash}
    finally:
        con.close()


def check_export() -> list[str]:
    cfg = repo_config().get("paths", {})
    con = connect()
    problems = []
    try:
        expected = {
            repo_path(cfg.get("prd", "docs/PRD.md")): render_prd(con),
            repo_path(cfg.get("status", "docs/STATUS.md")): render_status(con),
        }
        for p, text in expected.items():
            if not p.exists():
                problems.append(f"missing export: {p.relative_to(REPO_ROOT)}")
            elif p.read_text() != text:
                problems.append(f"stale export: {p.relative_to(REPO_ROOT)} (run `project export`)")
        feat_dir = repo_path(cfg.get("features", "tests/features"))
        expected_features = render_features(con)
        for name, text in expected_features.items():
            p = feat_dir / name
            if not p.exists() or p.read_text() != text:
                problems.append(f"stale feature: {p.relative_to(REPO_ROOT)}")
        manifest = feat_dir / ".generated.json"
        if manifest.exists():
            try:
                listed = json.loads(manifest.read_text()).get("files", [])
            except json.JSONDecodeError:
                listed = []
            for rel in listed:
                candidate = (REPO_ROOT / rel).resolve()
                if feat_dir.resolve() not in candidate.parents or candidate.suffix != ".feature":
                    problems.append(f"unsafe feature manifest entry: {rel}")
                elif candidate.name not in expected_features:
                    problems.append(f"stale generated feature (run `project export`): {rel}")
        for p in feat_dir.glob("*.feature"):
            if p.name not in expected_features:
                problems.append(f"orphan feature not produced by the DB (remove or adopt): {p.relative_to(REPO_ROOT)}")
        dump_path = repo_path(cfg.get("dump", "project/ledger/current.jsonl"))
        _, db_hash = render_dump(con)
        sidecar = dump_path.with_suffix(dump_path.suffix + ".sha256")
        if not dump_path.exists() or not sidecar.exists():
            problems.append("missing ledger dump (run `project export`)")
        else:
            expected = sidecar.read_text().strip()
            file_hash = hashlib.sha256(dump_path.read_bytes()).hexdigest()
            if expected != file_hash:
                problems.append("ledger dump file was modified outside `project export`")
            if expected != db_hash:
                problems.append("stale ledger dump (run `project export`)")
    finally:
        con.close()
    return problems
