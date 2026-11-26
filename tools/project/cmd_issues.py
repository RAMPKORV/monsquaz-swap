"""Issue, plan, label and component commands."""
from __future__ import annotations

import json
import sys

try:
    from . import core
except ImportError:  # running as a script
    import core


RESERVED_DETAIL_KEYS = {"merge_commit", "worktree_head", "evidence", "post_merge_check"}


def _parse_details(items):
    out = {}
    for item in items or []:
        if "=" not in item:
            raise core.ProjectError(f"--detail expects key=value, got {item!r}")
        k, v = item.split("=", 1)
        k = k.strip()
        if k in RESERVED_DETAIL_KEYS:
            raise core.ProjectError(f"{k} is managed by the worktree/merge flow, not --detail")
        out[k] = v
    return out


def cmd_issue_add(a):
    core.require_db()

    def fn(con):
        if not (a.title or "").strip():
            raise core.ProjectError("issue title is required")
        parent_id = None
        if a.parent:
            parent = core.get_issue(con, a.parent)
            if parent["status_key"] in core.TERMINAL:
                raise core.ProjectError("cannot add a child under a terminal parent; reopen the parent first")
            parent_id = parent["id"]
        milestone_id = None
        if a.milestone:
            m = core.one(con, "SELECT id FROM milestone WHERE key=?", (a.milestone,))
            if not m:
                raise core.ProjectError(f"milestone not found: {a.milestone}")
            milestone_id = m["id"]
        status = core.get_status(con, a.status)
        ceremony = a.ceremony or core.get_config_row(
            con, "workflow.ceremony_default",
            core.repo_config().get("workflow", {}).get("ceremony_default", "standard")) or "standard"
        if status["is_terminal"]:
            raise core.ProjectError("create issues in triage; close through `issue move` with evidence/reason")
        if status["key"] in ("review", "testing", "merging", "implementing"):
            raise core.ProjectError("create in triage/ready; claim and move explicitly")
        key = core.next_issue_key(con)
        kind = a.kind
        branch = core.branch_for(kind, key, a.title)
        details = _parse_details(a.detail)
        cur = con.execute(
            "INSERT INTO issue(key,parent_id,kind,title,description_md,purpose_md,reason_md,"
            "solution_idea_md,status_id,priority,confidence,impact,importance,foundational,"
            "complexity,mechanical,research_needed,time_criticality,ceremony,milestone_id,"
            "assignee,branch_name,details_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            (key, parent_id, kind, a.title, a.description or "", a.purpose or "", a.reason or "",
             a.solution or "", status["id"], a.priority, a.confidence, a.impact, a.importance,
             a.foundational, a.complexity, a.mechanical, a.research_needed, a.time_criticality,
             ceremony, milestone_id, a.assignee or "", branch, json.dumps(details)),
        )
        issue_id = cur.lastrowid
        core.audit(con, "issue", issue_id, "create", "", None,
                   {"key": key, "title": a.title, "kind": kind, "status": status["key"]},
                   actor=a.actor)
        for req_key in a.requirement or []:
            req = core.one(con, "SELECT id FROM requirement WHERE key=?", (req_key.upper(),))
            if not req:
                raise core.ProjectError(f"requirement not found: {req_key}")
            con.execute("INSERT OR IGNORE INTO issue_requirement(issue_id,requirement_id,relation) VALUES(?,?,?)",
                        (issue_id, req["id"], a.relation))
            core.audit(con, "issue", issue_id, "link_requirement", a.relation, None, req_key.upper(), actor=a.actor)
        for label_name in a.label or []:
            lab = core.one(con, "SELECT id FROM label WHERE name=?", (label_name,))
            if not lab:
                con.execute("INSERT INTO label(name) VALUES(?)", (label_name,))
                lab = core.one(con, "SELECT id FROM label WHERE name=?", (label_name,))
            con.execute("INSERT OR IGNORE INTO issue_label(issue_id,label_id) VALUES(?,?)", (issue_id, lab["id"]))
            core.audit(con, "issue", issue_id, "link_label", "label", None, label_name, actor=a.actor)
        for comp_name in a.component or []:
            comp = core.one(con, "SELECT id FROM component WHERE name=?", (comp_name,))
            if not comp:
                con.execute("INSERT INTO component(name) VALUES(?)", (comp_name,))
                comp = core.one(con, "SELECT id FROM component WHERE name=?", (comp_name,))
            con.execute("INSERT OR IGNORE INTO issue_component(issue_id,component_id) VALUES(?,?)", (issue_id, comp["id"]))
            core.audit(con, "issue", issue_id, "link_component", "component", None, comp_name, actor=a.actor)
        if status["key"] == "ready" and ceremony != "light":
            row = core.get_issue(con, key)
            core.assert_stage_gates(con, row, "ready")
        core.bump_graph(con)
        return {"key": key, "id": issue_id, "status": status["key"], "branch": branch}

    result = core.write(fn)
    core.emit(result, a.json)


