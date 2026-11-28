# Domain model and invariants

This is the reference for names, states, and rules. If code and this file
disagree, stop and decide which is wrong; record the decision.

## Glossary

| Term | Meaning |
|---|---|
| **Event / swap** | One album-making run with a roster, a pack (optional), a schedule, files, and a reveal. |
| **Host** | The person running the desktop app and bot. May also participate, with disclosure. |
| **Co-host** | Optional helper with full visibility, audited. Decided, building in MSW-117 (D-059). |
| **Participant** | A guild member who joins and seeds a track. |
| **Fill-in** | A volunteer (or converted non-seeder) who covers a skipped slot; never receives a track twice. |
| **Seed** | The participant's initial track submission (Round 1 / version 1). |
| **Track / lineage** | The evolving file, identified by an opaque code; versions are append-only. |
| **Owner** | The participant who seeded the track. Never assigned their own track. |
| **Round** | In classic mode, one synchronized handoff cycle. `R1` is the seed round; `R2..RN` are exchange rounds. A file version is `v1..vN`, where `v1` is the seed. `R#` never means a file version. |
| **Phase** | A run of rounds with fixed group membership/pools. |
| **Handoff / assignment** | One participant's turn with one track. |
| **Offer** | A claim-first notification with a track code and deadline; no file yet. |
| **Claim** | The participant actively accepts the offer; the file is sent and the pair becomes "seen". |
| **Skip** | A deliberate pass. Pre-claim skip does not consume "seen"; claim-then-skip does. |
| **Seen ledger** | Append-only set of `(track, participant)` pairs that may never repeat. |
| **Pack** | Host-provided module template + samples. Instruments may be added; new samples are not allowed; slight derivatives of pack samples are allowed. |
| **Reveal** | The host-triggered publication of titles, credits, schedule, comments, and the album. |
| **Backup record** | A local copy of the event data, the stored track files, and the audit log. A backup record never holds the bot token. |
| **Backup folder** | The host-chosen folder that holds backup records. The folder must be outside the app data folder. |
| **Restore** | The host action that replaces the current event data with a backup record. |
| **Bot lock** | The single-computer claim that stops two computers from running the bot for the same data folder. The lock clears 5 minutes after the last check. |

## Hard invariants

1. **No repeat, ever.** For every `(track, participant)` pair, at most one
   claimed/submitted/skipped-after-claim event exists in the event's lifetime.
   Group swaps, fill-ins, leaves, rejoins, and replans never reset it.
2. **No own track.** A participant is never assigned the track they seeded.
3. **One active custody per track and one active track per person.** A person
   may hold at most one claimed track at a time; a track has at most one holder.
4. **No silent auto-advance.** A missed deadline creates a host decision and
   reminders; it does not move a track by itself unless the host explicitly
   configured a standing order (and that order was disclosed).
5. **Anonymity until reveal.** Participant-facing DTOs contain no owner,
   contributor, original title, comments, or full schedule before reveal.
6. **Seen means seen.** Claim/receipt, submission, and claim-then-skip count.
   Pre-claim skip/expiry does not. Seed ownership counts for the own-track rule.
7. **Files are local.** Discord attachments are downloaded immediately; CDN
   URLs are never forwarded or treated as storage.
8. **UTC in the DB, local in the UI.** Every timestamp is an instant; display
   uses the participant's IANA timezone.

## Event lifecycle

```
Draft → Signup/Seed → Schedule review → Active → Paused → Reveal → Archived
```

- **Draft:** host configures the event; nothing is announced.
- **Signup/Seed:** participants join and submit seeds; pre-signups exist but
  only a valid seed makes a participant.
- **Schedule review:** host previews/edits the generated schedule, validates,
  and locks/commits it.
- **Active:** offers, claims, edits, submissions, skips, fill-ins, reminders,
  extensions, group phases.
- **Paused:** global freeze; clocks shift by the pause duration; no replan.
  The host pause action is decided, building in MSW-113 (D-055).
- **Reveal:** host-triggered after the release stream; schedule, titles,
  credits, comments, and album are published.
- **Archived:** read-only; exportable; can be cloned.

Roster lock is a host action with guidance (default suggestion: seed deadline +
24h). After lock, new people can only be fill-ins; no new tracks unless the host
explicitly reopens with a replan.

## Participant model

Three independent axes, not one enum:

```
membership: signed_up | seeded | active | left | removed
role:       participant | fill_in
readiness:  ready | paused   (paused_until optional)
```

- A signed-up person who never seeds becomes a **fill-in** at roster lock
  (configurable) or is removed/waitlisted.
