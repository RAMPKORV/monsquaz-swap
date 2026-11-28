# Architecture

Public system design. Detailed agent rules live in
`ai-rules/ARCHITECTURE.md` and `ai-rules/SECURITY.md`.

## Context

Monsquaz Swap is a desktop host for anonymous music-swap events. The host runs
a Tauri app that embeds a Discord bot. Participants use Discord DMs and buttons.
The app stores the event in SQLite and the files in a local library. There is no
inbound server and no website.

```text
Host app (Tauri 2.12)
├─ React/TS UI
├─ Rust main process
│  ├─ domain + scheduler
│  ├─ SQLite (rusqlite, WAL)
│  ├─ file store + scrub + preview
│  ├─ Discord bot (Twilight 0.18)
│  └─ openmpt123 sidecar (preview; render later)
└─ OS keychain (bot token)

Discord
├─ guild: announcements, chat, reveal, help
├─ DMs: offers, claims, submissions, reminders
└─ guild commands + buttons
```

## Process model

- Tauri owns the window, tray, single-instance lock, keychain, SQLite writer,
  file store, and command handlers.
- The bot is a supervised Tokio task. A panic restarts the task with backoff.
  A fatal token/permission error stops and shows a fix-it message.
- One writer owns SQLite. Writes run on `spawn_blocking` with short
  transactions. The bot never opens a second writer.
- The openmpt123 renderer is a separate child process. A malformed module can
  kill the renderer, not the app.
- Shutdown cancels the bot, closes the shard, checkpoints WAL, and closes the
  DB.
- A watchdog detects a dead gateway; a periodic DM backfill recovers messages
  sent while the app was offline.

## Crates

| Crate | Responsibility |
|---|---|
| `swap-core` | pure domain, invariants, scheduler, state machines |
| `swap-db` | rusqlite repositories, migrations, ledger projection |
| `swap-engine` | classic event loop, plans, commitments, effects |
| `swap-discord` | Twilight adapter, router, DM/claim delivery |
| `swap-files` | quarantine, hash, validate, scrub, preview, export |
| `swap-sim` | fake Discord/clock/FS, synthetic agents, adversarial runs |
| `swap-acceptance` | cucumber-rs step definitions |
| `src-tauri` | Tauri commands, state, supervisor, UI IPC |

`swap-engine` is synchronous. It commits state and returns Discord effects.
The caller dispatches the effects through `DiscordPort`. This keeps the event
loop deterministic in tests and makes retries explicit.

The current slice ships `swap-core`, `swap-db`, `swap-files`, `swap-discord`,
`swap-engine`, and `src-tauri`. The live gateway, durable delivery loop,
restart recovery, classic production loop, and gateway health are in place. The
async queue, deadline automation, reveal presenter and exports, and remaining
UI screens land with their issues.

## Data flow

A handoff:

```text
scheduler chooses (track, person)
  → app creates an offer + audit entry
  → Discord adapter sends a DM card
  → person taps Claim
  → app checks the seen ledger and custody
  → app sends the scrubbed file
  → person edits and submits (DM attachment)
  → app downloads, hashes, validates, scrubs, stores
  → app records the version and sends a receipt
  → next handoff
```

A participant joins with `/swap join` and one seed attachment in a direct
message. The app stores the seed, creates the track, and enqueues the receipt in
one transaction.

The host drives the classic loop. The host locks the roster, generates and
locks the schedule, opens one round at a time, and triggers the reveal. No timer
opens a round. The reveal checks the active assignments and writes the credits
message in one transaction.

A deadline:

```text
deadline reaches
  → bot sends the participant a prompt
  → bot sends the host a decision card
  → host extends, skips, reassigns, or waits
  → app records the action
  → no silent auto-advance
```

## Manual schedule edits

A host can change one assignment before the event starts. The engine swaps the
two people in that round, validates the whole plan, and writes a new plan
version with a new commitment. The previous version stays. A complete classic
plan has no valid single-cell swap, because every legal pairing is already
used. The UI then reports that no valid swap exists and offers a new draft or a
shorter plan. See `docs/adr/0005-manual-schedule-edits.md`.

## Classic group lookup

The planner applies the group lookup when it generates a classic plan. The
lookup reads the active ready roster. It uses one group up to 9 people, two
groups for 10 to 13, three for 14 to 18 and 23 to 32, and four for 19 to 22
and 33 to 40. The plan stores the effective count and size. A manual group
count and size override the lookup. The soft bounds are 5 to 12 people per
group. A small group, a large group, unequal groups, a cadence outside 3 to 5
phases, or an exhausted policy that cannot fire produces an information
warning. A warning does not block the plan.

