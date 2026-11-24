"""Init, health, export, review, milestone, human queue, session, agent and test commands."""
from __future__ import annotations

import json
import os
import re
import shutil
import sqlite3
import subprocess
import sys

try:
    from . import core
except ImportError:
    import core


# ---------------------------------------------------------------------- init --
def cmd_init(a):
    if (core.REPO_ROOT / ".git").is_file():
        raise core.ProjectError("run `project init` from the main checkout, not from a worktree")
    cfg = core.ws_config()
    existing_repo = cfg.get("repo", "")
    if existing_repo:
        existing = core.Path(existing_repo).resolve()
        wts = core.Path(cfg.get("worktrees_dir", "")).resolve() if cfg.get("worktrees_dir") else None
        if existing != core.REPO_ROOT.resolve() and not (wts and (core.REPO_ROOT == wts or wts in core.REPO_ROOT.parents)):
            raise core.ProjectError(f"workspace belongs to {existing_repo}; use a separate workspace")
    dump = core.repo_path(core.repo_config().get("paths", {}).get("dump", "project/ledger/current.jsonl"))
    if not core.db_path().exists() and dump.exists() and not getattr(a, "from_dump", False):
        raise core.ProjectError(f"database is missing but the history dump exists at {dump}; "
                                "run `project rebuild --yes` (preserves history), not init")
    core.db_path().parent.mkdir(parents=True, exist_ok=True)
    cfg = core.ws_config()
    cfg.setdefault("version", 1)
    cfg.setdefault("workspace", str(core.workspace_root()))
    cfg.setdefault("repo", str(core.REPO_ROOT))
    cfg.setdefault("db_path", str(core.db_path()))
    cfg.setdefault("worktrees_dir", str(core.workspace_root() / "worktrees"))
    core.save_ws_config(cfg)

    def fn(con):
        core.apply_schema(con)
        rc = core.repo_config()
        core.set_config_row(con, "project.name", rc["project"]["name"])
        core.set_config_row(con, "project.prefix", rc["project"]["prefix"])
        core.set_config_row(con, "project.kind", rc["project"].get("kind", "generic"))
        core.set_config_row(con, "workflow.review_bar", rc["workflow"].get("review_bar", 8))
        core.set_config_row(con, "workflow.ceremony_default", rc["workflow"].get("ceremony_default", "standard"))
        core.set_config_row(con, "workflow.wip_limit", rc["workflow"].get("wip_limit", 2))
        core.set_config_row(con, "workflow.score_ttl_days", rc["workflow"].get("score_ttl_days", 14))
        core.set_config_row(con, "prd.required_sections", json.dumps(core.REQUIRED_SECTIONS))
        if core.get_config_row(con, "plan.graph_version", "") == "":
            core.set_config_row(con, "plan.graph_version", 0)
        con.execute("UPDATE prd_document SET title=? WHERE id=1 AND title='Untitled project'",
                    (rc["project"]["name"],))
        return {"db": str(core.db_path()), "schema_version": core.get_config_row(con, "schema.version", "1")}

    result = core.write(fn)
    if a.from_dump or (a.dump and core.Path(a.dump).exists()):
        result["imported"] = cmd_rebuild(argparse_namespace(dump=a.dump or None, yes=True, init=False, quiet=True))
    core.emit(result, a.json)


class argparse_namespace:
    def __init__(self, **kw):
        self.__dict__.update(kw)