- A seeded participant becomes **active** at event start.
- `paused` means do not offer them new tracks; a pause requested while holding
  a track takes effect after they submit/return it.
- **Leave (D-051).** A participant can leave with `/swap leave` or with the
  Leave button on a DM that carries a file or a prompt. The host can mark a
  participant as left from the Participants view.
- Every leave path shows a warning first. The participant must press Confirm
  on an ephemeral reply within 15 minutes. The host must confirm a two-step
  dialog. Without a confirm, nothing changes.
- A leave is immediate. A claimed or overdue track becomes
  `skipped_after_claim` (seen). An unclaimed offer becomes
  `skipped_before_claim` (not seen). The track returns unchanged, the fill-in
  flow starts, and the host gets an alert.
- Future planned turns of a leaver are gaps. The app does not replan.
- The seed of a leaver stays in the event and keeps circulating. This is also
  true for a leave before the roster lock.
- A leaver keeps past credits. The bot cancels every unsent DM to the leaver
  and sends the leaver no new DM. A command from a leaver gets an ephemeral
  answer.
- **Rejoin (D-051).** Only the host can start a return. The bot then sends one
  DM with Confirm and Decline. Only a confirm makes the person `active`
  again, with planned turns from the next round that opens. A decline or no
  answer keeps the person `left`. The invitation expires after 7 days. A
  skipped turn stays skipped. A confirm in schedule review voids the draft
  plan, because the roster changed; the host must generate the schedule
  again.
- Every leave, cancellation, and restore writes an audit entry.

## Assignment lifecycle

```
OFFERED ──claim──► CLAIMED/WORKING ──submit──► SUBMITTED
   │                     │
   │ expire              ├──skip──► SKIPPED (seen)
   ▼                     ├──deadline──► OVERDUE (host decides)
EXPIRED (not seen)       └──host revoke──► REVOKED / SUPERSEDED
```

- Async claim window default **24h** (host-set). On expiry, the track goes
  back to the async queue. The expiry preset decides the next step for the
  participant (see "Async" below and D-041).
- Classic has one shared deadline for each round. The offer expires at that
  deadline and a claim uses the same deadline. The seed deadline is separate.
- A claimed track may be skipped: the track returns unchanged, the pair is
  recorded as seen, and the participant can leave.
- An unclaimed offer is not "seen"; the track can be offered to someone else.
  The re-offer setting decides whether the same person can get the same track
  again: never (default), after cooldown, last resort, or host (D-041).
- Classic "mark missed": the host marks an overdue turn missed. The pair is
  recorded as seen. The engine sets the track state to Pool, and the DM says
  the track returns to the pool. The track still passes unchanged into the next
  round, or a fill-in covers the turn. MSW-121 fixes the state name and the
  copy.
- Once a fill-in claims a slot, the original holder's turn is forfeited by
  default; the host can override with an audit entry.

## Scheduling

### Classic

- Default: `N` total rounds for `N` seeded participants. R1 is the seed round;
  R2..RN are exchange rounds. Every participant can touch every other track.
  The planner warns above 12 participants that each track will have many
  contributors and the event will be long; the host can cap.
- Construction: a randomized partial Latin rectangle over tracks × participants,
  built row by row with a perfect matching. Invariants: no own track, no repeat,
  one track per person per round, one person per track per round.
- Groups: default lookup from the group simulation; sizes 5–12 (soft); rotate
  participants; swap every `K ≈ T/4` rounds (2–5 phases); early swap if a group
  cannot fill; allow "exhausted" only with a warning when `group_size > T+1`; a
  global-pool fallback for an infeasible phase, logged.
- Calendar phases are allowed; default behavior (strict vs auto-early) is a
  host choice. Merge/re-split is a repair tool, not a routine phase.
- Groups are hidden from participants; the host sees stable names (A/B/C).
- Unequal groups are allowed with warnings (`max/min > 1.5`, `min < 5`) and a
  rebalance suggestion.
- Strict participation requires every active `max_handoffs` to cover every
  exchange round. Flexible participation gives the earliest rounds up to the
  cap and reports coverage gaps. The seed is not an edit.
- An away range removes the participant from overlapping projected rounds. The
  gap stays visible. Strict participation refuses the plan.
- The default participant view is the caller's own assignments. Open schedule
  shows who edits which track code in each round. It must never show owners,
  titles, or comments before the reveal (D-045). Decided, building in MSW-120:
  the open mode currently exposes owners, titles, and comments. The join card
  discloses it.

### Async

