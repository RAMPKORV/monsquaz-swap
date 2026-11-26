# Monsquaz Swap

A cross-platform desktop host for **anonymous, Discord-native music-swap
events**. Participants seed a track (usually a tracker module), receive tracks
they have never worked on, edit them, and pass them on. Nobody learns who
worked on what until the reveal. The result is a collaborative album made by
surprise.

> Status: **spec-first scaffold.** The Tauri shell, Rust core invariant, tracker,
> and release wiring exist. The classic event loop is the next milestone.

## Why

The original Monsquaz Swap ran on a website. Hosting, accounts, uploads, and
notifications were the pain. Discord already has the people, roles, DMs, and
file transport; the host only needs a small local app that runs the bot and
keeps the schedule, files, and credits.

## Stack

- **Tauri 2.12** desktop shell (Windows + Linux; macOS deferred until signing)
- **Rust** for the domain, scheduler, SQLite, file pipeline, and Discord bot
- **Twilight 0.18** for Discord (no privileged intents; DM-first)
- **rusqlite** (bundled SQLite) with a simple migration ladder
- **React 19 + TypeScript + Vite** for the host UI
- **openmpt123** sidecar for module preview (and future rendering)
- **SQLite issue tracker + append-only ledger** in `tools/project/`

## Quick start

Prerequisites: Rust 1.90+, Node 24+, pnpm 12, and the Tauri system
dependencies for your OS (WebKitGTK 4.1 on Linux).

```bash
pnpm install
pnpm tauri dev          # development app
pnpm tauri build        # platform installers
cargo test --workspace
scripts/check.sh        # tracker + Rust + UI checks
```

The first run is a scaffold: a health check proves the Tauri ↔ Rust bridge. The
event engine is tracked as the first milestone.

## Repository map

```
crates/swap-core/     pure domain, invariants, scheduler (no I/O)
src-tauri/            Tauri shell, commands, services, bot supervisor
ui/                   React/TypeScript host UI
specs/features/       hand-written Gherkin acceptance specs
docs/                 PRD, event settings, architecture, research, roadmap
ai-rules/             agent and contributor rules
tools/project/        SQLite tracker CLI + schema
project/ledger/       committed append-only ledger dump
```

## Releases

GitHub Releases, built by GitHub Actions:

- Windows: NSIS installer (unsigned; SmartScreen will warn)
- Linux: `.deb` and AppImage
- macOS: deferred until Apple Developer signing/notarization is funded
- Auto-update: off in v1; installers are published per tag

## License

Code: MIT. Event albums: CC BY-NC-SA 4.0 by default (participants retain
authorship). See `LICENSE` and `docs/PRIVACY.md`.