def cmd_issue_edit(a):
    core.require_db()

    def fn(con):
        issue = core.get_issue(con, a.key)
        old = dict(issue)
        updates = core.parse_kv(a.set, core.ISSUE_FIELDS)
        if a.title is not None:
            updates["title"] = a.title
        if a.kind is not None:
            updates["kind"] = a.kind
        if a.parent is not None:
            updates["parent_id"] = core.get_issue(con, a.parent)["id"] if a.parent else None
        if a.milestone is not None:
            if a.milestone == "":
                updates["milestone_id"] = None
            else:
                m = core.one(con, "SELECT id FROM milestone WHERE key=?", (a.milestone,))
                if not m:
                    raise core.ProjectError(f"milestone not found: {a.milestone}")
                updates["milestone_id"] = m["id"]
        if a.details:
            details = json.loads(old.get("details_json") or "{}")
            details.update(_parse_details(a.details))
            updates["details_json"] = json.dumps(details, sort_keys=True)
        if "kind" in updates and updates["kind"] not in ("work", "bug", "spike", "chore"):
            raise core.ProjectError("kind must be work, bug, spike or chore")
        if "ceremony" in updates and updates["ceremony"] not in ("light", "standard", "full"):
            raise core.ProjectError("ceremony must be light, standard or full")
        if updates.get("ceremony") == "light" and not a.reason:
            raise core.ProjectError("lowering ceremony to light requires --reason")
        for restricted in ("blocked", "assignee", "branch_name", "worktree_path", "status_id"):
            if restricted in updates:
                raise core.ProjectError(f"use the dedicated command for {restricted}")
        if updates.get("complexity") is not None and updates["complexity"] not in (1, 2, 3, 5, 8, 13, 21):
            raise core.ProjectError("complexity must be one of 1,2,3,5,8,13,21")
        if "parent_id" in updates and updates["parent_id"] == issue["id"]:
            raise core.ProjectError("an issue cannot be its own parent")
        if "parent_id" in updates and updates["parent_id"]:
            parent = core.get_issue(con, updates["parent_id"])
            if parent["status_key"] in core.TERMINAL:
                raise core.ProjectError("cannot reparent under a terminal issue")
            # Reparenting must not create a hierarchy cycle.
            row = core.one(con, """WITH RECURSIVE up(id) AS (
                    SELECT ? UNION ALL SELECT i.parent_id FROM issue i JOIN up ON i.id=up.id
                    WHERE i.parent_id IS NOT NULL)
                SELECT 1 FROM up WHERE id=?""", (updates["parent_id"], issue["id"]))
            if row:
                raise core.ProjectError("reparent would create a hierarchy cycle")
        if not updates:
            raise core.ProjectError("nothing to change; use --set FIELD=VALUE")
        if not (updates.get("title", issue["title"]) or "").strip():
            raise core.ProjectError("issue title cannot be empty")
        sets = ", ".join(f"{k}=?" for k in updates)
        con.execute(f"UPDATE issue SET {sets}, updated_at=datetime('now') WHERE id=?",
                    (*updates.values(), issue["id"]))
        core.audit_fields(con, issue["id"], old, updates, reason=a.reason or "",
                          reason_code=a.reason_code or "", actor=a.actor)
        for req_key in getattr(a, "requirement", None) or []:
            req = core.one(con, "SELECT id FROM requirement WHERE key=?", (req_key.upper(),))
            if not req:
                raise core.ProjectError(f"requirement not found: {req_key}")
            con.execute("INSERT OR IGNORE INTO issue_requirement(issue_id,requirement_id,relation) VALUES(?,?,?)",
                        (issue["id"], req["id"], getattr(a, "relation", None) or "implements"))
            core.audit(con, "issue", issue["id"], "link_requirement", "requirement", None,
                       req_key.upper(), reason=a.reason or "", actor=a.actor)
        for req_key in getattr(a, "unlink_requirement", None) or []:
            req = core.one(con, "SELECT id FROM requirement WHERE key=?", (req_key.upper(),))
            if req:
                con.execute("DELETE FROM issue_requirement WHERE issue_id=? AND requirement_id=?",
                            (issue["id"], req["id"]))
                core.audit(con, "issue", issue["id"], "unlink_requirement", "requirement", req_key.upper(),
                           None, reason=a.reason or "", actor=a.actor)
        if any(k in updates for k in ("parent_id", "impact", "importance", "foundational",
                                      "complexity", "mechanical", "research_needed",
                                      "time_criticality", "confidence", "priority", "rank",
                                      "milestone_id")):
            core.bump_graph(con)
        new = core.rowdict(core.get_issue(con, issue["key"]))
        return {"key": new["key"], "changed": sorted(updates), "status": new["status_key"]}

    core.emit(core.write(fn), a.json)


