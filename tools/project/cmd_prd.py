"""PRD, requirements, scenarios, decisions, assumptions, risks and evidence."""
from __future__ import annotations

import json
import re

try:
    from . import core
except ImportError:
    import core


def _section_id(con, key):
    if not key:
        return None
    row = core.one(con, "SELECT id FROM prd_section WHERE key=?", (key,))
    if not row:
        raise core.ProjectError(f"PRD section not found: {key}")
    return row["id"]


def _req(con, key):
    row = core.one(con, "SELECT * FROM requirement WHERE key=?", (str(key).upper(),))
    if not row:
        raise core.ProjectError(f"requirement not found: {key}")
    return row


def _audit_prd(con, entity, entity_id, field, old, new, reason, actor):
    core.audit(con, entity, entity_id, "update", field, old, new, reason=reason, actor=actor)


def cmd_prd_init(a):
    core.require_db()

    def fn(con):
        cfg = core.repo_config()
        doc = core.one(con, "SELECT * FROM prd_document WHERE id=1")
        title = a.title or cfg.get("project", {}).get("name", core.project_name())
        if doc["title"] not in ("Untitled project", "") and not getattr(a, "force", False):
            title = doc["title"]
        if doc["status"] == "signed_off" and title != doc["title"]:
            raise core.ProjectError("PRD is signed off; use `prd revise --reason ...` before changing the title")
        con.execute("UPDATE prd_document SET title=?, updated_at=datetime('now') WHERE id=1", (title,))
        created = []
        for key, name, pos in core.DEFAULT_SECTIONS:
            if not core.one(con, "SELECT 1 FROM prd_section WHERE key=?", (key,)):
                con.execute("INSERT INTO prd_section(key,title,position) VALUES(?,?,?)", (key, name, pos))
                created.append(key)
        core.set_config_row(con, "prd.required_sections", json.dumps(core.REQUIRED_SECTIONS))
        return {"title": title, "created_sections": created}

    core.emit(core.write(fn), a.json)


def cmd_prd_show(a):
    core.require_db()
    con = core.connect()
    try:
        doc = core.rowdict(core.one(con, "SELECT * FROM prd_document WHERE id=1"))
        sections = core.all_rows(con, "SELECT key,title,status,length(body_md) n,position FROM prd_section ORDER BY position,id")
        reqs = core.one(con, "SELECT COUNT(*) n FROM requirement")["n"]
        scenarios = core.one(con, "SELECT COUNT(*) n FROM scenario")["n"]
        questions = core.one(con, "SELECT COUNT(*) n FROM prd_question WHERE status='open'")["n"]
        assumptions = core.one(con, "SELECT COUNT(*) n FROM assumption WHERE status='open'")["n"]
        validation, vnote = core.prd_validation(con)
    finally:
        con.close()
    if a.json:
        core.emit({"document": doc, "validation": validation, "validation_note": vnote,
                   "sections": sections, "requirements": reqs,
                   "scenarios": scenarios, "open_questions": questions,
                   "open_assumptions": assumptions}, True)
    else:
        print(f"{doc['title']}  [{doc['status']} v{doc['version']}]" +
              ("  UNVALIDATED v0.1" if validation == "unvalidated" else ""))
        if doc.get("summary"):
            print(doc["summary"])
        print(f"requirements={reqs} scenarios={scenarios} open_questions={questions} open_assumptions={assumptions}")
        core.table(sections, ["position", "key", "status", "n", "title"])


def cmd_prd_section_list(a):
    core.require_db()
    con = core.connect()
    try:
        rows = core.all_rows(con, """SELECT s.key,s.title,s.status,s.position,length(s.body_md) n,
            (SELECT COUNT(*) FROM requirement r WHERE r.section_id=s.id) requirements
            FROM prd_section s ORDER BY s.position,s.id""")
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["position", "key", "status", "n", "requirements", "title"])


