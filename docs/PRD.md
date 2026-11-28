# Monsquaz Swap — Product Requirements

Status: **v0.1 draft, pending adversarial review**
Owner: Patrik Weibull
Last updated: 2026-11-28
Language: requirements, constraints, risks, and open questions follow
ASD-STE100 Simplified Technical English (`ai-rules/STE.md`). Narrative sections
are explanatory. If a narrative sentence conflicts with a requirement, the
requirement wins.

This is the hand-written product contract. Architecture and agent rules live in
`ai-rules/`; every event knob is in `docs/EVENT_SETTINGS.md`; acceptance
scenarios live in `specs/features/`.

## 1. Problem & evidence

The original Monsquaz Swap proved the format: a group of people each seed a
track, pass it anonymously, and reveal a collaborative album. It ran on a
website, and the website became the bottleneck:

- accounts and logins were friction; some people never completed signup;
- uploads/downloads were clunky, versions were easy to lose, and filenames were
  confusing;
- people forgot deadlines; notifications were weak;
- skips, dropouts, and fill-ins were manual and error-prone;
- the host had to run, pay for, and maintain a server;
- assembling the reveal and credits was manual.

Discord already provides identity, roles, channels, DMs, and file transport.
The remaining job is a small local host app that runs the bot, keeps the
schedule and files, and makes the host's decisions one click.

## 2. Users & jobs-to-be-done

| User | Job | Context |
|---|---|---|
| **Host** (primary) | Run a complete anonymous swap without a server, with control over deadlines, skips, and the reveal. | Non-technical; Windows or Linux; will tolerate a 5-minute bot setup, not a terminal. |
| **Participant** | Join by submitting a seed, receive a track they have never touched, edit it, submit before the deadline, and see the album at the end. | Desktop or mobile Discord; may skip, pause, or leave; may not know the schedule. |
| **Fill-in** | Cover a skipped slot without ever receiving the same track twice. | May be a dedicated volunteer, someone who signed up but never seeded, or a seeded participant who takes one extra turn in a round. |
| **Co-host** | Help with nudges, extensions, pauses, and decisions; may see the mapping. Decided, building in MSW-117 (D-059). | Trusted helper; actions audited. |
| **Server member** | Watch the reveal and celebrate. | No assignments; no spectator role needed. |

Not a user: an anonymous website visitor; a public API consumer; a mobile app
user (Discord is the mobile client).

## 3. Goals, success metrics, kill criteria

**Goals**

1. A host can install the app, connect a bot, and run a complete event without a
   server or a terminal.
2. The hard invariants hold: nobody works on a track twice; nobody receives
   their own track.
3. Participants never learn authors/titles before reveal; the host can see the
   mapping.
4. The host can handle skips, late work, leaves, extensions, and fill-ins
   without breaking the schedule or silently advancing.
5. The reveal is a delightful show and a clean album package.
6. The app ships on Windows and Linux with GitHub Releases.

**Success metrics**

| Metric | Target |
|---|---|
| Host setup time (token → bot online) | ≤ 5 minutes |
| Invariant violations in simulation (10k seeds) | 0 |
| Event completion (active assignments resolved) | ≥ 95% |
| Async exposure coverage for active participants | ≥ 95% of the planned pairs, with gaps reported |
| Participants reporting they would join again | ≥ 90% |
| Critical anonymity leak in the red-team review | 0 |
| Host interventions per round | ≤ 2 (excluding deliberate extensions) |
| Public release install success on clean VMs | Windows 10/11 + Ubuntu 22.04/24.04 |

**Kill / pivot criteria**

- Discord blocks the BYO-bot DM flow or makes the required scopes impractical.
- Tauri cannot produce reliable Windows + Linux installers for the target hosts.
- Anonymity cannot be maintained against ordinary participants even with the
  scrub/filename controls (then reframe as "anonymous-ish" or change the format).
- The host burden remains above ~10 minutes per round after automation.

## 4. Non-goals

