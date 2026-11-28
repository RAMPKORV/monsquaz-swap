# Workflow

Lean, research-first TDD with one batch or issue per worktree. The tracker is
the durable memory; chat is not. We keep the scaffold's issue tree, decisions,
evidence, and append-only ledger, but **not** its PRD/sign-off gates. The
product contract is hand-written in `docs/`.

## The loop

1. **Orient.** `./bin/project status`, `./bin/project next`, then read
   `docs/PRD.md`, `docs/EVENT_SETTINGS.md`, and the issue. If the issue touches
   a requirement, setting, decision, or scenario, read
   `specs/requirements/README.md`.
2. **Claim.** `./bin/project issue claim MSW-### --as-agent <name>`.
   Respect the batch WIP limit (6). Claim every issue in the batch.
3. **Worktree.** `./bin/project worktree new MSW-### --as-agent <name>` for
   the first issue. See "PRD gate and `--force`" below. Work the other batch issues in that worktree; do not make a
   second worktree. Worktrees live in `../MonsquazSwap-worktrees/`, outside the
   repo. Never share a mutable SQLite DB across worktrees.
4. **Research.** Codebase first, then official docs/web. Record sources and
   findings in the tracker (`source add`, `finding add`, `issue note add`).
   A factual unknown that can change the plan becomes a timeboxed spike.
5. **Specify.** Write or update a scenario in `specs/features/`. State the
   observable behaviour, the error path, and the verification command. Write
   the spec in ASD-STE100 Simplified Technical English (`ai-rules/STE.md`).
   Tag a scenario `@pending` until a test proves it (`ai-rules/GHERKIN.md`).
   Update the requirement model in `specs/requirements/` for a requirement,
   setting, decision, or scenario change. Run
   `scripts/requirements-check.sh`.
6. **Red.** Add the smallest failing test. For Rust domain behaviour, a unit
   test or `proptest` property. For a user journey, a cucumber-rs scenario.
7. **Green.** Implement the minimum. No speculative abstractions.
8. **Refactor.** Remove duplication, keep the domain pure, rerun tests.
9. **Model.** Do this step when the change touches a modeled action, a
   property, or a mapped file. `specs/tla/mapping.json` lists the mapped files.
   Update the TLA+ model or the trace mapping in the same PR. Run
   `scripts/formal/check.sh`. A requirement change also updates the Z3 model in
   `specs/requirements/`; see "Requirement changes". Skip this step for other
   changes.
10. **Self-review.** Read the diff as a hostile reviewer: invariants, anonymity,
   error paths, timezones, idempotency, secrets, path safety. Record the review
   for `standard`/`full` ceremony. Then run the review loop (below).
11. **Check.** `scripts/check.sh`. Fix everything; do not weaken tests.
12. **Merge.** Squash merge the batch from the main checkout with
    `./bin/project worktree finish ...`. Record evidence for every issue.
13. **Export.** `./bin/project export` and commit the ledger dump with the code.

## Requirement changes

An agent must update the formal model in `specs/requirements/` before it adds
or changes a PRD requirement. The same rule applies to an EVENT_SETTINGS row,
a decision, or a Gherkin scenario. Then run `scripts/requirements-check.sh`.

- A contradiction, an unsatisfiable scenario, or a coverage gap blocks the
  change.
- The natural-language source wins. The formal model is a reviewed
  formalization. A stale model is a defect.
- The model change is reviewed like a test. The reviewer reads the rule
  formula, the source links, and the scenario links.
- A rule has a stable id, a source link, a Z3 formula, and a paraphrase in
  ASD-STE100. A scenario links the rules that it exercises.
- Add the natural-language change, the model change, and the generated report
  in the same PR.
- The trusted gap is in `docs/FORMAL_METHODS.md`. A wrong or incomplete model
  can miss a contradiction. Review and source links are mandatory.

## Ceremony

| Tier | Use | Required |
|---|---|---|
| `light` | docs, chores, one-line fixes | issue + verification |
| `standard` | normal feature/bug | scenario + tests + self-review + review loop |
| `full` | schema, scheduling invariants, anonymity, security, irreversible data | design note + review loop + rollback/restore evidence + formal evidence (`scripts/formal/check.sh` output, or a note that no modeled action changed) + requirement consistency evidence (`scripts/requirements-check.sh` output, or a note that no requirement changed) |