def cmd_rebuild(a):
    if (core.REPO_ROOT / ".git").is_file():
        raise core.ProjectError("run `project rebuild --yes` from the main checkout, not from a worktree")
    dump_path = core.Path(a.dump) if getattr(a, "dump", None) else core.repo_path(
        core.repo_config().get("paths", {}).get("dump", "project/ledger/current.jsonl"))
    if not dump_path.exists():
        raise core.ProjectError(f"dump not found: {dump_path}")
    if not getattr(a, "yes", False):
        raise core.ProjectError("rebuild replaces the database; pass --yes")
    if getattr(a, "force", False) and not getattr(a, "reason", ""):
        raise core.ProjectError("--force requires --reason")
    dump_bytes = dump_path.read_bytes()
    actual = __import__("hashlib").sha256(dump_bytes).hexdigest()
    sidecar = dump_path.with_suffix(dump_path.suffix + ".sha256")
    if not sidecar.exists():
        if not getattr(a, "force", False):
            raise core.ProjectError("dump hash sidecar is missing; refusing to rebuild "
                                    "(use --force --reason to accept an unverified dump)")
    else:
        expected = sidecar.read_text().strip()
        if expected != actual and not getattr(a, "force", False):
            raise core.ProjectError("dump hash does not match current.jsonl.sha256; refusing to rebuild "
                                    "tampered history (use --force --reason to override)")
    try:
        lines = [json.loads(line) for line in dump_path.read_text().splitlines() if line.strip()]
    except json.JSONDecodeError as exc:
        raise core.ProjectError(f"invalid JSON in dump: {exc}")
    if not lines or "_meta" not in lines[0]:
        raise core.ProjectError("invalid dump: missing _meta line")
    meta = lines[0]["_meta"]
    rows = [line for line in lines[1:] if "_table" in line]
    # Build a fresh database in a temp file; replace the live DB only on success.
    db = core.db_path()
    db.parent.mkdir(parents=True, exist_ok=True)
    if db.exists():
        backups = db.parent / "backups"
        backups.mkdir(parents=True, exist_ok=True)
        stamp = __import__("datetime").datetime.now().strftime("%Y%m%d-%H%M%S")
        backup = backups / f"project-{stamp}.db"
        try:
            src = sqlite3.connect(db)
            dst = sqlite3.connect(backup)
            src.backup(dst)
            dst.close()
            src.close()
        except sqlite3.Error:
            shutil.copy2(db, backup)
    tmp = db.with_suffix(db.suffix + ".rebuild")
    for pth in (tmp, core.Path(str(tmp) + "-wal"), core.Path(str(tmp) + "-shm")):
        if pth.exists():
            pth.unlink()
    imported = 0
    try:
        con = sqlite3.connect(tmp)
        con.row_factory = sqlite3.Row
        con.execute("PRAGMA foreign_keys=OFF")
        core.apply_schema(con)
        con.commit()
        con.execute("PRAGMA foreign_keys=OFF")
        tables = {r["name"] for r in con.execute(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").fetchall()}
        for entry in rows:
            t = entry["_table"]
            if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", t):
                raise core.ProjectError(f"invalid table name in dump: {t!r}")
            if t not in tables:
                raise core.ProjectError(f"unknown table in dump: {t!r}")
            row = entry["row"]
            cols = list(row)
            for c in cols:
                if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", c):
                    raise core.ProjectError(f"invalid column name in dump: {c!r}")
            sql = f"INSERT OR REPLACE INTO '{t}' ({','.join('\"'+c+'\"' for c in cols)}) VALUES ({','.join('?' for _ in cols)})"
            con.execute(sql, [row[c] for c in cols])
            imported += 1
        con.commit()
        con.execute("PRAGMA foreign_keys=ON")
        core.set_config_row(con, "schema.version", max(int(meta.get("schema_version") or 1),
                                                       int(core.get_config_row(con, "schema.version", "1") or "1")))
        bad = con.execute("PRAGMA foreign_key_check").fetchall()
        if bad:
            raise core.ProjectError(f"dump import left {len(bad)} foreign-key violations")
        core.set_config_row(con, "plan.computed_version", -1)
        con.commit()
        con.close()
    except Exception:
        try:
            con.close()
        except Exception:
            pass
        for pth in (tmp, core.Path(str(tmp) + "-wal"), core.Path(str(tmp) + "-shm")):
            if pth.exists():
                pth.unlink()
        raise
    for suffix in ("-wal", "-shm"):
        pth = core.Path(str(db) + suffix)
        if pth.exists():
            pth.unlink()
    os.replace(tmp, db)
    if not core.ws_config_path().exists():
        cfg = core.ws_config()
        cfg.setdefault("version", 1)
        cfg.setdefault("workspace", str(core.REPO_ROOT))
        cfg.setdefault("repo", str(core.REPO_ROOT))
        cfg.setdefault("db_path", str(db))
        cfg.setdefault("worktrees_dir", str(core.REPO_ROOT / "worktrees"))
        core.save_ws_config(cfg)
    core.write(lambda c: core.compute_plan(c))
    if getattr(a, "force", False):
        def mark(con):
            core.audit(con, "config", 0, "rebuild", "dump_hash", None, actual,
                       reason=getattr(a, "reason", "") or "forced rebuild", actor=a.actor)
        core.write(mark)
    result = {"db": str(db), "rows": imported, "schema_version": meta.get("schema_version"),
              "plan_recomputed": True,
              "hash_verified": bool(sidecar.exists() and sidecar.read_text().strip() == actual)}
    if not getattr(a, "quiet", False):
        core.emit(result, a.json)
    return result


def cmd_stack_set(a):
    core.require_db()
    if not (a.stack or "").strip():
        raise core.ProjectError("stack cannot be empty")
    cfg = core.repo_config()
    cfg.setdefault("project", {})["stack"] = a.stack
    core.save_repo_config(cfg)
    files = [core.REPO_ROOT / "README.md", core.REPO_ROOT / "docs" / "ARCHITECTURE.md",
             core.REPO_ROOT / "docs" / "ai" / "ENVIRONMENT.md",
             core.REPO_ROOT / "docs" / "ai" / "CODING.md",
             core.REPO_ROOT / "docs" / "ai" / "PRESET.md"]
    changed = []
    for p in files:
        if not p.exists():
            continue
        text = p.read_text()
        if "TODO: selected stack" in text:
            p.write_text(text.replace("TODO: selected stack", a.stack))
            changed.append(str(p.relative_to(core.REPO_ROOT)))
    def fn(con):
        core.set_config_row(con, "project.stack", a.stack)
        core.audit(con, "config", 0, "set", "project.stack", None, a.stack, actor=a.actor)
        return {"stack": a.stack, "changed": changed}
    result = core.write(fn)
    core.do_export(write_status=True)
    result["exported"] = True
    core.emit(result, a.json)


# -------------------------------------------------------------------- health --
def cmd_status(a):
    core.require_db()
    con = core.connect()
    try:
        core.ensure_plan(con)
        doc = core.rowdict(core.one(con, "SELECT * FROM prd_document WHERE id=1"))
        counts = core.all_rows(con, """SELECT s.key, COUNT(i.id) n FROM status s
            LEFT JOIN issue i ON i.status_id=s.id AND i.archived_at IS NULL GROUP BY s.id ORDER BY s.position""")
        blocked = core.one(con, "SELECT COUNT(*) n FROM v_blocked")["n"]
        questions = core.one(con, "SELECT COUNT(*) n FROM prd_question WHERE status='open'")["n"]
        high_q = core.one(con, "SELECT COUNT(*) n FROM prd_question WHERE status='open' AND impact='high'")["n"]
        reviews = core.one(con, "SELECT COUNT(*) n FROM review WHERE status='open'")["n"]
        human = core.one(con, "SELECT COUNT(*) n FROM human_request WHERE status IN ('open','asked')")["n"]
        last = core.rowdict(core.one(con, "SELECT * FROM v_last_session"))
        next_rows = core.plan_rows(con, limit=a.limit, ready_only=True)
        validation, _ = core.prd_validation(con)
    finally:
        con.close()
    if a.json:
        core.emit({"prd": doc, "prd_validation": validation, "counts": counts, "blocked": blocked,
                   "open_questions": questions, "high_questions": high_q, "open_reviews": reviews,
                   "human_queue": human, "last_session": last, "next": next_rows}, True)
        return
    print(f"{core.project_name()}  [{doc['status']} PRD v{doc['version']}]" +
          ("  UNVALIDATED v0.1" if validation == "unvalidated" else ""))
    print(f"blocked={blocked}  open questions={questions} ({high_q} high)  open reviews={reviews}  human queue={human}")
    print("status: " + "  ".join(f"{c['key']}={c['n']}" for c in counts))
    if last:
        print(f"last session: {last.get('ended_at') or last.get('started_at')} — {last.get('summary') or 'open'}")
    print("\nnext:")
    core.table(next_rows, ["topo_rank", "key", "effective_bucket", "title"], "  nothing ready — run triage/research/spec")


def cmd_doctor(a):
    core.require_db()
    con = core.connect()
    errors, warnings = [], []
    try:
        ok = con.execute("PRAGMA integrity_check").fetchone()[0]
        if ok != "ok":
            errors.append(f"sqlite integrity_check: {ok}")
        fk = con.execute("PRAGMA foreign_key_check").fetchall()
        if fk:
            errors.append(f"{len(fk)} foreign-key violation(s)")
        # Hierarchy cycles.
        cycles = core.all_rows(con, """WITH RECURSIVE up(start,id,depth) AS (
                SELECT id,parent_id,1 FROM issue WHERE parent_id IS NOT NULL
                UNION ALL SELECT up.start,i.parent_id,up.depth+1 FROM issue i
                JOIN up ON i.id=up.id WHERE i.parent_id IS NOT NULL AND up.depth<100)
            SELECT DISTINCT start FROM up WHERE id=start""")
        for c in cycles:
            errors.append(f"issue hierarchy cycle involving id {c['start']}")
        # Blocking cycles.
        bcycles = core.all_rows(con, """WITH RECURSIVE reach(start,id) AS (
                SELECT src_id,dst_id FROM issue_link WHERE type='blocks'
                UNION SELECT r.start,l.dst_id FROM issue_link l JOIN reach r ON l.src_id=r.id WHERE l.type='blocks')
            SELECT DISTINCT start FROM reach WHERE start=id""")
        for c in bcycles:
            errors.append(f"blocking cycle involving issue id {c['start']}")
        # Gates that should have been satisfied.
        for r in core.all_rows(con, """SELECT key FROM v_issue WHERE archived_at IS NULL
                AND status_key IN ('implementing','review','testing','merging') AND assignee=''"""):
            warnings.append(f"{r['key']}: active but unassigned")
        for r in core.all_rows(con, """SELECT key FROM v_issue WHERE archived_at IS NULL
                AND status_key IN ('implementing','review','testing','merging') AND branch_name=''"""):
            warnings.append(f"{r['key']}: active but no branch_name")
        for r in core.all_rows(con, "SELECT key FROM v_issue WHERE is_blocked=1 AND status_key='ready'"):
            warnings.append(f"{r['key']}: ready but blocked")
        for r in core.all_rows(con, "SELECT key,worktree_path FROM issue WHERE worktree_path<>'' AND archived_at IS NULL"):
            p = core.Path(r["worktree_path"])
            p = p if p.is_absolute() else core.workspace_root() / p
            if not p.exists():
                warnings.append(f"{r['key']}: recorded worktree is missing ({r['worktree_path']}); run `worktree recover`")
        if not (core.repo_path("scripts/check.sh")).exists():
            warnings.append("scripts/check.sh is missing; worktree finish will refuse to merge")
        # Requirements.
        for r in core.all_rows(con, """SELECT key FROM requirement WHERE status IN ('active','proposed')
                AND priority='must' AND NOT EXISTS (SELECT 1 FROM scenario s WHERE s.requirement_id=requirement.id
                AND s.status='active' AND length(trim(s.when_md))>0 AND length(trim(s.then_md))>0)"""):
            warnings.append(f"must requirement without active scenario: {r['key']}")
        for r in core.all_rows(con, """SELECT key FROM requirement WHERE status IN ('active','proposed')
                AND priority='must' AND NOT EXISTS (SELECT 1 FROM issue_requirement ir WHERE ir.requirement_id=requirement.id)"""):
            warnings.append(f"must requirement without linked issue: {r['key']}")
        for r in core.all_rows(con, "SELECT id,question FROM prd_question WHERE status='open' AND impact='high'"):
            warnings.append(f"open high-impact PRD question #{r['id']}: {r['question']}")
        for r in core.all_rows(con, "SELECT key FROM assumption WHERE status='open' AND impact='high'"):
            warnings.append(f"open high-impact assumption: {r['key']}")
        stale = core.one(con, "SELECT COUNT(*) n FROM v_stale_scores")["n"]
        if stale:
            warnings.append(f"{stale} issue(s) have stale or missing scores")
        reviews = core.all_rows(con, "SELECT id,subject,round FROM review WHERE status='open'")
        for r in reviews:
            warnings.append(f"open review #{r['id']} ({r['subject']}, round {r['round']})")
        human = core.one(con, "SELECT COUNT(*) n FROM human_request WHERE status IN ('open','asked')")["n"]
        if human:
            warnings.append(f"{human} unanswered human request(s)")
        plan_version = core.get_config_row(con, "plan.graph_version", "0")
        computed = core.get_config_row(con, "plan.computed_version", "-1")
        if plan_version != computed:
            warnings.append("implementation plan is stale (run `project plan --refresh`)")
        doc = core.one(con, "SELECT status FROM prd_document WHERE id=1")
        if doc["status"] != "signed_off":
            warnings.append(f"PRD is {doc['status']}, not signed_off")
        for m in core.all_rows(con, "SELECT key FROM milestone WHERE status='completed' AND length(trim(evidence))=0"):
            errors.append(f"milestone completed without evidence: {m['key']}")
        for m in core.all_rows(con, "SELECT key,review_bar FROM milestone WHERE status='completed'"):
            latest = core.latest_review(con, "milestone", m["key"])
            milestone_review_ok = bool(
                latest and latest.get("status") == "addressed"
                and latest.get("score") is not None
                and latest["score"] >= m["review_bar"]
                and not (latest.get("must_fix") or "").strip()
            )
            if not milestone_review_ok:
                forced = core.get_config_row(con, f"milestone.forced.{m['key']}", "")
                if forced:
                    warnings.append(f"milestone {m['key']} completed by forced exception: {forced}")
                else:
                    errors.append(f"milestone completed without a latest review at/above bar: {m['key']}")
        stack_todo = []
        for p in [core.REPO_ROOT / "README.md", core.REPO_ROOT / "docs" / "ai" / "ENVIRONMENT.md",
                  core.REPO_ROOT / "docs" / "ai" / "CODING.md", core.REPO_ROOT / "docs" / "ai" / "PRESET.md"]:
            if p.exists() and "TODO: selected stack" in p.read_text():
                stack_todo.append(str(p.relative_to(core.REPO_ROOT)))
        if stack_todo:
            warnings.append("stack is not set; run `project stack set <stack>` (files: " + ", ".join(stack_todo) + ")")
        # Export drift is checked outside a read transaction.
    finally:
        con.close()
    if a.prd:
        con = core.connect()
        try:
            blockers, prd_warn = core.prd_blockers(con)
            doc_status = core.one(con, "SELECT status FROM prd_document WHERE id=1")["status"]
            validation, _ = core.prd_validation(con)
        finally:
            con.close()
        if doc_status == "signed_off" and validation != "unvalidated":
            errors.extend(blockers)
        else:
            warnings.extend(blockers)
        warnings.extend(prd_warn)
    if not a.no_export:
        problems = core.check_export()
        if a.strict:
            errors.extend(problems)
        else:
            warnings.extend(problems)
    errors = list(dict.fromkeys(errors))
    warnings = list(dict.fromkeys(warnings))
    if a.json:
        core.emit({"errors": errors, "warnings": warnings, "ok": not errors}, True)
    else:
        print(f"doctor: {len(errors)} error(s), {len(warnings)} warning(s)")
        for e in errors:
            print(f"  ERROR  {e}")
        for w in warnings:
            print(f"  warn   {w}")
        if not errors:
            print("OK")
    if errors and a.strict:
        raise SystemExit(1)


def cmd_export(a):
    core.require_db()
    if a.check:
        problems = core.check_export()
        if a.json:
            core.emit({"ok": not problems, "problems": problems}, True)
        else:
            if problems:
                print("export check failed:")
                for p in problems:
                    print(f"  - {p}")
            else:
                print("export check OK")
        if problems:
            raise SystemExit(1)
        return
    result = core.do_export(write_status=True)
    core.emit(result, a.json)


def cmd_report(a):
    core.require_db()
    result = core.do_export(write_status=True)
    core.emit(result, a.json)


def cmd_sql(a):
    core.require_db()
    q = a.query.strip()
    if not q.lower().startswith(("select", "with", "explain")):
        raise core.ProjectError("only read-only SELECT/WITH/EXPLAIN queries are allowed")
    con = core.connect()
    try:
        con.execute("PRAGMA query_only=ON")
        rows = core.all_rows(con, q)
    finally:
        con.close()
    if a.json:
        core.emit(rows, True)
    else:
        core.table(rows, list(rows[0].keys()) if rows else ["result"], "no rows")


def cmd_config(a):
    core.require_db()
    if a.action == "get":
        con = core.connect()
        try:
            if a.key:
                value = core.get_config_row(con, a.key, "")
                core.emit({"key": a.key, "value": value}, a.json) if a.json else print(value)
            else:
                rows = core.all_rows(con, "SELECT key,value,updated_at FROM config ORDER BY key")
                core.emit(rows, a.json) if a.json else core.table(rows, ["key", "value", "updated_at"])
        finally:
            con.close()
    else:
        protected = ("prd.validation", "prd.validation_note", "prd.reviewed_version",
                     "schema.version", "plan.", "milestone.forced.")
        if a.key.startswith(protected):
            raise core.ProjectError("protected config key; use the dedicated command/decision path")
        if a.key in ("workflow.review_bar",):
            if not a.value.isdigit() or not (1 <= int(a.value) <= 10):
                raise core.ProjectError("workflow.review_bar must be an integer 1-10")
        if a.key in ("workflow.wip_limit", "workflow.score_ttl_days"):
            if not a.value.isdigit():
                raise core.ProjectError(f"{a.key} must be an integer")
        if a.key == "workflow.ceremony_default" and a.value not in ("light", "standard", "full"):
            raise core.ProjectError("workflow.ceremony_default must be light, standard or full")
        def fn(con):
            core.set_config_row(con, a.key, a.value)
            core.audit(con, "config", 0, "set", a.key, None, a.value, actor=a.actor)
            return {"key": a.key, "value": a.value}
        core.emit(core.write(fn), a.json)


# ------------------------------------------------------------------- reviews --
def cmd_review_add(a):
    core.require_db()

    def fn(con):
        bar = a.bar
        if bar is None:
            bar = int(core.get_config_row(con, "workflow.review_bar",
                                          core.repo_config().get("workflow", {}).get("review_bar", 8)) or 8)
        issue_id = core.get_issue(con, a.issue)["id"] if a.issue else None
        milestone_id = None
        if a.milestone:
            m = core.one(con, "SELECT id FROM milestone WHERE key=?", (a.milestone,))
            if not m:
                raise core.ProjectError(f"milestone not found: {a.milestone}")
            milestone_id = m["id"]
        max_round = core.one(con, "SELECT COALESCE(MAX(round),0) n FROM review WHERE scope=? AND subject=?",
                             (a.scope, a.subject))["n"]
        rnd = a.round
        if rnd is None:
            rnd = max_round + 1
        elif rnd <= max_round:
            raise core.ProjectError(f"review round must increase (latest is {max_round})")
        cur = con.execute(
            "INSERT INTO review(scope,subject,round,score,bar,rubric_json,summary,must_fix,reviewer,"
            "commit_sha,issue_id,milestone_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
            (a.scope, a.subject, rnd, a.score, bar, a.rubric or "{}", a.summary or "",
             a.must_fix or "", a.reviewer or "", a.commit or "", issue_id, milestone_id))
        core.audit(con, "review", cur.lastrowid, "create", "", None,
                   {"scope": a.scope, "subject": a.subject, "round": rnd, "score": a.score}, actor=a.actor)
        if a.scope == "prd":
            doc = core.one(con, "SELECT version FROM prd_document WHERE id=1")
            con.execute("INSERT OR REPLACE INTO review_prd_version(review_id,prd_version) VALUES(?,?)",
                        (cur.lastrowid, doc["version"]))
        return {"id": cur.lastrowid, "scope": a.scope, "subject": a.subject, "round": rnd,
                "score": a.score, "bar": bar}

    core.emit(core.write(fn), a.json)


def cmd_review_list(a):
    core.require_db()
    con = core.connect()
    try:
        sql = "SELECT * FROM review"
        if a.open:
            sql += " WHERE status='open'"
        sql += " ORDER BY id DESC"
        rows = core.all_rows(con, sql)
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["id", "scope", "subject", "round", "score", "bar", "status", "summary"])


def cmd_review_resolve(a):
    core.require_db()

    def fn(con):
        row = core.one(con, "SELECT * FROM review WHERE id=?", (a.id,))
        if not row:
            raise core.ProjectError(f"review not found: {a.id}")
        if a.status == "addressed":
            if row["score"] is None or row["score"] < row["bar"] or (row["must_fix"] or "").strip():
                raise core.ProjectError("cannot mark a below-bar review addressed; fix it and add a "
                                        "new review round at/above the bar")
            if not a.commit or not re.fullmatch(r"[0-9a-fA-F]{7,40}", a.commit):
                raise core.ProjectError("addressed review requires a real --commit <sha> (7-40 hex)")
        if a.status == "wontfix" and not (a.reason or "").strip():
            raise core.ProjectError("wontfix requires --reason (and ideally a decision/issue)")
        con.execute("UPDATE review SET status=?, resolved_at=datetime('now'), commit_sha=? WHERE id=?",
                    (a.status, a.commit or row["commit_sha"], a.id))
        core.audit(con, "review", a.id, "resolve", "status", row["status"], a.status,
                   reason=a.reason or "", actor=a.actor)
        # Do not update the review's recorded PRD version on resolve: a review
        # written before a revision stays stale by design.
        return {"id": a.id, "status": a.status}

    core.emit(core.write(fn), a.json)


# ---------------------------------------------------------------- milestones --
def cmd_milestone_add(a):
    core.require_db()

    def fn(con):
        key = a.key or core.slugify(a.name)
        if core.one(con, "SELECT 1 FROM milestone WHERE key=?", (key,)):
            raise core.ProjectError(f"milestone already exists: {key}")
        pos = core.one(con, "SELECT COALESCE(MAX(position),0)+10 p FROM milestone")["p"]
        cur = con.execute(
            "INSERT INTO milestone(key,name,description,target_date,definition_of_done,review_bar,position) "
            "VALUES(?,?,?,?,?,?,?)",
            (key, a.name, a.description or "", a.target or "", a.dod or "", a.bar, pos))
        core.audit(con, "milestone", cur.lastrowid, "create", "", None, {"key": key, "name": a.name}, actor=a.actor)
        return {"key": key, "id": cur.lastrowid}

    core.emit(core.write(fn), a.json)


def cmd_milestone_list(a):
    core.require_db()
    con = core.connect()
    try:
        rows = core.all_rows(con, """SELECT m.*,
            (SELECT COUNT(*) FROM issue i WHERE i.milestone_id=m.id AND i.archived_at IS NULL) issues,
            (SELECT COUNT(*) FROM issue i JOIN status s ON s.id=i.status_id
                WHERE i.milestone_id=m.id AND s.is_terminal=0 AND i.archived_at IS NULL) open_issues
            FROM milestone m ORDER BY m.position,m.id""")
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["key", "status", "target_date", "review_bar", "issues", "open_issues", "name"])


