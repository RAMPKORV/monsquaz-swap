"""Git worktree lifecycle commands."""
from __future__ import annotations

import json
import os
import subprocess
import sys

try:
    from . import core
except ImportError:
    import core


def _git(args, cwd=None, check=True):
    p = subprocess.run(["git", *args], cwd=str(cwd or core.REPO_ROOT),
                       text=True, capture_output=True)
    if check and p.returncode != 0:
        raise core.ProjectError(f"git {' '.join(args)} failed:\n{p.stderr.strip() or p.stdout.strip()}")
    return p


def main_repo() -> core.Path:
    cfg = core.ws_config()
    if cfg.get("repo"):
        return core.Path(cfg["repo"])
    return core.REPO_ROOT


def main_branch() -> str:
    return core.repo_config().get("git", {}).get("main_branch", "main")


def worktrees_dir() -> core.Path:
    cfg = core.ws_config()
    if cfg.get("worktrees_dir"):
        return core.Path(cfg["worktrees_dir"])
    return core.workspace_root() / "worktrees"


GENERATED_PREFIXES = ("docs/PRD.md", "docs/DECISIONS.md", "docs/STATUS.md",
                      "tests/features/", "project/ledger/")


def branch_new_source(main: core.Path, main_branch_name: str, branch: str) -> list[str]:
    out = _git(["diff", "--name-only", main_branch_name, branch], cwd=main, check=False).stdout
    return [line for line in out.splitlines()
            if line.strip() and not line.startswith(GENERATED_PREFIXES)]


def export_main(main: core.Path) -> None:
    cli = main / "tools" / "project" / "cli.py"
    p = subprocess.run([sys.executable, str(cli), "export"], cwd=str(main),
                       text=True, capture_output=True)
    if p.returncode != 0:
        raise core.ProjectError("completion export failed in main:\n" + (p.stderr or p.stdout).strip())


def export_worktree(wt: core.Path) -> None:
    cli = wt / "tools" / "project" / "cli.py"
    env = dict(os.environ, PROJECT_WORKSPACE=str(core.workspace_root()))
    p = subprocess.run([sys.executable, str(cli), "export"], cwd=str(wt), env=env,
                       text=True, capture_output=True)
    if p.returncode != 0:
        raise core.ProjectError("pre-merge export failed in the worktree:\n" + (p.stderr or p.stdout).strip())


def resolve_wt(issue) -> core.Path:
    raw = issue["worktree_path"] or ""
    if not raw:
        raise core.ProjectError(f"{issue['key']} has no recorded worktree")
    p = core.Path(raw)
    return p if p.is_absolute() else core.workspace_root() / p


def _prd_ok(con, force: bool) -> tuple[bool, str]:
    doc = core.one(con, "SELECT * FROM prd_document WHERE id=1")
    validation, note = core.prd_validation(con)
    if doc["status"] != "signed_off":
        if not force:
            raise core.ProjectError("PRD is not signed off; finish the interrogation/sign-off first "
                                    "(or --force with a reason)")
        return False, "PRD not signed off"
    if validation == "unvalidated":
        if not force:
            raise core.ProjectError("PRD is v0.1 unvalidated; resolve assumptions and re-sign, "
                                    "or --force with a reason to accept the risk")
        return False, "PRD v0.1 unvalidated" + (f": {note}" if note else "")
    return True, ""