- A website, hosted service, accounts, or passwords.
- In-app chat, public ratings, leaderboards, or competitive scoring.
- A spectator role; the reveal is whole-server.
- macOS support in v1 (deferred until Apple signing/notarization is funded).
- Auto-update in v1 (release wiring only).
- Windows code signing in v1 (SmartScreen warning documented).
- Perfect anonymity against the host, Discord, or server admins.
- Full album audio rendering in v1 (module preview only; rendering later).
- Built-in live streaming; the host uses OBS/Twitch/YouTube.
- Mobile native app.
- Import from the old website in v1 (a later nice-to-have).

## 5. Scope & walking skeleton

**First slice (walking skeleton):** setup wizard → create event → join/seed →
lock roster → generate a classic schedule → one offer/claim/submit round →
reveal one track with credits. This proves the Tauri ↔ bot ↔ SQLite ↔ file loop.

**v1 scope:** both classic and async modes; classic groups/participant rotation;
async global queue with the diversity objective; claim-first delivery;
skips/fill-ins; deadlines/extensions; host pack and file pipeline; anonymity and
reveal presenter; exports; Windows + Linux installers.

**Deferred:** album audio render, listening party, macOS, auto-update, paid
signing, old-site import, public archive page.

## 6. Requirements

Priorities: must / should / could / wont. Verification is a Gherkin feature or a
named command. IDs are stable.

### Host app & onboarding

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-HOST-001 | must | The app SHALL guide the host through bot token entry, validation, invite, channel mapping, and a DM test in six steps. | `specs/features/ops.feature` |
| REQ-HOST-002 | must | When the token is valid, the app SHALL show the bot identity and online status without ever revealing the token. | `ops.feature` |
| REQ-HOST-003 | must | The app SHALL store the token in the OS keychain; if unavailable, it SHALL refuse silent plaintext storage and offer an explicit fallback. | `security.feature` |
| REQ-HOST-004 | must | The app SHALL run the bot while the window is closed to tray, and SHALL stop it only on explicit Quit. | `ops.feature` |
| REQ-HOST-005 | should | The app SHALL provide a mission-control dashboard with the next action, bot health, at-risk participants, and pending decisions. | UI test |

### Event lifecycle

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-EVENT-001 | must | The host SHALL create an event with mode, length, pack, allowlist, anonymity, deadlines, groups, and reveal settings, using presets and a reset-to-recommended action. | `ops.feature` |
| REQ-EVENT-002 | must | The event SHALL move through Draft → Signup/Seed → Schedule review → Active → Paused → Reveal → Archived. The host pause action that enters Paused: decided, building in MSW-113 (D-055). | `ops.feature` |
| REQ-EVENT-003 | must | When the schedule is committed, the app SHALL lock core rules and allow only operational knobs to change with an audit entry. | `scheduling.feature` |
| REQ-EVENT-004 | should | The host SHALL clone a past event's settings and archive/export/import event bundles. | `ops.feature` |
| REQ-EVENT-005 | must | The app SHALL bind one active event per Discord server and store past events in a library. | `ops.feature` |

### Join, seed, leave

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-JOIN-001 | must | A guild member SHALL join by submitting a valid seed file and accepting the rules; joining without a seed is allowed but does not make them a participant. | `join_and_seed.feature` |
| REQ-JOIN-002 | must | Joining SHALL include a DM handshake; if DMs are blocked, the bot SHALL offer a host-approved private-thread fallback. | `join_and_seed.feature` |
| REQ-JOIN-003 | must | The signup SHALL collect timezone, max handoffs, optional away dates, and notification preferences. | `join_and_seed.feature` |
| REQ-JOIN-004 | must | When the host locks the roster, signed-up people without a valid seed SHALL become fill-ins by default (configurable). | `join_and_seed.feature` |
| REQ-JOIN-005 | must | A participant SHALL leave at any time after one confirmation. A claimed or overdue track SHALL become skipped_after_claim and count as seen. An unclaimed offer SHALL become skipped_before_claim and SHALL NOT count as seen. The track SHALL return unchanged. The seed SHALL stay in the event. Past credits SHALL keep the name. Future planned turns SHALL become gaps and SHALL NOT be removed. | `join_and_seed.feature` |
| REQ-JOIN-006 | must | Only the host SHALL start a return. The person SHALL confirm it. A confirmed return SHALL make the person active with their previous readiness and role, and SHALL give planned turns from the next round that opens. A skipped turn SHALL stay skipped. The invitation SHALL expire after 7 days. In schedule review, a confirmed return SHALL void the draft plan and the host SHALL generate the schedule again. | `join_and_seed.feature` |
| REQ-JOIN-007 | must | A seed after the seed deadline SHALL be held for host approval while Round 1 is open. A seed after the host closes Round 1 SHALL be refused until the host reopens Round 1. | `join_and_seed.feature` |