def cmd_prd_section_set(a):
    core.require_db()

    def fn(con):
        core.ensure_prd_mutable(con, a.reason or "", a.actor)
        if not re.fullmatch(r"[a-z0-9][a-z0-9_-]*", a.key):
            raise core.ProjectError("PRD section key must be lowercase letters/digits/_/-")
        body = a.body or ""
        if a.body_file:
            body = core.Path(a.body_file).read_text()
        row = core.one(con, "SELECT * FROM prd_section WHERE key=?", (a.key,))
        if row:
            old = dict(row)
            updates = {}
            if a.title is not None:
                updates["title"] = a.title
            if a.body is not None or a.body_file:
                updates["body_md"] = body
            if a.status is not None:
                updates["status"] = a.status
            if a.position is not None:
                updates["position"] = a.position
            if not updates:
                raise core.ProjectError("nothing to change")
            sets = ", ".join(f"{k}=?" for k in updates)
            con.execute(f"UPDATE prd_section SET {sets}, updated_at=datetime('now') WHERE key=?",
                        (*updates.values(), a.key))
            for field, value in updates.items():
                _audit_prd(con, "prd_section", row["id"], field, old.get(field), value, a.reason or "", a.actor)
            return {"key": a.key, "changed": sorted(updates)}
        else:
            if not a.title:
                raise core.ProjectError("new section requires --title")
            cur = con.execute("INSERT INTO prd_section(key,title,body_md,position,status) VALUES(?,?,?,?,?)",
                              (a.key, a.title, body, a.position or 999, a.status or "draft"))
            core.audit(con, "prd_section", cur.lastrowid, "create", "", None,
                       {"key": a.key, "title": a.title}, reason=a.reason or "", actor=a.actor)
            return {"key": a.key, "created": True}

    core.emit(core.write(fn), a.json)


def cmd_requirement_add(a):
    core.require_db()

    def fn(con):
        core.ensure_prd_mutable(con, getattr(a, "reason", "") or "", a.actor)
        area = (a.area or "GEN").upper()
        key = (a.key or "").upper()
        if not key:
            n = core.one(con, "SELECT COUNT(*) n FROM requirement WHERE area=?", (area,))["n"] + 1
            key = f"REQ-{area}-{n:03d}"
        if not re.fullmatch(r"REQ-[A-Z0-9]{1,16}-[0-9]{3}", key):
            raise core.ProjectError("requirement key must match REQ-<AREA>-<NNN>, e.g. REQ-AUTH-001")
        if core.one(con, "SELECT 1 FROM requirement WHERE key=?", (key,)):
            raise core.ProjectError(f"requirement key already exists: {key}")
        statement = core.validate_requirement_statement(a.statement, a.kind)
        if not (a.title or "").strip():
            raise core.ProjectError("requirement title is required")
        sid = _section_id(con, a.section) if a.section else None
        cur = con.execute(
            "INSERT INTO requirement(key,area,title,statement,kind,priority,status,rationale,section_id) "
            "VALUES(?,?,?,?,?,?,?,?,?)",
            (key, area, a.title, statement, a.kind, a.priority, a.status, a.rationale or "", sid))
        core.audit(con, "requirement", cur.lastrowid, "create", "", None,
                   {"key": key, "title": a.title, "priority": a.priority}, actor=a.actor)
        return {"key": key, "id": cur.lastrowid}

    core.emit(core.write(fn), a.json)