- No formal groups in async. Use one global queue and a social-diversity
  objective. Prefer unfamiliar owners/editors and low-reach people. Avoid
  repeat pairs. Do not add group membership, migration, barriers, or queues.
  Optional genre/mood content tags can come later.
- Per-person queue; one claimed track per person at a time.
- Claim-first everywhere: offer → claim → deliver. The 24h claim window starts
  at the offer. The claim reminder is a setting: 6h, 12h (default), none, or
  custom. Validation refuses a reminder at or after the claim window. Decided,
  building in MSW-086; the code sends a fixed 12h reminder.
- Ready gate: offer only to a participant who is Ready. A new participant is
  Ready. A paused participant must tap Ready (`/swap resume`) to resume.
- Expiry protection is a preset (D-041). An expired offer never counts as
  seen, and the app reports the missed exposure as a gap. The presets are
  decided, building in MSW-084. The code applies only the default preset:
  - **strike + cooldown** (default): one active offer; the first expiry starts
    a 24-hour cooldown; the second expiry pauses the participant until
    `/swap resume` or a host action;
  - **cooldown only**: every expiry starts a 24-hour cooldown; the app never
    pauses the participant;
  - **host review**: an expiry has no automatic effect except a host decision
    card; the host chooses pause, cooldown, or ignore;
  - **none**: the participant stays Ready and can get a new offer in the next
    wave; the app never offers the same track again unless the re-offer
    setting allows it.
- Not built yet (MSW-084): strikes reset when the participant resumes, and an
  auto-pause alerts the host.
- Same-track re-offer after expiry is a setting: never (default), after
  cooldown, last resort, or host. Decided, building in MSW-085; the code
  applies "never".
- Dispatch default: deficit-ordered hybrid (most-constrained ready track, then
  the eligible person with the largest remaining quota, with anti-clustering
  and matching repair). Strict queues and an opportunistic pool are
  configurable.
- Completeness means **exposure**: every participant receives every track that
  the event plans to expose to them. Full N-1 exposure is the special case.
  Capped events report coverage and gaps. Edit completeness is not promised
  because a claim-then-skip or a claimed deadline miss counts as seen. Report
  exposure gaps; do not loop forever.
- Length: the host sets expected songs and claims per participant. One track per
  seeder, so the seeded track count equals the participant count. The app
  computes editors per track and warns when the value is fractional. Full
  exposure needs claims equal to tracks minus one.
- Release: one daily window at 18:00 wall-clock time in the host IANA zone by
  default (D-042). The release keeps its local time across DST. Decided,
  building in MSW-087; the code reads the release time as UTC. Run two
  dispatch waves inside the window (the start and two hours later). The host
  can change the time; unclaimed offers shift, claimed assignments keep their
  due date.
- Deadline: 24h from claim by default. A claimed-but-missed track counts as
  seen. At the deadline, notify the host and show a decision card; target a 6h
  response. Do not auto-advance.
- Late seeds are a host choice. If allowed in a capped event, give the late
  joiner an extra quota and the late track a minimum editor quota. Otherwise
  late tracks get few editors.
- Participant progress: a personal season view (tracks seen, tracks edited,
  days remaining, current offer). Do not show group labels or a global queue.
- The host-only fun score remains a planning aid.

### Classic groups and fun

- Groups apply to classic mode only. Async uses the global diversity objective.
- Fun score is a **host-only planning aid**, not a participant score or a
  judgment of music. It combines social reach, contributor variety,
  cross-pollination, momentum, fairness, and freshness; guardrails are shown.
- Simulation lookup (for an 8-round event): 1 group ≤9; 2 for 10–13; 3 for
  14–18 and 23–32; 4 for 19–22 and 33–40; K≈T/4; rotate participants.
- Splitting's measurable advantage is cross-pollination; the host chooses
  whether that matters. Completeness ("a crack at every song") is a valid
  preference and is the default shape.

## Fill-ins

- Tier 1 (classic): dedicated fill-ins. A dedicated fill-in is a volunteer or
  a signup without a seed at roster lock. The host can appoint any eligible
  person.
- Tier 2 (classic, D-050): a seeded participant who submitted their own turn
  in the current round, and whom the current plan never gives the track. The
  app offers tier 2 only when no tier-1 person is eligible. A tier-2 person
  can take at most one extra turn in one round (D-063). A pre-claim skip does
  not count as an extra turn.
- Eligibility for both tiers: active, ready, no active assignment, not the
  owner, never saw the track. The handoff cap applies to tier 1 only. A
  tier-2 turn is an extra turn and does not count toward the cap.
