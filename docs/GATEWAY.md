# Gateway runtime

The gateway runtime turns Discord payloads into the `swap-discord` contract.
The runtime does not contain event rules. The application router receives
normalized events and decides what to do.

## Scope

- The runtime uses Twilight 0.18.
- The runtime requests `GUILDS` and `DIRECT_MESSAGES` only.
- The runtime never requests a privileged intent.
- The runtime registers one guild-scoped `swap` command with 16 subcommands.
- The runtime maps slash commands, buttons, modal submits, and DM attachments.
  Member joins come through `/swap join`; the runtime does not request the
  privileged `GUILD_MEMBERS` intent.
- Every interaction event carries an `InteractionRef` with the interaction ID
  and the short-lived response token. The token is redacted in `Debug` output.
- The runtime does not download attachment bytes. It passes the URL and size to
  the application. The application downloads and stores the bytes.
- The runtime does not send DMs or files. The live `HttpDiscordPort` adapter
  sends setup, offer, file, receipt, alert, and thread messages and backs
  `fetch_dm_after`.
- `src-tauri/src/live_runtime.rs` owns the supervisor, the inbound service, and
  the delivery worker. The app starts it when a keychain token and guild are
  configured. It exposes the supervisor health to the host commands.
- The inbound service handles `/swap join`, claim, skip, status, and help. A
  seed attachment in the signup state creates the participant and the track.

## Runtime loop

1. Create a shard with the bot token and the two intents.
2. Wait for Discord to send READY.
3. Register the guild command set. The registration is idempotent.
4. Publish `Online` health.
5. Pass each normalized event to the inbound handler.
6. On a clean close, a transport error, or a registration failure, publish
   `Reconnecting` health.
7. Wait with exponential backoff. The default delay starts at 1 second. The
   default maximum is 60 seconds.
8. Reset the backoff after a session stays online for 60 seconds.
9. On a stop signal, close the shard with code 1000 and publish `Stopped`.
10. On an authentication or configuration error, publish `Failed` and stop.

The supervisor gives each handler 5 seconds. The inbound service queues the
event per participant and returns at once, so a file render or a delivery retry
cannot cancel the state change. A processing error is logged by the queue
worker. The supervisor `handler_errors` counter covers queue and transport
errors, not processing errors. MSW-045 tracks a full health surface.

## Secrets

- The token exists in the `TwilightConfig` and the Twilight client only.
- `Debug` for the configuration prints `[redacted]`.
- Health snapshots contain no token.
- The runtime logs no message content.
- The live smoke example prints event discriminants only.

## Local Discord mock

There is no official Discord bot emulator. `crates/swap-discord/tests/setup_http.rs`
uses Twilight's HTTP proxy setting and a local Wiremock server. It exercises the
real setup, DM, and file serializers without a token. Run it with
`cargo test -p swap-discord --features test-proxy --test setup_http`. The mock
does not cover gateway identification, rate limits, or Discord policy. Use the
live smoke test for those.

## Live smoke test

Set the two environment variables. Run the example. Stop it with Ctrl+C.

```bash
export DISCORD_TOKEN="..."
export DISCORD_GUILD="..."
cargo run -p swap-discord --example gateway_smoke
```

The operator observes four things:

- the token is accepted;
- the guild command set is registered;
- the gateway becomes online;
- Ctrl+C closes the shard and the example exits with `Stopped`.

## Rollback

The gateway runtime has no migration of its own. The delivery outbox migrations
live in `swap-db`. To roll back to a build without them:

1. Stop the supervisor. It closes the shard with code 1000.
2. Restore the database backup from before the first delivery migration.
3. Remove the `live` feature. The contract and supervisor core still build.
4. No event data, ledger row, or file changes occur in the gateway.

Verification for the rollback path:

```bash
cargo test -p swap-discord --no-default-features
cargo check -p monsquaz-swap --no-default-features
```

## Restart and backfill

The app opens the SQLite database when it starts. It dispatches pending outbox
rows, then repeats the dispatch every 30 seconds, and runs one DM backfill for
each recorded inbound cursor. A command that produces an effect commits the
event state and its outbox rows in one transaction. A restart or reboot keeps
the event, seen ledger, files, and pending deliveries.

The outbound DM cursor and the inbound DM cursor are separate columns. A sent DM
can never move the inbound cursor past an unprocessed attachment. Backfill
ignores an attachment whose Discord message id is more than ten minutes older
than the current claim.
Backfill can only visit a participant who has an inbound cursor. Discord does
not replay a direct message that arrives while the gateway is offline. A
first-time submitter must send the file again after the gateway reconnects.

An offer seeds the inbound cursor to the offer message, so backfill cannot
return older history. Inbound events for one participant run in arrival order.
A live event for a participant with a saved cursor waits until that
participant's startup backfill succeeds. The dispatcher retries a failed backfill every 30 seconds and posts
one host alert for each failure streak. A later message never advances the
cursor past a failed attachment. A transient send retries for about ten
minutes before the worker marks the delivery failed and alerts the host.

A crash after Discord accepts a send and before the sent mark can duplicate a
message. The delivery id is stable. A future transport nonce or reconciliation
step must close that window. The formal delivery model allows duplicate sends
and one acknowledgement.

## Classic host actions

- `host_lock_roster` closes seed intake and classifies unseeded signups as
  fill-ins. The planner refuses to generate a schedule before the lock.
- `host_open_round` opens one round. The host must call it. No timer opens a
  round.
- `host_reveal` checks the reveal gate and writes one credits message to the
  mapped reveal channel in the same transaction as the state change.
- `/swap join` records the optional timezone and handoff budget, then sends the
  seed instructions. A seed attachment calls `join_with_seed`.
- The gateway health appears in the mission control and settings views. The
  states are starting, online, reconnecting, stopped, and failed.

## Deferred work

- The app must enforce the host permission for `host-lock`, `host-reveal`, and
  `host-status`. Discord command permissions do not apply per subcommand.
- The settings screen has no dedicated host-alert channel. The worker creates a
  private thread under the announcement channel and adds no participant. The
  host must be a thread member to read the alert. Keep the announcement channel
  free of participants, or add a host-alert channel before beta.
- The DM handshake test message and away dates are not collected in this slice.
  The timezone and handoff budget use slash options with UTC and four as
  defaults. A later issue adds the handshake.
- Transport nonce or reconciliation must close the duplicate-send window
  (MSW-044).
- The real test guild smoke test needs `DISCORD_TOKEN` and `DISCORD_GUILD`.
