# Formal methods

Formal verification is a design tool for the event rules. It is not a product
dependency, a release artifact, or an architecture driver. The program is
decision D-037 (formal verification program v2). This file describes the
current tools, the planned routes, and the gates.

## Goals

1. Find design bugs in claim, offer, custody, expiry, schedule, and delivery
   before they reach users.
2. Prevent invariant regressions when the modeled code changes.
3. Keep a precise, reviewable description of the rules for humans and agents.

## Tools

- **TLA+ 1.8.0 and TLC** for finite design models. The build is pinned by
  SHA-256 in `scripts/formal/check.sh`. The upstream `v1.8.0` asset was
  rebuilt on 2026-11-26; the pin tracks that build. A mismatch fails the check
  for review. The jar is not committed.
- **proptest-state-machine** for model-based tests against the real Rust types
  and engine.
- **Kani** was spiked for the header parser and custom-ID codec. The current
  harnesses exhaust CBMC on standard allocation, so Kani is not in CI. Parser
  and codec safety use property tests instead.
- **ITF replay** spike: TLC can dump a JSON counterexample with `-dumpTrace
  json`. That JSON is not the ITF schema, and the pinned jar has no ITF
  converter. Use NDJSON trace validation for Tier 3 until a pinned converter
  is justified.

TLAPS, Lean, Verus, Creusot, and Prusti are not part of the general strategy.
They are considered only for a small pure function when the benefit is clear.

## Planned routes (D-037)

These routes are decided. They are not built yet. Each route has an issue.

- **TLA+ trace validation (MSW-106).** Engine tests write an NDJSON trace of
  offer, claim, expire, skip, and submit steps. TLC checks that each trace is
  a legal behaviour of `SwapClaim.tla`. This replaces ITF replay and links
  the model to the real engine.
- **Type-level anonymity (MSW-105).** Owner, title, comment, and contributor
  values are wrapped in `HostOnly<T>`. Only code that holds a `Revealed`
  capability can unwrap them. The capability exists only after the host
  triggers the reveal. Participant DTOs cannot hold a `HostOnly<T>`.
- **DB-enforced invariants (MSW-081).** SQLite constraints and triggers
  refuse a repeat pair, an own-track assignment, and a second active holder.
  The engine checks stay; the database is the last line.
- **Apalache inductive invariants (MSW-108).** Apalache checks that the claim
  and delivery invariants are inductive. This removes the dependency on the
  TLC bounds for those properties.
- **Planner checker with certificates (MSW-107).** The planner is untrusted.
  A small checker verifies every plan against the hard invariants. When no
  plan exists, the planner returns a Hall infeasibility certificate: a set of
  tracks whose eligible participants are fewer than the tracks. The checker
  verifies the certificate.

New models stay on TLA+.

## Friction and ownership

- The formal artifacts are dev-only. Product crates do not import them.
- Any model, harness, or skill can be deleted in one PR. Deleting the artifacts
  does not change product behavior or the schema.
- `scripts/formal/check.sh` is authoritative. The shared skill
  `ai-rules/skills/formal-verification/SKILL.md` is guidance. A stale skill or a missing agent must not disable the checks.
- Agents may fix mechanical drift. A change to a property, its meaning, or its
  bounds needs human approval through a tracker decision.
- Verification does not mandate a product refactor. If a proof needs an
  architectural change, write an ADR and decide it on product grounds.

## Target list

The models follow their features. Each model lands with the feature it
describes.

| Target | Model or tool | Issue | Status |
|---|---|---|---|
| Claim, offer, custody, expiry | `specs/tla/SwapClaim.tla` | — | implemented |
| Assignment lifecycle | `crates/swap-core/tests/assignment_machine.rs` | — | implemented |
| Parser and custom-ID codec | `custom_id.rs` and `scrub.rs` property tests | — | implemented |
| Delivery terminal safety | `specs/tla/SwapDelivery.tla` | — | implemented |
| Anonymity projections | `swap-sim` outbound audit | — | partial |
| DB-enforced invariants | SQLite constraints and triggers | MSW-081 | planned |
| Type-level anonymity | `HostOnly<T>` + `Revealed` capability | MSW-105 | planned |
| Engine trace validation | NDJSON traces checked by TLC | MSW-106 | planned |
| Planner checker | verified checker + Hall certificates | MSW-107 | planned |
| Inductive invariants | Apalache | MSW-108 | planned |
| Classic schedule | `SwapSchedule.tla` | MSW-037 | planned |
| Async queue | `SwapAsync.tla` | MSW-038 | planned |
| Reveal gate | `SwapReveal.tla` | MSW-041 | planned |
| Rust mutation testing | `cargo-mutants` nightly | MSW-097 | planned |

## Gates

- `specs/tla/mapping.json` lists the Rust files and functions that each model
  covers. `scripts/formal/changed.mjs <base>` exits 0 when a diff touches a
  mapped file, `specs/tla/`, or `scripts/formal/`.
- The PR workflow `.github/workflows/formal.yml` runs `scripts/formal/check.sh`
  only when `changed.mjs` reports a mapped change. The nightly run is
  unconditional.
- `scripts/check.sh` runs `scripts/formal/check.sh` when `java` is on `PATH`.
  Otherwise it prints `formal: SKIPPED (no java)`.