- Flow (`fill_in_flow`, D-040): `host_appoints` (default), `auto_chain`, or
  `host_list`. The automatic chain makes at most 3 offers. It walks tier 1
  before tier 2 inside that budget (D-064). The host list has no size limit
  and keeps the host order inside each tier (D-070). Tier 1 still comes first;
  the host order does not override the tier rule. After the last offer, the
  host decides. The bot checks eligibility again before each offer.
- Claim and edit time (D-069): each chain or host-list offer gets
  `fill_in_claim_window` (6h). A claim starts the edit window
  `fill_in_window` (24h). A host appointment has the due time that the host
  sets, and the claim keeps it. The default due time is the later of the
  round deadline and now plus `fill_in_window`. A host due time is at most 14
  days away.
- A fill-in is refused when the cell is not stranded: a submitted turn or an
  active holder blocks it.
- Credit: the badge marks a rescued turn, whoever did it. Planned turns of a
  person with the fill-in role are plain credits (D-065).
- No eligible fill-in: classic passes the track unchanged; async returns it to
  the pool. Never relax the no-repeat invariant to rescue a slot.

## Deadlines, reminders, extensions

- Classic default: 24h per round. Async default: 24h from claim.
- Soft deadline: reminders, a final prompt, and a host alert; no automatic move
  while assignments are pending.
- Reminders: percentage-based at 50%, 80%, 95% of the window + at the deadline;
  quiet hours respected (configurable).
- At-deadline prompt: "still working / need extension / skip".
- Extensions: presets +1h/+6h/+12h; first request auto-approves up to 6h; max 2
  per assignment; host can override.
- No-response fallback: grace, then host decision; automatic skipping only if
  explicitly configured and disclosed.
- Host can pause the event; all clocks shift by the pause duration. Decided,
  building in MSW-113 (D-055).

## Files and naming

- Track IDs are opaque codes (for example `S7F3A`), not owner-derived or submission
  order. `v#` is the track version (v1 seed, v2 first edit, ...). `R#` is only a
  round number.
- Sent to a participant: `S7F3A.it` only — no version, holder, or original
  name.
- Canonical store (built): blobs by SHA-256 in the content store; SQLite
  records the version, code, and holder for each blob. The reveal ZIP names
  files `{pos}_{code}.{ext}`.
- Planned, not built (D-016): detailed host names such as `v4.S7F3A.alice.it`
  (version, code, holder), a round view (`R1/`, `R2/`), and a track history
  (`Tracks/<code>/`).
- Default allowlist: tracker modules + audio + zip, editable per event.
- Format changes are forbidden unless the host explicitly reconfigures.
- 20 MiB cap (Discord DM redistribution). The bot downloads external links when
  possible; otherwise host-mediated import. The host shares a neutral link if
  the file remains too large.
- Resubmission before deadline: latest valid version wins; every version is
  retained.
- Metadata: module-aware scrub (names/messages/AUTH/edit history where safe),
  A/B render validation, untouched originals in a host-only vault. Never claim
  perfect scrubbing for arbitrary formats.
- Pack: hash-based sample report + host review; never auto-reject for a
  suspected new sample; instruments may be added; derivatives of pack samples
  are allowed.

## Anonymity and reveal

- Default: authors + titles hidden; track codes opaque; group membership
  hidden; skips host-only; public progress is status + countdown only.
- Visible-schedule option: full past+future assignment matrix, **assignments
  only** (no owners/titles). Exposed via `/swap schedule` and a pinned post.
- Comments/questionnaire: host sees during the event; published at reveal (all
  attributed, host can redact). Not sent to the next holder.
- Last holder names the track at final submission; fallback to the previous
  submitter, then the host.
- Reveal is host-triggered after the release stream, then published atomically
  to the whole server: track files, final titles, full per-round credits,
  fill-ins marked, comments, schedule, fun stats, and a Markdown/HTML runbook.
- Skips show as anonymous "no edit" gaps by default; the host can choose.
- Commit-reveal: bot posts the schedule/policy commitment at lock; `/swap
  verify` recomputes it at reveal. The reveal publishes `verify.txt` so that a
  participant can check the seal with `sha256sum` (D-043). `verify.txt` and
  `/swap verify`: decided, building in MSW-088. Async seals the
  policy + hash-chained transcript, not a future fixed schedule. No file hashes
  are public. Replans have no cap; each replan creates a new commitment.

## Time

- All persisted instants are UTC; display uses IANA zones.
- Discord timestamps (`<t:...>`) render locally; the bot also shows a plain
  local string where useful.
- DST is handled by wall-clock intent at scheduling time, not naive offsets.
