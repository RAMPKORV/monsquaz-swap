# Research notes

This file records the evidence behind the design. It is a summary, not a
promise. Verify a claim before you rely on it. All sources were checked on
2026-11-24.

## Method

We used six research passes:

1. Discord platform limits and bot behavior.
2. Scheduling, Latin squares, module formats, and metadata.
3. Desktop stack comparison (Electron vs Tauri vs alternatives).
4. Quality-of-life and failure-mode brainstorm.
5. Availability and async queue design.
6. Adversarial and simulation review.

We also ran two group/async scheduling simulations and one release-pipeline
study. The simulations are design evidence, not user research.

## Discord findings

| Finding | Value | Source |
|---|---|---|
| Default upload limit | 20 MiB (raised from 10 MiB in 2026) | [File Attachments FAQ](https://support.discord.com/hc/en-us/articles/25444343291031) |
| Interaction response | 3 seconds to acknowledge; 15-minute token | [Interactions](https://docs.discord.com/developers/interactions/receiving-and-responding) |
| DM content without `MESSAGE_CONTENT` | DMs with the app are an exception; the bot can read them | [Privileged intents](https://docs.discord.com/developers/events/gateway#message-content-intent) |
| DM channel recovery | `POST /users/@me/channels` returns the existing DM channel | [Create DM](https://docs.discord.com/developers/resources/user#create-dm) |
| DM failure codes | `50007` DMs closed; `50278` no mutual guild; `40003` too many DMs | [JSON errors](https://docs.discord.com/developers/topics/opcodes-and-status-codes) |
| Privileged intents | Not required for guilds + DMs + slash-command attachments | [You Might Not Need a Privileged Intent](https://docs.discord.com/developers/gateway/you-might-not-need-a-privileged-intent) |
| CDN URLs | Signed and time-limited | [Reference](https://docs.discord.com/developers/reference#signed-attachment-cdn-urls) |
| Ephemeral attachments | Garbage-collected; not durable | [Attachment object](https://docs.discord.com/developers/resources/message#attachment-object) |
| Guild vs global commands | Guild commands register instantly; global may propagate slowly | [Application commands](https://docs.discord.com/developers/interactions/application-commands) |
| Private threads | Visible to `MANAGE_THREADS` (server admins) | [Threads](https://docs.discord.com/developers/topics/threads) |
| Bot token policy | Keep encrypted; never embed in an open-source project | [Developer ToS](https://support-dev.discord.com/hc/en-us/articles/8562894815383) |
| Self-bots | Prohibited | [OAuth2](https://docs.discord.com/developers/topics/oauth2) |

Consequences:

- Use DM-first, claim-first delivery. Do not use ephemeral attachments as
  storage.
- Do not request `MESSAGE_CONTENT`. Do not use prefix commands.
- Download attachments immediately. Never forward a CDN URL.
- Use guild commands and DM buttons. Do not expect guild commands in DMs.
- A private-thread fallback is visible to server admins. Disclose this.
- BYO token, stored in the OS keychain, never in the repository.

## Desktop stack

| Option | Verdict |
|---|---|
| **Tauri 2.12 + Rust + React/TS** | Chosen. Small installers, Rust backend, one runtime. |
| Electron + TypeScript | Rejected. ~100 MB Chromium, no Rust. |
| Tauri + Node sidecar | Rejected. Two runtimes and two IPC hops. |
| Tauri + Leptos/Yew | Rejected for v1. Slower UI work and a smaller component ecosystem. |
| Wails / Flutter / .NET | Rejected. Wrong language or weak Discord ecosystem. |

Key facts:

- Tauri 2.12.7, Rust 1.90+, WebView2 on Windows, WebKitGTK 4.1 on Linux,
  WKWebView on macOS.
- Linux AppImage needs FUSE2; `.deb` is the primary Linux artifact.
- Windows unsigned builds show SmartScreen. macOS unsigned builds are
  effectively broken for non-technical hosts, so macOS is deferred.
- Tauri updater uses Minisign; v1 has no updater.
- Tauri capabilities replace a broad Node API. Use `core:default` plus only
  what the app needs. No `fs`, `shell`, `http`, or `sql` plugins.

Rust choices:

- **Twilight 0.18** for Discord. Stable, active, data-oriented, current models.
  Serenity 0.12 is stable but older; Serenity `next` is unreleased.
- **rusqlite** (bundled) + `rusqlite_migration`. Single writer, WAL,
  `spawn_blocking`, in-memory tests.
- **keyring** for the bot token. Stronghold is deprecated.
- **React 19 + TypeScript + Vite** for the host UI.
- **openmpt123** as a sidecar for module preview and future rendering. It is
  BSD-3-Clause and crash-isolated. Do not use the abandoned `openmpt` crate.

Sources: [Tauri releases](https://tauri.app/release/tauri/),
[Tauri prerequisites](https://tauri.app/start/prerequisites),
[Tauri updater](https://tauri.app/plugin/updater/),
[Twilight](https://github.com/twilight-rs/twilight/releases),
[rustqlite](https://crates.io/crates/rusqlite),
[rusqlite_migration](https://docs.rs/rusqlite_migration),
[keyring](https://crates.io/crates/keyring),
[libopenmpt](https://lib.openmpt.org/libopenmpt/).

## Scheduling

The schedule is a partial Latin square:

- rows are rounds;
- columns are tracks;
- symbols are participants;
- no symbol repeats in a row or column;
- the owner is excluded from the column.

N-1 rounds is the complete classic shape for a fixed group. The planner has
three seeded strategies:

- `balanced` (default): row-by-row min-cost matching with a penalty for
  participant pairs that already met, especially in the last two rounds;
- `random_matching`: randomized row-by-row perfect matching, a partial Latin
  rectangle and a bipartite edge coloring;
- `cyclic`: a cyclic Latin square with permuted rows and a permuted offset
  sequence, which permutes columns and symbols together. It falls back to
  matching when each participant does not own one track.

All strategies avoid the predictability of a bare cyclic shift and are
deterministic for the same input and seed.

For skips and fill-ins, use bipartite matching and, when needed, min-cost flow.
A Hall violator certificate explains infeasibility. Use CP-SAT only for small
exact repairs. Never relax the no-repeat or own-track rules to rescue a slot.

Group simulation (classic):

- Splitting adds fun only through cross-group surprise. Core metrics tie with a
  single group.
- 1 group for 9 or fewer; 2 for 10–13; 3 for 14–18 and 23–32; 4 for 19–22 and
  33–40.
- Group size 5–12. Rotate participants by default. K≈T/4 (3–5 phases).
- "Swap when exhausted" often never fires for large groups. Warn.
- Rotating both people and tracks is not additive.
- Merge/re-split is a repair, not a routine phase.
- Group membership is hidden from participants.

Async simulation:

- Formal groups do not help. Per-person migration adds p95 wait and reduces
  cross-group handoffs. Use one global queue with a social-diversity objective.
- 24h claim window beats 12h: fewer expiries, faster event.
- Two dispatch waves in one daily window lower p95 wait.
- Full edit completeness is capped at ~93–95% because claim-then-skip counts as
  seen. Use exposure completeness and report gaps.
- Full exposure at 40 participants takes ~80–90 days. Default N-1 for 12 or
  fewer; 6–8 rounds above 12.
- Protect against repeated unclaimed offers: Ready gate, strike/cooldown,
  no same-track re-offer after expiry, gap report.
- Fill-ins recover stranded tracks, not a leaver's missing edits.

Sources: [Latin square](https://en.wikipedia.org/wiki/Latin_square),
[Latin rectangle](https://en.wikipedia.org/wiki/Latin_rectangle),
[Kőnig's theorem](https://en.wikipedia.org/wiki/K%C5%91nig%27s_theorem_(graph_theory)),
[Social golfer problem](https://en.wikipedia.org/wiki/Social_golfer_problem),
[CP-SAT](https://developers.google.com/optimization/cp/cp_solver),
[RFC 8785](https://www.rfc-editor.org/rfc/rfc8785).

## Files and metadata

Tracker modules can leak identity through:

- song names, instrument names, sample names, and sample filenames;
- song messages, comments, edit history, and timestamps;
- `AUTH`, `PWC`, `VWC`, plugin names, and external sample paths;
- tracker version fields and edit history;
- the filename, mtime, archive timestamps, and upload time;
- the music itself (style, voice, in-jokes).

Use a targeted, format-aware scrub for IT/XM/S3M/MOD/MPTM. Keep an untouched
original in a host-only vault. Render before and after; reject the scrub if the
audio changes. Do not claim perfect scrubbing for arbitrary formats.

`libopenmpt`/`openmpt123` is BSD-3-Clause and read-only. `xmrs` is MIT and can
parse. `lofty` strips audio tags. FFmpeg is LGPL if built without GPL features.

Sources: [IT format](https://wiki.openmpt.org/Development:_Formats/IT),
[OpenMPT extensions](https://wiki.openmpt.org/Development:_OpenMPT_Format_Extensions),
[XM format](https://wiki.openmpt.org/Development:_Formats/XM),
[libopenmpt](https://lib.openmpt.org/libopenmpt/),
[openmpt123](https://man.archlinux.org/man/extra/libopenmpt/openmpt123.1.en),
[xmrs](https://crates.io/crates/xmrs),
[lofty](https://crates.io/crates/lofty).

## Prior art

No project implements the full anonymous tracker-album swap over Discord. The
closest ideas:

- [purarue/filmswap](https://github.com/purarue/filmswap) — anonymous media swap,
  phase machine, SQLite, minimal Discord permissions.
- [amberosia/SecretSantaBot2022](https://github.com/amberosia/SecretSantaBot2022) —
  receive/store/output files for an art exchange.
- [darktohka/confessions-bot](https://github.com/darktohka/confessions-bot) —
  anonymous submissions through private threads.
- [jolicode/secret-santa](https://github.com/jolicode/secret-santa) — mature
  matching and exclusions.
- [OpenMPT](https://github.com/OpenMPT/openmpt) — format reference and player.
- [ZDBF Launcher](https://github.com/TheHolyOneZ/zdbf-launcher) — Tauri app that
  runs a Discord bot.
- [gptvoice-discord](https://github.com/mutedvector/gptvoice-discord) — Tauri +
  Rust bot runtime.

The event format itself is documented by the Monsquaz Collective:
[Swap Album 8](https://monsquazcollective.bandcamp.com/album/monsquaz-swap-album-8-the-gachimuchi-edition),
[Swap Album 9 notes](https://blog.jonasbengtson.se/monsquaz-swap-album-9).

## Discord transport testing

No official Discord bot emulator exists. Twilight supports a mock HTTP host
through its proxy setting. `crates/swap-discord/tests/setup_http.rs` uses it
with Wiremock for the setup, DM, and file serializers. Run it with
`cargo test -p swap-discord --features test-proxy --test setup_http`. Fake gateway sessions
cover the supervisor. Gateway identification, rate limits, and Discord policy
still need a live test guild and a human.

## Open evidence gaps

- The fun metric and group weights are a design proxy, not validated human
  preference. Calibrate after two real events.
- The async simulation uses synthetic editing times. Real hosts should measure
  completion and wait time.
- The group simulation assumes uniform social mixing. Real cliques may change
  the value of cross-group surprise.
- The 20 MiB limit can change. Probe it at runtime; do not hardcode.
- The Tauri Linux/WebKitGTK behavior on every distro needs a clean-VM test.
- The openmpt123 static build must be tested with real module fixtures before
  preview ships.
- Non-module audio and zip files are not metadata-scrubbed. ID3, Vorbis, and WAV
  LIST tags can carry identity. The allowlist decision needs a product review.