def cmd_requirement_edit(a):
    core.require_db()

    def fn(con):
        core.ensure_prd_mutable(con, a.reason or "", a.actor)
        r = _req(con, a.key)
        old = dict(r)
        updates = {}
        for field, val in (("title", a.title), ("statement", a.statement), ("kind", a.kind),
                           ("priority", a.priority), ("status", a.status),
                           ("rationale", a.rationale), ("area", a.area)):
            if val is not None:
                updates[field] = val
        if a.section is not None:
            updates["section_id"] = _section_id(con, a.section) if a.section else None
        if "statement" in updates:
            updates["statement"] = core.validate_requirement_statement(updates["statement"],
                                                                        updates.get("kind", r["kind"]))
        if "title" in updates and not (updates["title"] or "").strip():
            raise core.ProjectError("requirement title cannot be empty")
        if not updates:
            raise core.ProjectError("nothing to change")
        sets = ", ".join(f"{k}=?" for k in updates)
        con.execute(f"UPDATE requirement SET {sets}, updated_at=datetime('now') WHERE id=?",
                    (*updates.values(), r["id"]))
        for field, value in updates.items():
            _audit_prd(con, "requirement", r["id"], field, old.get(field), value, a.reason or "", a.actor)
        return {"key": r["key"], "changed": sorted(updates)}

    core.emit(core.write(fn), a.json)


def cmd_requirement_list(a):
    core.require_db()
    con = core.connect()
    try:
        sql = "SELECT * FROM v_requirement_coverage"
        args = []
        if a.priority:
            sql += " WHERE priority=?"
            args.append(a.priority)
        if a.missing_scenarios:
            sql += (" AND " if args else " WHERE ") + "missing_scenario=1"
        sql += " ORDER BY key"
        rows = core.all_rows(con, sql, tuple(args))
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(
        rows, ["key", "priority", "status", "scenario_count", "issue_count", "missing_scenario", "missing_issue", "title"])


def cmd_requirement_show(a):
    core.require_db()
    con = core.connect()
    try:
        r = _req(con, a.key)
        out = dict(r)
        out["scenarios"] = core.all_rows(con, "SELECT * FROM scenario WHERE requirement_id=? ORDER BY position,id", (r["id"],))
        out["issues"] = core.all_rows(con, """SELECT i.key,i.title,ir.relation FROM issue_requirement ir
            JOIN issue i ON i.id=ir.issue_id WHERE ir.requirement_id=? ORDER BY i.key""", (r["id"],))
        out["decisions"] = core.all_rows(con, "SELECT code,title,status FROM decision WHERE requirement_id=? ORDER BY code", (r["id"],))
    finally:
        con.close()
    core.emit(out, a.json)


def cmd_scenario_add(a):
    core.require_db()

    def fn(con):
        core.ensure_prd_mutable(con, getattr(a, "reason", "") or "", a.actor)
        r = _req(con, a.requirement)
        m = re.match(r"REQ-([A-Z0-9]+)-(\d+)", r["key"])
        area, num = (m.group(1), m.group(2)) if m else ("GEN", f"{r['id']:03d}")
        n = core.one(con, "SELECT COUNT(*) n FROM scenario WHERE requirement_id=?", (r["id"],))["n"] + 1
        key = (a.key or f"SC-{area}-{num}-{n:02d}").upper()
        if not (a.title or "").strip():
            raise core.ProjectError("scenario title is required")
        if a.status == "active" and (not (a.when or "").strip() or not (a.then or "").strip()):
            raise core.ProjectError("active scenarios require non-empty --when and --then")
        if not re.fullmatch(r"SC-[A-Z0-9-]{2,60}", key):
            raise core.ProjectError("scenario key must match SC-<AREA>-<NNN>-<NN>")
        if core.one(con, "SELECT 1 FROM scenario WHERE key=?", (key,)):
            raise core.ProjectError(f"scenario key already exists: {key}")
        pos = core.one(con, "SELECT COALESCE(MAX(position),0)+10 p FROM scenario WHERE requirement_id=?", (r["id"],))["p"]
        cur = con.execute(
            "INSERT INTO scenario(key,requirement_id,title,given_md,when_md,then_md,tags,status,position) "
            "VALUES(?,?,?,?,?,?,?,?,?)",
            (key, r["id"], a.title, a.given or "", a.when or "", a.then or "",
             a.tags or "", a.status, pos))
        core.audit(con, "scenario", cur.lastrowid, "create", "", None,
                   {"key": key, "requirement": r["key"], "title": a.title}, actor=a.actor)
        return {"key": key, "requirement": r["key"], "id": cur.lastrowid}

    core.emit(core.write(fn), a.json)


