# End-to-end tests

The E2E suite proves the host and participant journeys. It uses the real
engine, a real SQLite database, a real blob store, real files, the real
Twilight transport, and the real React UI. It never uses a real Discord
account.

## Layers

| Layer | Driver | Transport | Purpose |
|---|---|---|---|
| Engine | `cargo test -p swap-e2e` | Local Discord simulator | Prove the classic and async journeys through Twilight, the engine, the database, and the outbox. |
| Browser | Playwright | HTTP bridge and the same simulator | Prove the host UI with real commands, real state, and the real transport. |

## Local Discord simulator

The simulator is a local HTTP API and gateway WebSocket. The real Twilight
adapter connects through the `test-proxy` feature. The bot gets a fake token, a
fake guild, three text channels, and seven fake participants. No request leaves
the machine.

HTTP routes:

- `GET /users/@me` and `GET /users/@me/guilds`
- `GET /guilds/{id}` and `GET /guilds/{id}/channels`
- `GET /applications/@me`
- `PUT /applications/{id}/guilds/{id}/commands`
- `POST /users/@me/channels`
- `POST /channels/{id}/messages` with JSON or multipart files
- `PATCH /channels/{id}/messages/{id}`
- `GET /channels/{id}/messages?after=&limit=`
- `POST /channels/{id}/threads`
- `PUT /channels/{id}/thread-members/{user_id}`
- `GET /cdn/{attachment}/{file}`

Gateway support:

- `HELLO`, `IDENTIFY`, `READY`, heartbeats, `RESUMED`, and reconnect.
- `MESSAGE_CREATE`, `INTERACTION_CREATE`, `READY`.
- Text frames. Twilight only decompresses binary frames, so compression is not
  needed for the JSON contract.

The control API is test-only:

- `POST /_control/messages` sends a direct message as a fake participant.
- `POST /_control/interactions` sends a slash command, button, or modal.
- `GET /_control/messages` reads the bot messages.
- `POST /_control/reset` clears messages and attachment bytes.

The gateway sequence and the payload shapes are pinned by tests that decode
every payload with the real `twilight-model` types.

## Test clock

The engine reads the file named by `MSW_E2E_CLOCK` when `MSW_E2E=1`. The test
writes one epoch second value to the file. Deadlines, waves, and the final
48-hour window need no sleep. A production run ignores the file.

The simulator seeds snowflakes from the same clock. A message is never older
than the event that receives it.

## Audio fixtures

The harness generates the fixtures. Each module is a valid ProTracker file with
one looping sample. The harness also generates a WAV file and a ZIP pack.
openmpt123 renders the module before and after the metadata scrub. The test
fails with a clear message when openmpt123 is absent.

## Browser bridge

`crates/swap-e2e` has a bridge binary. It starts the headless app and serves
`ui/dist` plus `/api/invoke`. The dispatcher in `src-tauri/src/host/e2e_api.rs`
calls the same host helpers as the Tauri commands. Playwright injects a small
`invoke` shim, so the React code runs unchanged.

The shim also answers the dialog, notification, and opener plugins. The pack
upload and the reveal export use real paths on disk.

## Commands

```bash
cargo test -p swap-e2e                  # harness, simulator, classic, async
cargo test -p swap-e2e --test classic_journey
cargo test -p swap-e2e --test async_journey
scripts/e2e.sh                          # every layer and the evidence
pnpm --dir ui e2e                       # browser layer only
```

`scripts/e2e.sh` builds the UI and the bridge, runs both layers, and writes:

- `target/e2e/rust-e2e.log`
- `target/e2e/browser-e2e.log`
- `target/e2e/playwright-report/index.html`
- `target/e2e/screens/`

## CI and release gate

`.github/workflows/e2e.yml` is a reusable workflow. CI calls it on every pull
request. The release workflow calls it on the release tag and blocks both
installer builds until it passes. The job uploads `target/e2e` as evidence.

## Simulator decision

The research step compared Fauxcord, Spacebar, fakediscord, and a new
simulator. Fauxcord is close, but it has no license file and needs a Node
sidecar plus patches for components, modals, and thread events. This project
uses a small Rust simulator with the exact surface that the bot needs. The
trade-off is a simulator contract test on every Twilight upgrade.

## Limits

- The suite does not automate a real Discord user. User tokens and self-bots
  violate the Discord terms.
- The suite does not connect to Discord servers. A change to a Discord payload
  requires a simulator update and a contract test.
- The browser layer tests the host UI, not the native window, the tray, the
  keychain, or the audio preview protocol.
- The simulator URL is local. A real CDN, rate limit, or outage is out of
  scope until a manual bot-side run.
