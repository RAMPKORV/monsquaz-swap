# Scenario coverage

> <!-- GENERATED FILE — run `node scripts/trace-scenarios.mjs`; do not edit by hand. -->

Total: 224 scenarios. Mapped: 113. Pending: 111. Exempt: 0.

| Feature | Scenarios | Mapped | Pending | Exempt |
|---|---|---|---|---|
| `anonymity.feature` | 14 | 4 | 10 | 0 |
| `async_queue.feature` | 33 | 24 | 9 | 0 |
| `core_invariants.feature` (cucumber) | 6 | 6 | 0 | 0 |
| `deadlines_and_extensions.feature` | 14 | 3 | 11 | 0 |
| `delivery.feature` | 10 | 3 | 7 | 0 |
| `files.feature` | 10 | 1 | 9 | 0 |
| `fill_ins.feature` | 24 | 23 | 1 | 0 |
| `groups.feature` | 12 | 5 | 7 | 0 |
| `join_and_seed.feature` | 33 | 21 | 12 | 0 |
| `ops.feature` | 16 | 4 | 12 | 0 |
| `participation.feature` | 7 | 3 | 4 | 0 |
| `privacy.feature` | 8 | 2 | 6 | 0 |
| `reveal.feature` | 17 | 6 | 11 | 0 |
| `scheduling.feature` | 10 | 5 | 5 | 0 |
| `security.feature` | 10 | 3 | 7 | 0 |

## Unmapped scenarios

### `anonymity.feature`

- A participant view hides the owner (pending)
- The host store keeps a readable name (pending)
- Public progress does not expose a person (pending)
- An open schedule shows assignments only (pending)
- The host planner shows the R1 seed band (pending)
- Comments are host and reveal only (pending)
- Skips stay private (pending)
- Stream-safe mode hides the mapping (pending)
- The host participation disclosure is visible (pending)
- A small event is not blocked or warned (pending)

### `async_queue.feature`

- An offer does not deliver a file (pending)
- A claim delivers the file and starts the deadline (pending)
- The host starts and stops the queue explicitly (pending)
- Exposure completeness is reported, not forced (pending)
- A small async event uses N-1 rounds (pending)
- A large async event uses a capped length (pending)
- An async claimed deadline miss counts as seen (pending)
- The host deadline prompt does not auto-advance (pending)
- A late seed can get a minimum editor quota (pending)

### `deadlines_and_extensions.feature`

- One classic round has one shared deadline (pending)
- The final exchange round uses the final duration (pending)
- The host opens at most one round at a time (pending)
- The host moves a whole round deadline (pending)
- A moved deadline does not reopen submitted work (pending)
- Reminders follow the percentage ladder (pending)
- A second extension needs the host (pending)
- A third extension is refused (pending)
- A classic claimed deadline miss counts as seen (pending)
- The deadline decision card offers clear actions (pending)
- The extension decision card offers clear actions (pending)

### `delivery.feature`

- A claimed offer delivers one file (pending)
- A blocked DM uses a private thread (pending)
- A participant skips after claiming (pending)
- An expired offer does not mark the pair seen (pending)
- A file above the cap uses the fallback (pending)
- A receipt names the next action (pending)
- The host opens the next round (pending)

### `files.feature`

- The allowlist blocks a forbidden type (pending)
- A format change is refused (pending)
- The pack check reports an unknown sample (pending)
- Added instruments and altered pack samples are allowed (pending)
- An oversized file uses the fallback (pending)
- A corrupt file is rejected (pending)
- The bot downloads an external link (pending)
- A stored file has a stable identity (pending)
- The host previews a module (pending)

### `fill_ins.feature`

- A fill-in wins over a late original holder (pending)

### `groups.feature`

- The planner suggests groups (pending)
- Exhausted swap warns when it cannot fire (pending)
- An infeasible phase falls back to the global pool (pending)
- Unequal groups get a warning (pending)
- Hidden group membership (pending)
- The host sees the fun score (pending)
- The participant does not see the fun score (pending)

### `join_and_seed.feature`

- A guild member joins by submitting a seed (pending)
- A signup without a seed is provisional (pending)
- The DM handshake gates joining (pending)
- Blocked DMs use a private thread (pending)
- Signup collects scheduling inputs (pending)
- The host opens Round 1 with the pack (pending)
- The host locks the roster (pending)
- A late seed waits for host approval (pending)
- A second seed replaces the first before the deadline (pending)
- A participant replaces a seed after the deadline (pending)
- A rejoin with replan needs the event option (pending)
- The host pack is required with a review (pending)

### `ops.feature`

- The host completes the setup wizard (pending)
- Closing the window keeps the bot online (pending)
- A second instance is refused (pending)
- A crash after a send repeats that send (pending)
- The host restarts during an active event (pending)
- A failed command does not change state (pending)
- A backup can be restored (pending)
- The audit log records an override (pending)
- A bot outage is visible (pending)
- A DM sent during an outage is recovered (pending)
- The host runs a dry run (pending)
- The event library keeps a past event (pending)

### `participation.feature`

- Strict participation requires enough handoff budget (pending)
- The host records an away range (pending)
- Open schedule discloses the visible assignments (pending)
- A handoff cap below the plan is not silent (pending)

### `privacy.feature`

- Joining records consent (pending)
- Telemetry is off by default (pending)
- Diagnostics are redacted (pending)
- Backups expire (pending)
- The album license is stated (pending)
- A display name change does not change identity (pending)

### `reveal.feature`

- The host triggers the reveal (pending)
- Credits show the full history (pending)
- Skips show as anonymous gaps by default (pending)
- The presenter shows the lineage and the audio (pending)
- The presenter uses opaque audio URLs (pending)
- The host records a comment before the reveal (pending)
- Stream-safe mode hides the mapping in the presenter (pending)
- The app exports the album package (pending)
- A large export uses the host choice (pending)
- The schedule is disclosed at the reveal (pending)
- The reveal publishes the verification file (pending)

### `scheduling.feature`

- A default classic event runs N total rounds (pending)
- The planner warns about overcrowding (pending)
- An infeasible schedule is refused (pending)
- A replan creates a new commitment (pending)
- A replan has no cap (pending)

### `security.feature`

- The token never reaches the UI or a log (pending)
- File bytes never cross the IPC boundary (pending)
- An executable is blocked (pending)
- An archive bomb is refused (pending)
- A parser crash does not crash the app (pending)
- The bot uses no privileged intents or self-bot (pending)
- A co-host mapping view is audited (pending)