def cmd_milestone_update(a):
    core.require_db()

    def fn(con):
        row = core.one(con, "SELECT * FROM milestone WHERE key=?", (a.key,))
        if not row:
            raise core.ProjectError(f"milestone not found: {a.key}")
        updates = {}
        for field, val in (("status", a.status), ("evidence", a.evidence), ("definition_of_done", a.dod),
                           ("target_date", a.target), ("name", a.name)):
            if val is not None:
                updates[field] = val
        if updates.get("status") == "completed":
            if a.force and not a.reason:
                raise core.ProjectError("--force requires --reason")
            evidence = updates.get("evidence", row["evidence"])
            if not (evidence or "").strip():
                raise core.ProjectError("completed milestone requires --evidence")
            latest = core.latest_review(con, "milestone", row["key"])
            milestone_review_ok = bool(
                latest and latest.get("status") == "addressed"
                and latest.get("score") is not None
                and latest["score"] >= row["review_bar"]
                and not (latest.get("must_fix") or "").strip()
            )
            if not milestone_review_ok and not a.force:
                raise core.ProjectError(f"completed milestone requires a latest review at/above "
                                        f"{row['review_bar']} with no must-fix")
            if a.force:
                core.set_config_row(con, f"milestone.forced.{row['key']}", a.reason)
        if not updates:
            raise core.ProjectError("nothing to change")
        sets = ", ".join(f"{k}=?" for k in updates)
        con.execute(f"UPDATE milestone SET {sets}, updated_at=datetime('now') WHERE id=?",
                    (*updates.values(), row["id"]))
        for f, v in updates.items():
            core.audit(con, "milestone", row["id"], "update", f, row[f], v,
                       reason=getattr(a, "reason", "") or "", actor=a.actor)
        return {"key": a.key, "changed": sorted(updates)}

    core.emit(core.write(fn), a.json)


