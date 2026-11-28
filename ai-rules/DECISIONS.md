# Decision log

This file is the single human-readable decision log. It holds the
hand-written decisions D-001..D-028 and the tracker decisions D-029..D-070.
The tracker (`./bin/project decision`) is the durable record for D-029 and
later. The repository does not keep a generated decision dump. Read the
tracker only to confirm a code, a status, or a supersession link.

Accepted decisions are immutable. A later decision supersedes an earlier one.
A superseded entry keeps its text and gets a **Status** line that names the
newer decision. Add a new decision to the tracker, then append it here with the
same code.

## Code aliases

The tracker reused the codes D-001..D-011 for decisions that duplicate the
hand-written entries. The hand-written codes are authoritative because the
docs reference them. Use this table to read a tracker code.

| Tracker code | Tracker title | Hand-written entry |
|---|---|---|
| D-001 | Tauri 2 + Rust backend + React/TS UI | D-001 |
| D-002 | Twilight 0.18 for Discord | D-002 |
| D-003 | DM-first, claim-first delivery without privileged intents | D-004 |
| D-004 | Append-only seen ledger | D-007 |
| D-005 | Classic scheduling uses a randomized partial Latin rectangle | D-008 |
| D-006 | Async mode uses no formal groups | D-024 |
| D-007 | 24h claim window, two daily waves, exposure completeness | D-025 and D-026 |
| D-008 | Async anti-loop protection | D-026 |
| D-009 | GitHub Releases, Windows + Linux, unsigned, no updater | D-019 |
| D-010 | ASD-STE100 for specifications | D-023 |
| D-011 | Lean tracker with hand-written specs | D-021 |
| D-018 | test (superseded) | None. D-036 retracts this tooling test entry. The hand-written D-018 is unrelated and stays valid. |
| D-035 | Formal verification program v2 (superseded) | None. A placeholder entry; D-037 supersedes it. |

The tracker codes D-012..D-017 and D-019..D-028 do not exist. Those codes
always mean the hand-written entries below.

## Supersession summary

| Entry | Superseded part | By |
|---|---|---|
| D-009 | "Async group migration remains an open decision" | D-024 |
| D-011 | The 12h claim window and the auto-pause after two expiries | D-025, D-033, D-041 |
| D-014 | The 12h fill-in claim window, the 3-offer cap, and the shared handoff budget | D-069, D-070, D-050, D-065 |
| D-016 | The `R#` host file names and the round folders | Corrected to `v#`; the folder layout is not built |
| D-018 | The cap of two replans | D-043 |
| D-025 | The fixed 12h claim reminder | D-041 (12h stays the default) |
| D-026 | The fixed never-re-offer rule and the strike recommendation | D-041 |
| D-029 | The whole design scale | D-047 |
| D-039 | The whole fill-in candidate rule | D-049, then D-050 |
| D-040 | The fill-in due-time default | D-069 (a separate claim window starts the edit due time) |
| D-049 | The whole fill-in candidate rule | D-050 |
| D-032 | The owners, titles, names, and comments in an open schedule | D-045 |

## D-001 — Tauri 2 + Rust backend + React/TS UI

**Context:** cross-platform desktop host with an embedded Discord bot, SQLite,
and local files; solo developer; Rust preferred.
**Decision:** Tauri 2.12, Rust for all core logic, React 19 + TypeScript for the
UI. No Electron.
**Consequences:** small installers, one runtime, native Rust bot; three WebView
engines and Linux WebKitGTK packaging must be tested. The UI is TS, but no Node
runtime ships.
**Rejected:** Electron (100 MB Chromium, no Rust), Tauri + Node sidecar (two
runtimes), Leptos/Yew UI (slower UI velocity, smaller ecosystem).

## D-002 — Twilight 0.18 for Discord

**Context:** need guild slash commands, buttons/modals, DM attachments/history,
rate limits, and current API models; stable and testable.
**Decision:** Twilight 0.18, with a thin interaction adapter and pure domain
handlers.
**Consequences:** ~1–2 days of command plumbing; plain-data events make fixtures
easy; no macro framework.
**Rejected:** Serenity 0.12 + Poise (older API, no current CV2/file-upload
models), Serenity `next` (unreleased).

## D-003 — rusqlite (bundled) + migration ladder

**Decision:** rusqlite with bundled SQLite, single writer behind a mutex,
`spawn_blocking`, WAL, `user_version` migrations.
**Consequences:** no external SQLite, easy in-memory tests, no async DB tax; no
SQL over IPC.
**Rejected:** sqlx (MSRV/async complexity), tauri-plugin-sql (SQL in the
frontend), an ORM (premature).

## D-004 — DM-first, claim-first delivery; no privileged intents