### Scheduling (classic)

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-SCHED-001 | must | The scheduler SHALL never assign a participant their own track and never assign the same track to the same participant twice. | `scheduling.feature` |
| REQ-SCHED-002 | must | A full classic event SHALL run N total rounds for N seeded participants: R1 is the seed round and R2..RN are exchange rounds. The planner SHALL warn about overcrowding above 12 participants and allow a cap. | `scheduling.feature` |
| REQ-SCHED-010 | must | The host SHALL open Round 1 with the pack and a seed deadline. The host SHALL close seed intake softly and lock the roster in one action. At least two seeds are required. | `join_and_seed.feature` |
| REQ-SCHED-011 | must | One classic round SHALL have one shared deadline. A planned offer SHALL expire at that deadline, and a claim SHALL use the same deadline. The default is 24 hours for a normal exchange round and 48 hours for the final exchange round. A fill-in offer SHALL use its own claim and edit deadlines (REQ-FILL-003). | `deadlines_and_extensions.feature` |
| REQ-SCHED-012 | must | The host SHALL choose strict full exposure or flexible participation. Strict SHALL refuse a plan when an active handoff cap is below the exchange count. Flexible SHALL give the earliest exchange rounds up to the cap and report coverage gaps. | `participation.feature` |
| REQ-SCHED-013 | must | The host CAN record an away range for a participant. The planner SHALL skip overlapping projected rounds and report the gaps. History SHALL stay in UTC. | `participation.feature` |
| REQ-SCHED-003 | must | Each round SHALL assign at most one track per participant and at most one participant per track. | `scheduling.feature` |
| REQ-SCHED-004 | must | The host SHALL preview, regenerate with a diff, manually edit, validate, lock, and commit the schedule; hard invariants SHALL block invalid edits. | `scheduling.feature` |
| REQ-SCHED-005 | must | When a schedule is locked, the bot SHALL publish a commit-reveal hash; each replan SHALL create a new commitment. The reveal SHALL publish `verify.txt`, and `/swap verify` SHALL recompute the commitment (D-043); decided, building in MSW-088. | `scheduling.feature` |
| REQ-SCHED-006 | should | The planner SHALL show feasibility, overcrowding, group balance, swap-cadence, and fun-estimate warnings, and explain infeasibility with the conflicted people/tracks. | `groups.feature` |

