# swap-plan-check — Verus pilot

Status: **partial pilot**. The specification and the soundness theorems are
verified. The executable checker and certificate verifier are not verified.
This README states the gap. Do not treat the crate as a release gate.

## Purpose

This crate is the first code-tier proof target for D-071 and D-072. It owns a
small plan model for the classic swap planner:

- active participants and active tracks;
- one owner for each active track;
- an ordered list of rounds;
- a strict policy (each round covers all active people and tracks);
- a flexible policy (a round can contain a gap).

`check_plan` returns `Ok(())` or a typed `PlanReject`.
`verify_hall_certificate` accepts a track subset and a neighbor superset.

The crate is independent of the planner. No database, no I/O, no async, no
Discord, and no Tauri. No product crate imports it.

## Bounds

| Bound | Value |
|---|---|
| Active participants | 16 |
| Active tracks | 16 |
| Rounds | 16 |
| Assignments in one round | 16 |
| Participant ids and track ids | dense, less than 16 |

The bounds cover the classic cap of 12 participants. A larger event needs a new
model and a new proof.

## Verified properties

The command verifies 19 functions:

```bash
VERUS_HOME=/tmp/opencode/verus \
RUSTUP_HOME=/tmp/opencode/verus-rustup \
CARGO_HOME=/tmp/opencode/verus-cargo \
PATH=/tmp/opencode/verus-cargo/bin:$PATH \
/tmp/opencode/verus/verus --no-cheating --time plan_check.rs
```

| Property | Verus item | Status |
|---|---|---|
| A valid plan has no repeated `(participant, track)` pair | `no_repeated_pair` and `theorem_check_plan_sound` | proved |
| A valid plan has no own-track assignment | `no_own_track` and `theorem_check_plan_sound` | proved |
| A strict valid plan covers every active person and track in every round | `strict_complete` and `theorem_check_plan_sound` | proved |
| A strict valid plan has no duplicate participant in a round | `round_ok` | proved |
| A strict valid plan has no duplicate track in a round | `round_ok` | proved |
| An accepted Hall certificate excludes an injective assignment for its track subset | `lemma_certificate_counting` | proved |
| The counting argument does not use Hall's theorem | `lemma_map_size`, `lemma_len_subset` | proved |
| No `assume`, `admit`, or `external_body` in the proof crate | `--no-cheating` | enforced |

`lemma_certificate_counting` proves the direct counting argument. The track
subset is `T`. The neighbor set is `N`. The verifier checks that `N` is a
superset of the true neighborhood and that `|N| < |T|`. An injective holder
function maps `T` into `N`. An injection from a finite set into a smaller set
cannot exist. The proof uses `Set::injective_on`, `lemma_map_size`, and
`lemma_len_subset`.

## Not verified

The executable checker is not in the verified file. The Rust checker and the
certificate verifier are tested, but they are not refined from the Verus
specification.

The blocker is a Verus limitation in this version:

- A copied `Round` value from an array or a vector is equal to the
  specification view.
- Verus does not use that equality to relate a field of the copy to the field
  of the view.
- `assert(assignment == input.rounds@[r].assignments@[i])` fails after all
  bounds and length facts are asserted.
- The historical seen scan needs this relation. The strict coverage proof needs
  the same relation for nested rounds.

Evidence: the failing assertion was reduced to a 40-line probe. The probe reads
`let round = prior[rr]; let x = round.a[i]; assert(x == prior@[rr].a@[i]);`.
The assertion fails. The flat `Vec<u16>` version passes. A flat plan model
avoids the limitation. The follow-up is a flat assignment list with round
starts.

Consequences:

- The conformance comparison between the Rust checker and a verified twin did
  not run.
- The four proof mutations did not run. The four Rust mutations did run.
- The Hall theorem is verified, but the executable certificate verifier is not
  proved to implement it.
- The pilot does not justify a product dependency on Verus.

## Trusted base

| Item | Pin |
|---|---|
| Verus | `0.2026.11.22.1ce6a59` (`1ce6a59bd72713cf486a7660097fe5ac57519617`) |
| Verus zip | `verus-0.2026.11.22.1ce6a59-x86-linux.zip`, SHA-256 `c955d5958877f4f907cce80aa996d4239a481984829d82e325938c7bf242b0db` |
| Solver | Z3 `4.16.0`, bundled with Verus |
| Rust | `1.100.0-x86_64-unknown-linux-gnu` |
| vstd | The vstd library in the same Verus release |
| rustc for the Rust twin | `1.100.0` |

The toolchain is not in the repository. The pilot installs it under
`/tmp/opencode/verus`. The proof runs on the specification and the theorems.
The Rust twin runs with `cargo`.

## Local commands