def cmd_scenario_edit(a):
    core.require_db()

    def fn(con):
        core.ensure_prd_mutable(con, a.reason or "", a.actor)
        row = core.one(con, "SELECT * FROM scenario WHERE key=?", (a.key,))
        if not row:
            raise core.ProjectError(f"scenario not found: {a.key}")
        old = dict(row)
        updates = {}
        for field, val in (("title", a.title), ("given_md", a.given), ("when_md", a.when),
                           ("then_md", a.then), ("tags", a.tags), ("status", a.status)):
            if val is not None:
                updates[field] = val
        if a.position is not None:
            updates["position"] = a.position
        if not updates:
            raise core.ProjectError("nothing to change")
        if not (updates.get("title", row["title"]) or "").strip():
            raise core.ProjectError("scenario title cannot be empty")
        effective_status = updates.get("status", row["status"])
        effective_when = updates.get("when_md", row["when_md"])
        effective_then = updates.get("then_md", row["then_md"])
        if effective_status == "active" and (not (effective_when or "").strip()
                                             or not (effective_then or "").strip()):
            raise core.ProjectError("active scenarios require non-empty when and then")
        sets = ", ".join(f"{k}=?" for k in updates)
        con.execute(f"UPDATE scenario SET {sets}, updated_at=datetime('now') WHERE id=?",
                    (*updates.values(), row["id"]))
        for field, value in updates.items():
            _audit_prd(con, "scenario", row["id"], field, old.get(field), value, a.reason or "", a.actor)
        return {"key": a.key, "changed": sorted(updates)}

    core.emit(core.write(fn), a.json)


def cmd_scenario_list(a):
    core.require_db()
    con = core.connect()
    try:
        sql = """SELECT s.key,s.title,s.status,s.requirement_id,r.key AS requirement
                 FROM scenario s JOIN requirement r ON r.id=s.requirement_id"""
        args = []
        if a.requirement:
            sql += " WHERE r.key=?"
            args.append(a.requirement.upper())
        sql += " ORDER BY r.key,s.position,s.id"
        rows = core.all_rows(con, sql, tuple(args))
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["key", "requirement", "status", "title"])


def cmd_question_add(a):
    core.require_db()

    def fn(con):
        core.ensure_prd_mutable(con, getattr(a, "reason", "") or "", a.actor)
        cur = con.execute("INSERT INTO prd_question(section_key,question,why,impact) VALUES(?,?,?,?)",
                          (a.section or "", a.question, a.why or "", a.impact))
        core.audit(con, "prd_question", cur.lastrowid, "create", "", None,
                   {"question": a.question, "impact": a.impact}, actor=a.actor)
        return {"id": cur.lastrowid, "impact": a.impact}

    core.emit(core.write(fn), a.json)


def cmd_question_answer(a):
    core.require_db()

    def fn(con):
        core.ensure_prd_mutable(con, getattr(a, "reason", "") or "", a.actor)
        row = core.one(con, "SELECT * FROM prd_question WHERE id=?", (a.id,))
        if not row:
            raise core.ProjectError(f"question not found: {a.id}")
        if not (a.answer or "").strip():
            raise core.ProjectError("answer cannot be empty")
        con.execute("UPDATE prd_question SET status='answered', answer=?, answered_at=datetime('now') WHERE id=?",
                    (a.answer, a.id))
        core.audit(con, "prd_question", a.id, "answer", "answer", row["answer"], a.answer, actor=a.actor)
        return {"id": a.id, "status": "answered"}

    core.emit(core.write(fn), a.json)