### Scheduling (async)

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-ASYNC-001 | must | Async mode SHALL use one global queue with a social-diversity objective. It SHALL NOT use participant groups, group migration, barriers, or reserved queues. | `async_queue.feature` |
| REQ-ASYNC-002 | must | Offers SHALL be claim-first. The claim window SHALL be 24 hours by default. The claim reminder SHALL be a setting (6h, 12h, none, custom) with a 12-hour default (D-041); decided, building in MSW-086. The edit deadline SHALL start at claim. One daily release window SHALL run two dispatch waves at a wall-clock time in the host IANA zone (D-042); the zone handling is decided, building in MSW-087. | `async_queue.feature` |
| REQ-ASYNC-003 | must | The dispatcher SHALL support a deficit-ordered hybrid (default), strict precomputed queues, and an opportunistic pool. Each strategy SHALL preserve the seen ledger. | `async_queue.feature` |
| REQ-ASYNC-004 | must | Completeness SHALL mean exposure: every participant claims every track that the event plans to expose to them. Full N-1 exposure is the special case. Capped events report coverage and gaps. The app SHALL NOT promise edit completeness. The default length SHALL be N-1 for 12 or fewer participants and 6–8 rounds above 12. The host can choose full exposure after the planner shows the projected duration. | `async_queue.feature` |
| REQ-ASYNC-005 | should | The host SHALL be able to change the release time. Unclaimed offers SHALL shift. Claimed assignments SHALL keep their stated due date. | `async_queue.feature` |
| REQ-ASYNC-006 | must | The app SHALL protect against repeated unclaimed offers. It SHALL offer only to a Ready participant, SHALL apply the re-offer setting after an expiry (default: never re-offer the same track to the same person), SHALL apply the host-selected expiry preset (D-041), and SHALL report exposure gaps without looping. | `async_queue.feature` |
| REQ-ASYNC-007 | must | The app SHALL show each participant a personal season view: tracks seen, tracks edited, days remaining, and the current offer. It SHALL NOT show group labels or a global queue. | `async_queue.feature` |
| REQ-ASYNC-008 | must | At the deadline the bot SHALL notify the host and show a decision card with a 6-hour response target. It SHALL NOT auto-advance. | `async_queue.feature` |
| REQ-ASYNC-009 | could | Async mode MAY add optional genre or mood content tags to the diversity score after v1. It SHALL NOT add participant groups. | deferred |

### Groups

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-GROUP-001 | should | The planner SHALL suggest group count/sizes from the simulation lookup, with soft 5–12 bounds and a fun estimate. | `groups.feature` |
| REQ-GROUP-002 | should | Group swaps SHALL rotate participants by default; tracks/both/merge are configurable; group membership is hidden from participants. | `groups.feature` |
| REQ-GROUP-003 | should | The default swap cadence SHALL be K≈T/4 (3–5 phases), with an early swap when a group cannot fill and a warning when "exhausted" will never trigger. | `groups.feature` |
| REQ-GROUP-004 | should | An infeasible phase SHALL fall back to a global pool for that phase, preserve the seen ledger, and notify the host; the next phase restores groups. | `groups.feature` |

### Delivery, claim, submission

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-DELIVERY-001 | must | The bot SHALL offer a track with its opaque code and deadline; the file SHALL be sent only after the participant claims. | `delivery.feature` |
| REQ-DELIVERY-002 | must | An unclaimed offer SHALL expire after the claim window (default 24h) without marking the pair seen. The default preset starts a 24h cooldown at the first expiry and pauses the participant at the second expiry. The host alert is part of the default preset. The other presets are decided, building in MSW-084. | `delivery.feature` |
| REQ-DELIVERY-003 | must | A claimed submission SHALL be downloaded immediately, hashed, validated, and stored; a receipt SHALL confirm what happens next. | `files.feature` |
| REQ-DELIVERY-004 | must | A participant SHALL resubmit before the deadline; the latest valid version wins and all versions are retained. | `delivery.feature` |
| REQ-DELIVERY-005 | must | If a DM is blocked, the bot SHALL use a host-approved private thread and alert the host; it SHALL never silently drop a delivery. | `delivery.feature` |
| REQ-DELIVERY-006 | must | If a claimed track cannot be submitted, the participant SHALL be able to skip after claim; the pair is seen forever and the track returns unchanged. | `delivery.feature` |

### Deadlines, reminders, extensions

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-DEADLINE-001 | must | Deadlines SHALL be soft by default: reminders and a host alert, never a silent automatic transfer. | `deadlines_and_extensions.feature` |
| REQ-DEADLINE-002 | must | Reminders SHALL fire at 50%, 80%, and 95% of the window plus at the deadline, respecting local quiet hours. | `deadlines_and_extensions.feature` |
| REQ-DEADLINE-003 | must | A participant SHALL request +1h/+6h/+12h; the first request auto-approves up to 6h; max 2 per assignment; the host can override. | `deadlines_and_extensions.feature` |
| REQ-DEADLINE-004 | must | The host SHALL be able to extend one person, extend the round, pause the event, skip, fill, or advance, each with an audit entry. Event pause: decided, building in MSW-113 (D-055). | `deadlines_and_extensions.feature` |
| REQ-DEADLINE-005 | should | A bot outage SHALL be visible, and deadlines SHALL be extendable by the outage duration; DM messages sent while offline SHALL be reconciled on reconnect. | `ops.feature` |

