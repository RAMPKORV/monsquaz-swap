# Decision log — Monsquaz Swap

> <!-- GENERATED FILE — edit project.db via tools/project/cli.py; do not edit by hand. -->

| Code | Title | Status | Date |
|---|---|---|---|
| `D-001` | Tauri 2 + Rust backend + React/TS UI | accepted | 2026-11-24 |
| `D-002` | Twilight 0.18 for Discord | accepted | 2026-11-24 |
| `D-003` | DM-first, claim-first delivery without privileged intents | accepted | 2026-11-24 |
| `D-004` | Append-only seen ledger | accepted | 2026-11-24 |
| `D-005` | Classic scheduling uses a randomized partial Latin rectangle | accepted | 2026-11-24 |
| `D-006` | Async mode uses no formal groups | accepted | 2026-11-24 |
| `D-007` | 24h claim window, two daily waves, exposure completeness | accepted | 2026-11-24 |
| `D-008` | Async anti-loop protection | accepted | 2026-11-24 |
| `D-009` | GitHub Releases, Windows + Linux, unsigned, no updater | accepted | 2026-11-24 |
| `D-010` | ASD-STE100 for specifications | accepted | 2026-11-24 |
| `D-011` | Lean tracker with hand-written specs | accepted | 2026-11-24 |
| `D-029` | Lock the design scales | accepted | 2026-11-26 |
| `D-030` | Classic rounds start at the seed | accepted | 2026-11-26 |
| `D-031` | Classic round lifecycle and shared deadlines | accepted | 2026-11-26 |
| `D-032` | Own assignments by default; open schedule is the only anonymity exception | accepted | 2026-11-26 |
| `D-033` | Async intake, targets, windows, and expiry | accepted | 2026-11-26 |
| `D-034` | max_handoffs modes and the seed budget | accepted | 2026-11-26 |

## `D-001` — Tauri 2 + Rust backend + React/TS UI

- **Status:** accepted
- **Decided by:** Patrik · **Date:** 2026-11-24

### Context

Need a cross-platform desktop host with an embedded Discord bot, SQLite, and local files. Owner prefers Rust.

### Decision

Use Tauri 2.12, Rust for all core logic, React 19 + TypeScript for the UI. Do not use Electron.

### Consequences

Small installers and one runtime. Three WebView engines and Linux WebKitGTK packaging need testing. The UI is TS; no Node runtime ships.

## `D-002` — Twilight 0.18 for Discord

- **Status:** accepted
- **Decided by:** Patrik · **Date:** 2026-11-24

### Context

Need guild commands, buttons, DM attachments and history, rate limits, and current API models. Need a stable crate.

### Decision

Use Twilight 0.18 with a thin adapter and pure domain handlers.

### Consequences

About 1-2 days of command plumbing. Plain-data events make fixtures easy. No macro framework.

## `D-003` — DM-first, claim-first delivery without privileged intents

- **Status:** accepted
- **Decided by:** Patrik · **Date:** 2026-11-24

### Context

Interaction tokens expire in 15 minutes. Ephemeral attachments are not durable. DMs with the app are exempt from MESSAGE_CONTENT.

### Decision

Send offers by DM with buttons. Send the file only after Claim. Accept submissions by DM attachment. Use GUILDS and DIRECT_MESSAGES intents only.

### Consequences

Claim-first protects the seen ledger. DM backfill is possible. Blocked DMs need a private-thread fallback. No prefix commands.

## `D-004` — Append-only seen ledger

- **Status:** accepted
- **Decided by:** Patrik · **Date:** 2026-11-24

### Context

The core promise is that nobody works on a track twice.

### Decision

Record (track, participant) on claim/receipt, submit, and claim-then-skip. Do not record a pre-claim skip or expired offer. Exclude the owner separately. Never reset during the event.

### Consequences

Every scheduler and override validates against it. Never relax it to rescue a slot. Retire or hold instead.

## `D-005` — Classic scheduling uses a randomized partial Latin rectangle

- **Status:** accepted
- **Decided by:** Patrik · **Date:** 2026-11-24

### Context

Need fair no-repeat schedules without a predictable cyclic pattern.

### Decision

Build row by row with a seeded perfect matching. Exclude the owner. Validate no repeats. Default N-1 rounds with an overcrowding warning above 12 participants.

### Consequences

Commit-reveal is possible. Groups layer on the same primitive. Infeasible rounds return a conflict list.

## `D-006` — Async mode uses no formal groups

