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
7. `ai-rules/RELEASES.md` — versioning, tags, GitHub Releases, signing status.
8. `project` (`./bin/project`) — the SQLite issue tracker, decisions, evidence,
   and append-only ledger. It is not the PRD; the PRD is hand-written.
9. The issue you are working on, its notes, and linked docs.

## Current state

- Runnable Tauri 2 + Rust + React/TS shell; `swap-core` holds the first hard
  invariant (`SeenLedger`) and the seeded classic scheduler.
- `swap-db` owns the SQLite schema, migrations, and repositories. `swap-files`
  validates intake and stores content by SHA-256. `swap-discord` defines the
  transport contract. `swap-engine` runs the classic walking skeleton.
- Product spec and Gherkin live in `docs/` and `specs/features/`.
- Tracker is live in `.project/project.db` (never committed); the durable dump
  is `project/ledger/current.jsonl`.
- The live Twilight gateway, async queue, deadline automation, reveal exports,
  and UI screens are next.

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
./bin/project worktree new MSW-### --as-agent <name>
./bin/project worktree finish MSW-### --yes --as-agent <name>   # from main checkout

# app
pnpm install
pnpm tauri dev
pnpm tauri build
pnpm --dir ui test
pnpm --dir ui build   # before cargo test on a clean checkout
cargo test --workspace
scripts/check.sh
```

`src-tauri` embeds `ui/dist` at compile time. Run `pnpm --dir ui build` before
`cargo test --workspace` or `pnpm tauri build` on a clean checkout.

The `project` CLI also supports a subset of the scaffold: issues, decisions,
evidence, sessions, and the ledger. We do **not** use its PRD/sign-off gates;
`docs/PRD.md` is hand-written and editable.

## Specification language (ASD-STE100)

Use **ASD-STE100 Simplified Technical English** for every specification,
requirement, Gherkin scenario, event setting, test plan, runbook, CLI
instruction, and error message. Read `ai-rules/STE.md` before you write a spec.

- One word, one meaning. Use the glossary in `ai-rules/DOMAIN.md`.
- Active voice. Name the actor.
- Procedures: maximum 20 words per sentence. Descriptions: maximum 25 words.
- One instruction per sentence. One topic per paragraph, maximum six sentences.
- Do not use contractions, idioms, `etc.`, `e.g.`, `i.e.`, `and/or`,
  `appropriate`, `reasonable`, or `as needed`.
- State every condition, time, unit, and timezone.
- Use `must` for mandatory behaviour and `can` for capability. Do not use
  `should` in a requirement.

The playful product voice is allowed in user-facing copy. It must not make an
instruction ambiguous.

## UI quality gate

A UI issue is not done until the visual evidence exists and the review waves
pass. Use `docs/UI_REVIEW.md` and `ai-rules/UX.md`.

- Capture every affected view at 1280×800 and at the minimum 1024×700 width.
- Capture empty, loading, populated, error, long-content, and dense-table
  states.
- Check alignment, spacing, density, typography, contrast, overflow, focus, and
  responsive behavior.
- Run at least three independent reviews in waves: visual design, UX/copy, and
  accessibility. Fix every must-fix, then re-capture.
- Store the curated screenshots under `docs/ui/screens/` and record the review.
- Do not use placeholder text or fake-looking data in a review screenshot.

## Definition of done

- The behaviour has a Gherkin scenario in `specs/features/` (or a recorded
  verification command for a chore).
- A failing test existed first, or the change is a docs/chore.
- New specifications follow `ai-rules/STE.md`.
- For UI work, the UI quality gate passes with screenshots and review evidence.
- `scripts/check.sh` passes.
- The tracker has evidence and the issue is closed through the CLI.
- Docs and decisions are updated where intent changed.

## Where things live

| Path | Purpose |
|---|---|
| `ai-rules/` | project rules for humans and agents |
| `ai-rules/CLI.md` | command map, worktree, release, troubleshooting |
| `ai-rules/RELEASES.md` | versioning, tags, unsigned Windows, no updater |
| `docs/PRD.md` | hand-written product contract |
| `docs/EVENT_SETTINGS.md` | every event knob, its default and lock behaviour |
| `docs/ARCHITECTURE.md` | public system design |
| `docs/RESEARCH.md` | evidence and sources behind the design |
| `docs/ROADMAP.md` | milestones and exit criteria |
| `specs/features/` | hand-written Gherkin, executable later with cucumber-rs |
| `crates/swap-core/` | pure domain; no I/O, no async, no Discord/Tauri |
| `src-tauri/` | Tauri shell and future app services |
| `ui/` | React/TypeScript host UI |
| `tools/project/` | tracker CLI and schema |
| `project/ledger/` | committed append-only ledger dump |
| `worktrees/` | external worktrees (`../MonsquazSwap-worktrees/`) |
