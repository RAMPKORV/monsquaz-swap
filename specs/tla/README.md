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

## Properties

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

Every checked property has a mutation that makes TLC report that same
invariant. `OneTrackPerPerson` is kept as an implied reader property and is
covered by `AtMostOneActivePerPerson`. The script rejects parse errors and
unrelated failures. Run `scripts/formal/check.sh`.

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

The `banned` set is a design requirement. The async queue must add the backing
table before it relies on the no-reoffer rule. Until then, `NoExpiredReoffer`
is a model property and not a claim about the current engine.

## Bounds

The claim PR model is 2×2×3 with 750 distinct states. The claim nightly model is
3×3×2 with 2,338,158 distinct states. The delivery PR model is 3 effects × 2
attempts with 343 distinct states. The delivery nightly model is 4 effects × 3
attempts with 6,561 distinct states. Do not raise the bounds in a PR without
measuring the state count and the runtime.