def cmd_issue_move(a):
    core.require_db()

    def fn(con):
        issue = core.get_issue(con, a.key)
        target = core.get_status(con, a.status)
        current = core.get_status(con, issue["status_key"])
        if target["id"] == current["id"]:
            return {"key": issue["key"], "status": target["key"], "changed": False}
        reason = a.reason or ""
        if a.force and not reason:
            raise core.ProjectError("--force requires --reason")
        if target["requires_reason"] and not reason:
            raise core.ProjectError(f"moving to {target['key']} requires --reason")
        if current["is_terminal"] and not target["is_terminal"] and not reason:
            raise core.ProjectError("reopening a terminal issue requires --reason")
        if target["key"] == "duplicate":
            canonical = core.one(con, "SELECT 1 FROM issue_link WHERE src_id=? AND type='duplicates' LIMIT 1",
                                 (issue["id"],))
            if not canonical:
                raise core.ProjectError("link the canonical issue first: `issue link KEY duplicates CANONICAL`")
        if target["key"] == "done" and issue["open_child_count"]:
            raise core.ProjectError("cannot close a parent while children are open; close children first")
        if target["key"] == "done":
            core.assert_done_allowed(con, issue, evidence=a.evidence or "", force=a.force, reason=reason)
        if target["key"] in core.STATUS_ORDER and current["key"] in core.STATUS_ORDER:
            ci = core.STATUS_ORDER.index(current["key"])
            ti = core.STATUS_ORDER.index(target["key"])
            if ti < ci:
                if not reason:
                    raise core.ProjectError("backward transition requires --reason")
            elif ti > ci + 1:
                if not (a.force and reason):
                    raise core.ProjectError("skipping stages requires --force and --reason")
        if target["key"] == "ready" and not a.force:
            if issue["is_blocked"]:
                raise core.ProjectError("issue is blocked (run `project issue unblock` or use --force with a reason)")
            blockers = core.all_rows(con, """SELECT b.key FROM issue_link l JOIN v_issue b ON b.id=l.src_id
                WHERE l.dst_id=? AND l.type='blocks' AND b.archived_at IS NULL
                AND b.status_key NOT IN ('done','cancelled','duplicate')""", (issue["id"],))
            if blockers:
                raise core.ProjectError("issue has open blockers: " + ", ".join(b["key"] for b in blockers)
                                        + " (use --force with a reason to override)")
        if target["key"] == "implementing" and not a.force:
            if not issue["assignee"]:
                raise core.ProjectError("claim the issue (or pass --force) before implementation")
        core.assert_stage_gates(con, issue, target["key"], force=a.force, reason=reason, evidence=a.evidence or "")
        updates = {"status_id": target["id"], "status_entered_at": core.datetime.now().isoformat(timespec="seconds")}
        if target["key"] in ("implementing", "review", "testing", "merging") and not issue["first_started_at"]:
            updates["first_started_at"] = core.datetime.now().isoformat(timespec="seconds")
        if target["is_terminal"]:
            if target["key"] == "done":
                updates["completed_at"] = core.datetime.now().isoformat(timespec="seconds")
            else:
                updates["canceled_at"] = core.datetime.now().isoformat(timespec="seconds")
        else:
            updates["completed_at"] = None
            updates["canceled_at"] = None
        if target["key"] in core.STATUS_ORDER and current["key"] in core.STATUS_ORDER:
            if core.STATUS_ORDER.index(target["key"]) < core.STATUS_ORDER.index(current["key"]):
                updates["rework_count"] = int(issue["rework_count"] or 0) + 1
        backward = (current["key"] in core.STATUS_ORDER and target["key"] in core.STATUS_ORDER
                    and core.STATUS_ORDER.index(target["key"]) < core.STATUS_ORDER.index(current["key"]))
        if target["key"] not in ("done", "cancelled", "duplicate") and (current["is_terminal"] or backward):
            details = json.loads(issue["details_json"] or "{}")
            keys = ["merge_commit", "worktree_head", "evidence"]
            if details.get("post_merge_check") == "passed":
                keys.append("post_merge_check")
            cleared = {k: details.pop(k) for k in keys if k in details}
            if cleared:
                updates["details_json"] = json.dumps(details, sort_keys=True)
                core.audit(con, "issue", issue["id"], "rework", "details", None, cleared,
                           reason=reason, actor=a.actor)
            core.bump_evidence_epoch(con, issue["id"], a.actor)
        if a.evidence:
            details = json.loads(updates.get("details_json", issue["details_json"]) or "{}")
            details["evidence"] = a.evidence
            updates["details_json"] = json.dumps(details, sort_keys=True)
        sets = ", ".join(f"{k}=?" for k in updates)
        con.execute(f"UPDATE issue SET {sets}, updated_at=datetime('now') WHERE id=?",
                    (*updates.values(), issue["id"]))
        old = {"status_id": issue["status_id"]}
        core.audit(con, "issue", issue["id"], "status", "status_id", current["key"], target["key"],
                   reason_code=a.reason_code or ("rework" if updates.get("rework_count") else "move"),
                   reason=reason, actor=a.actor)
        if a.evidence:
            core.audit(con, "issue", issue["id"], "evidence", "evidence", None, a.evidence, actor=a.actor)
        core.bump_graph(con)
        return {"key": issue["key"], "from": current["key"], "to": target["key"],
                "rework_count": updates.get("rework_count", issue["rework_count"]), "changed": True}

    core.emit(core.write(fn), a.json)


