# swap-plan-check — verified flat planner checker

Status: **the flat plan checker is verified. The executable Hall certificate
verifier is not.** The Rust checker is an independent round-based twin. Read
the trust section before you use this crate as a gate.

## What this crate owns

A pure plan model for the classic swap planner:

- active participants and active tracks;
- one owner for each active track;
- a flat assignment list with round start and length arrays;
- a strict policy (each round covers all active people and tracks);
- a flexible policy (a round can contain a gap).

`check_plan` in the Rust crate takes the round-based model. The verified core
takes the flat model. A small adapter flattens the round model for conformance.

No database, no I/O, no async, no Discord, and no Tauri. No product crate
imports this crate.

## Verified core

The verified source is `crates/swap-plan-check/verus/flat_plan_check.rs`.

| Property | Verus item | Status |
|---|---|---|
| A valid plan has no repeated `(participant, track)` pair | `no_repeated_pair`, `theorem_flat_plan_sound` | proved |
| A valid plan has no own-track assignment | `no_own_track`, `theorem_flat_plan_sound` | proved |
| A strict valid plan covers every active person and track in every round | `strict_complete`, `theorem_flat_plan_sound` | proved |
| No duplicate participant or track in a round | `round_ok` | proved |
| `check_flat_plan` returns `Ok` only for `plan_ok` | `check_flat_plan` postcondition | proved |
| Termination and no panic, overflow, or index error on the verified inputs | all exec loops | proved |
| An accepted Hall certificate excludes an injective holder function | `lemma_flat_certificate_counting` | proved |
| The counting argument does not use Hall's theorem | `lemma_map_size`, `lemma_len_subset` | proved |
| No `assume`, `admit`, or `external_body` | `--no-cheating` | enforced |

The proof has 51 verified functions and 0 errors. Observed proof time is
about 0.7-0.9 s total and 0.4-0.5 s SMT on the pilot machine, with one thread.

The Hall proof is direct. Let `T` be the certificate track subset and `N` the
neighbor set. The verifier checks that `N` is a superset of the true
neighborhood and that `|N| < |T|`. An injective holder function maps `T` into
`N`. An injection from a finite set into a smaller set cannot exist.

### Not verified

- The executable Hall certificate verifier. The counting theorem is proved,
  but no verified function implements the verifier. The Rust verifier is
  tested, not refined.
- The Rust round checker. It is an independent implementation. It is not the
  same source as the verified flat core.
- The flattening adapter. It is ordinary Rust. The corpus tests it.

## Trust boundary

| Item | Pin |
|---|---|
| Verus | `0.2026.11.22.1ce6a59` (`1ce6a59bd72713cf486a7660097fe5ac57519617`) |
| Verus zip | `verus-0.2026.11.22.1ce6a59-x86-linux.zip`, SHA-256 `c955d5958877f4f907cce80aa996d4239a481984829d82e325938c7bf242b0db` |
| Solver | Z3 `4.16.0`, bundled with Verus |
| Rust | `1.100.0-x86_64-unknown-linux-gnu` |
| vstd | vstd from the same Verus release |

The Rust round checker and the flattening adapter are **untrusted**. The
product build does not use Verus. The proof crate is dev-only until D-071 gets
a later integration decision.

## Conformance

`verus/conformance.sh` compares the verified flat checker with the Rust round
checker on the generated corpus:

- 1,205 plans;
- rosters 2 to 12;
- strict and flexible policies;
- valid plans, repeats, own tracks, missing tracks, missing people, duplicate
  people, duplicate tracks, empty rounds, unknown ids, missing owners, capacity
  rejects, and Hall-infeasible plans;
- 600 certificates.

Result:

```text
plans compared: 1205
acceptance disagreements: 0
reason-class differences: 120
reason pairs: {(9, 8): 57, (10, 8): 63}
conformance OK: no acceptance disagreement
```

