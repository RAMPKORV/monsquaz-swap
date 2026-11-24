# 0001 — Stack selection

- **Status:** accepted
- **Date:** 2026-11-24
- **Decision code:** D-001 (`ai-rules/DECISIONS.md`)
- **PRD section:** constraints

## Context

The app must run a Discord bot, keep a local SQLite database, manage files, and
give a non-technical host a GUI on Windows and Linux. It must not need a server.
The owner prefers Rust. The app must install in minutes and stay small enough
for a community download.

## Options considered

| Option | Pros | Cons | Risk |
|---|---|---|---|
| **Tauri 2 + Rust + React/TS** | Small installers, one runtime, Rust core, strong security boundary | Three WebView engines, Linux WebKitGTK packaging | Medium |
| Electron + TypeScript | Fastest UI iteration, one Chromium | ~100 MB install, no Rust backend, larger memory | Medium |
| Tauri + Node sidecar | Rust shell with JS bot | Two runtimes, two IPC hops, sidecar packaging | High |
| Tauri + Leptos/Yew | All Rust | Slower UI work, fewer components, WASM debugging | High |

## Decision

Use **Tauri 2.12** with a **Rust** backend and a **React 19 + TypeScript** UI.
Use **Twilight 0.18** for Discord, **rusqlite** (bundled) for SQLite, and
**openmpt123** as a sidecar for module preview and later rendering.

The core domain, scheduler, persistence, file pipeline, and Discord adapter are
Rust. The UI is TypeScript. No Node runtime ships. No Electron.

## Consequences

- Installers are small. One runtime runs the bot and the app.
- The Rust core is pure and testable without Discord, Tauri, or the network.
- The UI must be tested on WebView2, WebKitGTK, and later WKWebView.
- Linux packaging needs WebKitGTK 4.1 and FUSE2 for AppImage. The `.deb` is the
  primary Linux artifact.
- The bot runs in the Tauri process and is supervised. A separate bot sidecar is
  not used.
- Replacing the UI framework later is possible because the IPC boundary is
  typed. Replacing Tauri would be expensive.

## Verification

- The scaffold builds on Ubuntu and Windows in CI.
- `pnpm tauri build --debug --bundle deb` produced a working `.deb`.
- The first draft release includes Windows NSIS and Linux `.deb`/AppImage.
- The Tauri smoke job passes on both platforms.

## Revisit triggers

- A Linux WebKitGTK bug blocks a core view on a supported distro.
- Tauri 3 stabilizes with a feature the app needs.
- macOS signing is funded and WKWebView needs a different approach.
- The UI needs a native capability that Tauri cannot provide.
