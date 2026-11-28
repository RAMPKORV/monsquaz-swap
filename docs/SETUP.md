# Bot setup

The setup service connects the host to Discord. It validates the bot token,
stores the token, maps two channels, and sends a test direct message.

## Steps

1. The host creates a bot in the Discord Developer Portal.
2. The host pastes the bot token in **Settings** and selects **Validate token**.
3. The app calls `GET /users/@me` and shows the bot name and id.
4. The host selects **Open invite** and adds the bot to the server.
5. The host selects the server and two text channels.
6. The host selects **Save connection**.
7. The host sends a test direct message to a Discord user id.

The app refuses a guild or channel that the bot cannot see. The announcement
channel and the reveal channel must be different.

## Token storage

- The token is stored in the OS keychain with the service
  `dev.monsquaz.swap` and the account `discord-bot-token`.
- The token is never in `host-settings.json`, SQLite, the audit log, a URL, or
  a log line.
- The UI holds the token only while the host types it. The form clears the
  token after a successful save.
- The app shows only the bot identity and a storage label after a save.

### Keychain unavailable

The app never stores plaintext. When the keychain is unavailable, the setup
dialog offers **Use this session only**. This option keeps the token in process
memory only. The bot stops when the app quits. The host must enter the token
again after a restart. The app clears a stale session label at startup.

## Channel mapping

The setup service reads the guild channels from Discord. It accepts text and
announcement channels only. It stores the channel id and the channel name.
Discord is the source of the name.

## Roles

This slice does not map Discord roles. The bot does not need a role. Event
membership comes from `/swap join`. A role gate is a future event setting.

## Test direct message

The test message uses the `DiscordPort` adapter. The adapter opens the direct
message channel, sends one message, and returns the Discord message id. The
app does not send a file in the test.

## Host timezone and release

The app stores the host IANA zone in the host settings. The event copies the
zone when the host creates the event. The async release time is a wall-clock
time in that stored zone. A change to the host settings does not change an
event that already exists. To use a new zone, create a new event after the
change. The event keeps its local release time across DST. The app moves a
spring-forward gap time forward by the gap width and uses the earlier offset
for an ambiguous time.

## Verification

```bash
cargo test -p swap-discord
cargo test -p monsquaz-swap
pnpm --dir ui test
```

The unit tests use a fake directory and a fake token store. A manual live check
needs `DISCORD_TOKEN`, a guild, two text channels, and a Discord user id.

## Rollback

The setup service adds no database migration. To remove the connection:

1. Select **Settings**.
2. Select **Reconnect bot**.
3. Select **Remove connection**.
4. Select **Confirm remove**.

The app deletes the stored token and the non-secret mapping. The Discord bot
and its messages stay on Discord. If the computer has no keychain store, the
removal succeeds. If the keychain is locked, the app clears the in-memory
session but keeps the mapping. Unlock the keychain and select **Remove
connection** again.