def cmd_issue_acknowledge_check(a):
    core.require_db()
    if not (a.reason or "").strip():
        raise core.ProjectError("acknowledge-check requires --reason")

    def fn(con):
        issue = core.get_issue(con, a.key)
        details = json.loads(issue["details_json"] or "{}")
        pm = details.get("post_merge_check", "")
        if pm not in ("pending", "failed"):
            raise core.ProjectError("no pending/failed post-merge check to acknowledge")
        details["post_merge_check"] = "skipped"
        con.execute("UPDATE issue SET details_json=?, updated_at=datetime('now') WHERE id=?",
                    (json.dumps(details, sort_keys=True), issue["id"]))
        core.audit(con, "issue", issue["id"], "check", "post_merge_check", pm, "skipped",
                   reason=a.reason, actor=a.actor)
        return {"key": issue["key"], "post_merge_check": "skipped", "reason": a.reason}

    core.emit(core.write(fn), a.json)


def cmd_issue_claim(a):
    core.require_db()

    def fn(con):
        issue = core.get_issue(con, a.key)
        old = dict(issue)
        if issue["status_key"] in core.TERMINAL:
            raise core.ProjectError("cannot claim a terminal issue")
        if a.force and not getattr(a, "reason", ""):
            raise core.ProjectError("--force requires --reason")
        if issue["assignee"] == a.as_agent:
            return {"key": issue["key"], "assignee": a.as_agent, "already": True}
        if issue["assignee"] and issue["assignee"] != a.as_agent and not a.force:
            raise core.ProjectError(f"already claimed by {issue['assignee']} (use --force --reason to steal)")
        limit = int(core.get_config_row(con, "workflow.wip_limit",
                    core.repo_config().get("workflow", {}).get("wip_limit", 6)) or 0)
        if not a.force and limit > 0:
            n = core.one(con, """SELECT COUNT(*) n FROM v_issue WHERE assignee=? AND archived_at IS NULL
                AND status_category='active' AND id<>?""", (a.as_agent, issue["id"]))["n"]
            if n >= limit:
                raise core.ProjectError(f"WIP limit reached for {a.as_agent}: {n} active issues (limit {limit})")
        con.execute("UPDATE issue SET assignee=?, updated_at=datetime('now') WHERE id=?", (a.as_agent, issue["id"]))
        core.audit(con, "issue", issue["id"], "claim", "assignee", old["assignee"], a.as_agent,
                   reason=getattr(a, "reason", "") or "", actor=a.actor)
        return {"key": issue["key"], "assignee": a.as_agent}

    core.emit(core.write(fn), a.json)