**Context:** Discord interaction tokens die in 15 minutes; ephemeral attachments
are GC'd; DMs to the app are exempt from `MESSAGE_CONTENT`; a bot can recover a
DM channel with `POST /users/@me/channels`.
**Decision:** offers are DMs with buttons; the file is sent only after Claim;
submissions are DM attachments (slash-command fallback). Intents: `GUILDS` +
`DIRECT_MESSAGES` only.
**Consequences:** claim-first protects the seen ledger; DM backfill is possible;
DM-closed users need a private-thread fallback. No prefix commands.
**Rejected:** ephemeral-only delivery (not durable), prefix commands (privileged
intent), forwarding CDN URLs (leaks identity/expiry).

## D-005 — SQLite + local files; Discord is not storage

**Decision:** SQLite holds state/credits; files live in a user-chosen library
folder; attachments are downloaded immediately.
**Consequences:** offline-capable, auditable, portable; the host must back up;
20 MiB bot redistribution cap.
**Rejected:** Discord CDN as the system of record, a hosted server.

## D-006 — Anonymous by default, with explicit boundaries

**Decision:** owners/titles hidden until reveal; opaque track codes; outbound
filenames strip holder/version; group membership hidden; skips host-only;
public progress aggregate-only.
**Consequences:** host and platform admins can still see; small events cannot be
perfectly anonymous. Disclose.
**Rejected:** public matrix by default, real names in filenames, public skip
lists.

## D-007 — Seen ledger is the core invariant

**Decision:** append-only `(track, participant)` ledger; claim/receipt, submit,
and claim-then-skip count; pre-claim skip/expiry does not; owner seed is a
separate own-track exclusion. It persists across groups, fill-ins, leaves, and
replans for the event's lifetime.
**Consequences:** every scheduling operation validates against it; never relax
it to rescue a slot; retire or hold instead.

## D-008 — Classic scheduling: randomized partial Latin rectangle

**Decision:** row-by-row perfect matching, seeded and canonical; no own track,
no repeat, one per person/track per round; N-1 default with a warning above 12
participants; host can cap.
**Rejected:** bare cyclic shift (predictable), pure greedy (dead-ends).
**Consequences:** commit-reveal is possible; groups are a layer over the same
primitive.

## D-009 — Async: per-person queue, configurable dispatch

**Decision:** one claimed track per person; claim-first; deficit-ordered hybrid
default, with strict queues and an opportunistic pool as advanced options;
completeness default on; one daily release window; releases changeable (future
shifts, claimed fixed).
**Consequences:** good throughput and fairness; async group migration remains
an open decision; no fixed future schedule to seal (seal policy + transcript).
**Status:** superseded in part. D-024 closes the group-migration question:
async has no groups and no migration.
**Rejected:** barrier-only async (wait), unbounded opportunistic pool (fast-pair
clustering).

## D-010 — Host control at deadlines; no silent auto-advance

**Decision:** soft deadlines, reminders, at-deadline prompt, host decision;
automatic skip only if explicitly configured and disclosed. Extensions +1/+6/+12
with the first auto-approved up to 6h, max 2.
**Consequences:** the host is the bottleneck; the app must make decisions
one-click and support a deputy/co-host.

## D-011 — Claim window 12h; expiry returns to pool

**Decision:** default 12h (host-set); unclaimed offers expire without "seen";
after two expiries auto-pause + host alert. A claimed file can be skipped
(seen) and the participant may leave.
**Consequences:** timezone-friendly; requires an active claim action.
**Status:** superseded in part. The claim window default is 24h (D-025, D-033).
The expiry policy is a preset setting (D-041). The default preset starts a 24h
cooldown at the first expiry and pauses the participant at the second expiry.
An expired offer does not return the track to the same person by default.

## D-012 — Groups: participants rotate, 5–12, K≈T/4

**Decision:** use the simulation lookup for group count; soft 5–12; rotate
participants; swap every ~T/4 rounds (3–5 phases), with early swap if a group
cannot fill; allow "exhausted" only with a warning; merge/re-split for repair;
global-pool fallback for an infeasible phase.
**Consequences:** cross-pollination is the main measurable benefit; hidden group
membership; the host sees A/B/C names.
**Rejected:** rotate both (not additive), exhaustion by default (often never
fires), calendar-only strict phases.

## D-013 — Fun score is host-only

**Decision:** a planning metric (social reach, variety, cross-pollination,
momentum, fairness, freshness) with guardrails; never shown to participants.
**Consequences:** helps choose configurations; it is a proxy, not a judgment of
music. The host can hide it.

## D-014 — Fill-ins: pool + converts + host picks; sequential offers

**Decision:** dedicated volunteers and auto-converted non-seeders; host can
appoint a specific eligible person; sequential first-accept, 12h, max 3; shared
handoff budget; marked in credits; can name if final.
**Rejected:** broadcast races, auto-assignment without consent, unlimited
rescues.
**Status:** refined on 2026-11-28 by D-069 and D-070. D-069 changes "12h" to a
6-hour default claim window; the edit due time starts at claim. D-070 removes
the 3-offer cap for a host-ordered list; the auto-chain keeps the 3-offer
budget. D-050 and D-065 change "shared handoff budget" and "marked in credits":
a tier-2 fill-in rescue is an extra turn outside the budget, and the badge
marks the rescued turn, not the role.

