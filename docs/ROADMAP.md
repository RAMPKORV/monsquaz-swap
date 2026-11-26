# Roadmap

v1 ships both classic and async events on Windows and Linux. Each milestone has
an exit criterion. Do not start a milestone before its dependencies pass.

## M0 — Spec and scaffold (current)

**Status:** in progress.

- Tauri 2 + Rust + React/TS shell.
- `swap-core` with the `SeenLedger` invariant and tests.
- Hand-written PRD, event settings, research, architecture, AI rules.
- Gherkin specs in `specs/features/`.
- SQLite tracker, ledger, worktree workflow.
- CI and GitHub Release workflows.
- Public `RAMPKORV/monsquaz-swap` repository.

**Exit:** `scripts/check.sh` passes; CI is green; a draft Windows + Linux
release builds; the plan review has no must-fix.

## M1 — Domain and scheduler

- Event, participant, track, assignment, and offer state machines.
- Seen ledger with claim/receipt/submit/skip semantics.
- Classic partial Latin rectangle with seeded perfect matching.
- Own-track exclusion and no-repeat validation.
- Feasibility and Hall-style conflict output.
- Group planning primitives (participant rotation, K phases).
- Property tests and golden seeds.
- cucumber-rs harness with the first scheduling scenarios.

**Exit:** 10k generated schedules have zero invariant violations; an infeasible
schedule produces a conflict list; Gherkin scheduling scenarios pass.

## M2 — Persistence and files

- SQLite schema, migrations, and repositories.
- Single-writer `Db` handle; WAL; backup/restore primitive.
- File quarantine, hash, allowlist, magic bytes, size cap.
- Pack hash report.
- Module-aware scrub and A/B render check.
- Preview WAV generation through the openmpt123 sidecar.
- Event library layout (round + track views).

**Exit:** migration tests pass; a corrupt file cannot crash the app; a scrubbed
module renders the same as the original; an oversized file follows the fallback.

## M3 — Discord adapter and onboarding

- Twilight gateway and guild command registration.
- Setup wizard backend: token validation, invite, channel mapping, test DM.
- DM handshake and private-thread fallback.
- Offer, claim, submit, skip, extension, leave flows.
- DM backfill after downtime.
- Host emergency commands (status, pause, resume, help).

**Exit:** a real test guild completes join, seed, offer, claim, submit, and
receipt. A DM sent during a short outage is recovered once.

## M4 — Host UI

- Setup wizard.
- Mission control with the next action.
- Event wizard with presets and reset-to-recommended.
- Participant manager.
- Schedule planner (matrix + graph + auto-generation + warnings).
- Round board (track cards + graph view).
- File browser and preview.
- Audit log.
- Settings and logs.
- View-as-participant and host dry run.
- **UI quality gate:** screenshot every view in empty, loading, populated,
  error, long-content, and dense states at 1280×800 and 1024×700; run three
  independent review waves (visual, UX/copy, accessibility); fix every must-fix
  and re-capture. Store the curated set under `docs/ui/screens/`.

**Exit:** a host can create and inspect a full event without editing files or
the database. The UI review waves have no must-fix. A first-time host can
complete the main journey without help.

## M5 — Classic end-to-end

**Status:** in progress. Seed-by-DM, roster lock, round opening, claim, submit,
skip, and reveal credits are wired.

- Seed phase, roster lock, schedule generation/lock/commit.
- Claim-first delivery, deadlines, reminders, extensions.
- Skips and fill-ins.
- Reveal one track with credits.
- Backup/restore and crash recovery.

**Exit:** an 8-person classic event completes end-to-end on Windows and Linux.
The invariant suite is green.

## M6 — Async, groups, and fill-ins

- Async global queue with the social-diversity objective.
- 24h claim window, 12h reminder, two daily waves.
- Ready gate, expiry protection, gap report.
- Exposure length rules (N-1 small, 6–8 above 12).
- Classic groups, participant rotation, K cadence, merge repair.
- Fill-in pool and sequential offers.
- Host deadline decision cards.

**Exit:** a 20-person async event with two skips, one fill-in, one late seed,
and one unresponsive participant completes without an invariant violation. A
classic grouped event completes with a mid-event swap.

## M7 — Reveal, presenter, export

- Host-triggered Discord reveal, whole-server audience.
- Credits matrix, comments, anonymous gaps, fun stats.
- Presenter window with lineage and R1/final playback.
- Stream-safe mode.
- Full ZIP export, runbook, personal keepsake.
- Commit-reveal verify.

**Exit:** a game day streams a reveal from the presenter and publishes the album
package. No owner/title leaks before the reveal.

## M8 — Hardening

- Full simulation suite (churn, groups, async, outages, restore, adversarial).
- Anonymity leak audit.
- File-security fuzzing and parser isolation.
- Idempotency and crash-recovery tests.
- Performance at 40 participants.
- Accessibility pass.
- Privacy export/delete.
- Final UI polish: repeat the screenshot matrix at 200% zoom and light theme;
  run a fourth review wave on density, consistency, and the reveal presenter.

**Exit:** 10k-seed nightly run is green for seven days; the red-team rubric has
no hard fail; a clean-machine restore drill passes.

## M9 — Beta and 0.1.0

- Windows NSIS + Linux `.deb`/AppImage release.
- README, quick start, host runbook, known limits.
- Signed tag, draft release, smoke test, publish.
- At least two real events (one classic, one async).
- Feedback triage and a patch release if needed.

**Exit:** two real events complete; no critical bug; release assets verify.

## Deferred

| Item | Trigger |
|---|---|
| macOS build | Apple Developer signing funded |
| Album audio rendering | Preview pipeline stable; real demand |
| Discord voice listening party | After the reveal presenter ships |
| Auto-update | After a manual updater-capable release |
| Windows signing | If SmartScreen blocks adoption; SignPath/Certum |
| Async content tags | After v1; optional genre/mood tags |
| Old-site import | If the old data is available and requested |
| Spectator role | Not planned; the reveal is whole-server |
| In-app chat/ratings | Not planned |
| Public archive page | Not planned; Discord + ZIP only |

## v1 definition

v1 is complete when:

- a clean Windows 11 or Ubuntu 22.04 host can install the app, connect a bot,
  create an event, run it end-to-end, and publish the reveal;
- classic and async both work, including one skip and one fill-in;
- the seen ledger, own-track, anonymity, and file-safety suites are green;
- `SHA256SUMS` and the GitHub Release assets verify;
- the runbook is usable by a host who did not build the app.
