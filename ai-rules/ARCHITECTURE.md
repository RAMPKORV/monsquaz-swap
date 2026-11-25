# Architecture

Target architecture for the Tauri + Rust app. It is deliberately hexagonal:
the domain does not know about Discord, SQLite, Tauri, or the filesystem.

## System context

```
Host desktop app (Tauri 2.12)
├─ React/TS UI (WebView)
├─ Tauri main process (Rust)
│  ├─ app services / use cases
│  ├─ SQLite (rusqlite, single writer, WAL)
│  ├─ file store (content-addressed + human views)
│  ├─ Discord bot supervisor (Tokio task)
│  └─ openmpt123 sidecar (preview; later render)
└─ OS keychain (bot token)

Discord
├─ guild channels: announcements, chat, reveal, help
├─ DMs: offers, claims, submissions, reminders
└─ guild slash commands + persistent buttons
```

No inbound ports. The bot is outbound-only (WSS + HTTPS). Discord is a
transport and identity provider, not the system of record.

## Process model

- **Tauri main process:** owns the window, tray, single-instance lock, SQLite
  writer, file store, keychain access, updater (off), and command handlers.
- **Bot supervisor:** a Tokio task on Tauri's runtime. It runs the Twilight
  shard, backfills DMs, and reconnects with exponential backoff. A bot panic
  must not kill the UI; release builds keep `panic = "unwind"` and the
  supervisor restarts the task with a backoff and a fatal-error stop.
- **No separate bot sidecar.** One process keeps packaging, token handling, and
  logs simple. The openmpt123 renderer is a child process (crash isolation).
- **Single writer:** all DB writes go through a `Db` handle (mutex + short
  transactions) on `spawn_blocking`. Do not open a second writer from the bot.
- **Shutdown:** cancel the bot, close the shard with 1000, wait briefly,
  checkpoint WAL, close the DB, then exit.
- **Sleep/wake:** a watchdog detects missed heartbeats; a periodic DM backfill
  covers timeouts. Do not rely on gateway resume for long outages.

## Crates

Target Cargo workspace:

| Crate | Responsibility | Rules |
|---|---|---|
| `swap-core` | domain types, invariants, scheduling, state machines | pure, sync, no I/O, no async, no Discord/Tauri |
| `swap-db` | rusqlite repositories, migrations, ledger projection | one writer, `user_version`, in-memory tests |
| `swap-engine` | event commands, classic loop, plan commitment, effects | sync, no transport, no Tauri; returns effects |
| `swap-discord` | Twilight adapter contract, command router, DM/claim delivery | maps Discord payloads to domain DTOs; no domain rules |
| `swap-files` | quarantine, hashing, validation, scrub, preview, export | never trusts filenames; no auto-open |
| `swap-sim` | fake Discord/clock/FS, synthetic participants, adversarial scenarios | deterministic seeds; used by tests and host dry-run |
| `swap-acceptance` | cucumber-rs step definitions | calls app services through ports |
| `src-tauri` | Tauri commands, state, supervisor, UI IPC | thin; validates DTOs; no business rules |

The current slice includes `swap-core`, `swap-db`, `swap-files`,
`swap-discord`, `swap-engine`, and `src-tauri`. Add the live gateway, async
queue, scrub/preview, simulator, and UI as the corresponding issues start.

## Ports and adapters

```rust
trait DiscordPort { /* send DM, send file, edit message, offer, fetch DMs */ }
trait Repo { /* events, participants, tracks, assignments, ledger, audit */ }
trait FileStore { /* ingest, get, preview, export, hash */ }
trait Renderer { /* render module to WAV, later album audio */ }
trait Clock { /* now, sleep, wall-clock conversion */ }
trait IdGen { /* opaque track codes, event IDs */ }
```

Domain services return effects (`SendOffer`, `SendFile`, `EditMessage`,
`Persist`, `Emit`) rather than performing I/O. This is the main testability
seam: 90% of the logic is plain data.

## Persistence

- SQLite via `rusqlite` with the `bundled` feature; WAL, `foreign_keys=ON`,
  `synchronous=NORMAL`, `busy_timeout`.
- Migrations with `rusqlite_migration` (or a tiny `user_version` runner); never
  edit an applied migration. Back up before migrating.
- Tables (target): events, participants, tracks/lineage, versions/files,
  assignments, seen ledger, offers, extensions, audit, DM cursors, settings,
  pack checks, comments/questionnaires.
- Separate author/owner table from participant-facing projections. A recipient
  query never joins it.
- Discord snowflakes are 64-bit; persist as INTEGER, serialize to the UI as
  **strings** (JS precision).