## D-015 — Files: allowlist, no format changes, module-aware scrub

**Decision:** default modules+audio+zip; per-event allowlist; format changes
forbidden; 20 MiB cap; external-link/host-import fallback; module-aware scrub
with A/B render; untouched originals in a host vault; pack check is a hash report
plus host review, never a false-positive rejection.
**Consequences:** arbitrary formats are accepted only if allowlisted; preview
only for supported types; no perfect scrub guarantee.

## D-016 — Naming and storage

**Decision:** opaque track codes (for example `S7F3A`); `v#` = track version
(`v1` is the seed); `R#` = round number only; host file name
`v4.S7F3A.alice.it` (version, code, holder); outbound `S7F3A.it`; round folders
+ track history; SQLite maps them. Last holder names the track at final
submission; fallback previous submitter.
**Rejected:** sequential/song numbers tied to submission order, holder names in
outbound files.
**Status:** corrected on 2026-11-28. The original text used `R#` for the
version, which conflicts with the round model (D-030). The detailed host file
names and the round/track folders are not built. The code stores blobs by
SHA-256 (`swap-files` `BlobStore`), and SQLite records the version, code, and
holder. The reveal ZIP names files `{pos}_{code}.{ext}`.

## D-017 — Reveal: stream first, Discord second

**Decision:** host-triggered after the release stream; whole-server Discord
reveal; presenter window with lineage, R1/final playback, credits, comments,
fun stats; per-track layout host-configurable; Markdown+HTML runbook; schedule
disclosed at reveal by default; comments attributed with host redaction; skips
anonymous gaps by default.
**Consequences:** a delightful show; the app needs a presenter mode and export;
no progressive elimination leaks.

## D-018 — Commit-reveal seal when the schedule is locked

**Decision:** bot posts a hash at lock; `/swap verify` recomputes at reveal; no
file hashes public. Classic seals the schedule; async seals the policy + a
hash-chained transcript. Replans create a new commitment and audit; max 2.
**Consequences:** proves policy adherence, not a fixed future async schedule.
**Status:** superseded in part by D-043. There is no replan cap. The reveal
publishes `verify.txt`, and the bot offers `/swap verify`.

## D-019 — Releases: GitHub, Windows+Linux, unsigned, no updater, macOS deferred

**Decision:** GitHub Releases via Actions; Windows NSIS + Linux .deb/AppImage;
no macOS until Apple signing is funded; Windows unsigned (SmartScreen warning
documented); auto-update off in v1.
**Consequences:** simple and free; non-technical Windows users may hesitate at
SmartScreen; no silent updates.
**Rejected:** Electron auto-update stack, paid signing now, macOS unsigned.

## D-020 — Licenses and privacy posture

**Decision:** code MIT; album content CC BY-NC-SA 4.0 by default; minimal data
retention; participant export/delete; no telemetry; opt-in redacted diagnostics.
**Consequences:** community-friendly; hosts must publish a privacy note; no
usage analytics.

## D-021 — Lean tracker, hand-written specs

**Decision:** SQLite tracker for issues/decisions/evidence/ledger; hand-written
`docs/PRD.md`, `docs/EVENT_SETTINGS.md`, and `specs/features/`; lean TDD +
worktrees; one plan review, no PRD sign-off gate.
**Consequences:** durable work memory without scaffold ceremony; specs are
editable; the tracker remains the evidence source.

## D-022 — English, i18n-ready, WCAG AA

**Decision:** English UI/bot copy in v1; externalized strings; localized
dates/times; WCAG AA contrast and keyboard navigation.
**Consequences:** translation can come later without a rewrite.

## D-023 — ASD-STE100 for specifications

**Context:** the product has many conditional rules (claim, skip, seen, group
swaps, extensions). Ambiguous prose causes implementation and test mistakes.
**Decision:** use ASD-STE100 Simplified Technical English for the PRD,
requirements, Gherkin, event settings, test plans, runbooks, CLI instructions,
and error messages. Keep the playful voice for user-facing copy, but keep its
instructions short and active.
**Consequences:** writers must use the glossary and short sentences; the
specification reads less like marketing; automated linting can be added later.
**Rejected:** ad-hoc plain English (ambiguous), formal RFC 2119 prose only (does
not control sentence structure or terminology).

## D-024 — Async has no formal groups

