# TLA+ models

The models describe the event rules, not the implementation. See
`docs/FORMAL_METHODS.md` for scope, bounds, and maintenance.

## Files

- `SwapClaim.tla`: claim, offer, custody, expiry, skip, and submit.
- `SwapClaimPR.tla`: 2 participants × 2 tracks, 3 ticks.
- `SwapClaimNightly.tla`: 3 participants × 3 tracks, 2 ticks.
- `SwapClaimMutation.cfg`: 3 participants × 3 tracks, a zero claim window, and one clock tick for the mutation set.
- `SwapDelivery.tla`: durable outbox, send attempts, acknowledgements, the
  retry cap, host alerts, and dedupe.
- `SwapDeliveryPR.tla`: 3 effects, 2 attempts.
- `SwapDeliveryNightly.tla`: 4 effects, 3 attempts.
- `SwapDeliveryMutation.tla` and `.cfg`: duplicate-key bounds for the mutation set.
- `SwapSchedule.tla`: classic seed round R1, exchange rounds R2..RN, the plan,
  group rotation, and the fallback-phase flag.
- `SwapSchedulePR.tla`: 4 participants × 4 tracks, 3 exchange rounds, one group.
- `SwapScheduleNightly.tla`: 6 participants × 6 tracks, 2 groups of 3, 4
  exchange rounds, one round per phase.
- `SwapScheduleMutation.tla` and `.cfg`: the grouped 6 × 6 bound for the
  schedule mutation set.
- `SwapReveal.tla`: the host reveal gate, the event state, active assignments,
  the participant projection, and the stream-safe presenter.
- `SwapRevealPR.tla`: 2 participants × 2 tracks.
- `SwapRevealNightly.tla`: 3 participants × 3 tracks.
- `SwapRevealMutation.tla` and `.cfg`: the 2 × 2 bound for the reveal mutation
  set.

## Properties

Claim properties:

| Property | Meaning | Requirement |
|---|---|---|
| `NoRepeat` | A seen pair is never offered again. | PRD REQ-SCHED-001 |
| `NoOwnTrack` | A participant never holds their own track. | PRD REQ-SCHED-001 |
| `NoOwnOffer` | A participant is never offered their own track. | PRD REQ-SCHED-001 |
| `AtMostOneOfferPerTrack` | A track has at most one open offer. | PRD REQ-SCHED-003 |
| `AtMostOneOfferPerPerson` | A participant has at most one open offer. | PRD REQ-SCHED-003 |
| `AtMostOneActivePerPerson` | A participant has at most one open offer or held track. | PRD REQ-SCHED-003 |
| `NoExpiredReoffer` | An expired pair is never offered again. | PRD REQ-DELIVERY-002 |
| `TypeOK` | Variables have the expected shape. | model hygiene |

Delivery properties:

| Property | Meaning | Requirement |
|---|---|---|
| `SentWereEnqueued` | A sent effect was enqueued first. | PRD REQ-DELIVERY-002 |
| `AckedWereSent` | An acknowledgement refers to a sent effect. | PRD REQ-DELIVERY-002 |
| `AckedWereEnqueued` | An acknowledgement refers to an enqueued effect. | PRD REQ-DELIVERY-002 |
| `NoLostEffect` | Every enqueued effect is pending, acknowledged, or failed. | PRD REQ-DELIVERY-002 |
| `FailedWereEnqueued` | A failed effect was enqueued first. | PRD REQ-DELIVERY-002 |
| `FailedWereAlerted` | Every failed effect queued a host alert. | PRD REQ-DELIVERY-002 |
| `TerminalExclusive` | An effect is not both acknowledged and failed. | PRD REQ-DELIVERY-002 |
| `UniqueKeys` | Dedupe keys are unique. | PRD REQ-DELIVERY-002 |
| `AttemptsBounded` | The attempt counter stays within the bound. | model hygiene |
| `TypeOK` | Variables have the expected shape. | model hygiene |

Schedule properties:

| Property | Meaning | Requirement |
|---|---|---|
| `CompleteRounds` | R1 covers every track with its owner. Every planned exchange round gives one track to each participant and one participant to each track. | PRD REQ-SCHED-002, REQ-SCHED-003 |
| `NoRepeat` | No participant-track pair appears in two exchange rounds. | PRD REQ-SCHED-001 |
| `NoOwnTrack` | No exchange round gives a participant the track they seeded. | PRD REQ-SCHED-001 |
| `GroupContainment` | A participant receives a track from the group one phase step forward. A fallback phase can use the global pool. | PRD REQ-GROUP-002, REQ-GROUP-004 |
| `SealedPlanComplete` | A locked plan has every exchange round. | PRD REQ-SCHED-004 |
| `TypeOK` | Variables have the expected shape. | model hygiene |

Reveal properties:

| Property | Meaning | Requirement |
|---|---|---|
| `NoRevealBeforeHostAction` | The reveal is not published until the host presses reveal. | PRD REQ-REVEAL-001 |
| `NoRevealWithActiveAssignments` | The reveal is not published while an assignment is active. | PRD REQ-REVEAL-001 |
| `NoIdentityBeforeReveal` | The participant projection has no owner, contributor, title, comments, or mapping fact before the gate. | PRD REQ-ANON-001 |
| `StreamSafeDefault` | Stream-safe mode is on before the reveal, hides identity facts, and turns off only after host confirmation. | PRD REQ-ANON-007, REQ-REVEAL-003 |
| `TypeOK` | Variables have the expected shape. | model hygiene |

Every checked property has a mutation that makes TLC report that same
invariant. `OneTrackPerPerson` is kept as an implied reader property and is
covered by `AtMostOneActivePerPerson`. The script rejects parse errors and
unrelated failures. `scripts/formal/check.sh` runs every model and every
mutation set.

The schedule mutation set breaks `TypeOK`, `CompleteRounds`, `NoRepeat`,
`NoOwnTrack`, `GroupContainment`, and `SealedPlanComplete`. The reveal mutation
set breaks `TypeOK`, `NoRevealBeforeHostAction`,
`NoRevealWithActiveAssignments`, `NoIdentityBeforeReveal` (one mutation for
each identity kind), and `StreamSafeDefault` (an unsafe presenter default and a
disable without confirmation).

## Mapping

`mapping.json` lists the Rust files and functions that each model covers. The
CI formal gate reads it through `scripts/formal/changed.mjs`. Keep the table
below and `mapping.json` in sync.

| TLA+ | Rust / SQLite |
|---|---|
| `phase[t][p]` | `assignments.status` |
| `seen` | `seen_pairs` primary key |
| `banned` | offer history (planned with the async queue) |
| `holder[t]` | active assignment plus `tracks.state` |
| `due[t][p]` | `assignments.due_at` in UTC seconds |
| `now` | injected clock value |
| `enqueued` | `deliveries` rows |
| `pending` | `deliveries.status = 'pending'` |
| `sent` | a send attempt was made |
| `acked` | `deliveries.status = 'sent'` |
| `failed` | `deliveries.status = 'failed'` |
| `alerted` | a committed `audit_log` row `delivery.failed` |
| `Key[e]` | `deliveries.id` (the dedupe key) |
| `plan[r][t]` | a plan assignment row for round `r`, track `t` |
| `Owner[t]` | `tracks.owner_id` |
| `GroupOf[p]` | the plan group shape |
| `PhaseOf(r)`, `Cadence` | the plan phase windows |
| `FallbackPhases` | phases with `GroupPhase.global_pool = true` |
| `eventState` | `events.state` (`Active`, `Paused`, `Reveal`) |
| `hostAction` | the host reveal command and its audit row |
| `active` | active `assignments` rows |
| `projection` | the participant-facing reveal payload |
| `presenter` | the presenter window projection |
| `streamSafe`, `confirmed` | the presenter stream-safe switch and its confirmation dialog |

The `banned` set is a design requirement. The async queue must add the backing
table before it relies on the no-reoffer rule. Until then, `NoExpiredReoffer`
is a model property and not a claim about the current engine.

The schedule model assumes strict participation. Flexible handoff caps, away
ranges, fill-ins, bench states, and manual edits are outside the checked bound.
The model supports `FallbackPhases`, but both checked bounds use an empty set.
The group bounds keep the group sizes equal because the classic group planner
requires one track per participant per group.

The reveal model abstracts the participant projection to identity facts. It
does not model the opaque assignment text, the Discord message split, redaction,
or the ZIP export. Atomic publish is structural: one action sets the state and
the projections.

## Bounds

The claim PR model is 2×2×3 with 750 distinct states. The claim nightly model is
3×3×2 with 2,338,158 distinct states. The delivery PR model is 3 effects × 2
attempts with 343 distinct states. The delivery nightly model is 4 effects × 3
attempts with 6,561 distinct states.

The schedule PR model is 4 participants × 4 tracks × 3 exchange rounds with 82
distinct states. The schedule nightly model is 6 participants × 6 tracks, 2
groups of 3, and 4 exchange rounds, with 1,445 distinct states and a 2-second
runtime on 24 workers.

The reveal PR model is 2 participants × 2 tracks with 67 distinct states. The
reveal nightly model is 3 participants × 3 tracks with 2,051 distinct states
and a sub-second runtime on 24 workers.

Do not raise the bounds in a PR without measuring the state count and the
runtime.

## Commands

```bash
scripts/formal/check.sh                 # every PR model and mutation set
scripts/formal/check.sh nightly         # every nightly model
node scripts/formal/changed.mjs origin/main   # does this branch touch a model?
```

`scripts/formal/check.sh` downloads the mirrored TLA+ 1.8.0 asset and verifies
its SHA-256 pin. The pin and update procedure are in `docs/FORMAL_METHODS.md`.