def cmd_question_defer(a):
    core.require_db()

    def fn(con):
        core.ensure_prd_mutable(con, a.reason or "", a.actor)
        row = core.one(con, "SELECT * FROM prd_question WHERE id=?", (a.id,))
        if not row:
            raise core.ProjectError(f"question not found: {a.id}")
        if row["impact"] == "high" and not (a.reason or "").strip():
            raise core.ProjectError("deferring a high-impact question requires --reason (revisit note)")
        con.execute("UPDATE prd_question SET status='deferred', answer=?, why=COALESCE(NULLIF(?, ''), why), "
                    "answered_at=datetime('now') WHERE id=?",
                    (a.reason or "", a.reason or "", a.id))
        core.audit(con, "prd_question", a.id, "defer", "status", "open", "deferred", reason=a.reason or "", actor=a.actor)
        return {"id": a.id, "status": "deferred"}

    core.emit(core.write(fn), a.json)


def cmd_question_list(a):
    core.require_db()
    con = core.connect()
    try:
        sql = "SELECT * FROM prd_question"
        if a.open:
            sql += " WHERE status='open'"
        sql += " ORDER BY CASE impact WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END,id"
        rows = core.all_rows(con, sql)
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["id", "impact", "status", "section_key", "question"])


def cmd_assumption_add(a):
    core.require_db()

    def fn(con):
        core.ensure_prd_mutable(con, getattr(a, "reason", "") or "", a.actor)
        n = core.one(con, "SELECT COUNT(*) n FROM assumption")["n"] + 1
        key = a.key or f"A-{n:03d}"
        cur = con.execute(
            "INSERT INTO assumption(key,statement,confidence,impact,status,owner,revisit_trigger,evidence) "
            "VALUES(?,?,?,?,?,?,?,?)",
            (key, a.statement, a.confidence, a.impact, a.status, a.owner or "", a.revisit or "", a.evidence or ""))
        core.audit(con, "assumption", cur.lastrowid, "create", "", None,
                   {"key": key, "statement": a.statement, "impact": a.impact}, actor=a.actor)
        return {"key": key, "id": cur.lastrowid}

    core.emit(core.write(fn), a.json)


def cmd_assumption_list(a):
    core.require_db()
    con = core.connect()
    try:
        sql = "SELECT * FROM assumption"
        if a.open:
            sql += " WHERE status='open'"
        sql += " ORDER BY CASE impact WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END,key"
        rows = core.all_rows(con, sql)
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["key", "impact", "status", "confidence", "statement"])


def cmd_assumption_resolve(a):
    core.require_db()

    def fn(con):
        core.ensure_prd_mutable(con, a.reason or "", a.actor)
        row = core.one(con, "SELECT * FROM assumption WHERE key=?", (a.key,))
        if not row:
            raise core.ProjectError(f"assumption not found: {a.key}")
        revisit = getattr(a, "revisit", None) or row["revisit_trigger"]
        if a.status == "deferred" and not (revisit or "").strip():
            raise core.ProjectError("deferred assumption requires --revisit trigger")
        con.execute("UPDATE assumption SET status=?, evidence=?, revisit_trigger=?, updated_at=datetime('now') WHERE id=?",
                    (a.status, a.evidence or row["evidence"], revisit, row["id"]))
        core.audit(con, "assumption", row["id"], "resolve", "status", row["status"], a.status,
                   reason=a.reason or "", actor=a.actor)
        return {"key": a.key, "status": a.status}

    core.emit(core.write(fn), a.json)


def cmd_risk_add(a):
    core.require_db()

    def fn(con):
        core.ensure_prd_mutable(con, getattr(a, "reason", "") or "", a.actor)
        n = core.one(con, "SELECT COUNT(*) n FROM risk")["n"] + 1
        key = a.key or f"R-{n:03d}"
        cur = con.execute(
            "INSERT INTO risk(key,title,description,likelihood,impact,mitigation,trigger,status,owner) "
            "VALUES(?,?,?,?,?,?,?,?,?)",
            (key, a.title, a.description or "", a.likelihood, a.impact,
             a.mitigation or "", a.trigger or "", a.status, a.owner or ""))
        core.audit(con, "risk", cur.lastrowid, "create", "", None,
                   {"key": key, "title": a.title, "likelihood": a.likelihood, "impact": a.impact}, actor=a.actor)
        return {"key": key, "id": cur.lastrowid}

    core.emit(core.write(fn), a.json)