def cmd_worktree_new(a):
    core.require_db()
    if a.force and not getattr(a, "reason", ""):
        raise core.ProjectError("--force requires --reason")
    if not getattr(a, "as_agent", None):
        raise core.ProjectError("worktree new requires --as-agent <name>")
    con = core.connect()
    try:
        issue = core.get_issue(con, a.key)
        if issue["assignee"] != a.as_agent and not (a.force and a.reason):
            raise core.ProjectError(f"issue is owned by {issue['assignee'] or 'nobody'}; "
                                    "claim it or use --force --reason")
        if issue["status_key"] in core.TERMINAL:
            raise core.ProjectError("cannot create a worktree for a terminal issue")
        if issue["status_key"] not in ("ready", "implementing"):
            raise core.ProjectError("move the issue to ready (or implementing) before creating a worktree")
        if issue["worktree_path"]:
            old_wt = resolve_wt(issue)
            if old_wt.exists():
                raise core.ProjectError(f"{issue['key']} already has a worktree: {issue['worktree_path']}")
            listed = _git(["worktree", "list", "--porcelain"], cwd=main_repo(), check=False).stdout
            if str(old_wt) in listed:
                raise core.ProjectError(f"{issue['key']} worktree is still registered: {issue['worktree_path']}")
            if not (a.force and getattr(a, "reason", "")):
                raise core.ProjectError("recorded worktree is missing; run `worktree recover KEY --reason ...` "
                                        "or use --force --reason to recreate")
            def clear_missing(c):
                issue2 = core.get_issue(c, issue["key"])
                details2 = json.loads(issue2["details_json"] or "{}")
                for k in ("merge_commit", "worktree_head", "evidence"):
                    details2.pop(k, None)
                if details2.get("post_merge_check") == "passed":
                    details2.pop("post_merge_check", None)
                c.execute("UPDATE issue SET worktree_path='', details_json=? WHERE id=?",
                          (json.dumps(details2, sort_keys=True), issue2["id"]))
                core.audit(c, "issue", issue2["id"], "worktree", "worktree_path",
                           issue2["worktree_path"], "",
                           reason="clear missing worktree before recreate", actor=a.actor)
                core.bump_evidence_epoch(c, issue2["id"], a.actor)
                return core.get_issue(c, issue["key"])
            issue = core.write(clear_missing)
        if issue["child_count"]:
            raise core.ProjectError("work happens on leaf issues, not parent containers")
        if issue["is_blocked"]:
            raise core.ProjectError("issue is blocked; unblock it before creating a worktree")
        if not a.as_agent:
            raise core.ProjectError("worktree new requires --as-agent <name>")
        if issue["assignee"] and issue["assignee"] != a.as_agent and not a.force:
            raise core.ProjectError(f"issue is claimed by {issue['assignee']}; use --force --reason to steal")
        limit = int(core.get_config_row(con, "workflow.wip_limit",
                    core.repo_config().get("workflow", {}).get("wip_limit", 6)) or 0)
        if limit > 0 and not a.force:
            n = core.one(con, """SELECT COUNT(*) n FROM v_issue WHERE assignee=? AND archived_at IS NULL
                AND status_category='active' AND id<>?""", (a.as_agent, issue["id"]))["n"]
            if n >= limit:
                raise core.ProjectError(f"WIP limit reached for {a.as_agent}: {n} active issues (limit {limit})")
        _, warning = _prd_ok(con, a.force)
    finally:
        con.close()
    branch = issue["branch_name"] or core.branch_for(issue["kind"], issue["key"], issue["title"])
    wt = worktrees_dir() / f"{issue['key']}-{core.slugify(issue['title'])}"
    adopted = False
    if wt.exists():
        listing = _git(["worktree", "list", "--porcelain"], cwd=main_repo(), check=False).stdout
        if str(wt) in listing and not issue["worktree_path"]:
            wt_branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=wt, check=False).stdout.strip()
            if wt_branch != branch:
                raise core.ProjectError(f"existing worktree is on {wt_branch!r}, expected {branch!r}")
            adopted = True
        else:
            raise core.ProjectError(f"worktree path already exists: {wt}")
    base = a.base or main_branch()
    if not adopted:
        exists = _git(["rev-parse", "--verify", branch], cwd=main_repo(), check=False).returncode == 0
        if exists:
            _git(["worktree", "add", str(wt), branch], cwd=main_repo())
        else:
            _git(["worktree", "add", "-b", branch, str(wt), base], cwd=main_repo())
    rel_wt = str(wt.relative_to(core.workspace_root())) if wt.is_relative_to(core.workspace_root()) else str(wt)

    def fn(con):
        issue2 = core.get_issue(con, issue["key"])
        old = dict(issue2)
        updates = {"branch_name": branch, "worktree_path": rel_wt}
        if a.as_agent:
            updates["assignee"] = a.as_agent
        status = core.get_status(con, "implementing")
        if issue2["status_key"] == "ready":
            updates["status_id"] = status["id"]
            updates["status_entered_at"] = core.datetime.now().isoformat(timespec="seconds")
            updates["first_started_at"] = core.datetime.now().isoformat(timespec="seconds")
        sets = ", ".join(f"{k}=?" for k in updates)
        con.execute(f"UPDATE issue SET {sets}, updated_at=datetime('now') WHERE id=?",
                    (*updates.values(), issue2["id"]))
        core.audit_fields(con, issue2["id"], old, updates, reason="worktree created", actor=a.actor)
        if a.force:
            core.audit(con, "issue", issue2["id"], "force", "worktree_new", None, a.reason,
                       reason=a.reason, actor=a.actor)
        core.bump_graph(con)
        return {"key": issue2["key"], "branch": branch, "worktree": rel_wt,
                "status": "implementing" if "status_id" in updates else issue2["status_key"]}

    result = core.write(fn)
    if warning:
        result["warning"] = warning
    setup = wt / "scripts/worktree-setup.sh"
    if setup.exists() and os.access(setup, os.X_OK) and not a.no_setup:
        env = dict(os.environ, PROJECT_WORKSPACE=str(core.workspace_root()),
                   PROJECT_ISSUE=issue["key"], PROJECT_BRANCH=branch)
        p = subprocess.run([str(setup), str(wt)], cwd=str(wt), env=env, text=True,
                           capture_output=True)
        result["setup_exit"] = p.returncode
        result["setup_output"] = (p.stdout + p.stderr).strip()[-2000:]
        if p.returncode != 0:
            result["warning"] = "worktree setup hook failed; worktree kept for inspection"
    core.emit(result, a.json)