- **Status:** accepted
- **Decided by:** Patrik · **Date:** 2026-11-24

### Context

The async simulation compared global queue, soft migration, barriers, generational queues, group-aware pools, and a social-diversity objective.

### Decision

Use one global queue with a social-diversity objective. Do not use participant groups, migration, barriers, or reserved queues. Classic mode keeps groups.

### Consequences

Simpler async state and less leak surface. Better reach and fewer repeat pairs. Optional content tags can come later.

## `D-007` — 24h claim window, two daily waves, exposure completeness

- **Status:** accepted
- **Decided by:** Patrik · **Date:** 2026-11-24

### Context

The async simulation measured claim expiry, release timing, and completeness.

### Decision

Default claim window 24h with a 12h reminder. One daily release window with two dispatch waves. Completeness means exposure. Default length N-1 for 12 or fewer participants; 6-8 rounds above 12. Host can choose full exposure.

### Consequences

Faster events, fewer expired offers, lower p95 wait. Edit completeness is not promised. Full exposure takes months at large N.

## `D-008` — Async anti-loop protection

- **Status:** accepted
- **Decided by:** Patrik · **Date:** 2026-11-24

### Context

Exposure completeness can loop forever if a participant never claims.

### Decision

Offer only to Ready participants. After an expiry, never offer the same track to the same person again. Apply a host-configured strike/cooldown policy. Report exposure gaps. A claimed deadline miss counts as seen.

### Consequences

Events finish without chasing people. Some pairs remain unexposed and appear in the gap report. Credits are preserved.

## `D-009` — GitHub Releases, Windows + Linux, unsigned, no updater

- **Status:** accepted
- **Decided by:** Patrik · **Date:** 2026-11-24

### Context

Need a simple distribution path for a public open-source desktop app. macOS signing is not funded.

### Decision

Build Windows NSIS and Linux .deb/AppImage in GitHub Actions. Publish GitHub Releases with SHA256SUMS and attestations. Windows is unsigned. No macOS in v1. No auto-update in v1.

### Consequences

Free and simple distribution. SmartScreen warns on Windows. Users download installers manually.

## `D-010` — ASD-STE100 for specifications

- **Status:** accepted
- **Decided by:** Patrik · **Date:** 2026-11-24

### Context

Many conditional rules make ambiguous prose expensive.

### Decision

Use ASD-STE100 Simplified Technical English for PRD requirements, Gherkin, event settings, test plans, runbooks, CLI instructions, and error messages. Keep playful copy for user-facing text.

### Consequences

Shorter, less ambiguous specs. Writers use the glossary. Automated linting can come later.

## `D-011` — Lean tracker with hand-written specs

- **Status:** accepted
- **Decided by:** Patrik · **Date:** 2026-11-24

### Context

The project-scaffold has useful issues, decisions, evidence, and a ledger, but heavy PRD gates.

### Decision

Use the SQLite tracker for issues, decisions, evidence, sessions, and the append-only ledger. Keep the PRD, event settings, and Gherkin hand-written and editable. Use research-first TDD with worktrees. One plan review, no sign-off gate.

### Consequences

Durable work memory without scaffold ceremony. Specs stay readable. The tracker remains the evidence source.

## `D-029` — Lock the design scales

- **Status:** accepted
- **Decided by:** agent · **Date:** 2026-11-26

### Context

The design gate started with locked core rules and advisory spacing, type, and radius scales. The UI had 154 scale findings. The scales must become a contract after the cleanup.

### Decision

Lock the spacing scale 0, 2, 4, 6, 8, 12, 16, 20, 24, 32, 40, 48, 64 pixels. Lock the type scale 10, 10.5, 11, 11.5, 12, 13, 14, 16, 18, 20, 24, 30, 44 pixels. Lock the radius scale 0, 4, 6, 8, 12, 16, 999 pixels. The radius tokens are 4, 6, and 8 pixels. Negative spacing is not in the scale. A reviewed exception needs a baseline entry with a reason and an issue or an expiry.

### Consequences

A new off-scale value fails the design gate. A scale change needs a tracker decision or an ADR, a policy diff, a baseline diff, and a gate run. The UI uses the scales at every enforced viewport.

## `D-030` — Classic rounds start at the seed

- **Status:** accepted
- **Decided by:** agent · **Date:** 2026-11-26

### Context

The engine treats seed intake as a pre-round phase and calls the first exchange R1. The event length and file versions both collide with the round number.

### Decision