def cmd_risk_list(a):
    core.require_db()
    con = core.connect()
    try:
        rows = core.all_rows(con, "SELECT * FROM risk ORDER BY (likelihood*impact) DESC,key")
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["key", "status", "likelihood", "impact", "title"])


def cmd_risk_update(a):
    core.require_db()

    def fn(con):
        core.ensure_prd_mutable(con, a.reason or "", a.actor)
        row = core.one(con, "SELECT * FROM risk WHERE key=?", (a.key,))
        if not row:
            raise core.ProjectError(f"risk not found: {a.key}")
        updates = {}
        for field, val in (("status", a.status), ("mitigation", a.mitigation), ("trigger", a.trigger),
                           ("likelihood", a.likelihood), ("impact", a.impact), ("owner", a.owner)):
            if val is not None:
                updates[field] = val
        if not updates:
            raise core.ProjectError("nothing to change")
        sets = ", ".join(f"{k}=?" for k in updates)
        con.execute(f"UPDATE risk SET {sets}, updated_at=datetime('now') WHERE id=?",
                    (*updates.values(), row["id"]))
        for f, v in updates.items():
            core.audit(con, "risk", row["id"], "update", f, row[f], v, actor=a.actor)
        return {"key": a.key, "changed": sorted(updates)}

    core.emit(core.write(fn), a.json)


def cmd_decision_add(a):
    core.require_db()

    def fn(con):
        code = a.code
        if not code:
            n = core.one(con, "SELECT COUNT(*) n FROM decision")["n"] + 1
            code = f"D-{n:03d}"
        if core.one(con, "SELECT 1 FROM decision WHERE code=?", (code,)):
            raise core.ProjectError(f"decision code already exists: {code}")
        supersedes_id = None
        if a.supersedes:
            old = core.one(con, "SELECT * FROM decision WHERE code=?", (a.supersedes,))
            if not old:
                raise core.ProjectError(f"decision to supersede not found: {a.supersedes}")
            supersedes_id = old["id"]
        req_id = None
        if a.requirement:
            req_id = _req(con, a.requirement)["id"]
        issue_id = None
        if a.issue:
            issue_id = core.get_issue(con, a.issue)["id"]
        cur = con.execute(
            "INSERT INTO decision(code,title,context_md,decision_md,consequences_md,status,"
            "supersedes_id,prd_section_key,issue_id,requirement_id,decided_by,revisit_trigger) "
            "VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
            (code, a.title, a.context or "", a.decision or "", a.consequences or "", a.status,
             supersedes_id, a.prd_ref or "", issue_id, req_id, a.by or a.actor, a.revisit or ""))
        new_id = cur.lastrowid
        if supersedes_id:
            con.execute("UPDATE decision SET status='superseded', superseded_by_id=? WHERE id=?",
                        (new_id, supersedes_id))
            core.audit(con, "decision", supersedes_id, "supersede", "status", "accepted", "superseded",
                       reason=f"superseded by {code}", actor=a.actor)
        core.audit(con, "decision", new_id, "create", "", None,
                   {"code": code, "title": a.title, "status": a.status}, actor=a.actor)
        return {"code": code, "id": new_id, "supersedes": a.supersedes or None}

    core.emit(core.write(fn), a.json)


def cmd_decision_list(a):
    core.require_db()
    con = core.connect()
    try:
        sql = "SELECT code,title,status,decided_by,decided_at,prd_section_key FROM decision"
        if a.status:
            sql += " WHERE status=?"
            sql += " ORDER BY code"
            rows = core.all_rows(con, sql, (a.status,))
        else:
            sql += " ORDER BY code"
            rows = core.all_rows(con, sql)
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["code", "status", "title", "prd_section_key"])