def cmd_issue_release(a):
    core.require_db()

    def fn(con):
        issue = core.get_issue(con, a.key)
        core.audit(con, "issue", issue["id"], "release", "assignee", issue["assignee"], "", actor=a.actor)
        con.execute("UPDATE issue SET assignee='', updated_at=datetime('now') WHERE id=?", (issue["id"],))
        return {"key": issue["key"], "assignee": ""}

    core.emit(core.write(fn), a.json)


def cmd_issue_block(a):
    core.require_db()

    def fn(con):
        issue = core.get_issue(con, a.key)
        if not a.reason:
            raise core.ProjectError("--reason is required")
        con.execute("UPDATE issue SET blocked=1, blocked_reason=?, updated_at=datetime('now') WHERE id=?",
                    (a.reason, issue["id"]))
        core.audit(con, "issue", issue["id"], "block", "blocked", 0, 1, reason=a.reason, actor=a.actor)
        core.bump_graph(con)
        return {"key": issue["key"], "blocked": True, "reason": a.reason}

    core.emit(core.write(fn), a.json)


def cmd_issue_unblock(a):
    core.require_db()

    def fn(con):
        issue = core.get_issue(con, a.key)
        con.execute("UPDATE issue SET blocked=0, blocked_reason='', updated_at=datetime('now') WHERE id=?",
                    (issue["id"],))
        core.audit(con, "issue", issue["id"], "unblock", "blocked", 1, 0, reason=a.reason or "", actor=a.actor)
        core.bump_graph(con)
        return {"key": issue["key"], "blocked": False}

    core.emit(core.write(fn), a.json)


def cmd_issue_link(a):
    core.require_db()

    def fn(con):
        src = core.get_issue(con, a.src)
        dst = core.get_issue(con, a.dst)
        if src["id"] == dst["id"]:
            raise core.ProjectError("cannot link an issue to itself")
        if a.type == "blocks":
            row = core.one(con, """WITH RECURSIVE reach(id) AS (
                    SELECT dst_id FROM issue_link WHERE src_id=? AND type='blocks'
                    UNION SELECT l.dst_id FROM issue_link l JOIN reach r ON l.src_id=r.id WHERE l.type='blocks')
                SELECT 1 FROM reach WHERE id=?""", (dst["id"], src["id"]))
            if row:
                raise core.ProjectError("link would create a blocking cycle")
        cur = con.execute("INSERT OR IGNORE INTO issue_link(src_id,dst_id,type,created_by) VALUES(?,?,?,?)",
                          (src["id"], dst["id"], a.type, a.actor))
        if cur.rowcount == 0:
            return {"src": src["key"], "type": a.type, "dst": dst["key"], "changed": False}
        core.audit(con, "issue", src["id"], "link", a.type, None, dst["key"], reason=a.reason or "", actor=a.actor)
        core.audit(con, "issue", dst["id"], "linked_by", a.type, None, src["key"], reason=a.reason or "", actor=a.actor)
        if a.type == "blocks":
            core.bump_graph(con)
        return {"src": src["key"], "type": a.type, "dst": dst["key"], "changed": True}

    core.emit(core.write(fn), a.json)


def cmd_issue_unlink(a):
    core.require_db()

    def fn(con):
        src = core.get_issue(con, a.src)
        dst = core.get_issue(con, a.dst)
        cur = con.execute("DELETE FROM issue_link WHERE src_id=? AND dst_id=? AND type=?",
                          (src["id"], dst["id"], a.type))
        if cur.rowcount == 0:
            raise core.ProjectError("link not found")
        core.audit(con, "issue", src["id"], "unlink", a.type, dst["key"], None, actor=a.actor)
        core.audit(con, "issue", dst["id"], "unlinked_by", a.type, src["key"], None, actor=a.actor)
        if a.type == "blocks":
            core.bump_graph(con)
        return {"src": src["key"], "type": a.type, "dst": dst["key"], "removed": True}

    core.emit(core.write(fn), a.json)


def cmd_issue_note_add(a):
    core.require_db()

    def fn(con):
        issue = core.get_issue(con, a.key)
        body = a.body
        if a.body_file:
            body = core.Path(a.body_file).read_text()
        if not body.strip():
            raise core.ProjectError("note body is empty")
        cur = con.execute("INSERT INTO issue_note(issue_id,kind,body_md,confidence,author) VALUES(?,?,?,?,?)",
                          (issue["id"], a.kind, body, a.confidence, a.actor))
        core.audit(con, "issue", issue["id"], "note", a.kind, None, body[:500], actor=a.actor)
        return {"id": cur.lastrowid, "key": issue["key"], "kind": a.kind}

    core.emit(core.write(fn), a.json)