Round 1 is the seed round. Rounds 2..N are exchange rounds. A full-exposure classic event with N seeded participants has N total rounds. File versions use v1..vN. The host opens R1 explicitly with the pack and a seed deadline (default 7 days). The host planner shows a host-only R1 seed band. Participant-visible schedules start at R2.

### Consequences

The scheduler has N-1 exchange rounds. Plan and assignment rows use the display round number 2..N. Every surface derives the seed round from the round type, not from a minus-one offset. The commitment covers the R1 owner map.

## `D-031` — Classic round lifecycle and shared deadlines

- **Status:** accepted
- **Decided by:** agent · **Date:** 2026-11-26

### Context

The engine has no round table, no shared deadline, and gives each claim its own edit window. Seed intake opens implicitly at event creation.

### Decision

The host explicitly opens R1, then closes seed intake softly and locks the roster in one atomic action with at least two seeds. Seeds after the deadline wait for host approval. Reopen R1 is allowed before commit. Each round has one shared deadline set at open. Offers expire at that deadline and a claim uses the same deadline. Defaults are 7 days for R1, 24 hours for a normal exchange round, and 48 hours for the final exchange round. The host can move a whole round deadline for all offered, claimed, and overdue assignments, preserve individual extensions, return overdue to claimed, re-arm reminders, notify holders, and write one audit entry. Later planned rounds and the reveal shift by the same delta. Submitted work stays final.

### Consequences

Start commits and announces the schedule. Opening R2 is a separate host action. Deadline moves do not create a new commitment. The host can reopen one submitted assignment explicitly.

## `D-032` — Own assignments by default; open schedule is the only anonymity exception

- **Status:** accepted
- **Decided by:** agent · **Date:** 2026-11-26

### Context

A full participant-visible matrix plus full exposure is an owner oracle: each row's missing track is that person's seed.

### Decision

Participants see only their own assignments by default. They do not see group membership, group labels, song numbers, seed owners, titles, or comments before the reveal. The host can enable an open schedule. Open schedule means full transparency: the R2..RN matrix, seed owners, editor names, titles, and comments. The host sees a strong warning and the join card discloses the zero-anonymity exception. The R1 seed band is always host-only.

### Consequences

Participant DTOs never carry owner IDs, group labels, or the full matrix unless open schedule is on. Stream-safe mode must redact the planner and people views, not only the reveal presenter.

## `D-033` — Async intake, targets, windows, and expiry

- **Status:** accepted
- **Decided by:** agent · **Date:** 2026-11-26

### Context

Async exposure is over-determined when the host sets songs, claims per person, and editors per track independently. The engine reuses classic rounds for async.

### Decision

Async has its own seed intake, separate from classic R1. The host sets expected songs and claims per participant. One track per seeder, so the seeded participant count equals the track count. The host explicitly closes intake and starts the queue. Fewer seeds: the host waits or explicitly shrinks or reserves. Late seeds are off by default; the host can enable them with a feasibility check and a minimum claim quota. Claim and edit windows default to 24 hours each, are set per event, and lock at queue start. After start the host can extend a single offer, never shorten one or change the global window. Async uses handoffs, not synchronized rounds, and has no final round. First expired offer gives a 24-hour cooldown, then a re-offer. A second expiry pauses the participant until Ready or a host action. Expiry does not mark a pair seen and the pair is never re-offered. A claim marks the pair seen.

### Consequences

The app computes editors per track from songs and claims and warns when the value is fractional. The gap report separates exposure gaps from edit gaps. Async credits use a handoff index, never a round label.

## `D-034` — max_handoffs modes and the seed budget

- **Status:** accepted
- **Decided by:** agent · **Date:** 2026-11-26

### Context

The join flow collects max_handoffs, defaults to 4, and the classic scheduler ignores it. Full exposure for six people needs five exchange edits.

### Decision

The seed is not an edit and never consumes the max_handoffs budget. The host chooses per event between strict full exposure and flexible participation. Strict mode requires every active participant to have max_handoffs at least N-1; the host must raise a low budget, bench the person as a fill-in, or switch modes before R1 closes. Flexible mode gives each participant their earliest exchange rounds up to their budget, leaves gaps for the rounds they skip, and the host fills gaps with fill-ins or manual edits. Participants do not request specific tracks; the host edits the plan manually. Date-driven availability is a separate feature and can combine with either mode.

### Consequences

Classic no longer silently ignores max_handoffs. The planner shows coverage gaps for flexible events. Async uses the same budget for extra handoffs.