**Context:** the async simulation compared a global queue, per-person soft
migration, group barriers, generational queues, group-aware pools, and a
global queue with a social-diversity objective.
**Decision:** async mode uses one global queue with a social-diversity
objective. It does not use participant groups, migration, barriers, or reserved
queues. Classic mode keeps groups. Optional genre/mood content tags can come
later.
**Consequences:** simpler async state and less anonymity leak surface; social
reach improves and repeat pairs decrease. Cross-group handoff is not a formal
metric. The host cannot configure async groups.
**Rejected:** per-person migration (no makespan gain, +4–7h p95 wait, 10–12
points fewer cross-group handoffs), barriers (starvation), generational queues
(supply lockup).

## D-025 — 24h claim window and two daily waves

**Context:** the async simulation measured claim expiry and release timing.
**Decision:** the claim window is 24h with a 12h reminder. A daily release
window uses two dispatch waves. The host can change the time. Unclaimed offers
shift; claimed assignments keep their due date.
**Consequences:** 2–3 days faster events, 8–11h lower p95 wait, and 2–3% expired
offers instead of ~10%. Participants get a full day to claim across timezones.
**Rejected:** 12h claim window (slower, more expiry), one wave per day (higher
p95 wait), four waves (noise, no gain at N=40).
**Status:** amended by D-041. The claim reminder is a setting (6h, 12h, none,
custom); 12h stays the default. D-042 sets the release time as wall-clock
HH:MM in the host IANA zone.

## D-026 — Exposure completeness, with anti-loop protection

**Context:** the seen rule says a claim/receipt counts as seen. A post-claim skip
or a claimed deadline miss therefore removes the pair permanently. Full edit
completeness is capped at ~93–95%. Full exposure (everyone receives every
track) is achievable but takes months at large N.
**Decision:** "complete" means exposure. Async length is N-1 for 12 or fewer
participants and 6–8 rounds above 12; the host can choose full exposure after
seeing the duration. Do not loop forever on an unresponsive participant. Offer
only to Ready participants. After an expired offer, never re-offer the same
track to the same person in that event. Apply a host-configured strike/cooldown
policy (recommended: 2 strikes auto-pause, 3 unresponsive) and report exposure
gaps. Keep credits. A claimed deadline miss counts as seen.
**Consequences:** the event can finish without chasing people. Some participants
will not receive every track; the gap report makes this explicit. Edit
completeness is not a promise.
**Rejected:** edit-complete targets (impossible under the seen rule), infinite
re-offers (harassment/loop), auto-removal (harsh), changing the seen rule
(weakens the core invariant).
**Status:** superseded in part by D-041. Same-track re-offer after expiry is a
setting: never (default), after cooldown, last resort, or host. The expiry
policy is a preset: strike + cooldown (default), cooldown only, host review, or
none.

## D-027 — No small-event warning and host-chosen large exports

**Context:** a hard minimum would block private small events. Large album ZIPs
can exceed the Discord limit.
**Decision:** do not enforce a minimum participant count and do not show a
small-event anonymity warning; the rules state the limits. Large album and
keepsake exports use the host-chosen delivery method; the default is a
host-mediated neutral link.
**Consequences:** the host can run a 2-person event. The app does not protect the
host from weak anonymity. The host must choose a safe link host and verify the
file.
**Rejected:** a hard minimum, a mandatory warning, and a single fixed export
channel.

## D-028 — Classic schedule strategy suite

**Context:** a single randomized matching can produce generic squares and does
not control repeated participant pairs. The host wants varied and balanced
classic schedules.
**Decision:** the classic planner has three seeded strategies. `balanced` is the
default. It uses min-cost matching with a penalty for participant pairs that
already met, especially in the last two rounds. `random_matching` keeps the
row-by-row perfect matching. `cyclic` builds a cyclic Latin square and permutes
the rows, columns, and symbols. Cyclic falls back to matching when the roster
does not have exactly one track per participant. All strategies are
deterministic for the same canonical input and seed. The event setting
`scheduler` selects the strategy. The default is `balanced`.
**Consequences:** the host can trade speed for social balance. Balance metrics
are available for tests and future UI. The demo planner uses the same randomized
shape instead of a bare cyclic shift.
**Rejected:** bare cyclic shift as the default (predictable), pure greedy
(dead-ends), and one fixed algorithm for every event.

## Tracker decisions D-029 and later

The entries below copy the tracker decisions. The tracker text is the source.
Run `./bin/project decision show D-0xx` for the full record.

## D-029 — Lock the design scales

**Date:** 2026-11-26.
**Decision:** lock the spacing scale 0, 2, 4, 6, 8, 12, 16, 20, 24, 32, 40,
48, 64 pixels. Lock the type scale 10, 10.5, 11, 11.5, 12, 13, 14, 16, 18, 20,
24, 30, 44 pixels. Lock the radius scale 0, 4, 6, 8, 12, 16, 999 pixels. The
radius tokens are 4, 6, and 8 pixels. Negative spacing is not in the scale. A
reviewed exception needs a baseline entry with a reason and an issue or an
expiry.
**Consequences:** a new off-scale value fails the design gate. A scale change
needs a tracker decision or an ADR, a policy diff, a baseline diff, and a gate
run.
**Status:** superseded by D-047.

