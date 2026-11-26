# TLA+ models

The models describe the event rules, not the implementation. See
`docs/FORMAL_METHODS.md` for scope, bounds, and maintenance.

## Files

- `SwapClaim.tla`: claim, offer, custody, expiry, skip, and submit.
- `SwapClaimPR.tla`: 2 participants × 2 tracks, 3 ticks.
- `SwapClaimNightly.tla`: 3 participants × 3 tracks, 2 ticks.
- `SwapClaimMutation.cfg`: 3 participants × 3 tracks, a zero claim window, and one clock tick for the mutation set.

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

Every checked property has a mutation that makes TLC report that same
invariant. `OneTrackPerPerson` is kept as an implied reader property and is
covered by `AtMostOneActivePerPerson`. The script rejects parse errors and
unrelated failures. Run `scripts/formal/check.sh`.

## Mapping

| TLA+ | Rust / SQLite |
|---|---|
| `phase[t][p]` | `assignments.status` |
| `seen` | `seen_pairs` primary key |
| `banned` | offer history (planned with the async queue) |
| `holder[t]` | active assignment plus `tracks.state` |
| `due[t][p]` | `assignments.due_at` in UTC seconds |
| `now` | injected clock value |

The `banned` set is a design requirement. The async queue must add the backing
table before it relies on the no-reoffer rule. Until then, `NoExpiredReoffer`
is a model property and not a claim about the current engine.

## Bounds

The PR model is 2×2×3. The nightly model is 3×3×2. Do not raise the bounds in a
PR without measuring the state count and the runtime.
