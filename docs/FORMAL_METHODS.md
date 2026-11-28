# Formal methods

Formal verification is a design tool for the event rules. It is not a product
dependency, a release artifact, or an architecture driver. The program is
decision D-037 (formal verification program v2). This file describes the
current tools, the planned routes, and the gates.

## Goals

1. Find design bugs in claim, offer, custody, expiry, schedule, delivery,
   reveal, and anonymity before they reach users.
2. Prevent invariant regressions when the modeled code changes.
3. Keep a precise, reviewable description of the rules for humans and agents.

## Tools

- **TLA+ 1.8.0 and TLC** for finite design models. The jar is not committed.
  `scripts/formal/check.sh` downloads it from our own release asset
  `tools-tla2tools-1.8.0` and checks a pinned SHA-256 (D-062). A mismatch
  fails the check.

  We mirror the jar because the upstream `v1.8.0` asset is mutable. Upstream
  rebuilt it on 2026-11-26 and again on 2026-11-28, and each rebuild broke
  the pin. The mirror holds the 2026-11-28 build (`sha256:df7cef5d…`). The
  script keeps the upstream URL as a provenance comment.

  To update the mirror:
  1. Download the new upstream `tla2tools.jar`.
  2. Compare its SHA-256 with the digest that the GitHub API reports for the
     upstream asset.
  3. Run `scripts/formal/check.sh` and `scripts/formal/check.sh nightly` with
     the new jar. Every model and every mutation must pass.
  4. Upload the jar to a new release tag `tools-tla2tools-<version>`. Do not
     replace the asset of an existing tag.
  5. Change `TLA_URL`, `TLA_SHA256`, and the provenance comment in
     `scripts/formal/check.sh` in one PR. The script replaces the cached
     `target/formal/tla2tools.jar` when the new hash does not match the pin.
     Change the cache key in `.github/workflows/formal.yml` in the same PR.
  6. Record the change in a tracker decision or in the issue evidence.
- **proptest-state-machine** for model-based tests against the real Rust types
  and engine.
- **Verus** is the code tier for pure Rust (D-071). The first proof is the
  flat planner checker in `crates/swap-plan-check/verus/flat_plan_check.rs`
  (MSW-133). The toolchain is pinned to Verus 0.2026.11.22.1ce6a59 and its
  bundled Z3 4.16.0. The crate is dev-only. The product build does not use
  Verus.
- **Kani** was spiked for the header parser and custom-ID codec. The current
  harnesses exhaust CBMC on standard allocation, so Kani is not in CI. Parser
  and codec safety use property tests instead.
- **ITF replay** spike: TLC can dump a JSON counterexample with `-dumpTrace
  json`. That JSON is not the ITF schema, and the pinned jar has no ITF
  converter. Use NDJSON trace validation for Tier 3 until a pinned converter
  is justified.

TLAPS, Lean, Creusot, Prusti, Dafny, Aeneas, and Rocq are not part of the
pillar (D-071, D-073). Verus is the code tier. A new tool needs a decision.

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
  The flat checker is proved in MSW-133. The Hall counting theorem is proved.
  The executable certificate verifier is the remaining step. The shipped
  checker is not yet refined from the proof.

New protocol models stay on TLA+. Pure Rust functions use Verus.

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
| Planner checker | `crates/swap-plan-check/verus/flat_plan_check.rs` | MSW-107, MSW-133 | pilot: checker proved; certificate verifier open |
| Inductive invariants | Apalache | MSW-108 | planned |
| Classic schedule | `specs/tla/SwapSchedule.tla` | MSW-037 | implemented |
| Async queue | `SwapAsync.tla` | MSW-038 | planned |
| Reveal gate | `specs/tla/SwapReveal.tla` | MSW-041 | implemented |
| Rust mutation testing | `cargo-mutants` nightly | MSW-097 | planned |

## Gates

- `specs/tla/mapping.json` lists the Rust files and functions that each model
  covers. `scripts/formal/changed.mjs <base>` exits 0 when a diff touches a
  mapped file, `specs/tla/`, or `scripts/formal/`.
- `ci.yml` calls `.github/workflows/formal.yml` for every pull request, so the
  formal job is part of `ci-gate`. The job runs `scripts/formal/check.sh` only
  when `changed.mjs` reports a mapped change; a skipped job counts as success.
  The nightly run is unconditional. `changed.mjs` uses `git diff
  --no-renames`, so a moved or renamed mapped file also runs the gate.
- The PR formal job runs only for `pull_request` events. A direct push to
  `main` does not run it; the nightly job is the backstop. Add a `push` trigger
  if the release gate must cover direct pushes.
- `scripts/check.sh` runs `scripts/formal/check.sh` when `java` is on `PATH`.
  Otherwise it prints `formal: SKIPPED (no java)`.
- The "Model" step in `ai-rules/WORKFLOW.md` applies to every change to a
  mapped file. A `full` ceremony issue records the formal evidence.
- `formal.yml` has a `verus-pr` job for changes under `crates/swap-plan-check/`
  or `formal.yml`, and a `verus-nightly` job for the full set. The job installs
  the pinned Verus zip by SHA-256, runs `verus --no-cheating --triggers-mode silent --time`, the
  conformance comparison, and the two mutation sets. It has no fixed time cap
  (D-074). The observed proof time is about one second.
