---
name: formal-verification
description: Check the Monsquaz Swap event rules with TLA+/TLC, mutation tests, model-based proptests, and parser property tests, and plan the D-037 routes (trace validation, type-level anonymity, DB-enforced invariants, Apalache, planner certificates). Use when a change touches claim, offer, custody, expiry, scheduling, delivery, a parser/codec, or a file in specs/tla/mapping.json.
---

# Formal verification

`scripts/formal/check.sh` is the source of truth. This skill explains how to
use it and how to keep the models honest. If this skill is stale or missing,
run the script from the repository root. The program is decision D-037; the
full plan is `docs/FORMAL_METHODS.md`.

## Before a change

1. Read `docs/FORMAL_METHODS.md` and `specs/tla/README.md`.
2. Check whether the change touches a mapped file:
   `node scripts/formal/changed.mjs --files <path>...`. The map is
   `specs/tla/mapping.json`.
3. If a mapped file changes, do the "Model" step in `ai-rules/WORKFLOW.md`.
   Update the TLA+ model or the mapping in the same PR.
4. Do not change a property or its meaning without a tracker decision. You
   can fix mechanical drift.
5. A new Rust file that implements a modeled action must go into
   `specs/tla/mapping.json` in the same PR.

## Run

```bash
scripts/formal/check.sh                 # PR models + mutation set
scripts/formal/check.sh nightly         # deeper bounds
cargo test -p swap-core --test assignment_machine
node --test scripts/tests/formal-changed.test.mjs
```

`scripts/check.sh` runs the formal checks when `java` is on `PATH`. Otherwise
it prints `formal: SKIPPED (no java)`. CI runs the PR models when
`scripts/formal/changed.mjs` reports a mapped change.

Kani is not a CI gate. Parser and codec changes use the property tests in
`crates/swap-discord/src/custom_id.rs` and `crates/swap-files/src/scrub.rs`.

## D-037 routes

Each route has a tracker issue. Do not start a route without its issue.

| Route | Issue | What it proves |
|---|---|---|
| DB-enforced invariants | MSW-081 | SQLite constraints and triggers refuse a repeat, an own track, and a second holder. |
| Type-level anonymity | MSW-105 | A `HostOnly<T>` value cannot reach a participant DTO without a `Revealed` capability. |
| TLA+ trace validation | MSW-106 | Engine traces from tests replay as legal steps of the TLA+ model. |
| Planner checker and certificates | MSW-107 | An untrusted planner proposes; a small checker verifies; an infeasible plan gets a Hall certificate. |
| Apalache inductive invariants | MSW-108 | The claim and delivery invariants are inductive, not only true within the TLC bounds. |
| Schedule, async, reveal models | MSW-037, MSW-038, MSW-041 | New TLA+ models for the classic schedule, the async queue, and the reveal gate. |
| cargo-mutants nightly | MSW-097 | The Rust tests kill mutants in the modeled code. |

New models stay on TLA+.

## Read a result

- A TLC violation is a finding. Read the shortest trace in event words:
  offer, claim, expire, skip, submit.
- Do not weaken the invariant or add a state constraint to make a failure
  pass.
- A pass covers only the finite bounds in the config.
- Report the command, the bounds, and the exact trace in the PR or tracker.
- If a mutation does not fail, the property is not covered. Fix the mutation
  or the model.

## Keep it maintainable

- You can delete a model in one PR if it no longer finds bugs or costs too
  much to keep. Product code must not depend on it.
- Update the mapping table in `specs/tla/README.md` and
  `specs/tla/mapping.json` together.
- Keep each model small. If a new feature needs a bigger model, split it.
- Do not use a proof to justify a product refactor. Write an ADR.

## Findings stay findings

If the code breaks a property, report it with the counterexample and the
requirement. Do not change product code unless the task asks for the fix.