def cmd_issue_note_list(a):
    core.require_db()
    con = core.connect()
    try:
        issue = core.get_issue(con, a.key)
        rows = core.all_rows(con, "SELECT * FROM issue_note WHERE issue_id=? ORDER BY id", (issue["id"],))
    finally:
        con.close()
    if a.json:
        core.emit(rows, True)
    else:
        for r in rows:
            print(f"[{r['id']}] {r['created_at']} {r['kind']} ({r['confidence']}) by {r['author']}\n{r['body_md']}\n")


def cmd_issue_score(a):
    core.require_db()
    fields = {"impact": int, "importance": int, "foundational": int, "complexity": int,
              "mechanical": int, "research_needed": int, "time_criticality": int,
              "confidence": float, "priority": int, "rank": float, "score_rationale": str}
    updates = core.parse_kv(a.set, fields)
    if a.rationale:
        updates["score_rationale"] = a.rationale
    if not updates:
        raise core.ProjectError("nothing to score; use --set FIELD=VALUE or --rationale")

    def fn(con):
        issue = core.get_issue(con, a.key)
        old = dict(issue)
        if updates.get("complexity") is not None and updates["complexity"] not in (1, 2, 3, 5, 8, 13, 21):
            raise core.ProjectError("complexity must be one of 1,2,3,5,8,13,21")
        updates["scored_by"] = a.actor
        updates["scored_at"] = core.datetime.now().isoformat(timespec="seconds")
        sets = ", ".join(f"{k}=?" for k in updates)
        con.execute(f"UPDATE issue SET {sets}, updated_at=datetime('now') WHERE id=?",
                    (*updates.values(), issue["id"]))
        core.audit_fields(con, issue["id"], old, updates, reason=a.rationale or "",
                          reason_code="score", actor=a.actor)
        core.bump_graph(con)
        return {"key": issue["key"], "scored": sorted(updates)}

    core.emit(core.write(fn), a.json)


def cmd_issue_label(a):
    core.require_db()

    def fn(con):
        issue = core.get_issue(con, a.key)
        if a.action == "add":
            lab = core.one(con, "SELECT id FROM label WHERE name=?", (a.label,))
            if not lab:
                con.execute("INSERT INTO label(name) VALUES(?)", (a.label,))
                lab = core.one(con, "SELECT id FROM label WHERE name=?", (a.label,))
            con.execute("INSERT OR IGNORE INTO issue_label(issue_id,label_id) VALUES(?,?)", (issue["id"], lab["id"]))
        else:
            lab = core.one(con, "SELECT id FROM label WHERE name=?", (a.label,))
            if lab:
                con.execute("DELETE FROM issue_label WHERE issue_id=? AND label_id=?", (issue["id"], lab["id"]))
        core.audit(con, "issue", issue["id"], "label", a.action, None, a.label, actor=a.actor)
        return {"key": issue["key"], "label": a.label, "action": a.action}

    core.emit(core.write(fn), a.json)


def cmd_issue_component(a):
    core.require_db()

    def fn(con):
        issue = core.get_issue(con, a.key)
        if a.action == "add":
            comp = core.one(con, "SELECT id FROM component WHERE name=?", (a.component,))
            if not comp:
                con.execute("INSERT INTO component(name) VALUES(?)", (a.component,))
                comp = core.one(con, "SELECT id FROM component WHERE name=?", (a.component,))
            con.execute("INSERT OR IGNORE INTO issue_component(issue_id,component_id) VALUES(?,?)", (issue["id"], comp["id"]))
        else:
            comp = core.one(con, "SELECT id FROM component WHERE name=?", (a.component,))
            if comp:
                con.execute("DELETE FROM issue_component WHERE issue_id=? AND component_id=?", (issue["id"], comp["id"]))
        core.audit(con, "issue", issue["id"], "component", a.action, None, a.component, actor=a.actor)
        return {"key": issue["key"], "component": a.component, "action": a.action}

    core.emit(core.write(fn), a.json)