# --------------------------------------------------------------- human queue --
def cmd_human_add(a):
    core.require_db()

    def fn(con):
        issue_id = core.get_issue(con, a.issue)["id"] if a.issue else None
        options = a.options or "[]"
        try:
            json.loads(options)
        except json.JSONDecodeError:
            raise core.ProjectError("--options must be a JSON array")
        cur = con.execute(
            "INSERT INTO human_request(kind,issue_id,title,context_md,how_to,options_json) VALUES(?,?,?,?,?,?)",
            (a.kind, issue_id, a.title, a.context or "", a.how_to or "", options))
        core.audit(con, "human_request", cur.lastrowid, "create", "", None,
                   {"title": a.title, "kind": a.kind}, actor=a.actor)
        return {"id": cur.lastrowid, "title": a.title, "kind": a.kind}

    core.emit(core.write(fn), a.json)


def cmd_human_list(a):
    core.require_db()
    con = core.connect()
    try:
        sql = """SELECT h.id,h.kind,h.status,h.issue_id,i.key AS issue_key,h.title,
                        h.asked_at,h.answered_at FROM human_request h
                 LEFT JOIN issue i ON i.id=h.issue_id"""
        if a.open:
            sql += " WHERE h.status IN ('open','asked')"
        sql += " ORDER BY h.id"
        rows = core.all_rows(con, sql)
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["id", "kind", "status", "issue_key", "title"])