def cmd_worktree_sync(a):
    core.require_db()
    con = core.connect()
    try:
        issue = core.get_issue(con, a.key)
    finally:
        con.close()
    wt = resolve_wt(issue)
    if not wt.exists():
        raise core.ProjectError(f"worktree not found: {wt}")
    p = _git(["merge", main_branch(), "--no-edit"], cwd=wt, check=False)
    if p.returncode != 0:
        conflicts = _git(["diff", "--name-only", "--diff-filter=U"], cwd=wt, check=False).stdout.strip()
        raise core.ProjectError("merge from main hit conflicts"
                                + (f":\n{conflicts}" if conflicts else "")
                                + "\nResolve them, then run `git add` and `git commit`.")
    core.emit({"key": issue["key"], "worktree": str(wt), "merged": main_branch(),
               "output": (p.stdout or "").strip()}, a.json)


def cmd_worktree_finish(a):
    core.require_db()
    if a.force and not a.reason:
        raise core.ProjectError("--force requires --reason")
    if not a.yes:
        raise core.ProjectError("finish is destructive; pass --yes after reviewing the preflight")
    if not getattr(a, "as_agent", None):
        raise core.ProjectError("worktree finish requires --as-agent <name>")
    main = main_repo()
    if core.REPO_ROOT.resolve() != main.resolve():
        raise core.ProjectError("run `worktree finish` from the main checkout, not from the worktree")
    con = core.connect()
    try:
        issue = core.get_issue(con, a.key)
        if issue["assignee"] != a.as_agent and not (a.force and a.reason):
            raise core.ProjectError(f"issue is owned by {issue['assignee'] or 'nobody'}; "
                                    "claim it or use --force --reason")
        if issue["status_key"] in core.TERMINAL:
            details = json.loads(issue["details_json"] or "{}")
            if not issue["worktree_path"] and details.get("merge_commit"):
                export_main(main)
                _git(["add", "-A"], cwd=main, check=False)
                _git(["commit", "-m", f"chore: reconcile {issue['key']}"], cwd=main, check=False)
                core.emit({"key": issue["key"], "status": issue["status_key"],
                           "already_done": True, "merge_commit": details["merge_commit"]}, a.json)
                return
            raise core.ProjectError("issue is already terminal")
        if issue["status_key"] not in ("review", "testing", "merging") and not a.force:
            raise core.ProjectError("move the issue to review/testing/merging first "
                                    "(or --force with a reason)")
        core.assert_done_allowed(con, issue, evidence=a.evidence or "", force=a.force,
                                 reason=a.reason or "", require_merge=False)
        _, prd_warning = _prd_ok(con, a.force)
        details = json.loads(issue["details_json"] or "{}")
        already = details.get("merge_commit", "")
        stored_head = details.get("worktree_head", "")
    finally:
        con.close()

    wt = resolve_wt(issue)
    if not wt.exists():
        raise core.ProjectError(f"worktree not found: {wt}; run `worktree recover` if it was lost")
    head_branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=main, check=True).stdout.strip()
    if head_branch != main_branch():
        raise core.ProjectError(f"main checkout is on {head_branch!r}, not {main_branch()!r}; "
                                "checkout the main branch before finishing")
    dirty = _git(["status", "--porcelain"], cwd=main, check=True).stdout.strip()
    if dirty:
        raise core.ProjectError("main checkout is dirty; commit/stash before finishing an issue:\n" + dirty)
    wt_dirty = _git(["status", "--porcelain"], cwd=wt, check=True).stdout.strip()
    if wt_dirty:
        raise core.ProjectError("worktree has uncommitted changes; commit/stash them before finish:\n" + wt_dirty)
    branch = issue["branch_name"] or _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=wt).stdout.strip()
    wt_branch = _git(["rev-parse", "--abbrev-ref", "HEAD"], cwd=wt, check=True).stdout.strip()
    if wt_branch != branch:
        raise core.ProjectError(f"worktree HEAD is on {wt_branch!r}, expected {branch!r}; "
                                "checkout the issue branch before finish")
    current_tip = _git(["rev-parse", branch], cwd=main, check=False).stdout.strip()
    wt_head = _git(["rev-parse", "HEAD"], cwd=wt, check=True).stdout.strip()
    if not current_tip or wt_head != current_tip:
        raise core.ProjectError("worktree HEAD does not match the issue branch tip; "
                                "commit or checkout the expected branch before finish")
    resuming = bool(already and _git(["merge-base", "--is-ancestor", already, main_branch()],
                                     cwd=main, check=False).returncode == 0)
    if resuming and stored_head and current_tip != stored_head:
        raise core.ProjectError("late commits exist after the recorded merge; create a new issue "
                                "instead of reusing this finish")
    pre_check = "skipped"
    merged_now = False
    head_before = stored_head or current_tip

    if not resuming:
        if not a.no_check:
            check = wt / "scripts/check.sh"
            if not check.exists() or not os.access(check, os.X_OK):
                raise core.ProjectError("worktree scripts/check.sh is missing or not executable")
            env = dict(os.environ, PROJECT_NO_EXPORT_CHECK="1",
                       PROJECT_WORKSPACE=str(core.workspace_root()))
            p = subprocess.run([str(check)], cwd=str(wt), text=True, env=env,
                               capture_output=True)
            if p.returncode != 0:
                raise core.ProjectError("pre-merge checks failed in the worktree:\n"
                                        + (p.stdout + p.stderr).strip()[-3000:])
            pre_check = "ran"
        msg = a.message or f"{issue['key']}: {issue['title']}"
        p = _git(["merge", "--squash", branch], cwd=main, check=False)
        if p.returncode != 0:
            _git(["reset", "--merge"], cwd=main, check=False)
            raise core.ProjectError("squash merge conflicted; main was restored. Sync/resolve in the "
                                    "worktree, then retry:\n" + (p.stderr or p.stdout).strip())
        staged = _git(["diff", "--cached", "--quiet"], cwd=main, check=False)
        if staged.returncode == 0:
            if not a.evidence:
                raise core.ProjectError("nothing to merge; pass --evidence for a no-op change")
            sha = _git(["rev-parse", "HEAD"], cwd=main).stdout.strip()
        else:
            _git(["commit", "-m", msg], cwd=main)
            sha = _git(["rev-parse", "HEAD"], cwd=main).stdout.strip()
            merged_now = True

        def mark_merging(con):
            issue2 = core.get_issue(con, issue["key"])
            details2 = json.loads(issue2["details_json"] or "{}")
            details2["merge_commit"] = sha
            details2["worktree_head"] = head_before
            details2["post_merge_check"] = "pending"
            merging = core.get_status(con, "merging")
            con.execute("UPDATE issue SET status_id=?, details_json=?, updated_at=datetime('now') WHERE id=?",
                        (merging["id"], json.dumps(details2, sort_keys=True), issue2["id"]))
            core.audit(con, "issue", issue2["id"], "status", "status_id", issue2["status_key"], "merging",
                       reason=f"squash-merged as {sha[:12]}", actor=a.actor)
            core.audit(con, "issue", issue2["id"], "merge", "details.merge_commit", None, sha, actor=a.actor)
            core.audit(con, "issue", issue2["id"], "check", "post_merge_check", None, "pending", actor=a.actor)
            core.audit(con, "issue", issue2["id"], "worktree", "worktree_path", None,
                       issue2["worktree_path"], reason="worktree_head recorded", actor=a.actor)
            core.bump_graph(con)
        core.write(mark_merging)
        # Persist the merge state into the generated docs before the main check, so a
        # successful retry starts from a clean main checkout.
        export_main(main)
        _git(["add", "-A"], cwd=main, check=False)
        _git(["commit", "-m", f"chore: record {issue['key']} merge"], cwd=main, check=False)
    else:
        sha = already
        export_main(main)
        _git(["add", "-A"], cwd=main, check=False)
        _git(["commit", "-m", f"chore: resume {issue['key']} merge"], cwd=main, check=False)

    post_check = "skipped"
    if not a.no_check:
        check = main / "scripts/check.sh"
        if not check.exists() or not os.access(check, os.X_OK):
            raise core.ProjectError("main scripts/check.sh is missing or not executable")
        p = subprocess.run([str(check)], cwd=str(main), text=True, capture_output=True)
        post_check = "ran"
        if p.returncode != 0:
            def mark_failed(con):
                issue2 = core.get_issue(con, issue["key"])
                details2 = json.loads(issue2["details_json"] or "{}")
                details2["post_merge_check"] = "failed"
                con.execute("UPDATE issue SET details_json=?, updated_at=datetime('now') WHERE id=?",
                            (json.dumps(details2, sort_keys=True), issue2["id"]))
                core.audit(con, "issue", issue2["id"], "check", "post_merge_check", "pending", "failed",
                           reason="main post-merge check failed", actor=a.actor)
            core.write(mark_failed)
            export_main(main)
            _git(["add", "-A"], cwd=main, check=False)
            _git(["commit", "-m", f"chore: record {issue['key']} failed check"], cwd=main, check=False)
            raise core.ProjectError("merged, but the main post-merge check failed:\n"
                                    + (p.stdout + p.stderr).strip()[-3000:]
                                    + "\nFix main, then rerun `worktree finish`; the merge commit is recorded "
                                      "and the worktree is kept.")

    def finalize(con):
        issue2 = core.get_issue(con, issue["key"])
        done = core.get_status(con, "done")
        details2 = json.loads(issue2["details_json"] or "{}")
        details2["merge_commit"] = sha
        details2["worktree_head"] = details2.get("worktree_head") or head_before
        old_pm = details2.get("post_merge_check", "pending")
        details2["post_merge_check"] = "skipped" if a.no_check else "passed"
        if a.evidence:
            details2["evidence"] = a.evidence
        con.execute("UPDATE issue SET status_id=?, completed_at=datetime('now'), status_entered_at=datetime('now'), "
                    "worktree_path='', details_json=?, updated_at=datetime('now') WHERE id=?",
                    (done["id"], json.dumps(details2, sort_keys=True), issue2["id"]))
        core.audit(con, "issue", issue2["id"], "status", "status_id", issue2["status_key"], "done",
                   reason=f"squash-merged as {sha[:12]}", actor=a.actor)
        core.audit(con, "issue", issue2["id"], "check", "post_merge_check", old_pm,
                   details2["post_merge_check"], actor=a.actor)
        core.audit(con, "issue", issue2["id"], "worktree", "worktree_path", issue2["worktree_path"], "",
                   reason="worktree removed after merge", actor=a.actor)
        if a.force:
            core.audit(con, "issue", issue2["id"], "force", "worktree_finish", None, a.reason,
                       reason=a.reason, actor=a.actor)
        if a.no_check:
            core.audit(con, "issue", issue2["id"], "check", "no_check", None, "skipped",
                       reason="explicit --no-check", actor=a.actor)
        core.bump_graph(con)
        return {"key": issue2["key"], "merge_commit": sha, "status": "done",
                "merged_now": merged_now, "pre_merge_check": pre_check,
                "post_merge_check": post_check}
    anc = _git(["merge-base", "--is-ancestor", sha, main_branch()], cwd=main, check=False)
    if anc.returncode != 0:
        raise core.ProjectError("recorded merge commit is not an ancestor of the main branch; "
                                "inspect main before continuing")
    result = core.write(finalize)
    export_main(main)
    _git(["add", "-A"], cwd=main, check=False)
    s = _git(["commit", "-m", f"chore: record {issue['key']} completion"], cwd=main, check=False)
    if s.returncode != 0 and _git(["status", "--porcelain"], cwd=main, check=False).stdout.strip():
        raise core.ProjectError("completion commit failed; inspect main (worktree kept)")
    if prd_warning:
        result["warning"] = prd_warning
    rm = _git(["worktree", "remove", str(wt), "--force"], cwd=main, check=False)
    br = _git(["branch", "-D", branch], cwd=main, check=False)
    _git(["worktree", "prune"], cwd=main, check=False)
    result["worktree_removed"] = rm.returncode == 0
    if rm.returncode != 0:
        result["worktree_cleanup_warning"] = (
            "worktree removal failed; inspect `git worktree list` before reusing the path")
    if br.returncode != 0:
        result["branch_cleanup_warning"] = (
            "branch deletion failed; it may already be merged or still checked out")
    core.emit(result, a.json)