### Skips & fill-ins

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-FILL-001 | must | A skip SHALL return the track unchanged and never count as an edit; a claim-then-skip SHALL count as seen. | `fill_ins.feature` |
| REQ-FILL-002 | must | A fill-in SHALL be a dedicated volunteer, a converted non-seeder, or an eligible seeded participant. A seeded participant SHALL take at most one extra turn in a round. A seeded participant SHALL be eligible only after they submit their own turn and only when the plan never gives them the stranded track. A fill-in SHALL never receive a track twice. | `fill_ins.feature` |
| REQ-FILL-003 | must | Fill-in offers SHALL be sequential first-accept. The flow SHALL be host appointment, automatic chain, or an ordered host list. The auto-chain SHALL offer tier 2 while the 3-offer budget remains. A host decision SHALL open after the 3-offer budget or both tiers are exhausted, whichever comes first. Each chain or list offer SHALL have a claim window (default 6h) and an edit due time that starts at the claim (default 24h). A host appointment SHALL keep the due time that the host sets. The host list SHALL have no size limit. | `fill_ins.feature` |
| REQ-FILL-004 | must | A dedicated fill-in edit SHALL count toward the handoff budget. A seeded participant's extra turn SHALL NOT count toward the handoff budget. | `fill_ins.feature` |
| REQ-FILL-005 | must | If no eligible fill-in exists, classic SHALL pass the track unchanged; async SHALL return it to the pool. | `fill_ins.feature` |
| REQ-FILL-006 | should | Credits SHALL mark a rescued turn with a fill-in badge, whoever did it (D-065). A fill-in SHALL be able to name the track if they are the final editor.  | `fill_ins.feature` |

### Files, pack, scrub

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-FILES-001 | must | The pipeline SHALL enforce a per-event allowlist (default modules+audio+zip) and block executables/scripts. | `files.feature` |
| REQ-FILES-002 | must | The host pack SHALL be a module template + samples; the app SHALL produce a hash-based sample report and host review, never a false-positive auto-reject. | `files.feature` |
| REQ-FILES-003 | must | Instruments MAY be added; new samples are disallowed by default; slight derivatives of pack samples are allowed and flagged for review. | `files.feature` |
| REQ-FILES-004 | must | Format changes SHALL be forbidden by default. | `files.feature` |
| REQ-FILES-005 | must | Files over 20 MiB SHALL be rejected with alternatives; the bot SHALL download external links when possible, otherwise the host mediates. | `files.feature` |
| REQ-FILES-006 | must | Corrupt files SHALL be rejected with a clear reason and resubmission allowed; the app SHALL never crash on a bad file. | `files.feature` |
| REQ-FILES-007 | must | Module metadata SHALL be scrubbed where safe and A/B-render validated; untouched originals SHALL remain in a host-only vault. | `files.feature` |
| REQ-FILES-008 | should | A short module preview SHALL be available in the app; album audio rendering is deferred. | UI test |

