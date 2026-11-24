# Workflow

Lean, research-first TDD with one issue per worktree. The tracker is the
durable memory; chat is not. We keep the scaffold's issue tree, decisions,
evidence, and append-only ledger, but **not** its PRD/sign-off gates. The
product contract is hand-written in `docs/`.

## The loop

1. **Orient.** `./bin/project status`, `./bin/project next`, then read
   `docs/PRD.md`, `docs/EVENT_SETTINGS.md`, and the issue.
2. **Claim.** `./bin/project issue claim MSW-### --as-agent <name>`.
   Respect the WIP limit (2).
3. **Worktree.** `./bin/project worktree new MSW-### --as-agent <name>`.
   Worktrees live in `../MonsquazSwap-worktrees/`, outside the repo. Never
   share a mutable SQLite DB across worktrees.
4. **Research.** Codebase first, then official docs/web. Record sources and
   findings in the tracker (`source add`, `finding add`, `issue note add`).
   A factual unknown that can change the plan becomes a timeboxed spike.
5. **Specify.** Write or update a scenario in `specs/features/`. State the
   observable behaviour, the error path, and the verification command. Write
   the spec in ASD-STE100 Simplified Technical English (`ai-rules/STE.md`).
6. **Red.** Add the smallest failing test. For Rust domain behaviour, a unit
   test or `proptest` property. For a user journey, a cucumber-rs scenario.
7. **Green.** Implement the minimum. No speculative abstractions.
8. **Refactor.** Remove duplication, keep the domain pure, rerun tests.
9. **Self-review.** Read the diff as a hostile reviewer: invariants, anonymity,
   error paths, timezones, idempotency, secrets, path safety. Record the review
   for `standard`/`full` ceremony.
10. **Check.** `scripts/check.sh`. Fix everything; do not weaken tests.
11. **Merge.** Squash merge from the main checkout with
    `./bin/project worktree finish ...`. Record test evidence.
12. **Export.** `./bin/project export` and commit the ledger dump with the code.

## Ceremony

| Tier | Use | Required |
|---|---|---|
| `light` | docs, chores, one-line fixes | issue + verification |
| `standard` | normal feature/bug | scenario + tests + self-review |
| `full` | schema, scheduling invariants, anonymity, security, irreversible data | design note + adversarial review + rollback/restore evidence |

The default is `light`; raise it deliberately for `full` work. The tracker's
status gates still exist, but we do not require an independent reviewer for
every routine issue. An independent review is required for `full`.

## Issue rules

- One issue = one observable outcome. No epic/story/task ladder; use hierarchy.
- New idea → new `triage` issue. Never absorb scope silently.
- A parent is a container; do not close it automatically when children finish.
- No issue is `done` without evidence: a test run, merge commit, or explicit
  verification recorded in the tracker.
- Do not hand-edit `.project/project.db` or generated ledger files. Use the CLI.
- One writer per issue. If two agents need the same file, split the issue.

## Git

- Trunk-based, short-lived branches; one worktree per issue.
- Conventional Commits: `feat:`, `fix:`, `test:`, `docs:`, `chore:`,
  `refactor:`, `perf:`, `build:`, `ci:`.
- Keep commits small and reviewable; no drive-by refactors.
- Never commit secrets, `.env`, `.project/`, `worktrees/`, `target/`, or
  `node_modules/`.
- Do not rewrite published history or force-push `main`.

## Review

- **Plan review:** one fresh adversarial review of the PRD/specs before
  implementation. Fix must-fixes; record it; no formal sign-off gate.
- **Issue review:** self-review for `standard`; independent adversarial review
  for `full` (anonymity, scheduling, schema, security, file handling).
- Reviewer looks for contradictions, untestable claims, anonymity leaks,
  missing error paths, silent auto-advance, and duplicated truth.

## Handoff

End a session with:

```bash
./bin/project session end \
  --summary "what changed" \
  --handoff "what the next agent needs to know" \
  --next-issue MSW-###
```

A fresh agent must be able to resume from the tracker, the ledger, and the
docs without reading chat history.

## Stop rule

Stop asking questions when the next answer cannot change the plan. Record the
decision, update the spec, and move on. If a question is genuinely blocking and
researchable, create a spike with a timebox instead of guessing.
