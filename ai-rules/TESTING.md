# Testing strategy

Prove the dangerous things first: scheduling invariants, anonymity, file
handling, and recovery. UI tests are valuable but not where the expensive bugs
live.

## Test pyramid

| Layer | Tools | What it proves | CI |
|---|---|---|---|
| Domain unit | `cargo test` in `swap-core` | state machines, invariants, scheduling primitives, scrub rules | every PR |
| Property | `proptest` | no-repeat, no-own-track, completeness, fairness, determinism | every PR (bounded cases) |
| DB integration | `rusqlite` in-memory + temp files | migrations, constraints, transactions, ledger projection | every PR |
| Acceptance | `cucumber-rs` + fake Discord | user journeys and error paths against app services | every PR |
| Adapter contract | recorded Discord JSON / `wiremock` | Twilight request shapes, custom IDs, rate-limit handling | every PR |
| Simulation | `swap-sim` + fake clock/FS | long events, churn, groups, async queues, adversarial leaks | nightly + release |
| UI component | Vitest + Testing Library | views, states, accessibility basics | every PR |
| E2E | WebdriverIO/Tauri driver (later) | packaged app boot, setup, one full journey | release |
| Live smoke | real test guild, consenting humans | Discord actually accepts intents/commands/DMs | manual/nightly |

No live Discord bot in pull-request CI. Use a fake `DiscordPort` and recorded
payloads. A nightly/manual job may use a dedicated test guild.

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
cargo test -p swap-core
pnpm --dir ui test
pnpm --dir ui typecheck
scripts/check.sh
# later: cargo test -p swap-acceptance --test cucumber
```

## CI split

- **PR:** fmt, clippy, core/property (bounded), DB, cucumber, UI typecheck/test,
  secret scan, dependency audit.
- **Main:** full property runs, more simulation seeds, migrations.
- **Nightly:** 10k seeds, chaos/outage matrix, DST matrix, restore drill.
- **Release:** packaged smoke on clean VMs, one live test-guild run, installer
  upgrade test.

## Flake policy

No blind retries. A flaky test gets a seed, an issue, and a quarantine label;
it does not gate releases until fixed. Never weaken an assertion to hide a real
intermittency.

## DoD

A behaviour is done when its scenario exists, its test fails before the change
and passes after, `scripts/check.sh` is green, and the tracker has evidence.