## D-030 — Classic rounds start at the seed

**Date:** 2026-11-26.
**Decision:** Round 1 is the seed round. Rounds 2..N are exchange rounds. A
full-exposure classic event with N seeded participants has N total rounds. File
versions use v1..vN. The host opens R1 explicitly with the pack and a seed
deadline (default 7 days). The host planner shows a host-only R1 seed band.
Participant-visible schedules start at R2.
**Consequences:** the scheduler has N-1 exchange rounds. Plan and assignment
rows use the display round number 2..N. Every surface derives the seed round
from the round type, not from a minus-one offset. The commitment covers the R1
owner map.

## D-031 — Classic round lifecycle and shared deadlines

**Date:** 2026-11-26.
**Decision:** the host explicitly opens R1, then closes seed intake softly and
locks the roster in one atomic action with at least two seeds. Seeds after the
deadline wait for host approval. Reopen R1 is allowed before commit. Each round
has one shared deadline set at open. Offers expire at that deadline and a claim
uses the same deadline. Defaults are 7 days for R1, 24 hours for a normal
exchange round, and 48 hours for the final exchange round. The host can move a
whole round deadline for all offered, claimed, and overdue assignments,
preserve individual extensions, return overdue to claimed, re-arm reminders,
notify holders, and write one audit entry. Later planned rounds and the reveal
shift by the same delta. Submitted work stays final.
**Consequences:** start commits and announces the schedule. Opening R2 is a
separate host action. Deadline moves do not create a new commitment. The host
can reopen one submitted assignment explicitly.

## D-032 — Own assignments by default; open schedule is the only anonymity exception

**Date:** 2026-11-26.
**Decision:** participants see only their own assignments by default. They do
not see group membership, group labels, song numbers, seed owners, titles, or
comments before the reveal. The host can enable an open schedule. Open schedule
means full transparency: the R2..RN matrix, seed owners, editor names, titles,
and comments. The host sees a strong warning and the join card discloses the
zero-anonymity exception. The R1 seed band is always host-only.
**Consequences:** participant DTOs never carry owner IDs, group labels, or the
full matrix unless open schedule is on. Stream-safe mode must redact the
planner and people views, not only the reveal presenter.
**Status:** superseded in part by D-045. An open schedule shows assignments
only. It never shows owners, titles, or comments before the reveal.

## D-033 — Async intake, targets, windows, and expiry

**Date:** 2026-11-26.
**Decision:** async has its own seed intake, separate from classic R1. The host
sets expected songs and claims per participant. One track per seeder, so the
seeded participant count equals the track count. The host explicitly closes
intake and starts the queue. Fewer seeds: the host waits or explicitly shrinks
or reserves. Late seeds are off by default; the host can enable them with a
feasibility check and a minimum claim quota. Claim and edit windows default to
24 hours each, are set per event, and lock at queue start. After start the host
can extend a single offer, never shorten one or change the global window. Async
uses handoffs, not synchronized rounds, and has no final round. First expired
offer gives a 24-hour cooldown, then a re-offer. A second expiry pauses the
participant until Ready or a host action. Expiry does not mark a pair seen and
the pair is never re-offered. A claim marks the pair seen.
**Consequences:** the app computes editors per track from songs and claims and
warns when the value is fractional. The gap report separates exposure gaps from
edit gaps. Async credits use a handoff index, never a round label.
**Status:** D-041 supersedes the expiry sentences. The re-offer option is a
setting with `never` as the default. The first expiry starts a 24-hour
cooldown. The second expiry pauses the participant. The pair is not re-offered
by default.

## D-034 — max_handoffs modes and the seed budget

**Date:** 2026-11-26.
**Decision:** the seed is not an edit and never consumes the `max_handoffs`
budget. The host chooses per event between strict full exposure and flexible
participation. Strict mode requires every active participant to have
`max_handoffs` at least N-1; the host must raise a low budget, bench the person
as a fill-in, or switch modes before R1 closes. Flexible mode gives each
participant their earliest exchange rounds up to their budget, leaves gaps for
the rounds they skip, and the host fills gaps with fill-ins or manual edits.
Participants do not request specific tracks; the host edits the plan manually.
Date-driven availability is a separate feature and can combine with either
mode.
**Consequences:** classic no longer silently ignores `max_handoffs`. The
planner shows coverage gaps for flexible events. Async uses the same budget for
extra handoffs.

## D-035 — Formal verification program v2 (placeholder)

**Date:** 2026-11-28.
**Status:** superseded by D-037. The tracker entry has placeholder text only.

## D-036 — Retract test entry