### Anonymity

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-ANON-001 | must | Participant-facing surfaces SHALL NOT expose seed owners, contributors, original titles, comments, or the full schedule before reveal. | `anonymity.feature` |
| REQ-ANON-002 | must | Track codes SHALL be opaque and not derived from owner or submission order; outbound filenames SHALL omit holder and version. | `anonymity.feature` |
| REQ-ANON-003 | must | Public progress SHALL be status + countdown only before the deadline; no per-person counts. | `anonymity.feature` |
| REQ-ANON-004 | must | By default, participants SHALL see only their own assignments. The host CAN choose an open schedule that shows which participant edits which track code in each round. An open schedule SHALL NOT show owners, titles, or comments before the reveal (D-045); decided, building in MSW-120. The open mode currently exposes owners, titles, and comments. The join card SHALL disclose the open schedule. | `anonymity.feature` |
| REQ-ANON-005 | must | Comments/questionnaire SHALL be host+reveal only by default; at reveal they are attributed with host redaction. | `anonymity.feature` |
| REQ-ANON-006 | must | Skips SHALL be host-only during the event and show as anonymous gaps by default at reveal. | `anonymity.feature` |
| REQ-ANON-007 | must | Stream-safe mode SHALL hide the mapping and redact IDs for screen sharing. | `anonymity.feature` |
| REQ-ANON-008 | should | The host-can-see disclosure SHALL appear in the rules; host participation is allowed with disclosure. | `anonymity.feature` |
| REQ-ANON-009 | must | The app SHALL NOT enforce a minimum participant count or show a small-event anonymity warning. The event rules SHALL state the anonymity limits and the host decides. | `anonymity.feature` |

### Reveal & export

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-REVEAL-001 | must | The host SHALL trigger the Discord reveal after the release stream; it SHALL publish atomically to the whole server. | `reveal.feature` |
| REQ-REVEAL-002 | must | The reveal SHALL include final files, final titles, full per-round credits (fill-ins marked), comments, the schedule, and fun stats. | `reveal.feature` |
| REQ-REVEAL-003 | must | The presenter window SHALL show the lineage timeline, R1/final playback, and credits, with stream-safe mode. | `reveal.feature` |
| REQ-REVEAL-004 | must | The app SHALL export a full ZIP (finals, credits, manifest, cover, README) and a Markdown+HTML runbook. | `reveal.feature` |
| REQ-REVEAL-005 | should | Each participant SHALL receive a personal keepsake of every version they touched, with credits unlocked at reveal. | `reveal.feature` |
| REQ-REVEAL-006 | must | The album order SHALL be secret-shuffled by HMAC(track code, event secret), not submission order. | `reveal.feature` |
| REQ-REVEAL-007 | should | Large album and keepsake exports SHALL use the host-chosen delivery method. The default is a host-mediated neutral link. | `reveal.feature` |

### Ops, recovery, privacy, release

| ID | Pri | Requirement | Verification |
|---|---|---|---|
| REQ-OPS-001 | must | State SHALL persist in SQLite (WAL); the app SHALL recover from a crash without duplicate transitions. Automatic backups and a restore wizard: decided, building in MSW-115 (D-057). Delivery SHALL be at least once. A crash after Discord accepts a send can repeat that send until transport reconciliation lands. | `ops.feature` |
| REQ-OPS-002 | must | The app SHALL be single-instance and SHALL keep the bot running in the tray until explicit Quit. | `ops.feature` |
| REQ-OPS-003 | must | Every host override, mapping access, and security-relevant event SHALL be recorded in an append-only audit log. | `ops.feature` |
| REQ-OPS-004 | must | The app SHALL expose a participant view preview and a host dry-run simulator. | `ops.feature` |
| REQ-PRIV-001 | must | The app SHALL collect only necessary data, offer export/delete, keep no telemetry by default, and redact diagnostics. | `privacy.feature` |
| REQ-PRIV-002 | should | Album content SHALL default to CC BY-NC-SA 4.0 with participant authorship; the host is the data controller for their event. | `privacy.feature` |
| REQ-SEC-001 | must | The bot token SHALL never appear in the UI, logs, SQLite, or git; files SHALL never travel through IPC. | `security.feature` |
| REQ-SEC-002 | must | File handling SHALL resist path traversal, executable masquerading, archive bombs, and parser crashes. | `security.feature` |
| REQ-REL-001 | must | GitHub Actions SHALL build Windows NSIS and Linux .deb/AppImage on tags and publish a draft GitHub Release; no macOS in v1. | CI check |
| REQ-REL-002 | must | Auto-update SHALL be off in v1; Windows builds SHALL be unsigned with a SmartScreen note. | release docs |
| REQ-UX-001 | must | The UI and bot copy SHALL be English, i18n-ready, WCAG AA, keyboard-navigable, and free of color-only status. | UI test |
| REQ-UX-002 | must | Quiet hours (default 22:00–08:00 local, configurable) SHALL be respected for non-urgent DMs. | `ops.feature` |