def cmd_worktree_recover(a):
    core.require_db()
    con = core.connect()
    try:
        issue = core.get_issue(con, a.key)
    finally:
        con.close()
    if not issue["worktree_path"]:
        raise core.ProjectError("issue has no recorded worktree")
    wt = resolve_wt(issue)
    if wt.exists():
        raise core.ProjectError(f"worktree still exists: {wt}")
    listed = _git(["worktree", "list", "--porcelain"], cwd=main_repo(), check=False).stdout
    if str(wt) in listed:
        _git(["worktree", "prune"], cwd=main_repo(), check=False)

    def fn(con):
        issue2 = core.get_issue(con, a.key)
        old = dict(issue2)
        details = json.loads(issue2["details_json"] or "{}")
        cleared = {k: details.pop(k) for k in ("merge_commit", "worktree_head", "evidence") if k in details}
        updates = {"worktree_path": ""}
        if cleared:
            updates["details_json"] = json.dumps(details, sort_keys=True)
        ready = core.get_status(con, "ready")
        if issue2["status_key"] in ("implementing", "review", "testing", "merging"):
            updates["status_id"] = ready["id"]
            updates["status_entered_at"] = core.datetime.now().isoformat(timespec="seconds")
        sets = ", ".join(f"{k}=?" for k in updates)
        con.execute(f"UPDATE issue SET {sets}, updated_at=datetime('now') WHERE id=?",
                    (*updates.values(), issue2["id"]))
        core.audit_fields(con, issue2["id"], old, updates, reason=a.reason or "worktree lost", actor=a.actor)
        core.bump_evidence_epoch(con, issue2["id"], a.actor)
        core.bump_graph(con)
        return {"key": issue2["key"], "recovered": True, "status": "ready" if "status_id" in updates else issue2["status_key"]}

    core.write(fn)
    main = main_repo()
    export_main(main)
    _git(["add", "-A"], cwd=main, check=False)
    _git(["commit", "-m", f"chore: recover {issue['key']} worktree"], cwd=main, check=False)
    core.emit({"key": issue["key"], "recovered": True}, a.json)


def cmd_worktree_list(a):
    core.require_db()
    p = _git(["worktree", "list", "--porcelain"], cwd=main_repo(), check=False)
    entries, cur = [], {}
    for line in p.stdout.splitlines():
        if not line.strip():
            if cur:
                entries.append(cur)
                cur = {}
        elif " " in line:
            k, v = line.split(" ", 1)
            cur[k] = v
        else:
            cur[line] = True
    if cur:
        entries.append(cur)
    core.emit(entries, a.json) if a.json else core.table(entries, ["worktree", "HEAD", "branch"])


def cmd_worktree_prune(a):
    core.require_db()
    p = _git(["worktree", "prune", "-v"], cwd=main_repo(), check=False)
    core.emit({"output": (p.stdout + p.stderr).strip()}, a.json)