**Date:** 2026-11-28.
**Decision:** the tracker entry D-018 ("test") was a tooling test entry created
by mistake on 2026-11-28. It has no meaning. The hand-written D-018 is not
affected.

## D-037 — Formal verification program v2

**Date:** 2026-11-28.
**Decision:** add TLA+ trace validation of the engine, type-level anonymity,
DB-enforced invariants, Apalache inductive invariants, and an
untrusted-planner/verified-checker pattern with infeasibility certificates. New
models stay on TLA+.
**Consequences:** `specs/tla/mapping.json` drives the CI formal gate.
`scripts/check.sh` runs the formal checks when Java is present and reports a
skip otherwise. See `docs/FORMAL_METHODS.md`.

## D-038 — Gherkin traceability and STE lint gates

**Date:** 2026-11-28.
**Decision:** every scenario maps to an executing test or carries `@pending`.
The STE linter is a hard gate for `specs/features` and `docs/EVENT_SETTINGS.md`
and advisory elsewhere.
**Consequences:** the definition of done becomes checkable. Unbuilt behaviour
is visible as `@pending`.

## D-039 — Fill-in candidates exclude planned-later people

**Date:** 2026-11-28.
**Decision:** in classic mode, a person whom the locked plan assigns to a track
in a later round must not take that track as a fill-in. Offer time refuses any
seen pair. The seen-ledger insert fails on a duplicate.
**Consequences:** in a full N-1 classic event, regular participants cannot fill
in; dedicated fill-ins are the pool.
**Status:** superseded by D-049, which D-050 supersedes. Chain: D-039 → D-049 → D-050.

## D-040 — Fill-in due time and flow setting

**Date:** 2026-11-28.
**Decision:** the host sets the fill-in due time (default max(round deadline,
now+24h)). The event setting `fill_in_flow` is `host_appoints` (default),
`auto_chain`, or `host_list`.
**Consequences:** the next classic round waits for a fill-in. No silent
auto-advance.
**Status:** refined by D-069. D-069 supersedes the old due-time default for
auto-chain and host-list offers. Those offers have a separate claim window
(default 6 hours). The edit due time starts at claim (default 24 hours). The
D-040 slot default `max(round deadline, now + 24h)` remains for a host
appointment.

## D-041 — Build the async expiry, re-offer, and claim-reminder settings

**Date:** 2026-11-28.
**Decision:** build expiry presets (strike + cooldown default, cooldown only,
host review, none), re-offer options (never default, after cooldown, last
resort, host), and the claim-reminder setting (6h, 12h, none, custom). Strikes
reset on resume; auto-pause alerts the host.
**Consequences:** more settings to test. Validation refuses a reminder at or
after the claim window. Decided, building in MSW-084 (presets), MSW-085
(re-offer), and MSW-086 (claim reminder).

## D-042 — Async release time is wall-clock in the host zone

**Date:** 2026-11-28.
**Decision:** the async release time is HH:MM in the host IANA zone, not UTC.
**Consequences:** the release keeps its local time across DST. Decided,
building in MSW-087; the code still reads the release time as UTC.

## D-043 — Verifiable commitment; no replan cap

**Date:** 2026-11-28.
**Decision:** the reveal publishes `verify.txt` and the bot offers
`/swap verify`. The unenforced cap of 2 replans is dropped.
**Consequences:** participants can verify the seal with `sha256sum`.
`verify.txt` and `/swap verify` are decided, building in MSW-088.

## D-044 — Build pack types, rotation modes, async late seeds

**Date:** 2026-11-28.
**Decision:** build pack types including none, rotation modes participants /
track pools / merge-re-split (rotate-both stays rejected per D-012), and async
late seeds.
**Consequences:** no-pack events become possible.

## D-045 — Open schedule shows assignments only

**Date:** 2026-11-28.
**Decision:** an open schedule shows who edits which track code in each round.
It never shows owners or titles before the reveal. This supersedes the
owner/title part of D-032.
**Consequences:** invariant 3 holds in every mode with no exception. Decided,
building in MSW-120; the open mode currently exposes owners, titles, and
comments.

## D-046 — Host UI direction: restrained tracker-retro Monsquaz brand

**Date:** 2026-11-28.
**Decision:** green #1de376 is the only action color; pink #ff007f is for the
mark and the reveal only; danger is orange-red; a banded 3px green stripe;
Archivo plus IBM Plex Mono; theme follows the OS; lifecycle nav; queue-first
mission control; reveal stream-safe by default; engine-generated demo fixtures.
**Consequences:** supersedes the visual parts of `ai-rules/UX.md`.
**Status:** decided, building in MSW-099. The current UI does not use the new
brand tokens.

## D-047 — Design scale reform