At each phase, participants rotate one group forward and tracks stay in their
owner group. The default cadence is K about T divided by 4. A matching never
uses an own track or a track that the person has seen. When a group cannot fill
a phase, the phase uses the global pool, the host sees a warning, and groups
return in the next phase. Before the event starts, the host can repair a draft
with an underfilled or unequal group. The repair merges or re-splits the
groups, writes a new plan version with the reason `group repair`, and records
an audit entry. A routine plan generation does not merge groups.

## Async queue

Async mode uses one global queue and no participant groups. The host starts
the event without a classic plan. Each dispatch wave chooses one track for each
Ready participant who holds no active offer. The choice excludes the person's
own track, every track they have seen, tracks with an active offer, and tracks
they let expire. The score prefers tracks whose owner or past holders the
person has not worked with. The tie-break is deterministic.

An offer contains the track code and the 24-hour claim deadline and no file.
The claim sends the file and marks the pair as seen. A claim reminder fires with
12 hours left. An expired offer returns the track to the pool, does not mark the
pair as seen, pauses the participant for the Ready gate, and blocks a new offer
of that track to that person. The two daily waves, event length caps, and the
exposure gap report are the next slices.

## Simulation

`swap-sim` drives the real classic engine with deterministic participants:
Reliable, Expires, SkipBeforeClaim, and SkipAfterClaim. Each run checks the
schedule, per-round uniqueness, own-track exclusion, no-repeat pairs, seen
semantics, retained versions, claim/file effect counts, reveal completeness,
and opaque outbound names. The fuzz tests feed random containers, hostile
header counts, and malformed WAV chunks. Groups, async, and outage profiles are
next.

## Persistence

- SQLite with WAL, `foreign_keys=ON`, `synchronous=NORMAL`, `busy_timeout`.
- Migrations use `user_version`; never edit an applied migration.
- Tables: events, participants, tracks, versions/files, assignments, seen
  ledger, offers, extensions, audit, DM cursors, pack reports, comments, and
  the delivery outbox.
- Owner/author data is separate from recipient-facing projections.
- Discord IDs are 64-bit snowflakes. Store them as TEXT and pass them through
  unchanged; never parse them into a 53-bit JavaScript number.
- Files live in a user-chosen library. The DB stores hashes, sizes, relative
  paths, and opaque IDs. The UI never sees absolute paths.
- Every command changes state and enqueues its effects in one transaction. The
  app commits the transaction before it sends. The app dispatches pending rows
  after the command and again on startup. A restart or reboot is safe.
- A crash after Discord accepts a send but before the sent mark can duplicate a
  send. The delivery id is stable. A future transport nonce or reconciliation
  step closes that window. The model allows duplicate sends and one ack.

## Discord

The gateway runtime design, health states, backoff, rollback path, and live
smoke command are in `docs/GATEWAY.md`. The bot setup and token storage rules
are in `docs/SETUP.md`.

- Twilight 0.18. Guild commands for instant registration. Persistent DM
  buttons for participant actions.
- Intents: `GUILDS` + `DIRECT_MESSAGES`. No privileged intents.
- Claim-first delivery: an offer is not a file. The file is sent after Claim.
- DM handshake at join stores the DM channel ID. Blocked DMs use a
  host-approved private thread. The host sees the risk.
- Download attachments immediately. Never forward CDN URLs.
- Map Discord errors to human actions. Stagger DMs and respect 429s.
- No prefix commands, no self-bot, no user token.

## Host UI

- React 19 + TypeScript in one shell: Mission control, Audit log, and Settings.
- Tauri commands return typed snapshots. No file bytes, absolute paths, or
  secrets cross IPC.
- The shell shows one primary action per view and names the next host decision.
- The window hides to the tray when a tray is available. Quit is explicit.
- `VITE_DEMO=1` builds deterministic preview data for the screenshot gate.
- Screenshots live in `docs/ui/screens/`; `docs/ui/capture.sh` regenerates them.

## Files

- Allowlist by extension and magic bytes. Default: modules, audio, zip.
- 20 MiB cap for bot redistribution. Larger files use a link download or a
  host import.
- Format changes are forbidden by default.
- Module-aware scrub zeroes the song title, message, tracker name, instrument,
  sample, and file names. A malformed header is refused, not guessed.
