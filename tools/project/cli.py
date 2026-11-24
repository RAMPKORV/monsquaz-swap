#!/usr/bin/env python3
"""CLI entry point. Run `python tools/project/cli.py --help`."""
from __future__ import annotations

import argparse
import os
import sqlite3
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

import core  # noqa: E402
import cmd_issues  # noqa: E402
import cmd_misc  # noqa: E402
import cmd_prd  # noqa: E402
import cmd_worktree  # noqa: E402


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="project", description="Canonical project state: PRD, issues, plan, evidence.")
    p.add_argument("--json", action="store_true", help="machine-readable output")
    p.add_argument("--workspace", help="workspace root (default: auto-detect)")
    p.add_argument("--actor", default=os.environ.get("PROJECT_ACTOR", "agent"), help="actor recorded in the ledger")
    sub = p.add_subparsers(dest="command", required=True)

    # init / health -----------------------------------------------------------
    x = sub.add_parser("init", help="create/upgrade the database")
    x.add_argument("--from-dump", action="store_true")
    x.add_argument("--dump")
    x.set_defaults(func=cmd_misc.cmd_init)

    x = sub.add_parser("rebuild", help="replace the database from the committed JSONL dump")
    x.add_argument("--dump")
    x.add_argument("--yes", action="store_true")
    x.add_argument("--force", action="store_true")
    x.add_argument("--reason")
    x.set_defaults(func=cmd_misc.cmd_rebuild)

    x = sub.add_parser("status", help="short project status")
    x.add_argument("--limit", type=int, default=8)
    x.set_defaults(func=cmd_misc.cmd_status)

    x = sub.add_parser("doctor", help="structural checks and export drift")
    x.add_argument("--strict", action="store_true")
    x.add_argument("--prd", action="store_true")
    x.add_argument("--no-export", dest="no_export", action="store_true")
    x.set_defaults(func=cmd_misc.cmd_doctor)

    x = sub.add_parser("export", help="write PRD, decisions, features, status and ledger dump")
    x.add_argument("--check", action="store_true")
    x.set_defaults(func=cmd_misc.cmd_export)

    x = sub.add_parser("report", help="export and write docs/STATUS.md")
    x.set_defaults(func=cmd_misc.cmd_report)

    x = sub.add_parser("sql", help="read-only SQL query")
    x.add_argument("query")
    x.set_defaults(func=cmd_misc.cmd_sql)

    x = sub.add_parser("stack", help="set the project stack and fill placeholders")
    xc = x.add_subparsers(dest="stack_command", required=True)
    y = xc.add_parser("set"); y.add_argument("stack"); y.set_defaults(func=cmd_misc.cmd_stack_set)

    x = sub.add_parser("config", help="get/set database config")
    xc = x.add_subparsers(dest="action", required=True)
    y = xc.add_parser("get"); y.add_argument("--key")
    y = xc.add_parser("set"); y.add_argument("key"); y.add_argument("value")
    x.set_defaults(func=cmd_misc.cmd_config)

    # issues ------------------------------------------------------------------
    issue = sub.add_parser("issue", help="hierarchical issues")
    ic = issue.add_subparsers(dest="issue_command", required=True)

    y = ic.add_parser("add")
    y.add_argument("--title", required=True)
    y.add_argument("--kind", default="work", choices=["work", "bug", "spike", "chore"])
    y.add_argument("--parent")
    y.add_argument("--description")
    y.add_argument("--purpose")
    y.add_argument("--reason")
    y.add_argument("--solution")
    y.add_argument("--status", default="triage")
    y.add_argument("--priority", type=int)
    y.add_argument("--confidence", type=float)
    y.add_argument("--impact", type=int)
    y.add_argument("--importance", type=int)
    y.add_argument("--foundational", type=int)
    y.add_argument("--complexity", type=int)
    y.add_argument("--mechanical", type=int)
    y.add_argument("--research-needed", dest="research_needed", type=int)
    y.add_argument("--time-criticality", dest="time_criticality", type=int)
    y.add_argument("--ceremony", choices=["light", "standard", "full"])
    y.add_argument("--milestone")
    y.add_argument("--assignee", default="")
    y.add_argument("--requirement", action="append")
    y.add_argument("--relation", default="implements", choices=["implements", "verifies", "relates", "blocks"])
    y.add_argument("--label", action="append")
    y.add_argument("--component", action="append")
    y.add_argument("--detail", action="append")
    y.set_defaults(func=cmd_issues.cmd_issue_add)

    y = ic.add_parser("list")
    y.add_argument("--status"); y.add_argument("--kind"); y.add_argument("--assignee")
    y.add_argument("--milestone"); y.add_argument("--parent"); y.add_argument("--label")
    y.add_argument("--blocked", action="store_true"); y.add_argument("--ready", action="store_true")
    y.add_argument("--leaf", action="store_true"); y.add_argument("--limit", type=int)
    y.set_defaults(func=cmd_issues.cmd_issue_list)

    y = ic.add_parser("show"); y.add_argument("key"); y.set_defaults(func=cmd_issues.cmd_issue_show)
    y = ic.add_parser("tree"); y.add_argument("--key"); y.set_defaults(func=cmd_issues.cmd_issue_tree)

    y = ic.add_parser("edit")
    y.add_argument("key"); y.add_argument("--title"); y.add_argument("--kind", choices=["work", "bug", "spike", "chore"])
    y.add_argument("--parent"); y.add_argument("--milestone"); y.add_argument("--set", action="append")
    y.add_argument("--requirement", action="append"); y.add_argument("--unlink-requirement", dest="unlink_requirement", action="append")
    y.add_argument("--relation", default="implements", choices=["implements", "verifies", "relates", "blocks"])
    y.add_argument("--details", action="append"); y.add_argument("--reason"); y.add_argument("--reason-code")
    y.set_defaults(func=cmd_issues.cmd_issue_edit)

    y = ic.add_parser("move")
    y.add_argument("key"); y.add_argument("status")
    y.add_argument("--reason"); y.add_argument("--reason-code"); y.add_argument("--force", action="store_true")
    y.add_argument("--evidence")
    y.set_defaults(func=cmd_issues.cmd_issue_move)

    y = ic.add_parser("acknowledge-check"); y.add_argument("key"); y.add_argument("--reason", required=True)
    y.set_defaults(func=cmd_issues.cmd_issue_acknowledge_check)
    y = ic.add_parser("claim"); y.add_argument("key"); y.add_argument("--as-agent", dest="as_agent", default="agent")
    y.add_argument("--force", action="store_true"); y.add_argument("--reason")
    y.set_defaults(func=cmd_issues.cmd_issue_claim)
    y = ic.add_parser("release"); y.add_argument("key"); y.set_defaults(func=cmd_issues.cmd_issue_release)

    y = ic.add_parser("block"); y.add_argument("key"); y.add_argument("--reason", required=True)
    y.set_defaults(func=cmd_issues.cmd_issue_block)
    y = ic.add_parser("unblock"); y.add_argument("key"); y.add_argument("--reason")
    y.set_defaults(func=cmd_issues.cmd_issue_unblock)

    y = ic.add_parser("link"); y.add_argument("src"); y.add_argument("type",
        choices=["blocks", "relates", "duplicates", "caused_by", "discovered_from", "validates", "supersedes"])
    y.add_argument("dst"); y.add_argument("--reason"); y.set_defaults(func=cmd_issues.cmd_issue_link)
    y = ic.add_parser("unlink"); y.add_argument("src"); y.add_argument("type",
        choices=["blocks", "relates", "duplicates", "caused_by", "discovered_from", "validates", "supersedes"])
    y.add_argument("dst"); y.set_defaults(func=cmd_issues.cmd_issue_unlink)

    note = ic.add_parser("note"); nc = note.add_subparsers(dest="note_command", required=True)
    y = nc.add_parser("add"); y.add_argument("key"); y.add_argument("--kind", default="comment",
        choices=["comment", "progress", "finding", "decision", "risk", "question", "handoff", "review", "retro"])
    y.add_argument("--body", default=""); y.add_argument("--body-file", dest="body_file")
    y.add_argument("--confidence", default="medium", choices=["high", "medium", "low"])
    y.set_defaults(func=cmd_issues.cmd_issue_note_add)
    y = nc.add_parser("list"); y.add_argument("key"); y.set_defaults(func=cmd_issues.cmd_issue_note_list)

    y = ic.add_parser("score"); y.add_argument("key"); y.add_argument("--set", action="append")
    y.add_argument("--rationale"); y.set_defaults(func=cmd_issues.cmd_issue_score)
    y = ic.add_parser("label"); y.add_argument("action", choices=["add", "remove"]); y.add_argument("key"); y.add_argument("label")
    y.set_defaults(func=cmd_issues.cmd_issue_label)
    y = ic.add_parser("component"); y.add_argument("action", choices=["add", "remove"]); y.add_argument("key"); y.add_argument("component")
    y.set_defaults(func=cmd_issues.cmd_issue_component)

    x = sub.add_parser("plan", help="persisted implementation order")
    x.add_argument("--refresh", action="store_true"); x.add_argument("--limit", type=int)
    x.set_defaults(func=cmd_issues.cmd_plan)
    x = sub.add_parser("next", help="next ready issues")
    x.add_argument("--limit", type=int, default=5); x.set_defaults(func=cmd_issues.cmd_next)
    x = sub.add_parser("rescore", help="list stale or all scores")
    x.add_argument("--stale", action="store_true"); x.set_defaults(func=cmd_issues.cmd_rescore)

    x = sub.add_parser("label"); lc = x.add_subparsers(dest="action", required=True)
    y = lc.add_parser("add"); y.add_argument("name"); y.add_argument("--color"); y.add_argument("--description")
    y.set_defaults(func=cmd_issues.cmd_label_add)
    y = lc.add_parser("list"); y.set_defaults(func=cmd_issues.cmd_label_list)
    x = sub.add_parser("component"); cc = x.add_subparsers(dest="action", required=True)
    y = cc.add_parser("add"); y.add_argument("name"); y.add_argument("--description")
    y.set_defaults(func=cmd_issues.cmd_component_add)
    y = cc.add_parser("list"); y.set_defaults(func=cmd_issues.cmd_component_list)

    # PRD ---------------------------------------------------------------------
    prd = sub.add_parser("prd", help="PRD, requirements and scenarios")
    pc = prd.add_subparsers(dest="prd_command", required=True)
    y = pc.add_parser("init"); y.add_argument("--title"); y.add_argument("--force", action="store_true")
    y.set_defaults(func=cmd_prd.cmd_prd_init)
    y = pc.add_parser("show"); y.set_defaults(func=cmd_prd.cmd_prd_show)
    y = pc.add_parser("coverage"); y.set_defaults(func=cmd_prd.cmd_prd_coverage)
    y = pc.add_parser("review"); y.add_argument("--reason"); y.set_defaults(func=cmd_prd.cmd_prd_review)
    y = pc.add_parser("export"); y.set_defaults(func=cmd_prd.cmd_prd_export)

    sec = pc.add_parser("section"); sc = sec.add_subparsers(dest="section_command", required=True)
    y = sc.add_parser("list"); y.set_defaults(func=cmd_prd.cmd_prd_section_list)
    y = sc.add_parser("set"); y.add_argument("key"); y.add_argument("--title"); y.add_argument("--body")
    y.add_argument("--body-file", dest="body_file"); y.add_argument("--status")
    y.add_argument("--position", type=int); y.add_argument("--reason"); y.set_defaults(func=cmd_prd.cmd_prd_section_set)

    req = pc.add_parser("req"); rc = req.add_subparsers(dest="req_command", required=True)
    y = rc.add_parser("add"); y.add_argument("--key"); y.add_argument("--area"); y.add_argument("--title", required=True)
    y.add_argument("--statement", required=True); y.add_argument("--kind", default="functional",
        choices=["functional", "nonfunctional", "constraint"])
    y.add_argument("--priority", default="should", choices=["must", "should", "could", "wont"])
    y.add_argument("--status", default="proposed", choices=["proposed", "active", "deferred", "deprecated"])
    y.add_argument("--section"); y.add_argument("--rationale"); y.add_argument("--reason")
    y.set_defaults(func=cmd_prd.cmd_requirement_add)
    y = rc.add_parser("edit"); y.add_argument("key"); y.add_argument("--title"); y.add_argument("--statement")
    y.add_argument("--kind", choices=["functional", "nonfunctional", "constraint"]); y.add_argument("--priority",
        choices=["must", "should", "could", "wont"]); y.add_argument("--status",
        choices=["proposed", "active", "deferred", "deprecated"]); y.add_argument("--section")
    y.add_argument("--rationale"); y.add_argument("--area"); y.add_argument("--reason")
    y.set_defaults(func=cmd_prd.cmd_requirement_edit)
    y = rc.add_parser("list"); y.add_argument("--priority"); y.add_argument("--missing-scenarios", dest="missing_scenarios", action="store_true")
    y.set_defaults(func=cmd_prd.cmd_requirement_list)
    y = rc.add_parser("show"); y.add_argument("key"); y.set_defaults(func=cmd_prd.cmd_requirement_show)

    scen = pc.add_parser("scenario"); snc = scen.add_subparsers(dest="scenario_command", required=True)
    y = snc.add_parser("add"); y.add_argument("--requirement", required=True); y.add_argument("--key")
    y.add_argument("--title", required=True); y.add_argument("--given"); y.add_argument("--when"); y.add_argument("--then")
    y.add_argument("--tags"); y.add_argument("--status", default="draft", choices=["draft", "active", "deprecated"])
    y.add_argument("--reason"); y.set_defaults(func=cmd_prd.cmd_scenario_add)
    y = snc.add_parser("edit"); y.add_argument("key"); y.add_argument("--title"); y.add_argument("--given")
    y.add_argument("--when"); y.add_argument("--then"); y.add_argument("--tags"); y.add_argument("--status",
        choices=["draft", "active", "deprecated"]); y.add_argument("--position", type=int); y.add_argument("--reason")
    y.set_defaults(func=cmd_prd.cmd_scenario_edit)
    y = snc.add_parser("list"); y.add_argument("--requirement"); y.set_defaults(func=cmd_prd.cmd_scenario_list)

    q = pc.add_parser("question"); qc = q.add_subparsers(dest="question_command", required=True)
    y = qc.add_parser("add"); y.add_argument("--question", required=True); y.add_argument("--section"); y.add_argument("--why")
    y.add_argument("--impact", default="medium", choices=["high", "medium", "low"]); y.add_argument("--reason")
    y.set_defaults(func=cmd_prd.cmd_question_add)
    y = qc.add_parser("list"); y.add_argument("--open", action="store_true"); y.set_defaults(func=cmd_prd.cmd_question_list)
    y = qc.add_parser("answer"); y.add_argument("id", type=int); y.add_argument("--answer", required=True)
    y.add_argument("--reason"); y.set_defaults(func=cmd_prd.cmd_question_answer)
    y = qc.add_parser("defer"); y.add_argument("id", type=int); y.add_argument("--reason")
    y.set_defaults(func=cmd_prd.cmd_question_defer)

    y = pc.add_parser("signoff"); y.add_argument("--by"); y.add_argument("--note"); y.add_argument("--reason")
    y.add_argument("--force", action="store_true"); y.set_defaults(func=cmd_prd.cmd_prd_signoff)
    y = pc.add_parser("revise"); y.add_argument("--reason", required=True); y.set_defaults(func=cmd_prd.cmd_prd_revise)

    # assumptions / risks / decisions ---------------------------------------
    a = sub.add_parser("assumption"); ac = a.add_subparsers(dest="assumption_command", required=True)
    y = ac.add_parser("add"); y.add_argument("--key"); y.add_argument("--statement", required=True)
    y.add_argument("--confidence", type=float, default=0.5); y.add_argument("--impact", default="medium",
        choices=["high", "medium", "low"]); y.add_argument("--status", default="open",
        choices=["open", "validated", "invalidated", "deferred"]); y.add_argument("--owner"); y.add_argument("--revisit")
    y.add_argument("--evidence"); y.add_argument("--reason"); y.set_defaults(func=cmd_prd.cmd_assumption_add)
    y = ac.add_parser("list"); y.add_argument("--open", action="store_true"); y.set_defaults(func=cmd_prd.cmd_assumption_list)
    y = ac.add_parser("resolve"); y.add_argument("key"); y.add_argument("--status", required=True,
        choices=["validated", "invalidated", "deferred"]); y.add_argument("--evidence"); y.add_argument("--reason")
    y.add_argument("--revisit")
    y.set_defaults(func=cmd_prd.cmd_assumption_resolve)

    r = sub.add_parser("risk"); rkc = r.add_subparsers(dest="risk_command", required=True)
    y = rkc.add_parser("add"); y.add_argument("--key"); y.add_argument("--title", required=True); y.add_argument("--description")
    y.add_argument("--likelihood", type=int, default=3); y.add_argument("--impact", type=int, default=3)
    y.add_argument("--mitigation"); y.add_argument("--trigger"); y.add_argument("--status", default="open",
        choices=["open", "mitigated", "accepted", "realized", "closed"]); y.add_argument("--owner")
    y.add_argument("--reason"); y.set_defaults(func=cmd_prd.cmd_risk_add)
    y = rkc.add_parser("list"); y.set_defaults(func=cmd_prd.cmd_risk_list)
    y = rkc.add_parser("update"); y.add_argument("key"); y.add_argument("--status",
        choices=["open", "mitigated", "accepted", "realized", "closed"]); y.add_argument("--mitigation")
    y.add_argument("--trigger"); y.add_argument("--likelihood", type=int); y.add_argument("--impact", type=int)
    y.add_argument("--owner"); y.add_argument("--reason"); y.set_defaults(func=cmd_prd.cmd_risk_update)

    d = sub.add_parser("decision"); dc = d.add_subparsers(dest="decision_command", required=True)
    y = dc.add_parser("add"); y.add_argument("--code"); y.add_argument("--title", required=True)
    y.add_argument("--context"); y.add_argument("--decision"); y.add_argument("--consequences")
    y.add_argument("--status", default="accepted", choices=["proposed", "accepted", "superseded", "rejected"])
    y.add_argument("--supersedes"); y.add_argument("--prd-ref", dest="prd_ref"); y.add_argument("--issue")
    y.add_argument("--requirement"); y.add_argument("--by"); y.add_argument("--revisit")
    y.set_defaults(func=cmd_prd.cmd_decision_add)
    y = dc.add_parser("list"); y.add_argument("--status"); y.set_defaults(func=cmd_prd.cmd_decision_list)
    y = dc.add_parser("show"); y.add_argument("code"); y.set_defaults(func=cmd_prd.cmd_decision_show)
    y = dc.add_parser("supersede"); y.add_argument("old"); y.add_argument("--code"); y.add_argument("--title", required=True)
    y.add_argument("--context"); y.add_argument("--decision"); y.add_argument("--consequences"); y.add_argument("--by")
    y.add_argument("--revisit"); y.add_argument("--prd-ref", dest="prd_ref")
    y.add_argument("--status", default="accepted", choices=["proposed", "accepted", "superseded", "rejected"])
    y.set_defaults(func=lambda args: _supersede(args))

    # evidence ----------------------------------------------------------------
    s = sub.add_parser("source"); sc2 = s.add_subparsers(dest="source_command", required=True)
    y = sc2.add_parser("add"); y.add_argument("--title", required=True); y.add_argument("--kind", default="web",
        choices=["web", "paper", "book", "docs", "video", "local", "experiment", "other"])
    y.add_argument("--url"); y.add_argument("--artifact"); y.add_argument("--reliability", default="medium",
        choices=["high", "medium", "low", "unknown"]); y.add_argument("--summary"); y.add_argument("--tags")
    y.set_defaults(func=cmd_prd.cmd_source_add)
    y = sc2.add_parser("list"); y.set_defaults(func=cmd_prd.cmd_source_list)

    f = sub.add_parser("finding"); fc = f.add_subparsers(dest="finding_command", required=True)
    y = fc.add_parser("add"); y.add_argument("--claim", required=True); y.add_argument("--source", type=int)
    y.add_argument("--issue"); y.add_argument("--requirement"); y.add_argument("--category"); y.add_argument("--quote")
    y.add_argument("--confidence", default="medium", choices=["high", "medium", "low"]); y.set_defaults(func=cmd_prd.cmd_finding_add)
    y = fc.add_parser("list"); y.add_argument("--issue"); y.set_defaults(func=cmd_prd.cmd_finding_list)

    # reviews / milestones / human / sessions / agents / tests ---------------
    rv = sub.add_parser("review"); rvc = rv.add_subparsers(dest="review_command", required=True)
    y = rvc.add_parser("add"); y.add_argument("--scope", default="issue",
        choices=["issue", "milestone", "prd", "design", "code", "test", "security", "other"])
    y.add_argument("--subject", required=True); y.add_argument("--round", type=int); y.add_argument("--score", type=int)
    y.add_argument("--bar", type=int); y.add_argument("--rubric"); y.add_argument("--summary")
    y.add_argument("--must-fix", dest="must_fix"); y.add_argument("--reviewer"); y.add_argument("--commit")
    y.add_argument("--issue"); y.add_argument("--milestone"); y.set_defaults(func=cmd_misc.cmd_review_add)
    y = rvc.add_parser("list"); y.add_argument("--open", action="store_true"); y.set_defaults(func=cmd_misc.cmd_review_list)
    y = rvc.add_parser("resolve"); y.add_argument("id", type=int); y.add_argument("--status", default="addressed",
        choices=["addressed", "wontfix"]); y.add_argument("--commit"); y.add_argument("--reason")
    y.set_defaults(func=cmd_misc.cmd_review_resolve)

    m = sub.add_parser("milestone"); mc = m.add_subparsers(dest="milestone_command", required=True)
    y = mc.add_parser("add"); y.add_argument("--key"); y.add_argument("--name", required=True)
    y.add_argument("--description"); y.add_argument("--target"); y.add_argument("--dod"); y.add_argument("--bar", type=int, default=9)
    y.set_defaults(func=cmd_misc.cmd_milestone_add)
    y = mc.add_parser("list"); y.set_defaults(func=cmd_misc.cmd_milestone_list)
    y = mc.add_parser("update"); y.add_argument("key"); y.add_argument("--name"); y.add_argument("--target")
    y.add_argument("--dod"); y.add_argument("--status", choices=["planned", "in_progress", "blocked", "completed", "cancelled"])
    y.add_argument("--evidence"); y.add_argument("--force", action="store_true"); y.add_argument("--reason")
    y.set_defaults(func=cmd_misc.cmd_milestone_update)

    h = sub.add_parser("human"); hc = h.add_subparsers(dest="human_command", required=True)
    y = hc.add_parser("add"); y.add_argument("--title", required=True); y.add_argument("--kind", default="question",
        choices=["question", "review", "taste", "approval", "playtest", "research"]); y.add_argument("--issue")
    y.add_argument("--context"); y.add_argument("--how-to", dest="how_to"); y.add_argument("--options")
    y.set_defaults(func=cmd_misc.cmd_human_add)
    y = hc.add_parser("list"); y.add_argument("--open", action="store_true"); y.set_defaults(func=cmd_misc.cmd_human_list)
    y = hc.add_parser("show"); y.add_argument("id", type=int); y.set_defaults(func=cmd_misc.cmd_human_show)
    y = hc.add_parser("ask"); y.add_argument("id", type=int); y.set_defaults(func=cmd_misc.cmd_human_ask)
    y = hc.add_parser("answer"); y.add_argument("id", type=int); y.add_argument("--answer", required=True)
    y.set_defaults(func=cmd_misc.cmd_human_answer)
    y = hc.add_parser("defer"); y.add_argument("id", type=int); y.add_argument("--reason"); y.add_argument("--until")
    y.set_defaults(func=cmd_misc.cmd_human_defer)
    y = hc.add_parser("cancel"); y.add_argument("id", type=int); y.add_argument("--reason")
    y.set_defaults(func=cmd_misc.cmd_human_cancel)

    se = sub.add_parser("session"); sec2 = se.add_subparsers(dest="session_command", required=True)
    y = sec2.add_parser("start"); y.add_argument("--as-agent", "--agent", dest="as_agent", default="agent")
    y.set_defaults(func=cmd_misc.cmd_session_start)
    y = sec2.add_parser("end"); y.add_argument("--summary"); y.add_argument("--handoff")
    y.add_argument("--next-issue", dest="next_issue"); y.add_argument("--commit")
    y.set_defaults(func=cmd_misc.cmd_session_end)
    y = sec2.add_parser("list"); y.add_argument("--limit", type=int, default=10); y.set_defaults(func=cmd_misc.cmd_session_list)

    ag = sub.add_parser("agent"); agc = ag.add_subparsers(dest="agent_command", required=True)
    y = agc.add_parser("add"); y.add_argument("--as-agent", dest="as_agent", required=True)
    y.add_argument("--purpose", required=True); y.add_argument("--model"); y.add_argument("--issue")
    y.set_defaults(func=cmd_misc.cmd_agent_add)
    y = agc.add_parser("done"); y.add_argument("id", type=int); y.add_argument("--status", default="done",
        choices=["done", "failed", "cancelled"]); y.add_argument("--outcome"); y.add_argument("--artifact")
    y.set_defaults(func=cmd_misc.cmd_agent_done)
    y = agc.add_parser("list"); y.add_argument("--limit", type=int, default=20); y.set_defaults(func=cmd_misc.cmd_agent_list)

    t = sub.add_parser("test"); tc = t.add_subparsers(dest="test_command", required=True)
    y = tc.add_parser("record"); y.add_argument("--kind", default="test",
        choices=["unit", "integration", "e2e", "acceptance", "mutation", "fuzz", "manual", "ci", "smoke", "test"])
    y.add_argument("--status", required=True, choices=["pass", "fail", "error", "skipped"])
    y.add_argument("--command"); y.add_argument("--checks", type=int, default=0); y.add_argument("--failed", type=int, default=0)
    y.add_argument("--commit"); y.add_argument("--branch"); y.add_argument("--output"); y.add_argument("--issue")
    y.set_defaults(func=cmd_misc.cmd_test_record)
    y = tc.add_parser("list"); y.add_argument("--limit", type=int, default=20); y.set_defaults(func=cmd_misc.cmd_test_list)
    y = tc.add_parser("result"); y.add_argument("--run", type=int, required=True); y.add_argument("--scenario")
    y.add_argument("--issue"); y.add_argument("--status", required=True, choices=["pass", "fail", "error", "skipped"])
    y.add_argument("--duration", type=int); y.add_argument("--error"); y.set_defaults(func=cmd_misc.cmd_test_result)

    # worktrees ---------------------------------------------------------------
    w = sub.add_parser("worktree"); wc = w.add_subparsers(dest="worktree_command", required=True)
    y = wc.add_parser("new"); y.add_argument("key"); y.add_argument("--base"); y.add_argument("--as-agent", dest="as_agent")
    y.add_argument("--no-setup", dest="no_setup", action="store_true"); y.add_argument("--force", action="store_true")
    y.add_argument("--reason"); y.set_defaults(func=cmd_worktree.cmd_worktree_new)
    y = wc.add_parser("sync"); y.add_argument("key"); y.set_defaults(func=cmd_worktree.cmd_worktree_sync)
    y = wc.add_parser("finish"); y.add_argument("key"); y.add_argument("--yes", action="store_true")
    y.add_argument("--message"); y.add_argument("--no-check", dest="no_check", action="store_true")
    y.add_argument("--evidence"); y.add_argument("--force", action="store_true"); y.add_argument("--reason")
    y.add_argument("--as-agent", dest="as_agent")
    y.set_defaults(func=cmd_worktree.cmd_worktree_finish)
    y = wc.add_parser("recover"); y.add_argument("key"); y.add_argument("--reason", required=True)
    y.set_defaults(func=cmd_worktree.cmd_worktree_recover)
    y = wc.add_parser("list"); y.set_defaults(func=cmd_worktree.cmd_worktree_list)
    y = wc.add_parser("prune"); y.set_defaults(func=cmd_worktree.cmd_worktree_prune)

    return p


def _supersede(args):
    old = args.old
    ns = argparse.Namespace(**{**vars(args), "supersedes": old, "requirement": None,
                               "issue": None, "status": getattr(args, "status", "accepted")})
    return cmd_prd.cmd_decision_add(ns)


def main(argv=None) -> int:
    parser = build_parser()
    args, unknown = parser.parse_known_args(argv)
    if "--json" in unknown:
        args.json = True
        unknown.remove("--json")
    if unknown:
        parser.error("unrecognized arguments: " + " ".join(unknown))
    if args.workspace:
        core._workspace_override = args.workspace
    try:
        args.func(args)
        return 0
    except core.ProjectError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    except sqlite3.Error as exc:
        print(f"database error: {exc}", file=sys.stderr)
        return 3
    except KeyboardInterrupt:
        return 130


if __name__ == "__main__":
    raise SystemExit(main())
