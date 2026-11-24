# Decision index

Architecture and product decisions with their rejected alternatives. Issue-level
decisions also live in the tracker (`./bin/project decision`); this file is the
human-readable index and the place to record decisions that shape the codebase.
Accepted decisions are immutable; supersede them with a new entry.

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

## D-015 — Files: allowlist, no format changes, module-aware scrub

**Decision:** default modules+audio+zip; per-event allowlist; format changes
forbidden; 20 MiB cap; external-link/host-import fallback; module-aware scrub
with A/B render; untouched originals in a host vault; pack check is a hash report
plus host review, never a false-positive rejection.
**Consequences:** arbitrary formats are accepted only if allowlisted; preview
only for supported types; no perfect scrub guarantee.

## D-016 — Naming and storage

**Decision:** opaque track codes (e.g. `S7F3A`); `R#` = track version; host
storage `R4.S7F3A.alice.it`; outbound `S7F3A.it`; round folders + track history;
SQLite maps them. Last holder names the track at final submission; fallback
previous submitter.
**Rejected:** sequential/song numbers tied to submission order, holder names in
outbound files.

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