- The "Model" step in `ai-rules/WORKFLOW.md` applies to every change to a
  mapped file. A `full` ceremony issue records the formal evidence.

## Invariants

The claim model checks:

- `NoRepeat`: a seen pair is never offered again.
- `NoOwnTrack`: a participant never holds the track they seeded.
- `NoOwnOffer`: a participant is never offered the track they seeded.
- `AtMostOneActivePerPerson`: a participant has at most one open offer or held track.
- `AtMostOneOfferPerTrack`: a track has at most one open offer.
- `AtMostOneOfferPerPerson`: a participant has at most one open offer.
- `NoExpiredReoffer`: an expired pair is never offered again.
- `TypeOK`: every variable has the expected shape.

Claim windows use a bounded logical clock. Real time, DST, reminders, SQLite,
Discord, files, and the UI are outside the model.

The delivery model checks:

- `SentWereEnqueued`: a sent effect was enqueued first.
- `AckedWereSent`: an acknowledgement refers to a sent effect.
- `AckedWereEnqueued`: an acknowledgement refers to an enqueued effect.
- `NoLostEffect`: every enqueued effect is pending, acknowledged, or failed.
- `FailedWereEnqueued`: a failed effect was enqueued first.
- `FailedWereAlerted`: every failed effect committed a host-visible audit row.
- `TerminalExclusive`: an effect is not both acknowledged and failed.
- `UniqueKeys`: dedupe keys are unique.
- `AttemptsBounded`: the attempt counter stays within the bound.
- `TypeOK`: every variable has the expected shape.

A restart is a stuttering step because the outbox and event state live in
SQLite. The model proves safety across duplicate sends, lost responses, and
the retry cap. It does not prove wall-clock liveness.

## Model-checking bounds

| Run | Model | Bound | Time budget |
|---|---|---|---|
| Claim PR | 2 participants × 2 tracks | 3 ticks | 5 minutes |
| Claim nightly | 3 participants × 3 tracks | 2 ticks | 20 minutes |
| Delivery PR | 3 effects | 2 attempts | 5 minutes |
| Delivery nightly | 4 effects | 3 attempts | 20 minutes |

The claim PR model has 750 distinct states and the nightly model has 2,338,158.
The delivery PR model has 343 distinct states and the nightly model has 6,561.
The nightly timeout is a hard ceiling. If a model exceeds its budget, reduce the
scope or use Apalache. A skipped model must be reported, never silently
ignored.

## Mutation rule

A property is not covered until a deliberately broken variant makes it fail.
`scripts/formal/check.sh` replaces one guard or update at a time and requires
TLC to report the named invariant. The set covers `TypeOK`, `NoRepeat`,
`NoOwnTrack`, `NoOwnOffer`, `AtMostOneOfferPerTrack`,
`AtMostOneOfferPerPerson`, `AtMostOneActivePerPerson`, and
`NoExpiredReoffer`, plus `AckedWereSent`, `SentWereEnqueued`, `UniqueKeys`,
`NoLostEffect`, `FailedWereAlerted`, `AttemptsBounded`, and the delivery
`TypeOK`. A parse error or an unrelated failure is not counted as a detected
mutation.

## Conformance

The Rust state machine and the TLA+ model use overlapping claim, submit, skip,
and expiry vocabulary. The Rust lifecycle reference is checked against
`Assignment` and `SeenLedger` by `proptest-state-machine`. It is a separate
lifecycle model, not a trace-conformance check. The pinned TLC JSON
counterexample is the fallback trace artifact. NDJSON trace validation (MSW-106)
replaces ITF replay until a pinned converter is justified.

## Discord transport

There is no official Discord bot emulator. The repository tests the transport in
three layers:

- `FakeDiscord` for the `DiscordPort` contract;
- fake gateway sessions for the supervisor;
- `crates/swap-discord/tests/setup_http.rs` uses Twilight's HTTP proxy setting
  and a local Wiremock server for the real setup, DM, and file serializers.
  Run it with `cargo test -p swap-discord --features test-proxy --test setup_http`.

The local mock does not cover gateway WebSocket identification or Discord rate
limits. A live test guild still needs a human and a token.

## Anonymity

TLA+ cannot prove noninterference end to end. Use participant projections,
canary strings, outbound-effect audits, log scans, and adversarial review. Use
TLA+ only for the reveal gate and the modeled outbound schema. Type-level
anonymity (MSW-105) makes a leak a compile error for the wrapped values.

Module metadata is scrubbed. Non-module audio and zip files pass through
unchanged, so ID3, Vorbis, and WAV LIST tags remain a residual anonymity risk.
The default allowlist includes audio. Treat this as a product decision, not a
scrub claim.

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
node scripts/formal/changed.mjs origin/main   # does this branch touch a model?
cargo test -p swap-core --test assignment_machine
cargo test -p swap-discord custom_id   # custom-ID property tests
cargo test -p swap-files scrub         # module-scrub property tests
```

## Change rules

- Update the model and the Rust code in the same PR when a modeled action
  changes.
- Update `specs/tla/README.md` and `specs/tla/mapping.json` when a property,
  bound, or mapping changes.
- Run the mutation set after a model change.
- Do not weaken a property to make a check pass. Report the finding.
- If the model and the code disagree, decide which one is wrong before editing
  either.