```bash
# Rust tests for the pure crate
cargo test -p swap-plan-check

# Format and lint
cargo fmt -p swap-plan-check
cargo clippy -p swap-plan-check --all-targets -- -D warnings

# Generate 1,205 plans and 600 certificates
cargo run -p swap-plan-check --bin gen_corpus -- \
  --plans 1200 --certs 600 \
  --corpus target/plan-check/corpus.txt \
  --expected target/plan-check/expected.txt

# Four Rust mutations; each must fail a test
crates/swap-plan-check/mutations/run.sh

# Verus proof (specification and soundness theorems)
cd crates/swap-plan-check/verus
/tmp/opencode/verus/verus --no-cheating --time plan_check.rs
```

## Conformance corpus

`gen_corpus` writes 1,205 plans and 600 certificates. The generator uses a fixed
seed. The case set covers:

- valid strict plans for rosters 2 to 12;
- valid flexible plans with gaps and empty rounds;
- repeated pairs, own tracks, missing tracks, missing people;
- duplicate people and duplicate tracks in a round;
- empty strict and empty flexible rounds;
- unknown tracks, unknown people, out-of-range ids, and missing owners;
- Hall-infeasible plans;
- capacity rejects;
- accepted and rejected Hall certificates.

`tests/plan_check.rs` compares every plan verdict with an independent oracle
that uses `HashSet`. The oracle does not call the checker. The test also checks
that every typed reject code occurs.

The verified-twin comparison is **not** run. The verified file has no executable
checker. The `expected.txt` file is the Rust verdict file. A future verified
checker must produce the same file.

## Mutation set

`mutations/run.sh` builds a standalone copy of the crate, applies one change,
and requires `cargo test` to fail. The changes are:

1. accept a seen pair;
2. accept an own track;
3. accept an incomplete round;
4. accept an invalid Hall violator that omits a neighbor.

All four mutations fail at least one test. The exact failing tests are in the
command output. The proof mutation set is blocked by the missing executable
refinement.

## Proof effort

| Item | Lines |
|---|---|
| Verus specification and proofs | 580 |
| Rust checker and verifier | 669 |
| Corpus generator | 651 |
| Rust tests | 222 |

Solver result on the pilot machine:

```text
verification results:: 19 verified, 0 errors
total verify-time: 131-151 ms
total smt-time: 36-45 ms
```

The run is stable across three repeats on the pilot machine. No timeout, no random seed change, and
no solver restart was needed. The result is small because the verified file has
no executable loops. The executable twin is the missing work.

## Integration recommendation

Do not integrate this crate into the lock path yet.

1. Keep the Rust checker as a test oracle. It is pure and fast.
2. Build the flat-plan verified twin. Store assignments in one flat array. Store
   the start and length of each round. Prove the same contract on the flat
   model. The flat model avoids the nested-struct limitation.
3. Add an adapter that flattens the engine plan. Compare the flat verdict with
   the round checker on the 1,205-case corpus.
4. Use the checker before lock only after the flat proof and the conformance
   comparison pass. Keep the product build free of Verus.

The single-source idea is not practical here. The `verus!` macro is not
available to the product rustc. The verified twin plus a conformance corpus is
the approved fallback in MSW-133. The current gap is larger than planned
because the twin is not executable.

## CI proposal

Do not add a workflow in this pilot. PR #62 owns the workflow files. Proposed
job:

- Trigger on a change to `crates/swap-plan-check/**` or
  `scripts/formal/verus*`.
- Install the pinned Verus zip with the SHA-256 check.
- Run the Verus proof, the Rust tests, the corpus, and the four mutations.
- Run the full proof set nightly. No fixed time cap, per D-074.
- Keep the product build independent. The job is a separate status.

The job must fail on a proof error, a corpus disagreement, or a mutation that
passes. It must not cache a Verus binary without the pin check.

## Coverage matrix for follow-on targets

| Target | Property | Tool | Status | Note |
|---|---|---|---|---|
| Fill-in eligibility (D-063, D-064) | A fill-in is active, has not seen the track, is not the owner | Verus + proptest | planned | Small pure predicate. Reuse the flat assignment model. |
| Seen ledger | A pair is inserted once and never removed | Verus + TLA+ | planned | The ledger is a set with a monotone insert. Prove idempotence and no re-offer. |
| Plan-edit swap | A two-person swap preserves no-repeat and no-own-track | Verus | planned | Prove the local swap condition. Then prove the whole plan. |
| Anonymity projection | A participant projection has no owner, author, title, or comment | TLA+ + canary tests | partial | Verus cannot prove noninterference end to end. Use projections, canaries, and outbound audits. |
| Parser and codec | A custom id and a scrub result round-trip and stay in bounds | Kani/proptest | partial | Kani exhausted CBMC before. Use bounded property tests until a bounded harness fits. |
| Classic schedule feasibility | A Hall violator means no perfect matching | Verus | proved (spec) | The executable verifier is the gap. |
| Delivery terminal safety | An effect is acknowledged or failed, not both | TLA+ | implemented | `specs/tla/SwapDelivery.tla`. |

## Change rules

- Do not weaken a property to make a check pass. Report the finding.
- A proof change needs the mutation result and the solver time.
- A change to the model or the bounds needs a tracker decision.
- Delete this crate in one PR if the upkeep is higher than the assurance. The
  product build does not depend on it.
