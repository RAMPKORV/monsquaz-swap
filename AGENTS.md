# Monsquaz Swap — agent entry point

Monsquaz Swap is a cross-platform desktop host for anonymous, Discord-native
music-swap events ("exquisite corpse" albums). The host runs a Tauri app that
embeds a Discord bot; participants join by submitting a seed track, receive
tracks they have never worked on, edit and submit them, and learn who made what
only at the reveal.

This file is a router. Detailed rules live in `ai-rules/`.

## Orientation order

1. This file.
2. `ai-rules/WORKFLOW.md` — the working loop and issue/worktree rules.
3. `ai-rules/STE.md` — the specification language (ASD-STE100).
4. `ai-rules/DOMAIN.md` — glossary, invariants, state machines.
5. `ai-rules/ARCHITECTURE.md` — process model, crates, ports/adapters.
6. `ai-rules/TESTING.md`, `ai-rules/GHERKIN.md`, and `ai-rules/CLI.md` — how to
   prove behaviour and where the commands live.
7. `specs/requirements/` — the formal requirement model and
   `scripts/requirements-check.sh`.
8. `ai-rules/DESIGN.md` — the deterministic design policy, the Refract gate,
   and how to change a locked rule.
9. `ai-rules/RELEASES.md` — versioning, tags, GitHub Releases, signing status.
10. `ai-rules/DECISIONS.md` — the single decision log.
11. `ai-rules/skills/` — the shared agent skills (formal verification, design
    review, STE writing).
12. `project` (`./bin/project`) — the SQLite issue tracker, decisions, evidence,
    and append-only ledger. It is not the PRD; the PRD is hand-written.
13. The issue you are working on, its notes, and linked docs.

## Current state

- Runnable Tauri 2 + Rust + React/TS shell; `swap-core` holds the first hard
  invariant (`SeenLedger`) and the seeded classic scheduler.
- `swap-db` owns the SQLite schema, migrations, and repositories. `swap-files`
  validates intake, scrubs module metadata, A/B renders through openmpt123, and
  stores content by SHA-256. `swap-discord` defines the transport contract.
  `swap-engine` runs the classic walking skeleton.
- Product spec and Gherkin live in `docs/` and `specs/features/`. The formal
  requirement model is in `specs/requirements/`; the gate is
  `scripts/requirements-check.sh`.
- Tracker is live in `.project/project.db` (never committed); the durable dump
  is `project/ledger/current.jsonl`.
- The host UI shell covers mission control, settings, and the audit log, with a
  tray and a reviewed screenshot matrix. The setup service stores the bot token
  in the OS keychain, maps channels, and sends a test DM. The live Twilight
  gateway runs behind a supervisor. `swap-sim` runs deterministic classic
  events and checks the hard invariants. `swap-privacy` exports participant
  data and anonymizes deletion with a hash-only audit. The durable delivery
  loop, startup and periodic dispatch, DM backfill, restart recovery, and
  private-thread fallback are in place. The classic production loop (join,
  roster lock, open round, and reveal credits) and gateway health are in place.
  Deadline automation, the reveal presenter and exports, the async queue,
  event tools, and remaining UI screens are next.

## Non-negotiable invariants

1. **No repeat, ever.** A participant may never work on the same track twice.
   A claim/receipt, a submission, and a claim-then-skip all count as "seen".
   A pre-claim skip or expired offer does **not**. Seed ownership is a separate
   own-track exclusion.
2. **No own track.** A participant is never assigned the track they seeded.
3. **Anonymity until reveal.** Participant-facing surfaces must not expose the
   seed owner, any contributor identity, the original track title, comments, or
   the assignment matrix before the host triggers the reveal.
4. **Discord is transport, not storage.** Download attachments on receipt,
   store bytes locally, and never forward a Discord CDN URL.
5. **No silent auto-advance.** Deadlines trigger reminders and host alerts, not
   automatic transitions. The host decides.
6. **Files never travel through IPC.** The UI receives opaque IDs and preview
   URLs, not bytes or absolute paths.
7. **Secrets never leak.** The bot token lives only in the OS keychain and the
   bot process memory; never in the UI, logs, SQLite, or git.
8. **UTC in the database, local in the UI.** Use explicit IANA timezones.
9. **One writer per issue and per database.** No shared mutable SQLite DB
   across worktrees; no hand-editing the DB.

## Commands

```bash
# tracker / workflow
./bin/project status
./bin/project next
./bin/project issue show MSW-###
./bin/project issue claim MSW-### --as-agent <name>
./bin/project worktree new MSW-### --as-agent <name> \
  --force --reason "D-021: no PRD sign-off gate"                 # first batch issue
./bin/project worktree finish MSW-### --yes --as-agent <name> \
  --force --reason "D-021: no PRD sign-off gate"                 # from main checkout

# app
pnpm install
pnpm tauri dev
pnpm tauri build
pnpm --dir ui test
pnpm --dir ui build   # before cargo test on a clean checkout
cargo test --workspace
scripts/check.sh
scripts/design-check.sh   # Refract + contrast + focus + token policy
scripts/requirements-check.sh     # Z3 requirement consistency gate
node scripts/ste-lint.mjs          # STE gate
node scripts/trace-scenarios.mjs   # Gherkin traceability gate
scripts/formal/check.sh            # TLA+ models (needs java)
crates/swap-plan-check/verus/conformance.sh  # Verus flat checker (dev-only)
```