def cmd_human_show(a):
    core.require_db()
    con = core.connect()
    try:
        row = core.one(con, "SELECT * FROM human_request WHERE id=?", (a.id,))
        if not row:
            raise core.ProjectError(f"human request not found: {a.id}")
        out = dict(row)
        issue = core.one(con, "SELECT key FROM issue WHERE id=?", (row["issue_id"],)) if row["issue_id"] else None
        out["issue_key"] = issue["key"] if issue else ""
    finally:
        con.close()
    core.emit(out, a.json)


def _human_status(a, status, reason=""):
    def fn(con):
        row = core.one(con, "SELECT * FROM human_request WHERE id=?", (a.id,))
        if not row:
            raise core.ProjectError(f"human request not found: {a.id}")
        con.execute("UPDATE human_request SET status=?, answer=?, asked_at=CASE WHEN ?='asked' THEN datetime('now') ELSE asked_at END, "
                    "answered_at=CASE WHEN ? IN ('answered','deferred','cancelled') THEN datetime('now') ELSE answered_at END WHERE id=?",
                    (status, getattr(a, "answer", "") or reason, status, status, a.id))
        core.audit(con, "human_request", a.id, status, "status", row["status"], status,
                   reason=reason, actor=a.actor)
        return {"id": a.id, "status": status}

    core.emit(core.write(fn), a.json)