def cmd_issue_tree(a):
    core.require_db()
    con = core.connect()
    try:
        if a.key:
            root = core.get_issue(con, a.key)
            rows = core.all_rows(con, """WITH RECURSIVE tree(id,depth,path) AS (
                    SELECT id,0,key FROM issue WHERE id=?
                    UNION ALL SELECT i.id,t.depth+1,t.path||'/'||i.key FROM issue i
                    JOIN tree t ON i.parent_id=t.id WHERE i.archived_at IS NULL)
                SELECT t.depth,t.path,v.* FROM tree t JOIN v_issue v ON v.id=t.id ORDER BY t.path""", (root["id"],))
        else:
            rows = core.all_rows(con, """WITH RECURSIVE tree(id,depth,path) AS (
                    SELECT id,0,key FROM issue WHERE parent_id IS NULL AND archived_at IS NULL
                    UNION ALL SELECT i.id,t.depth+1,t.path||'/'||i.key FROM issue i
                    JOIN tree t ON i.parent_id=t.id WHERE i.archived_at IS NULL)
                SELECT t.depth,t.path,v.* FROM tree t JOIN v_issue v ON v.id=t.id ORDER BY t.path""")
    finally:
        con.close()
    if a.json:
        core.emit(rows, True)
        return
    for r in rows:
        mark = "!" if r["is_blocked"] else " "
        kids = f" ({r['open_child_count']}/{r['child_count']})" if r["child_count"] else ""
        print(f"{'  ' * r['depth']}{mark} {r['key']} [{r['status_key']}]{kids} {r['title']}")


def cmd_issue_show(a):
    core.require_db()
    con = core.connect()
    try:
        issue = core.get_issue(con, a.key)
        out = dict(issue)
        out["labels"] = [r["name"] for r in con.execute(
            "SELECT l.name FROM issue_label il JOIN label l ON l.id=il.label_id WHERE il.issue_id=? ORDER BY l.name",
            (issue["id"],))]
        out["components"] = [r["name"] for r in con.execute(
            "SELECT c.name FROM issue_component ic JOIN component c ON c.id=ic.component_id WHERE ic.issue_id=? ORDER BY c.name",
            (issue["id"],))]
        out["requirements"] = core.all_rows(con, """SELECT r.key,r.title,ir.relation FROM issue_requirement ir
            JOIN requirement r ON r.id=ir.requirement_id WHERE ir.issue_id=? ORDER BY r.key""", (issue["id"],))
        out["links"] = core.all_rows(con, """SELECT l.type, s.key AS src, d.key AS dst FROM issue_link l
            JOIN issue s ON s.id=l.src_id JOIN issue d ON d.id=l.dst_id
            WHERE l.src_id=? OR l.dst_id=? ORDER BY l.type,s.key""", (issue["id"], issue["id"]))
        out["notes"] = core.all_rows(con, "SELECT id,kind,confidence,author,created_at,body_md FROM issue_note WHERE issue_id=? ORDER BY id", (issue["id"],))
        out["events"] = core.all_rows(con, "SELECT id,at,actor,kind,field,old_json,new_json,reason FROM event WHERE entity_type='issue' AND entity_id=? ORDER BY id DESC LIMIT 30", (issue["id"],))
        test_marker, review_marker = core.evidence_markers(con, issue["id"])
        out["epoch_markers"] = {"test": test_marker, "review": review_marker}
        out["reviews"] = core.all_rows(con, """SELECT id,round,score,bar,status,summary,must_fix
            FROM review WHERE issue_id=? OR (scope='issue' AND subject=?) ORDER BY round,id""",
            (issue["id"], issue["key"]))
        for r in out["reviews"]:
            r["stale"] = r["id"] <= review_marker
        out["tests"] = core.all_rows(con, """SELECT DISTINCT t.id,t.kind,t.status,t.command,t.created_at
            FROM test_run t LEFT JOIN test_run_issue ti ON ti.run_id=t.id
            WHERE ti.issue_id=? OR (t.branch<>'' AND t.branch=?)
               OR t.id IN (SELECT run_id FROM test_result WHERE issue_id=?)
            ORDER BY t.id DESC LIMIT 10""", (issue["id"], issue["branch_name"], issue["id"]))
        for r in out["tests"]:
            r["stale"] = r["id"] <= test_marker
    finally:
        con.close()
    core.emit(out, a.json)


