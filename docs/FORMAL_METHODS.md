# Formal methods

Formal verification is a design tool for the event rules. It is not a product
dependency, a release artifact, or an architecture driver.

## Goals

1. Find design bugs in claim, offer, custody, expiry, schedule, and delivery
   before they reach users.
2. Prevent invariant regressions when the modeled code changes.
3. Keep a precise, reviewable description of the rules for humans and agents.

## Tools

- **TLA+ 1.8.0 and TLC** for finite design models. The build is pinned by
  SHA-256 in `scripts/formal/check.sh`. The jar is not committed.
- **proptest-state-machine** for model-based tests against the real Rust types
  and engine.
- **Kani** was spiked for the header parser and custom-ID codec. The current
  harnesses exhaust CBMC on standard allocation, so Kani is not in CI. Parser
  and codec safety use property tests instead.
- **ITF replay** for Tier 3, after a spike. NDJSON trace validation is the
  fallback.

TLAPS, Lean, Verus, Creusot, and Prusti are not part of the general strategy.
They are considered only for a small pure function when the benefit is clear.

## Friction and ownership

- The formal artifacts are dev-only. Product crates do not import them.
- Any model, harness, or skill can be deleted in one PR. Deleting the artifacts
  does not change product behavior or the schema.
- `scripts/formal/check.sh` is authoritative. The project-local skill is
  guidance. A stale skill or a missing agent must not disable the checks.
- Agents may fix mechanical drift. A change to a property, its meaning, or its
  bounds needs human approval through a tracker decision.
- Verification does not mandate a product refactor. If a proof needs an
  architectural change, write an ADR and decide it on product grounds.

## Target list

The models follow their features. Each model lands with the feature it
describes.

| Target | Model | Status |
|---|---|---|
| Claim, offer, custody, expiry | `specs/tla/SwapClaim.tla` | implemented |
| Assignment lifecycle | `crates/swap-core/tests/assignment_machine.rs` | implemented |
| Parser and custom-ID codec | `custom_id.rs` and `scrub.rs` property tests | implemented |
| Anonymity projections | `swap-sim` outbound audit | partial |
| Classic schedule | `SwapSchedule.tla` | after planner repair |
| Async queue | `SwapAsync.tla` | after async queue |
| Delivery idempotency | `SwapDelivery.tla` | after delivery loop |
| Reveal gate | `SwapReveal.tla` | after reveal presenter |

## Invariants

The claim model checks:

- `NoRepeat`: a seen pair is never offered again.
- `NoOwnTrack`: a participant never holds the track they seeded.
- `OneTrackPerPerson`: a participant holds at most one track.
- `NoExpiredReoffer`: an expired pair is never offered again.
- `TypeOK`: every variable has the expected shape.

Claim windows use a bounded logical clock. Real time, DST, reminders, SQLite,
Discord, files, and the UI are outside the model.

## Model-checking bounds

| Run | Participants | Tracks | Max ticks | Time budget |
|---|---|---|---|---|
| PR | 2 | 2 | 3 | 5 minutes |
| Nightly | 3 | 3 | 2 | 20 minutes |

The 2×2 model has 750 distinct states. The 3×3 model is much larger; the
nightly timeout is a hard ceiling. If a model exceeds its budget, reduce the
scope or use Apalache. A skipped model must be reported, never silently
ignored.

## Mutation rule

A property is not covered until a deliberately broken variant makes it fail.
`scripts/formal/check.sh` replaces one guard or update at a time and requires
TLC to report the named invariant. The set covers `TypeOK`, `NoRepeat`,
`NoOwnTrack`, `OneTrackPerPerson`, and `NoExpiredReoffer`. A parse error or an
unrelated failure is not counted as a detected mutation.

## Conformance

The Rust state machine uses the same claim, submit, skip, and expiry
vocabulary as the TLA+ model. The claim model is checked against `Assignment`
and `SeenLedger` by `proptest-state-machine`. Tier 3 adds ITF replay after a
spike; if the tool is not reliable, use NDJSON trace validation.

## Discord transport

There is no official Discord bot emulator. The repository tests the transport in
three layers:

- `FakeDiscord` for the `DiscordPort` contract;
- fake gateway sessions for the supervisor;
- `crates/swap-discord/tests/setup_http.rs` uses Twilight's HTTP proxy setting
  and a local Wiremock server for the real setup and DM serializers.

The local mock does not cover gateway WebSocket identification or Discord rate
limits. A live test guild still needs a human and a token.

## Anonymity

TLA+ cannot prove noninterference end to end. Use participant projections,
canary strings, outbound-effect audits, log scans, and adversarial review. Use
TLA+ only for the reveal gate and the modeled outbound schema.

## Release gate

A failure in a model that exists for a release blocks that release unless the
team fixes it, retires the model in the same change, or records a tracked
waiver with a reason and expiry. `v0.1.0` is blocked only by the models that
exist at `v0.1.0` (the claim model and the parser tests). Schedule, async,
delivery, and reveal models are required before 1.0.0.

## Commands

```bash
scripts/formal/check.sh          # TLA+ PR model + mutations
scripts/formal/check.sh nightly  # deeper model
cargo test -p swap-core --test assignment_machine
cargo test -p swap-discord custom_id   # custom-ID property tests
cargo test -p swap-files scrub         # module-scrub property tests
```

## Change rules

- Update the model and the Rust code in the same PR when a modeled action
  changes.
- Update `specs/tla/README.md` when a property, bound, or mapping changes.
- Run the mutation set after a model change.
- Do not weaken a property to make a check pass. Report the finding.
- If the model and the code disagree, decide which one is wrong before editing
  either.