def cmd_human_ask(a):
    _human_status(a, "asked")


def cmd_human_answer(a):
    _human_status(a, "answered", a.answer)


def cmd_human_defer(a):
    def fn(con):
        row = core.one(con, "SELECT * FROM human_request WHERE id=?", (a.id,))
        if not row:
            raise core.ProjectError(f"human request not found: {a.id}")
        con.execute("UPDATE human_request SET status='deferred', defer_until=? WHERE id=?", (a.until or "", a.id))
        core.audit(con, "human_request", a.id, "defer", "status", row["status"], "deferred",
                   reason=a.reason or "", actor=a.actor)
        return {"id": a.id, "status": "deferred", "until": a.until or ""}
    core.emit(core.write(fn), a.json)


def cmd_human_cancel(a):
    _human_status(a, "cancelled", a.reason or "")


# ------------------------------------------------------------------ sessions --
def cmd_session_start(a):
    core.require_db()

    def fn(con):
        cur = con.execute("INSERT INTO session(agent) VALUES(?)", (a.as_agent,))
        core.audit(con, "session", cur.lastrowid, "start", "", None, {"agent": a.as_agent}, actor=a.actor)
        return {"id": cur.lastrowid, "agent": a.as_agent}

    core.emit(core.write(fn), a.json)