def cmd_issue_list(a):
    core.require_db()
    con = core.connect()
    try:
        sql = "SELECT * FROM v_issue WHERE archived_at IS NULL"
        args = []
        if a.status:
            sql += " AND status_key=?"
            args.append(a.status)
        if a.kind:
            sql += " AND kind=?"
            args.append(a.kind)
        if a.assignee:
            sql += " AND assignee=?"
            args.append(a.assignee)
        if a.milestone:
            sql += " AND milestone_id=(SELECT id FROM milestone WHERE key=?)"
            args.append(a.milestone)
        if a.parent:
            sql += " AND parent_id=(SELECT id FROM issue WHERE key=?)"
            args.append(a.parent)
        if a.blocked:
            sql += " AND is_blocked=1"
        if a.ready:
            sql += " AND status_key='ready' AND is_blocked=0"
        if a.leaf:
            sql += " AND child_count=0"
        if a.label:
            sql += " AND id IN (SELECT il.issue_id FROM issue_label il JOIN label l ON l.id=il.label_id WHERE l.name=?)"
            args.append(a.label)
        sql += " ORDER BY key"
        if a.limit:
            sql += " LIMIT ?"
            args.append(a.limit)
        rows = core.all_rows(con, sql, tuple(args))
    finally:
        con.close()
    if a.json:
        core.emit(rows, True)
    else:
        core.table(rows, ["key", "status_key", "kind", "title", "is_blocked", "assignee"])


def cmd_plan(a):
    core.require_db()
    if a.refresh:
        core.write(core.compute_plan)
    else:
        con = core.connect()
        try:
            core.ensure_plan(con)
        finally:
            con.close()
    con = core.connect()
    try:
        rows = core.plan_rows(con, limit=a.limit)
    finally:
        con.close()
    if a.json:
        core.emit(rows, True)
    else:
        core.table(rows, ["topo_rank", "key", "status_key", "effective_bucket", "ready", "actionable", "is_blocked", "title"])


def cmd_next(a):
    core.require_db()
    con = core.connect()
    try:
        core.ensure_plan(con)
        rows = core.plan_rows(con, limit=a.limit, ready_only=True)
        fallback = []
        if not rows:
            all_rows = core.plan_rows(con, limit=a.limit)
            fallback = [r for r in all_rows if r["actionable"] and not r["is_blocked"]][:a.limit]
    finally:
        con.close()
    if a.json:
        core.emit({"ready": rows, "fallback": fallback}, True)
    else:
        if rows:
            print("READY")
            core.table(rows, ["topo_rank", "key", "effective_bucket", "title"])
        else:
            print("Nothing is ready. Highest-ranked actionable issues (move them through triage/research/spec):")
            core.table(fallback, ["topo_rank", "key", "status_key", "effective_bucket", "title"])


def cmd_rescore(a):
    core.require_db()
    con = core.connect()
    try:
        if a.stale:
            rows = core.all_rows(con, "SELECT key,title,scored_at,impact,importance,complexity FROM v_stale_scores ORDER BY key")
        else:
            rows = core.all_rows(con, """SELECT key,title,scored_at,impact,importance,foundational,complexity,
                mechanical,research_needed,confidence FROM v_issue WHERE archived_at IS NULL
                AND status_key NOT IN ('done','cancelled','duplicate') ORDER BY key""")
    finally:
        con.close()
    if a.json:
        core.emit(rows, True)
    else:
        core.table(rows, list(rows[0].keys()) if rows else ["key"])


def cmd_label_add(a):
    core.require_db()

    def fn(con):
        con.execute("INSERT OR IGNORE INTO label(name,color,description) VALUES(?,?,?)",
                    (a.name, a.color or "", a.description or ""))
        return {"label": a.name}

    core.emit(core.write(fn), a.json)


def cmd_label_list(a):
    core.require_db()
    con = core.connect()
    try:
        rows = core.all_rows(con, "SELECT l.*, (SELECT COUNT(*) FROM issue_label il WHERE il.label_id=l.id) n FROM label l ORDER BY name")
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["name", "n", "description"])


def cmd_component_add(a):
    core.require_db()

    def fn(con):
        con.execute("INSERT OR IGNORE INTO component(name,description) VALUES(?,?)",
                    (a.name, a.description or ""))
        return {"component": a.name}

    core.emit(core.write(fn), a.json)


def cmd_component_list(a):
    core.require_db()
    con = core.connect()
    try:
        rows = core.all_rows(con, "SELECT c.*, (SELECT COUNT(*) FROM issue_component ic WHERE ic.component_id=c.id) n FROM component c ORDER BY name")
    finally:
        con.close()
    core.emit(rows, a.json) if a.json else core.table(rows, ["name", "n", "description"])