`worktree new` and `worktree finish` need `--force` because of a PRD sign-off
gate that this project does not use. See `ai-rules/WORKFLOW.md`.

Batch 3–6 related issues into one branch, one PR, and one CI run. Claim every
issue, create the worktree from the first issue, and work the rest in it. The
PR body lists every issue key. See `ai-rules/WORKFLOW.md`.

`src-tauri` embeds `ui/dist` at compile time. Run `pnpm --dir ui build` before
`cargo test --workspace` or `pnpm tauri build` on a clean checkout.

The `project` CLI also supports a subset of the scaffold: issues, decisions,
evidence, sessions, and the ledger. We do **not** use its PRD/sign-off gates;
`docs/PRD.md` is hand-written and editable.

## Specification language (ASD-STE100)

Use **ASD-STE100 Simplified Technical English** for every specification,
requirement, Gherkin scenario, event setting, test plan, runbook, CLI
instruction, and error message. `ai-rules/STE.md` owns the rules. The three
most important:

- One word, one meaning. Use the glossary in `ai-rules/DOMAIN.md`.
- Active voice, one instruction per sentence, at most 20 words (procedures)
  or 25 words (descriptions).
- Use `must` for mandatory behaviour and `can` for capability. Do not use
  `should`.

`node scripts/ste-lint.mjs` checks the mechanical rules (D-038).

The playful product voice is allowed in user-facing copy. It must not make an
instruction ambiguous.

## UI quality gate

A UI issue is not done until the visual evidence exists and the review waves
pass. `docs/UI_REVIEW.md` is the single owner of the screenshot matrix, the
states, the review waves, the human checkpoints, and the reject list.
`ai-rules/UX.md` owns the copy rules.

## Definition of done

- The behaviour has a Gherkin scenario in `specs/features/` (or a recorded
  verification command for a chore).
- A test declares the scenario with `// scenario: <name>`, and the scenario
  has no `@pending` tag. A feature file that cucumber-rs runs counts as mapped
  in full and needs no per-test comment. `node scripts/trace-scenarios.mjs`
  passes.
- A failing test existed first, or the change is a docs/chore.
- New specifications follow `ai-rules/STE.md`, and `node scripts/ste-lint.mjs`
  passes.
- A change to a file in `specs/tla/mapping.json` updates the model or the
  mapping, and `scripts/formal/check.sh` passes.
- A change to a PRD requirement, EVENT_SETTINGS row, decision, or Gherkin
  scenario updates `specs/requirements/`, and `scripts/requirements-check.sh`
  passes. A contradiction, an unsatisfiable scenario, or a coverage gap blocks
  the change.
- For UI work, the UI quality gate passes with screenshots and review evidence.
- `scripts/check.sh` passes.
- The tracker has evidence and the issue is closed through the CLI.
- Docs and decisions are updated where intent changed.

## Where things live

| Path | Purpose |
|---|---|
| `ai-rules/` | project rules for humans and agents |
| `ai-rules/DECISIONS.md` | the single decision log (hand-written D-001..D-028, tracker D-029 and later) |
| `ai-rules/skills/` | shared agent skills; `.claude/skills/` and `.opencode/skills/` symlink here |
| `ai-rules/CLI.md` | command map, worktree, release, troubleshooting |
| `design/` | design policy, page matrix, baseline, Refract lock |
| `ai-rules/RELEASES.md` | versioning, tags, unsigned Windows, no updater |
| `docs/PRD.md` | hand-written product contract |
| `docs/EVENT_SETTINGS.md` | every event knob, its default and lock behaviour |
| `docs/ARCHITECTURE.md` | public system design |
| `docs/SETUP.md` | bot token, keychain, channel mapping, test DM |
| `docs/FORMAL_METHODS.md` | formal verification scope, tools, and maintenance |
| `specs/tla/` | TLA+ design models; `mapping.json` lists the code each model covers |
| `specs/requirements/` | finite Z3 requirement model, known-bad corpus, and gate tests |
| `docs/generated/SCENARIO_COVERAGE.md` | generated scenario-to-test coverage |
| `docs/generated/REQUIREMENT_CONSISTENCY.md` | generated rule, scenario, contradiction, and coverage report |
| `docs/RESEARCH.md` | evidence and sources behind the design |
| `docs/ROADMAP.md` | milestones and exit criteria |
| `specs/features/` | hand-written Gherkin, executable later with cucumber-rs |
| `crates/swap-core/` | pure domain; no I/O, no async, no Discord/Tauri |
| `src-tauri/` | Tauri shell and future app services |
| `ui/` | React/TypeScript host UI |
| `tools/project/` | tracker CLI and schema |
| `project/ledger/` | committed append-only ledger dump |
| `worktrees/` | external worktrees (`../MonsquazSwap-worktrees/`) |
