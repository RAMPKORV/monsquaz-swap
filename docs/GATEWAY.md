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
  sends the setup test message and backs `fetch_dm_after`. The delivery loop
  remains in the offer and submission slice.
- `src-tauri/src/gateway_host.rs` owns the supervisor task on Tauri's runtime.
  The setup service starts it after it reads the keychain token.

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

A handler error increments the `handler_errors` counter. A handler error does
not close the session. The supervisor gives each handler 5 seconds. A handler
must not block for longer than that.

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

The runtime has no database migration. To roll back:

1. Stop the supervisor. It closes the shard with code 1000.
2. Remove the `live` feature. The contract and supervisor core still build.
3. No event data, ledger row, or file changes occur in the gateway.

Verification for the rollback path:

```bash
cargo test -p swap-discord --no-default-features
cargo check -p monsquaz-swap --no-default-features
```

## Restart and backfill

The app opens the SQLite database when it starts. It dispatches pending outbox
rows and runs one DM backfill for each recorded cursor. A command commits the
event state and its outbox rows in one transaction. A restart or reboot keeps
the event, seen ledger, files, and pending deliveries.

A crash after Discord accepts a send and before the sent mark can duplicate a
message. The delivery id is stable. A future transport nonce or reconciliation
step must close that window. The formal delivery model allows duplicate sends
and one acknowledgement.

## Deferred work

- The app must enforce the host permission for `host-lock`, `host-reveal`, and
  `host-status`. Discord command permissions do not apply per subcommand.
- The bot setup flow is in `docs/SETUP.md`. It stores the token in the OS
  keychain, maps channels, and sends a test direct message.
- The start/stop UI commands remain in the delivery slice.
- Outbound DM, file delivery, and DM backfill remain in the offer and
  submission slice. `DiscordPort::fetch_dm_after` has no live adapter yet.
- The real test guild smoke test needs `DISCORD_TOKEN` and `DISCORD_GUILD`.
