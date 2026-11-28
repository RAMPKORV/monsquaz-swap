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

The verified core has no database, I/O, async, Discord, or Tauri. The tooling
around it does use std I/O: `corpus.rs` reads and writes files, and the Verus
`main` parses the corpus. No product crate imports this crate.

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

The proof has 52 verified functions and 0 errors. Observed proof time is
about one second total and 0.4-0.5 s SMT on the pilot machine, with one thread.

The Hall proof is direct. Let `T` be the certificate track subset and `N` the
neighbor set. The certificate contract requires that `N` is a superset of the
true neighborhood and that `|N| < |T|`. An injective holder function maps `T`
into `N`. An injection from a finite set into a smaller set cannot exist.

### Not verified

- The executable Hall certificate verifier. The counting theorem is proved,
  but no verified function implements the verifier. The Rust verifier is
  tested, not refined.
- The Rust round checker. It is an independent implementation. It is not the
  same source as the verified flat core.
- The flattening adapter. It is ordinary Rust. The corpus tests it.

## Trust boundary

`parse_plan` and `main` in `flat_plan_check.rs` are ordinary Rust and are not
verified. They only decode the corpus. The flattening adapter is in the corpus
tooling, not in the core. The corpus generator is not trusted; the independent
oracle and conformance checks test it.


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

- 1,203 plans;
- rosters 2 to 12;
- strict and flexible policies;
- valid plans, repeats, own tracks, missing tracks, missing people, duplicate
  people, duplicate tracks, empty rounds, unknown ids, missing owners, capacity
  rejects, and Hall-infeasible plans;
- capacity rejects for too many rounds, a round over 16 assignments, too many
  active people, and too many active tracks;

Result:

```text
plans compared: 1203
acceptance disagreements: 0
reason-class differences: 110
reason pairs: {(9, 8): 55, (10, 8): 55}
conformance OK: no acceptance disagreement
```

The verified contract is acceptance soundness. Both checkers reject every
corrupt case. The reason classes differ because the flat checker checks the
global seen set before the round shape: a corrupt round can report a repeated
pair where the round checker reports a duplicate participant or duplicate
track. This is a known reason-ordering gap, not a soundness gap.

The independent `HashSet` oracle in `tests/plan_check.rs` agrees with the Rust
checker on all 1,203 plans. It does not call the checker.

Certificates are **not** compared with a verified verifier. The Rust
certificate verifier is tested against an independent `HashSet` oracle over all
600 certificates. The executable certificate verifier is not verified.

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

`mutations/run.sh` changes the Rust checker in four related cases. The proof
and Rust sets are not identical: proof mutation 3 skips person coverage, while
Rust mutation 3 skips the whole strict block. Proof mutation 4 drops the
certificate count, while Rust mutation 4 drops the neighbor completeness check.
Each mutation catches the property it targets.

## Local commands

```bash
cargo test -p swap-plan-check
cargo fmt -p swap-plan-check -- --check
cargo clippy -p swap-plan-check --all-targets -- -D warnings

# Generate 1,203 plans and 600 certificates
cargo run -p swap-plan-check --bin gen_corpus -- \
  --plans 1200 --certs 600 \
  --corpus target/plan-check/corpus.txt \
  --expected target/plan-check/expected.txt

# Verified core
cd crates/swap-plan-check/verus
/tmp/opencode/verus/verus --no-cheating --triggers-mode silent --time flat_plan_check.rs

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

The refinement choice is **port, not ship the Verus binary**. Do not put Verus
in the product build.

1. Refactor `swap-plan-check::check_plan` to flatten the round input at the
   boundary and call a flat checker with the same algorithm as
   `check_flat_plan`. Keep the public `PlanInput` and `Result<(), PlanReject>`
   API.
2. Add the Rust certificate verifier to the same flat core. Prove the flat
   verifier's postcondition against `cert_ok`, then compare it with the Rust
   verifier on the 600-certificate corpus.
3. Until step 2 lands, the shipped certificate verifier is the Rust code only.
   Its verdict is checked by the independent oracle, not by a proof.
4. Call the flat checker before lock only after the port and the certificate
   verifier pass the conformance and mutation gates.

Do not claim that the shipped Rust checker is verified. It is not. The flat
twin is verified. The executable certificate verifier is the remaining proof
step.

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
