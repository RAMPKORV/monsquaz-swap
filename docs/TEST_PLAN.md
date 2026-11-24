# Test plan

The detailed rules are in `ai-rules/TESTING.md`. This file lists the gates and
the acceptance coverage.

## Layers

| Layer | Tool | What it proves |
|---|---|---|
| Domain unit | Rust `cargo test` | state machines, invariants, scheduling, scrub rules |
| Property | `proptest` | no-repeat, no-own-track, completeness, fairness, determinism |
| DB | rusqlite in-memory + temp files | migrations, constraints, transactions, ledger |
| Acceptance | `cucumber-rs` + fake Discord | user journeys and error paths |
| Contract | recorded Discord payloads / `wiremock` | Twilight request shapes and rate limits |
| Simulation | `swap-sim` | long events, churn, groups, async, outages, leaks |
| UI | Vitest + Testing Library | views, states, accessibility basics |
| E2E | WebdriverIO/Tauri driver | packaged app boot and one full journey |
| Live smoke | real test guild + consenting humans | Discord accepts the real flow |

No live bot in pull-request CI. Use fake adapters and recorded payloads.

## Gate 0 — before the first real event

- [ ] Startup probe verifies token, guild membership, DM permission, and the
      attachment ceiling.
- [ ] Every participant passes the DM handshake or gets a host-approved
      fallback.
- [ ] The seen ledger and own-track invariants pass 10k seeds.
- [ ] Outbound filenames are opaque; metadata scrub passes A/B render.
- [ ] Delivery is batched; upload time is not a trigger.
- [ ] Public progress has no per-person counts before deadline.
- [ ] Reveal is atomic and HMAC-ordered.
- [ ] Crash recovery does not duplicate a send.
- [ ] Backups exist and a restore drill passed.
- [ ] The host can explain every override.
- [ ] The rules state the anonymity limits and the host's mapping access.

## Gate 1 — before public release

- [ ] Windows NSIS and Linux `.deb`/AppImage install on clean VMs.
- [ ] `SHA256SUMS` verifies; GitHub attestations verify.
- [ ] No updater keys, no `latest.json`, no signing secrets.
- [ ] Accessibility pass (WCAG AA, keyboard, screen reader basics).
- [ ] Privacy export/delete works.
- [ ] No telemetry by default; diagnostics are redacted.
- [ ] A 40-participant simulation completes within the host's tolerance.
- [ ] Dependency audit and secret scan are clean.
- [ ] Support docs and known limits are published.

## Acceptance coverage

| Area | Feature file |
|---|---|
| Classic scheduling | `specs/features/scheduling.feature` |
| Async queue | `specs/features/async_queue.feature` |
| Groups | `specs/features/groups.feature` |
| Join and seed | `specs/features/join_and_seed.feature` |
| Claim and submit | `specs/features/delivery.feature` |
| Fill-ins | `specs/features/fill_ins.feature` |
| Deadlines | `specs/features/deadlines_and_extensions.feature` |
| Anonymity | `specs/features/anonymity.feature` |
| Files and pack | `specs/features/files.feature` |
| Reveal | `specs/features/reveal.feature` |
| Ops and recovery | `specs/features/ops.feature` |
| Privacy | `specs/features/privacy.feature` |
| Security | `specs/features/security.feature` |

## Simulation scenarios

| Scenario | What it stresses |
|---|---|
| Classic 12, 3 rounds | baseline |
| Classic hidden/visible | anonymity and UI |
| Groups with swap | rotation, feasibility |
| Async 20, 6 rounds | queue, claim, deadlines |
| Async with unresponsive participant | Ready gate, strikes, gap report |
| Late seed | quotas, roster changes |
| Mass dropout | fill-ins, orphans, reveal gaps |
| DM closed / no mutual guild | fallback and host alert |
| Bot outage | backfill, idempotency |
| Crash between commit and send | outbox, exactly-once |
| Restore from backup | integrity, refusal of stale data |
| Hostile files | size, type, path, archive, parser |
| Timezone/DST | deadlines and display |
| Screen-share | stream-safe mode |
| Colluding pair | advisory signals, no auto-accusation |

## Property tests

- No `(track, participant)` pair repeats.
- No participant is assigned their own track.
- One active holder per track; one active track per person.
- Every active track is assigned, offered, or held with a reason.
- Same seed + roster + rules gives a canonical schedule.
- Commit-reveal recomputes to the published commitment.
- Fill-ins are eligible and under budget.
- Group swaps preserve the global ledger.
- Anonymity projection has no author/title/comments.
- Public aggregates are suppressed below the k-threshold.
- Claim, submit, advance, and reveal are idempotent.

## UI review gate

- Capture every view in empty, loading, populated, error, long-content, and
  dense states at 1280×800 and 1024×700.
- Capture 200% zoom and both themes.
- Run three independent review waves: visual design, UX/copy, accessibility.
- Store curated screenshots under `docs/ui/screens/`.
- No must-fix may remain before a UI milestone closes.
- Re-capture after every fix. A screenshot from an older build is not evidence.

## Manual game day

Cast: host, 6–8 participants, one mobile-only, one slow, one forgetful, one
fill-in, one observer. Use a separate test guild. Compress rounds to 10 minutes.

Acts:

1. Setup and DM handshake.
2. Seed and lock.
3. Offer, claim, edit, submit.
4. Skip, fill-in, bot outage.
5. Reveal and album download.

Hard fails:

- a participant learns an author before reveal;
- a file is lost;
- a DM-closed participant is silently dropped;
- the reveal order follows submission time;
- the host cannot explain an override.

## Commands

```bash
scripts/check.sh
cargo test --workspace
cargo test -p swap-core
pnpm --dir ui test
# later: cargo test -p swap-acceptance --test cucumber
```

## Flake policy

No blind retries. A flaky test gets a seed, an issue, and a quarantine label.
It does not gate a release until fixed. Never weaken a test to hide a real
intermittency.