**Date:** 2026-11-28.
**Decision:** type 12/13/14/16/20/24/32/44 with a locked 12px floor; spacing
0/4/8/12/16/24/32/48/64; radius 4/8/999. Supersedes D-029 and ADR 0006.
**Consequences:** Every view is migrated with no baseline exceptions for the
old sizes.
**Status:** decided, building in MSW-099 (scales and brand tokens) and MSW-100
(gate rules). The UI is not migrated. The old scales in `design/policy.json`
remain locked until those issues land.
*Corrected 2026-11-28 (MSW-092):* the consequence text above is the tracker
record, and the tracker is the source. It describes the target state, not the
current UI. The Status line records the current state.

## D-048 — New design-gate rules

**Date:** 2026-11-28.
**Decision:** one primary per view and per dialog, copy hygiene, a single date
formatter, 24px targets, the status triple, and a pink-token restriction.
**Consequences:** reject-list items become deterministic checks.
**Status:** decided, building in MSW-100. The gate does not enforce these rules
yet.

## D-049 — Classic fill-ins are never regular participants

**Date:** 2026-11-28.
**Decision:** in classic mode, a fill-in is only a person with the fill-in
role: a volunteer, or an unseeded signup that the app converts at roster lock.
A seeded participant must never take a slot as a fill-in. Offer time still
refuses any seen pair, and the seen-ledger insert fails on a duplicate. Async
mode does not change.
**Status:** supersedes D-039. Superseded by D-050.

## D-050 — Classic fill-in eligibility in two tiers

**Date:** 2026-11-28.
**Decision:** in classic mode, the app picks a fill-in from two tiers:
- **Tier 1:** a person with the fill-in role.
- **Tier 2:** used only when no tier-1 person is eligible. A seeded
  participant who already submitted their own track in the current round and
  whom the locked plan never gives that track. This happens only when the
  plan does not guarantee every track to that participant: groups, handoff
  caps, or away ranges.

A fill-in turn of a participant is an extra turn. It does not count toward
`max_handoffs`, and the credits mark it as a fill-in. Offer time refuses any
seen pair, and the seen-ledger insert fails on a duplicate. Async mode does not
change.
**Status:** supersedes D-049 (chain: D-039 → D-049 → D-050).

## D-051 — Complete opt-out with confirmation and host revert

**Date:** 2026-11-28.
**Decision:** a participant can leave at any time through `/swap leave`, a
Leave button on every bot DM, or the host. A warning states the consequences,
and the person (or the host) must confirm. Leave is immediate: a held track
becomes a skip after claim (seen), the track returns unchanged, the fill-in
flow starts, and the host gets an alert. Future plan turns become gaps. Queued
offers, reminders, and nudges are cancelled, and the bot sends no further DMs;
the confirmation is an ephemeral reply. The seed keeps circulating, also when
the person leaves before the roster lock. Past credits keep the name. The host
can revert a leave: the person confirms with a button, then gets planned turns
from the next round; an auto-skipped turn stays a skip, and a fill-in keeps a
taken slot. Every leave and revert is audited.
**Consequences:** the planner must handle a track whose owner is not in the
roster. Leavers return as fill-ins only through a host revert.

## D-052 — Classic holes instead of a blocked round

**Date:** 2026-11-28.
**Decision:** when a planned person is paused, left, or removed, the round
opens for everyone else. Their cell becomes a hole: the track waits and a
fill-in decision card appears (D-050 tiers, `fill_in_flow`). With no eligible
fill-in, the track passes on unchanged. Mission control shows the holes before
the host opens the round.
**Consequences:** one absent person no longer stalls the event.

## D-053 — Wrong-file repair: rewind with extra material

**Date:** 2026-11-28.
**Decision:** the host marks a version as wrong basis. The main line rewinds
to the last good version or a host-imported correct file. The current holder
receives the rewound version plus the orphaned version as optional extra
material. The orphaned edits are kept as a credited B-side in the album.
Versions stay append-only; seen is per track, so re-delivery is safe. The plan
does not change.
**Consequences:** every person's work survives as main line, merge material,
or B-side.

## D-054 — Outage recovery

**Date:** 2026-11-28.
**Decision:** on-time is judged by the Discord message timestamp with a small
skew allowance. Senders get a receipt that states the send time. Offers and
deadlines that passed while the bot was offline do not expire. The host sees
one welcome-back card with the downtime and a recommended extension by the
downtime; one click extends, and affected holders get one DM.
**Consequences:** an outage never costs a participant anything. No silent
change to the event clock.

## D-055 — Event pause

**Date:** 2026-11-28.
**Decision:** pause freezes the event clock: no offers, reminders, expiries,
or nudges. The bot posts one channel message. Participants can still submit.
Resume shifts every open deadline and claim window by the pause length; only
people who hold a track get one DM with the new deadline.
**Consequences:** implements the documented Paused state. Building in
MSW-113.

## D-056 — Change future round length after lock

**Date:** 2026-11-28.
**Decision:** the host can change the round length for rounds not yet opened,
with a date preview and one channel note. The current round uses Move round
deadline. The seal covers assignments, not dates.
**Consequences:** a wrong round length is fixable without a replan.