- The Verus trust boundary is Verus, its bundled Z3, rustc, and vstd. The Rust
  round checker and the flattening adapter are not verified. The conformance
  corpus covers the adapter. The executable certificate verifier is not
  verified.
- `scripts/check.sh` does not run Verus because the toolchain is not a product
  dependency. The CI job is the gate. A local run uses the pinned toolchain
  under `/tmp/opencode/verus`.

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

The schedule model checks:

- `CompleteRounds`: R1 covers every track with its owner. Every planned
  exchange round gives one track to each participant and one participant to
  each track.
- `NoRepeat`: no participant-track pair appears in two exchange rounds.
- `NoOwnTrack`: no exchange round gives a participant the track they seeded.
- `GroupContainment`: a participant receives a track from the group one phase
  step forward. A fallback phase can use the global pool.
- `SealedPlanComplete`: a locked plan has every exchange round.
- `TypeOK`: every variable has the expected shape.

The schedule model assumes strict participation. Flexible handoff caps, away
ranges, fill-ins, bench states, and manual edits are outside the checked bound.
The group bounds keep the group sizes equal because the classic group planner
requires one track per participant per group.

The reveal model checks:

- `NoRevealBeforeHostAction`: the reveal is not published before the one-shot
  host command.
- `NoRevealWithActiveAssignments`: the reveal is not published while an
  assignment is active.
- `NoIdentityBeforeReveal`: before the gate, the participant projection has no
  seed owner, title, or comment fact in any mode. It has no editor assignment
  fact in hidden mode. An open schedule can show the editor assignment (D-045).
- `StreamSafeDefault`: stream-safe is on at Init. While it is on, the presenter
  has no identity or assignment fact. It turns off only after host
  confirmation, before or after publish.
- `PublishedImpliesPlanComplete`: a reveal is published only after the plan is
  complete.
- `PublishedImpliesCommitmentVerified`: a reveal is published only after the
  commitment is verified.
- `TypeOK`: every variable has the expected shape.

The reveal model abstracts the participant projection to four fact kinds:
seed owner, title, comments, and editor assignment. It does not model opaque
assignment text, the Discord message split, redaction, or the ZIP export.
Atomic publish is structural: one action sets the state and the projection.
The model does not recompute the commitment hash.

The open schedule shows editor assignment facts before the reveal (D-045). It
never shows seed owners, titles, or comments. **Known code gap (MSW-120):** the
current engine `participant_schedule` and the Discord `handle_schedule` still
send owner names, titles, and comments in open mode. The model forbids that
leak. Do not treat the current open-schedule response as a legal behaviour.
`mapping.json` lists both projection functions, so a fix to MSW-120 runs the
reveal gate.

The stream-safe switch is a UI control (`ui/src/views/Reveal.tsx` and
`ui/src/App.tsx`). It is not listed in `mapping.json`, so a UI-only presenter
change does not trigger the formal gate. The model checks the protocol, not the
rendering.

## Model-checking bounds

| Run | Model | Bound | Time budget |
|---|---|---|---|
| Claim PR | 2 participants × 2 tracks | 3 ticks | 5 minutes |
| Claim nightly | 3 participants × 3 tracks | 2 ticks | 20 minutes |
| Delivery PR | 3 effects | 2 attempts | 5 minutes |
| Delivery nightly | 4 effects | 3 attempts | 20 minutes |
| Schedule PR | 4 participants × 4 tracks | 3 exchange rounds | 5 minutes |
| Schedule grouped PR | 4 participants × 4 tracks | 2 groups, 2 exchange rounds | 5 minutes |
| Schedule nightly | 6 participants × 6 tracks | 4 exchange rounds, 2 groups | 20 minutes |
| Reveal PR | 2 participants × 2 tracks | 1 publish gate, hidden schedule | 5 minutes |
| Reveal nightly | 3 participants × 3 tracks | 1 publish gate, open schedule | 20 minutes |

The claim PR model has 750 distinct states and the nightly model has 2,338,158.
The delivery PR model has 343 distinct states and the nightly model has 6,561.
The schedule PR model has 82 distinct states and the grouped PR model has 10.
The schedule nightly model has 1,445 and finishes in 2 seconds on 24 workers.
The reveal PR model has 771 distinct states and the nightly model has 24,579;
both finish in about one second on 24 workers.
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

`scripts/formal/check.sh` applies the same rule to the schedule and reveal
models. The schedule set breaks `TypeOK`, `CompleteRounds` (the seed round and
the exchange matching), `NoRepeat`, `NoOwnTrack`, `GroupContainment`, and
`SealedPlanComplete`. The reveal set breaks `TypeOK`,
`NoRevealBeforeHostAction`, `NoRevealWithActiveAssignments`,
`NoIdentityBeforeReveal` (a hidden-mode assignment leak and open-mode owner,
title, and comment leaks), `StreamSafeDefault` (default off, an unconfirmed
disable, and a full presenter), `PublishedImpliesPlanComplete`, and
`PublishedImpliesCommitmentVerified`. The gate detects 34 mutations in total.

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

The reveal model permits the D-045 open-schedule assignment view and forbids
seed owners, titles, and comments before the reveal. The current open-schedule
response still exposes those facts (MSW-120). The model records the required
behaviour, not the gap.

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
scripts/formal/check.sh          # TLA+ PR models + mutations
scripts/formal/check.sh nightly  # deeper models
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