- A/B render compares the WAV `data` chunks, not the container metadata.
  `openmpt123` runs in a temporary directory with a 30 second wall-clock limit.
  A preview is 15 seconds. The delivery blob and the untouched original are
  both content-addressed. SQLite stores both hashes. The original never leaves
  the host. The renderer is discovered next to the app binary, then on PATH;
  packaging the static sidecar remains.
- No auto-open. Parse in a child process with time and memory limits.
- Preview WAVs are served through a scoped custom protocol, not a broad asset
  directory.
- Album export writes a ZIP, credits, manifest, README, cover, and runbook.
  Large exports are host-mediated.

## Anonymity

- Separate owner/contributor tables. Recipient queries never join them.
- Participant DTOs pass a redaction assertion.
- Outbound filenames are opaque. Host storage is readable.
- Public progress is status + countdown. No per-person counts before deadline.
- Visible schedule exposes assignments only.
- Comments are host+reveal only. Skips are host-only.
- Reveal order is HMAC-sorted. No file hashes before reveal.
- Stream-safe mode hides the mapping during a stream.
- The host can see the mapping. The rules say so.

## Data control

- `swap-privacy` exports one participant ZIP: `participant.json`, a README, and
  the original files that participant submitted. It never includes another
  participant's rows or files.
- Deletion replaces the Discord user ID and display name with a per-event
  salted tombstone, removes future plan rows, keeps past credits and event
  files, and appends a hash-chained audit entry. An active assignment blocks
  deletion until the host resolves it.
- Diagnostics redaction takes the token, known paths, and known user IDs and
  replaces them with typed placeholders.
- `swap-db::backup` writes a VACUUM INTO snapshot and a manifest before each
  event state transition. It also writes one daily backup.
- Each backup stores files by SHA-256 and reuses a file from an earlier backup.
- Retention keeps the newest 20 state-change and manual backups and the newest
  14 daily backups. It removes each file that no kept backup refers to.
- A restore checks the manifest, the database hash, the SQLite integrity check,
  and every file hash. It moves the replaced database to a recovery folder.
- The host UI provides the folder choice, the backup history, the restore
  wizard, the last-failure status, and the delete confirmation.
- `swap-db::writer` records a computer id and a check time in the database and
  the backup folder. A second computer with a recent check cannot start the
  bot. The bot lock clears 5 minutes after the last check.
- `BackupRuntime` keeps the last backup failure and the time this computer lost
  the bot lock. Every backup command returns host copy with a stable code.

## Reveal package

- `reveal_package` builds a host-only read model from the event, assignments,
  files, titles, and comments. It never includes file bytes or storage paths.
- The album order is `sort(HMAC-SHA256(event seed, track code), track code)`.
  Submission time never affects the order.
- Migration 008 adds `track_titles` and `track_comments`. A redacted comment is
  excluded from the package. The host adds comments before the reveal.
- The final holder names the track with `/swap title`. If the final holder
  does not, the bot asks the previous submitter. The ask names the track code
  and the round in which that person submitted the track. That person uses
  `/swap title` with the track code. The final holder keeps priority until a
  title exists. The host can set or correct the title before the reveal.
- The presenter plays audio through the `swap-preview` protocol. The URL
  contains an opaque file id. Module files render to cached WAV through
  openmpt123. Byte ranges are supported.
- Stream-safe mode replaces names with role labels in the UI. It does not
  change the data or the published reveal.
- Final-file uploads, schedule disclosure, the ZIP package, and keepsakes stay
  in MSW-025.

## Security

- Token in the OS keychain. Never in the UI, logs, SQLite, or git.
- Tauri capabilities limited to `core:default`, `notification:default`, and
  explicit needs. No `fs`, `shell`, `http`, or `sql` plugins.
- File bytes and absolute paths never cross IPC.
- Validate every command argument. Return typed errors with no paths/secrets.
- CSP hardening is a pre-release task; the scaffold uses `null`.
- Single-instance lock; one bot session per token.

## Release

- GitHub Actions: tracker, UI, Rust, Tauri smoke on Ubuntu + Windows.
- GitHub Releases: Windows NSIS, Linux `.deb` + AppImage.
- Windows unsigned; SmartScreen warning documented.
- macOS deferred until Apple signing is funded.
- Auto-update off in v1. Update keys are not created.
- `SHA256SUMS` + GitHub attestations are attached.

## Open decisions

- Album/keepsake delivery for files above 20 MiB (host-mediated link).
- Exact retention period and backup expiry.
- openmpt123 static build and sidecar packaging.
- CSP hardening and final Tauri capability list.
