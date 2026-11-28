# Testing strategy

Prove the dangerous things first: scheduling invariants, anonymity, file
handling, and recovery. UI tests are valuable but not where the expensive bugs
live.

## Test pyramid

| Layer | Tools | What it proves | CI |
|---|---|---|---|
| Domain unit | `cargo test` in `swap-core` | state machines, invariants, scheduling primitives, scrub rules | every PR |
| Property | `proptest` | no-repeat, no-own-track, completeness, fairness, determinism | every PR (bounded cases) |
| Proof | Verus | pure Rust plan and certificate contracts | every PR for proof changes; nightly full |
| DB integration | `rusqlite` in-memory + temp files | migrations, constraints, transactions, ledger projection | every PR |
| Acceptance | `cucumber-rs` + fake Discord | user journeys and error paths against app services | every PR |
| Adapter contract | recorded Discord JSON / `wiremock` | Twilight request shapes, custom IDs, rate-limit handling | every PR |
| Simulation | `swap-sim` + fake clock/FS | long events, churn, groups, async queues, adversarial leaks | nightly + release |
| UI component | Vitest + Testing Library | views, states, accessibility basics | every PR |
| E2E engine | `swap-e2e` + local Discord simulator | classic and async journeys through Twilight, SQLite, files, and the scheduler | every PR |
| E2E browser | Playwright + HTTP bridge | the real React UI with real host commands, state, and Discord transport | every PR + release gate |
| Live smoke | dedicated test guild | bot-side Discord truth only | manual |

No live Discord bot in pull-request CI. Use the local Discord simulator and
recorded payloads. The simulator speaks the real HTTP and gateway shapes; a
Twilight upgrade must update the simulator contract tests.

## TDD loop

1. Write the Gherkin scenario in `specs/features/` (or update one).
2. Add the smallest failing test:
   - domain → Rust unit test in the crate;
   - invariant → `proptest`;
   - journey → cucumber-rs step;
   - adapter → contract test.
3. Run it and confirm it fails for the right reason.
4. Implement the minimum.
5. Refactor with tests green.
6. Run `scripts/check.sh`.

A bug fix must add a regression test that fails on the old code.

## Required properties

- No `(track, participant)` pair appears twice in the seen ledger.
- No participant is assigned their own track.
- At most one active holder per track and one active track per participant.
- Every active track is assigned, offered, or explicitly held with a reason.
- Same seed + roster + rules → byte-identical canonical schedule.
- Commit-reveal recomputes to the published commitment.
- Fill-ins are eligible (never seen, not owner, under budget).
- Group swaps never violate the global ledger.
- Deadlines are UTC instants; wall-clock display is correct across DST.
- Anonymity projection contains no author/title/comments before reveal.
- Public aggregates are suppressed below the k-threshold.
- Idempotent commands (claim, submit, advance, reveal) apply once.

## Simulation

`swap-sim` is both a test harness and (later) a host dry-run.

- Deterministic: virtual clock, seeded RNG streams, counter IDs, canonical JSON.
- Fake Discord: users, DMs, attachments, rate limits, `50007`, `50278`,
  `40003`, outages, duplicate events, CDN expiry.
- Fake FS: quota, partial writes, ENOSPC, path attacks.
- Profiles: fast, slow, forgetful, DM-closed, mobile-only, skipper, leaver,
  late seed, fill-in, adversarial, colluder, duplicate uploader, huge/corrupt
  file, timezone edge, lurker, spammer.
- Scenarios: classic, async, groups/pool swaps, hidden/visible schedule,
  extensions, bot downtime, crash/restart, restore, mass dropout, duplicates,
  hostile files, tiny pool, double-host, partial reveal.
- Every failure gets a seed and a minimized scenario committed as a regression.

## File fixtures

- Generate tiny synthetic modules for each supported format with unique
  sentinel strings in song name, sample/instrument names, comments, AUTH, and
  tracker fields.
- Include a few CC0 real modules; never commit copyrighted or personal files.
- Scrub tests assert the sentinel is absent from the delivered bytes and from
  libopenmpt/openmpt123 metadata; A/B render must match PCM.
- Path traversal, extension mismatch, executable content, zip bomb, oversized
  file, and corrupt module are mandatory cases.

## Anonymity / adversarial tests

- Participant-facing DTO scan: no owner/author/original filename/title/comments.
- Delivered file metadata scan: no canary strings, original mtime, or path.
- Timing: delivery is batched; upload time is not a trigger.
- Public aggregates: k-anonymity check; no per-person counts before deadline.
- Reveal: order is HMAC-sorted, credits complete, fill-ins marked, skips
  anonymous by default.
- Host/co-host/stream-safe: mapping is hidden in stream mode; access is logged.
- Log scan: no token, no author IDs, no original filenames at default level.

## Commands

```bash
cargo test --workspace
cargo test -p swap-plan-check
cargo test -p swap-core
pnpm --dir ui test
pnpm --dir ui typecheck
scripts/check.sh
# Verus proof and conformance (dev-only toolchain)
crates/swap-plan-check/verus/conformance.sh
cargo test -p swap-acceptance --test cucumber
node scripts/trace-scenarios.mjs     # writes docs/generated/SCENARIO_COVERAGE.md
```

## CI split

This list matches `.github/workflows/` on 2026-11-28. Update it in the same PR
when a workflow changes.

- **PR and push to `main` (`ci.yml`):**
  - tracker: rebuild from the ledger, `doctor`, smoke tests;
  - spec gates: gate script tests, `scripts/ste-lint.mjs`,
    `scripts/trace-scenarios.mjs --check`;
  - UI: typecheck, Vitest, build;
  - design: `scripts/design-check.sh`;
  - Rust: fmt, clippy, `cargo test --workspace` (unit, property with bounded
    cases, DB, cucumber, simulation), the `setup_http` proxy test;
  - Tauri smoke build on Ubuntu and Windows;
  - E2E (`e2e.yml`): `cargo test -p swap-e2e` and the Playwright journeys;
  - formal (`formal.yml`, called from `ci.yml` and part of `ci-gate`): the
    TLA+ PR models and the mutation set when `scripts/formal/changed.mjs`
    reports a mapped change (pull requests only); otherwise the job is
    skipped and counts as success.
- **Nightly (`formal.yml`):** the TLA+ nightly models.
- **Release (`release.yml`):** the E2E gate, the Windows and Linux installers,
  attestations, and `SHA256SUMS`.

Not in CI yet: a secret scan, a dependency audit, long simulation seed runs,
the chaos and DST matrices, the restore drill, cargo-mutants (MSW-097),
packaged smoke on clean VMs, and a live test-guild run.

## Flake policy

No blind retries. A flaky test gets a seed, an issue, and a quarantine label;
it does not gate releases until fixed. Never weaken an assertion to hide a real
intermittency.

## Scenario traceability

Every scenario in `specs/features/` maps to a test or carries `@pending`
(D-038). The convention and the gate are in `ai-rules/GHERKIN.md`. A feature
file that cucumber-rs runs counts as mapped in full and needs no per-test
comment. When you add a test that proves a scenario, add the `// scenario:`
comment and remove the `@pending` tag in the same PR.

## DoD

A behaviour is done when its scenario exists, its test fails before the change
and passes after, the test declares the scenario with `// scenario:`, the
scenario has no `@pending` tag, `scripts/check.sh` is green, and the tracker has
evidence.