def cmd_session_end(a):
    core.require_db()

    def fn(con):
        row = core.one(con, "SELECT * FROM session ORDER BY id DESC LIMIT 1")
        if not row:
            cur = con.execute("INSERT INTO session(agent) VALUES('agent')")
            row = core.one(con, "SELECT * FROM session WHERE id=?", (cur.lastrowid,))
        next_id = core.get_issue(con, a.next_issue)["id"] if a.next_issue else None
        con.execute("UPDATE session SET ended_at=datetime('now'), summary=?, handoff=?, next_issue_id=?, commit_sha=? WHERE id=?",
                    (a.summary or "", a.handoff or "", next_id, a.commit or "", row["id"]))
        core.audit(con, "session", row["id"], "end", "summary", None, a.summary or "", actor=a.actor)
        return {"id": row["id"], "summary": a.summary or "", "next_issue": a.next_issue or None}

    core.emit(core.write(fn), a.json)


def cmd_session_list(a):
    core.require_db()
    con = core.connect()
    try:
        rows = core.all_rows(con, """SELECT s.*, i.key AS next_issue_key FROM session s
            LEFT JOIN issue i ON i.id=s.next_issue_id ORDER BY s.id DESC LIMIT ?""", (a.limit,))
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["id", "agent", "started_at", "ended_at", "next_issue_key", "summary"])