The default is `light`; raise it deliberately for `full` work. The tracker's
status gates still exist.

## PRD gate and `--force`

The tracker tool has a PRD sign-off gate. This project does not use it
(D-021). `tools/project` has no configuration flag that turns the gate off, so
`worktree new` and `worktree finish` refuse to run without `--force`. Pass
`--force` with this exact reason:

```bash
./bin/project worktree new MSW-### --as-agent <name> \
  --force --reason "D-021: no PRD sign-off gate"
./bin/project worktree finish MSW-### --yes --as-agent <name> \
  --force --reason "D-021: no PRD sign-off gate"
```

`--force` waives the PRD sign-off gate. For `worktree new`, `--force` also
skips the claim and WIP checks. For `worktree finish`, `--force` does not skip
the review status, the resolved review, or the evidence check. Before you use
it, make sure that you claimed the issue and that the issue is in the correct
status. Do not use `--force` for any other reason.

## Batch PRs

CI is the main wait. Batch related work into one PR.

- A batch is 3–6 related issues or one milestone slice. One batch is one
  worktree, one branch, one PR, and one CI run.
- Claim every issue before you start. Use the first issue for `worktree new`;
  work the rest in the same worktree.
- The PR body lists every issue key and the evidence for each one.
- Close each issue after the merge with the same squash commit. Record one
  review for each `standard` or `full` issue. A shared review file can cover
  the batch, but the tracker needs a review record for each issue.
- Do not batch unrelated risky changes. Security, schema, and UI changes can
  share a batch only when they serve one outcome.
- A batch may delete or retire a formal artifact in the same PR. Keep the
  product build independent.

## Issue rules

- One issue = one observable outcome. No epic/story/task ladder; use hierarchy.
- New idea → new `triage` issue. Never absorb scope silently.
- A parent is a container; do not close it automatically when children finish.
- No issue is `done` without evidence: a test run, merge commit, or explicit
  verification recorded in the tracker.
- Do not hand-edit `.project/project.db` or generated ledger files. Use the CLI.
- One writer per issue. If two agents need the same file, split the issue.

## Git

- Trunk-based, short-lived branches; one worktree per batch or issue.
- Conventional Commits: `feat:`, `fix:`, `test:`, `docs:`, `chore:`,
  `refactor:`, `perf:`, `build:`, `ci:`.
- Keep commits small and reviewable; no drive-by refactors.
- Never commit secrets, `.env`, `.project/`, `worktrees/`, `target/`, or
  `node_modules/`.
- Do not rewrite published history or force-push `main`.

## Review

- **Plan review:** one fresh adversarial review of the PRD/specs before
  implementation. Fix must-fixes; record it; no formal sign-off gate.
- **Issue review:** use the review loop below. It covers `standard` and `full`
  issues. Focus on anonymity, scheduling, schema, security, and file handling.
- Reviewer looks for contradictions, untestable claims, anonymity leaks,
  missing error paths, silent auto-advance, duplicated truth, and a stale
  requirement model.

## Review loop

Every `standard` and `full` issue gets an independent review before the merge.
The reviewer roles depend on the ceremony tier:

| Work | Reviewer roles |
|---|---|
| `standard` | correctness |
| `full` | correctness; invariants and anonymity; security |
| UI work (any tier) | the three waves in `docs/UI_REVIEW.md`: visual design, UX and copy, accessibility; plus the human checkpoints in "Human in the loop" in `docs/UI_REVIEW.md` |

Procedure:

1. Start one fresh subagent for each role. A reviewer has no memory of the
   work.
2. Give each reviewer the diff, the issue, and the relevant rules files.
3. Do not give the reviewer your conclusions, your self-review, or your
   explanation of the change.
4. The reviewer reports each finding on one line in this format:
   `severity | file:line | problem | fix`. The severity is `must-fix`,
   `should-fix`, or `nit`.
5. Fix every `must-fix`. Fix a `should-fix` or record the reason to keep it.
6. Start a new fresh review of the new diff. Do not reuse a reviewer.
7. Stop when no `must-fix` remains.
8. After three rounds with a `must-fix` still open, stop and escalate to the
   human.

Record the findings and the result in the tracker as one review record for each
issue. A batch can record one review file that lists all issue keys, but each
`standard` or `full` issue needs its own tracker record.

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
