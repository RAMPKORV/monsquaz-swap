---
name: formal-verification
description: Check the Monsquaz Swap event rules with TLA+/TLC, mutation tests, model-based proptests, and parser property tests. Use when a change touches claim, offer, custody, expiry, scheduling, delivery, or a parser/codec.
---

# Formal verification

`scripts/formal/check.sh` is the source of truth. This skill explains how to
use it and how to keep the model honest. If this skill is stale or missing,
run the script from the repository root.

## Before a change

1. Read `docs/FORMAL_METHODS.md` and `specs/tla/README.md`.
2. Identify whether the change touches a modeled action, property, bound, or
   mapping. If yes, update the model in the same PR.
3. Do not change a property or its meaning without a tracker decision. Agents
   may fix mechanical drift.

## Run

```bash
scripts/formal/check.sh
scripts/formal/check.sh nightly
cargo test -p swap-core --test assignment_machine
```

Kani is not a CI gate. The parser spike exhausted CBMC on standard allocation.
Parser and codec changes use the property tests in
`crates/swap-discord/src/custom_id.rs` and `crates/swap-files/src/scrub.rs`.
Add a focused case there before you add a new tool.

## Read a result

- A TLC violation is a finding. Read the shortest trace in the event words:
  offer, claim, expire, skip, submit.
- Do not weaken the invariant or add a state constraint to make a failure pass.
- A pass covers only the finite bounds in the config.
- Report the command, the bounds, and the exact trace in the PR or tracker.
- If a mutation does not fail, the property is uncovered. Fix the mutation or
  the model.

## Keep it maintainable

- Delete a model in one PR if it no longer buys a finding or the upkeep is too
  high. Product code must not depend on it.
- Update the mapping table and the sync state in `specs/tla/README.md`.
- Keep each model small. If a new feature needs a bigger model, split it.
- Do not use a proof to justify a product refactor. Use an ADR.

## Findings stay findings

If the code breaks a property, report it with the counterexample and the
requirement. Do not change product code unless the task asks for the fix.