# -------------------------------------------------------------------- agents --
def cmd_agent_add(a):
    core.require_db()

    def fn(con):
        issue_id = core.get_issue(con, a.issue)["id"] if a.issue else None
        cur = con.execute("INSERT INTO agent_run(agent,purpose,model,issue_id) VALUES(?,?,?,?)",
                          (a.as_agent, a.purpose, a.model or "", issue_id))
        core.audit(con, "agent_run", cur.lastrowid, "create", "", None,
                   {"agent": a.as_agent, "purpose": a.purpose}, actor=a.actor)
        return {"id": cur.lastrowid, "agent": a.as_agent}

    core.emit(core.write(fn), a.json)


def cmd_agent_done(a):
    core.require_db()

    def fn(con):
        row = core.one(con, "SELECT * FROM agent_run WHERE id=?", (a.id,))
        if not row:
            raise core.ProjectError(f"agent run not found: {a.id}")
        con.execute("UPDATE agent_run SET status=?, outcome=?, artifact=?, ended_at=datetime('now') WHERE id=?",
                    (a.status, a.outcome or "", a.artifact or "", a.id))
        core.audit(con, "agent_run", a.id, "done", "status", row["status"], a.status, actor=a.actor)
        return {"id": a.id, "status": a.status}

    core.emit(core.write(fn), a.json)


def cmd_agent_list(a):
    core.require_db()
    con = core.connect()
    try:
        rows = core.all_rows(con, """SELECT a.*, i.key AS issue_key FROM agent_run a
            LEFT JOIN issue i ON i.id=a.issue_id ORDER BY a.id DESC LIMIT ?""", (a.limit,))
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["id", "agent", "status", "issue_key", "purpose"])


# --------------------------------------------------------------------- tests --
def cmd_test_record(a):
    core.require_db()

    def fn(con):
        if a.status == "pass" and not (a.command or "").strip():
            raise core.ProjectError("a passing test run requires --command")
        if a.failed != 0:
            raise core.ProjectError("failed count must be zero for a recorded run; record a fail/test issue")
        if a.checks < 0:
            raise core.ProjectError("checks cannot be negative")
        issue_id = core.get_issue(con, a.issue)["id"] if a.issue else None
        cur = con.execute(
            "INSERT INTO test_run(kind,command,commit_sha,branch,status,checks,failed,output_path,actor) "
            "VALUES(?,?,?,?,?,?,?,?,?)",
            (a.kind, a.command or "", a.commit or "", a.branch or "", a.status, a.checks, a.failed,
             a.output or "", a.actor))
        if issue_id:
            con.execute("INSERT OR IGNORE INTO test_run_issue(run_id,issue_id) VALUES(?,?)",
                        (cur.lastrowid, issue_id))
        core.audit(con, "test_run", cur.lastrowid, "record", "", None,
                   {"kind": a.kind, "status": a.status, "command": a.command or "",
                    "commit": a.commit or "", "branch": a.branch or "", "issue": a.issue or ""},
                   reason=a.issue or "", actor=a.actor)
        return {"id": cur.lastrowid, "status": a.status, "kind": a.kind, "issue": a.issue or None}

    core.emit(core.write(fn), a.json)


def cmd_test_list(a):
    core.require_db()
    con = core.connect()
    try:
        rows = core.all_rows(con, """SELECT t.*, GROUP_CONCAT(i.key) AS issues
            FROM test_run t LEFT JOIN test_run_issue ti ON ti.run_id=t.id
            LEFT JOIN issue i ON i.id=ti.issue_id
            GROUP BY t.id ORDER BY t.id DESC LIMIT ?""", (a.limit,))
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["id", "kind", "status", "checks", "failed", "branch", "issues", "created_at"])


def cmd_test_result(a):
    core.require_db()

    def fn(con):
        run = core.one(con, "SELECT * FROM test_run WHERE id=?", (a.run,))
        if not run:
            raise core.ProjectError(f"test run not found: {a.run}")
        scenario_id = None
        if a.scenario:
            s = core.one(con, "SELECT id FROM scenario WHERE key=?", (a.scenario,))
            if not s:
                raise core.ProjectError(f"scenario not found: {a.scenario}")
            scenario_id = s["id"]
        issue_id = core.get_issue(con, a.issue)["id"] if a.issue else None
        cur = con.execute("INSERT INTO test_result(run_id,scenario_id,issue_id,status,duration_ms,error_md) VALUES(?,?,?,?,?,?)",
                          (a.run, scenario_id, issue_id, a.status, a.duration, a.error or ""))
        core.audit(con, "test_result", cur.lastrowid, "record", "status", None, a.status, actor=a.actor)
        return {"id": cur.lastrowid, "run": a.run, "status": a.status}

    core.emit(core.write(fn), a.json)