- Files live outside the DB; the DB stores hashes, sizes, relative paths, and
  opaque IDs. The UI never sees absolute paths.
- Backups: checkpoint + copy / `VACUUM INTO`; restore wizard; integrity check.

## Discord adapter

- **Twilight 0.18** (stable, active, data-oriented). Guild-scoped commands for
  instant registration; participant DM actions use persistent buttons.
- Intents: `GUILDS` + `DIRECT_MESSAGES` only; **no privileged intents**. DMs
  with the app are exempt from `MESSAGE_CONTENT`. Verify at startup; fail closed
  with a fix-it message.
- DM channel: `POST /users/@me/channels` returns the existing channel for a
  known user; store the channel ID at the join handshake. Backfill messages
  after the last seen ID on reconnect.
- Offers: claim-first. The bot sends an offer card (track code, deadline,
  buttons); the file is sent only after Claim. This protects the seen ledger.
- Submission: DM attachment (primary); slash-command attachment (fallback).
  Download immediately with a byte cap and hash.
- Errors: map Discord codes (`50007` DMs closed, `50278` no mutual guild,
  `40003` too many DMs) to human actions. Stagger DMs and respect 429s.

## File pipeline

```
attachment → download (cap + hash) → quarantine → magic/extension check
          → parser/validation → metadata scan/scrub → A/B render
          → store original + scrubbed + preview → DB row → receipt
```

- Never trust the client filename; generate the stored name.
- Default allowlist: modules (.it/.mptm/.xm/.mod/.s3m/.mtm/.669), audio
  (.wav/.flac/.mp3/.ogg), .zip. Per-event override.
- Format changes are forbidden by default.
- 20 MiB cap for bot redistribution; external-link/host-import fallback.
- Module scrub: targeted fixed-offset edits, then A/B render; if the render
  differs, fail closed (keep the original and warn; do not distribute a
  corrupted module).
- openmpt123 sidecar for preview/render; serve WAV through a scoped custom
  protocol, not raw filesystem paths.
- Export: album ZIP, credits, manifest, cover, comments, runbook, keepsakes;
  host-mediated for files over the Discord limit.

## Anonymity boundary

- Separate `track_owner` and `track_contributors` data from public DTOs.
- A serializer/projection layer asserts that participant-facing payloads have
  no `owner_id`, `author_name`, `original_filename`, `title`, `comments`, or
  full schedule before reveal.
- Public progress is status + countdown, not per-person counts before the
  deadline. Aggregates use a k-threshold.
- No file hashes or CDN URLs on public surfaces before reveal.
- Reveal order is HMAC-sorted, independent of submission time.
- Stream-safe host mode hides the mapping and redacts IDs.

## UI

- React 19 + TypeScript + Vite; Tailwind/shadcn can be added when the design
  system issue starts. The scaffold uses plain CSS to keep the shell light.
- No business rules in the UI. It calls typed Tauri commands and renders DTOs.
- Views: setup wizard, mission control, event wizard, participant manager,
  schedule planner (matrix + graph), round board, file browser, audit, reveal
  presenter, export, settings/logs, dry-run simulator.
- Accessibility: WCAG AA, keyboard navigation, no color-only status, reduced
  motion.
- Playful retro visual language, but calm and legible for long admin sessions.

## Tauri security baseline

- `contextIsolation`, `sandbox`, no Node in the WebView, strict capabilities.
- No `fs`, `shell`, `http`, or `sql` plugins; all privileged work in commands.
- Validate every command argument; return typed `UiError` with no paths or
  secrets.
- The bot token is in the OS keychain (`keyring`); Linux without Secret Service
  falls back to re-entry or an explicit less-secure warning.
- Serve previews through a scoped custom protocol; never expose a broad asset
  directory.
- CSP is currently `null` in the scaffold; hardening is a `full` ceremony issue.

## Release

- Windows NSIS (unsigned; SmartScreen documented) + Linux `.deb`/AppImage.
- macOS deferred until Apple Developer signing/notarization is funded.
- GitHub Releases via Actions; auto-update off in v1; update config ready later.
- openmpt123 is bundled as a sidecar per platform when preview lands.

## Open architecture decisions

- Async group model (per-person soft migration vs no groups vs hybrid) — see
  `docs/PRD.md` open questions.
- Delivery of large album/keepsake ZIPs (host-mediated link vs split DMs).
- Whether to expose a read-only local HTTP preview server (current preference:
  scoped Tauri custom protocol, no server).
- Module renderer bundling strategy (openmpt123 sidecar now; WASM fallback).