The verified contract is acceptance soundness. Both checkers reject every
corrupt case. The reason classes differ because the flat checker checks the
global seen set before the round shape: a corrupt round can report a repeated
pair where the round checker reports a duplicate participant or duplicate
track. This is a known reason-ordering gap, not a soundness gap.

The independent `HashSet` oracle in `tests/plan_check.rs` agrees with the Rust
checker on all 1,205 plans. It does not call the checker.

## Mutations

`verus/mutations.sh` changes the verified source in four ways. Verus must fail
each time. A mutation that verifies is a failed pilot.

1. Accept a seen pair: skip `seen.contains`.
2. Accept an own track: skip the owner comparison.
3. Accept an incomplete round: skip the participant coverage check.
4. Accept an invalid Hall violator: drop `neighbors_len < tracks_len` from the
   certificate contract.

All four fail Verus. The incomplete-round mutation fails the loop invariant.
The invalid-Hall mutation fails the counting theorem.

`mutations/run.sh` changes the Rust checker in the same four ways. All four fail
at least one Rust test.

## Local commands

```bash
cargo test -p swap-plan-check
cargo fmt -p swap-plan-check -- --check
cargo clippy -p swap-plan-check --all-targets -- -D warnings

# Generate 1,205 plans and 600 certificates
cargo run -p swap-plan-check --bin gen_corpus -- \
  --plans 1200 --certs 600 \
  --corpus target/plan-check/corpus.txt \
  --expected target/plan-check/expected.txt

# Verified core
cd crates/swap-plan-check/verus
/tmp/opencode/verus/verus --no-cheating --time flat_plan_check.rs

# Conformance and proof mutations
VERUS=/tmp/opencode/verus/verus ./conformance.sh
VERUS=/tmp/opencode/verus/verus ./mutations.sh

# Rust mutations
crates/swap-plan-check/mutations/run.sh
```

## CI

`formal.yml` has two Verus jobs:

- `verus-pr` runs only when a file under `crates/swap-plan-check/` or
  `formal.yml` changes. It installs the pinned Verus zip with the SHA-256
  check, runs the Rust tests, the proof, the conformance comparison, and both
  mutation sets.
- `verus-nightly` runs the full proof set every day and on manual dispatch.

The jobs have no fixed time cap, per D-074. The observed job time is dominated
by toolchain download. The proof itself is about one second.

The toolchain stays out of the product build. The job is a separate status and
`ci-gate` fails if it fails.

## Integration plan for MSW-107

1. Keep the Rust round checker as the current oracle.
2. Move the Rust checker's internal representation to the flat core at the
   boundary. The public API stays round-based. This makes the shipped checker
   and the verified flat core the same algorithm.
3. If the same source cannot compile under rustc and Verus, keep the verified
   twin and the conformance adapter, and state the gap in the PR.
4. Add the executable certificate verifier. The counting theorem is ready.
   Port the Rust verifier to the flat representation and prove its
   postcondition against `cert_ok`.
5. Call the checker before lock only after the flat checker and the
   certificate verifier pass the conformance and mutation gates.

Do not claim that the shipped Rust checker is verified. It is not. The flat
twin is verified.

## Coverage matrix

| Target | Property | Tool | Status |
|---|---|---|---|
| Fill-in eligibility D-063/D-064 | active, unseen, not owner | Verus + proptest | planned |
| Seen ledger | insert once, never remove | Verus + TLA+ | planned |
| Plan-edit swap | swap preserves no-repeat and no-own | Verus | planned |
| Anonymity projection | no owner, author, title, comment | TLA+ + canaries | partial |
| Parser/codec | round-trip and bounds | Kani/proptest | partial |
| Hall certificate | violator excludes a perfect matching | Verus | counting theorem proved; executable verifier open |
| Delivery terminal safety | ack XOR fail | TLA+ | implemented |

## Change rules

- Do not weaken a property to make a check pass. Report the finding.
- A proof change needs the mutation result and the solver time.
- A change to the flat model or the bounds needs a tracker decision.
- Delete this crate in one PR if the upkeep is higher than the assurance. The
  product build does not depend on it.