## 7. Constraints

- Discord: 20 MiB default upload; bot DM redistribution cap; 15-minute
  interaction token; 3-second initial response; no privileged intents needed;
  DM requires a mutual guild; CDN URLs expire.
- Tauri 2.12 / Rust 1.90+; WebKitGTK 4.1 on Linux; Windows 10+; macOS deferred.
- SQLite single writer; local files; no inbound server.
- BYO bot token; no shared bot service; no self-bot.
- MIT code; CC BY-NC-SA 4.0 album default.
- Solo developer; GitHub Actions; unsigned Windows.

## 8. Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| DM delivery fails (closed DMs, no mutual guild) | high | high | mandatory handshake, private-thread fallback, host alert, preflight |
| Anonymity leak via filename/metadata/timing | high | high | opaque outbound names, scrub+A/B, batched release, public aggregate rules |
| Host becomes the bottleneck | high | high | mission control, one-click decisions, co-host, pause, dry run |
| Scheduler dead-ends after churn | medium | high | matching + Hall certificate, fill-ins, global fallback, never relax invariants |
| Linux WebKitGTK/packaging issues | medium | medium | test Ubuntu 22.04/24.04, `.deb` primary, simple UI |
| Discord API/limit change | medium | medium | pin Twilight, adapter boundary, capability probe, fix-it messages |
| File parser crash/malice | medium | high | sandboxed parser, allowlist, size cap, no auto-open |
| Data loss/corrupt DB | low | high | WAL, backups, restore drill, integrity check |
| Scope creep (render, streaming, macOS) | high | medium | milestones, non-goals, defer list |
| Unsigned Windows SmartScreen | certain | medium | document, offer checksum, pursue free signing later |

## 9. Milestones

See `docs/ROADMAP.md`. v1 is complete when classic + async + groups + fill-ins +
reveal + exports work end-to-end on Windows and Linux, with the invariant and
anonymity test suites green.

## 10. Open questions & assumptions

| # | Question | Status / default | Trigger |
|---|---|---|---|
| 1 | Host pack delivery: DM vs channel | DM to each participant at join/start; the host can also pin it. | Before REQ-FILES-002 |
| 2 | Default retention period after archive | Keep until host deletes; document; backups follow the count-based retention. | Before public release |
| 3 | Windows SmartScreen copy and checksum UX | Document; no signing in v1. | Before beta |
| 4 | Render pipeline (openmpt123 vs WASM) | openmpt123 sidecar; preview v1, render later. | Before REQ-FILES-008 |
| 5 | Release stream integration depth | Discord scheduled event + countdown; OBS external. | Before REQ-REVEAL-003 |
| 6 | Participant name changes between join and reveal | Join-time display name snapshot; refresh with host review. | Before REQ-REVEAL-002 |
| 7 | Async content tags | Optional later; no participant groups. | After v1 |
| 8 | Expiry strike defaults | Decided in D-041: strike + cooldown preset by default; strikes reset on resume. | Closed |

**Assumptions**

- The host's machine is trusted and has a working internet connection during
  active periods; DM catch-up handles short outages.
- Participants are in the host's Discord guild and can receive DMs (or accept
  the fallback).
- Module files are typically <20 MiB; larger files are rare and host-mediated.
- The community accepts a local desktop app and a BYO bot token.
- The group simulation's fun metric is a planning proxy, not a validated human
  preference; calibrate after the first two events.

## 11. Success definition for v1

- A host on a clean Windows 11 or Ubuntu 22.04 machine can install, connect a
  bot, create an event, run a classic event end-to-end, and publish the reveal
  without editing a file or database.
- The same flow works for an async event with at least one skip and one fill-in.
- `scripts/check.sh`, the Gherkin suite, the property tests, and the release
  build are green.
- One real 8–12 person game day completes with no hard-fail in the red-team
  rubric.