def cmd_decision_show(a):
    core.require_db()
    con = core.connect()
    try:
        row = core.one(con, "SELECT * FROM decision WHERE code=?", (a.code,))
        if not row:
            raise core.ProjectError(f"decision not found: {a.code}")
        out = dict(row)
        out["supersedes"] = core.rowdict(core.one(con, "SELECT code,title FROM decision WHERE id=?", (row["supersedes_id"],))) if row["supersedes_id"] else None
        out["superseded_by"] = core.rowdict(core.one(con, "SELECT code,title FROM decision WHERE id=?", (row["superseded_by_id"],))) if row["superseded_by_id"] else None
    finally:
        con.close()
    core.emit(out, a.json)


def cmd_source_add(a):
    core.require_db()

    def fn(con):
        cur = con.execute(
            "INSERT INTO source(kind,title,url,artifact_path,reliability,summary,tags) VALUES(?,?,?,?,?,?,?)",
            (a.kind, a.title, a.url or "", a.artifact or "", a.reliability, a.summary or "", a.tags or ""))
        core.audit(con, "source", cur.lastrowid, "create", "", None, {"title": a.title, "url": a.url}, actor=a.actor)
        return {"id": cur.lastrowid, "title": a.title}

    core.emit(core.write(fn), a.json)


def cmd_source_list(a):
    core.require_db()
    con = core.connect()
    try:
        rows = core.all_rows(con, "SELECT * FROM source ORDER BY id DESC")
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["id", "kind", "reliability", "title", "url"])


def cmd_finding_add(a):
    core.require_db()

    def fn(con):
        issue_id = core.get_issue(con, a.issue)["id"] if a.issue else None
        req_id = _req(con, a.requirement)["id"] if a.requirement else None
        cur = con.execute(
            "INSERT INTO finding(source_id,issue_id,requirement_id,category,claim,quote,confidence) "
            "VALUES(?,?,?,?,?,?,?)",
            (a.source, issue_id, req_id, a.category or "", a.claim, a.quote or "", a.confidence))
        core.audit(con, "finding", cur.lastrowid, "create", "", None,
                   {"claim": a.claim, "issue": a.issue}, actor=a.actor)
        return {"id": cur.lastrowid, "claim": a.claim}

    core.emit(core.write(fn), a.json)


def cmd_finding_list(a):
    core.require_db()
    con = core.connect()
    try:
        sql = """SELECT f.*, s.title AS source_title, i.key AS issue_key, r.key AS requirement_key
                 FROM finding f LEFT JOIN source s ON s.id=f.source_id
                 LEFT JOIN issue i ON i.id=f.issue_id LEFT JOIN requirement r ON r.id=f.requirement_id"""
        args = []
        if a.issue:
            sql += " WHERE i.key=?"
            args.append(a.issue.upper())
        sql += " ORDER BY f.id DESC"
        rows = core.all_rows(con, sql, tuple(args))
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["id", "confidence", "category", "issue_key", "claim"])


def cmd_prd_coverage(a):
    core.require_db()
    con = core.connect()
    try:
        blockers, warnings = core.prd_blockers(con)
        sections = core.all_rows(con, "SELECT key,title,length(body_md) n,status FROM prd_section ORDER BY position")
        reqs = core.all_rows(con, "SELECT * FROM v_requirement_coverage ORDER BY key")
        questions = core.all_rows(con, "SELECT id,impact,question FROM v_open_prd_questions")
        assumptions = core.all_rows(con, "SELECT key,impact,statement FROM assumption WHERE status='open' ORDER BY key")
    finally:
        con.close()
    if a.json:
        core.emit({"blockers": blockers, "warnings": warnings, "sections": sections,
                   "requirements": reqs, "questions": questions, "assumptions": assumptions}, True)
        return
    print(f"PRD coverage: {len(blockers)} blocker(s), {len(warnings)} warning(s)")
    if blockers:
        print("\nBLOCKERS")
        for b in blockers:
            print(f"  ! {b}")
    if warnings:
        print("\nWARNINGS")
        for w in warnings:
            print(f"  - {w}")
    print("\nREQUIREMENTS")
    core.table(reqs, ["key", "priority", "status", "scenario_count", "issue_count", "missing_scenario", "missing_issue", "title"])