## D-057 — Automatic backups and restore

**Date:** 2026-11-28.
**Decision:** before every state transition and daily: a consistent SQLite
snapshot plus a blob manifest in a host-chosen folder, with retention. A
restore wizard checks integrity and asks for the bot token, which is never in
a backup. A single-writer guard prevents two machines from running the same
event.
**Consequences:** fixes the false RUNBOOK and REQ-OPS-001 promise. Building in
MSW-115.

## D-058 — Host remove and alt-account link

**Date:** 2026-11-28.
**Decision:** host Remove uses the D-051 leave mechanics without consent,
audited. Link as same person merges seen sets and own-track exclusions; any
future plan cell that now breaks an invariant becomes a hole (D-052).
**Consequences:** bans and alt accounts no longer break invariants.

## D-059 — Co-host

**Date:** 2026-11-28.
**Decision:** the host names a co-host Discord user. The co-host can resolve
decision cards through slash commands (extend, missed, appoint fill-in). Every
host action records its actor. Discord never shows the mapping.
**Consequences:** implements the D-010 deputy. Building in MSW-117.

## D-060 — Host import version

**Date:** 2026-11-28.
**Decision:** the host can add a version on a holder's behalf from a chosen
file, with explicit overrides for size and format and an audited reason.
**Consequences:** implements the D-015 host-import fallback.

## D-061 — Post-reveal errata not planned

**Date:** 2026-11-28.
**Decision:** post-reveal credit, title, and track-withdrawal edits are not
planned. The reveal stays one-shot.
**Consequences:** a takedown after the reveal is handled outside the app.

## D-062 — Mirror the pinned TLA+ tools

**Date:** 2026-11-28.
**Decision:** the formal check downloads `tla2tools.jar` from our own release
asset, pinned by SHA-256, not from the mutable upstream asset.
**Consequences:** upstream changes cannot break or alter the formal check.
Implemented in MSW-119; the update procedure is in `docs/FORMAL_METHODS.md`.

## D-063 — At most one extra fill-in turn per person per round

**Date:** 2026-11-28.
**Decision:** a seeded participant can take at most one extra tier-2 fill-in
turn in one round. There is no event-wide cap.
**Consequences:** rescues spread across people.

## D-064 — Auto-chain falls back to tier 2 before the host

**Date:** 2026-11-28.
**Decision:** when tier-1 people decline or time out, an `auto_chain` continues
with eligible tier-2 participants inside the same 3-offer budget. Then the bot
opens a host decision card.
**Consequences:** fewer host interruptions.

## D-065 — Fill-in credit marks the turn

**Date:** 2026-11-28.
**Decision:** the fill-in badge marks a turn that rescued a slot, whoever did
it. Planned turns of a person with the fill-in role are plain credits. Stats
count rescued turns.
**Consequences:** credits describe what happened, not who someone is.

## D-066 — Final title fallback

**Date:** 2026-11-28.
**Decision:** when the final round cell is empty, the bot asks the person who
submitted the latest version to name the track. If that person does not answer
before the reveal, or has left, the host names the track.
**Consequences:** no track reaches the reveal without a title request.

## D-067 — Classic reveal projection recomputes on round open

**Date:** 2026-11-28.
**Decision:** the host UI recomputes the classic reveal projection each time a
round opens or a deadline moves. The host sees the result as an estimate:
"Reveal about Sat 19 Dec". The schedule-lock projection is not used after the
event starts.
**Consequences:** a late round open does not understate the days remaining. The
projection can move later during the event, and the UI must say that it is an
estimate.

## D-068 — A restore invitation expires after 7 days

**Date:** 2026-11-28.
**Decision:** the bot sends one restore invitation to a person who left. The
Confirm and Decline buttons stop working 7 days after the bot sends the
invitation. The host sees Restore expired and can send a new invitation.
**Consequences:** a person cannot rejoin weeks later in a different phase
without a new host decision. The invitation stores its sent time.

## D-069 — A fill-in offer has a separate claim window

**Date:** 2026-11-28.
**Decision:** each auto-chain or host-list fill-in offer must be claimed within
the claim window. The claim window default is 6 hours and it is an event
setting. The edit due time, default 24 hours, starts when the person claims.
This decision refines the due-time rule in D-040.
**Consequences:** a chain of 3 offers takes at most 18 hours before the host is
asked. Whoever claims gets the full edit window. The engine stores the claim
deadline and the edit due time separately.

## D-070 — The host-ordered fill-in list has no size limit

**Date:** 2026-11-28.
**Decision:** the host can order any number of people in a fill-in list. The
list has no fixed cap. Each offer in the list uses the claim window from D-069.
**Consequences:** a large list can delay the host decision. The UI shows the
claim deadline for each offer.