def cmd_prd_signoff(a):
    core.require_db()

    def fn(con):
        blockers, warnings = core.prd_blockers(con)
        if blockers and not a.force:
            raise core.ProjectError("PRD is not ready:\n  - " + "\n  - ".join(blockers)
                                    + "\nResolve the blockers (for reviews: `review resolve <id> --status addressed --commit <sha>`), "
                                      "or use --force --note for an explicit unvalidated exception.")
        if a.force and not a.note:
            raise core.ProjectError("forced signoff requires --note explaining the exception")
        doc = core.one(con, "SELECT * FROM prd_document WHERE id=1")
        version = int(doc["version"])
        if doc["status"] == "signed_off" and not a.reason:
            raise core.ProjectError("PRD is already signed off; use --reason to re-sign")
        con.execute("UPDATE prd_document SET status='signed_off', version=?, signed_off_at=datetime('now'), "
                    "signed_off_by=?, updated_at=datetime('now') WHERE id=1",
                    (version, a.by or a.actor))
        con.execute("UPDATE prd_section SET status='locked' WHERE status NOT IN ('deprecated')")
        validation = "unvalidated" if a.force else "validated"
        core.set_config_row(con, "prd.validation", validation)
        core.set_config_row(con, "prd.validation_note", (a.note or a.reason or "") if a.force else "")
        core.audit(con, "prd_document", 1, "signoff", "status", doc["status"], "signed_off",
                   reason=a.note or a.reason or "", actor=a.actor)
        core.audit(con, "prd_document", 1, "signoff", "validation", None, validation,
                   reason=a.note or a.reason or "", actor=a.actor)
        return {"status": "signed_off", "version": version, "validation": validation,
                "warnings": warnings, "forced": bool(blockers and a.force)}

    result = core.write(fn)
    core.do_export(write_status=True)
    result["exported"] = True
    core.emit(result, a.json)


def cmd_prd_revise(a):
    core.require_db()

    def fn(con):
        doc = core.one(con, "SELECT * FROM prd_document WHERE id=1")
        con.execute("UPDATE prd_document SET status='draft', version=version+1, "
                    "signed_off_at=NULL, signed_off_by='', updated_at=datetime('now') WHERE id=1")
        con.execute("UPDATE prd_section SET status='draft' WHERE status IN ('review','locked')")
        core.audit(con, "prd_document", 1, "revise", "status", doc["status"], "draft",
                   reason=a.reason, actor=a.actor)
        return {"status": "draft", "version": int(doc["version"]) + 1, "reason": a.reason}

    result = core.write(fn)
    core.do_export(write_status=True)
    result["exported"] = True
    core.emit(result, a.json)


def cmd_prd_review(a):
    core.require_db()

    def fn(con):
        doc = core.one(con, "SELECT * FROM prd_document WHERE id=1")
        if doc["status"] == "signed_off":
            core.ensure_prd_mutable(con, getattr(a, "reason", "") or "", a.actor)
        con.execute("UPDATE prd_document SET status='review', updated_at=datetime('now') WHERE id=1")
        con.execute("UPDATE prd_section SET status='review' WHERE status='draft'")
        core.audit(con, "prd_document", 1, "review", "status", doc["status"], "review", actor=a.actor)
        return {"status": "review"}

    result = core.write(fn)
    core.do_export(write_status=True)
    result["exported"] = True
    core.emit(result, a.json)


def cmd_prd_export(a):
    core.require_db()
    result = core.do_export(write_status=True)
    core.emit(result, a.json)
