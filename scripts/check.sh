#!/usr/bin/env bash
# One command that proves the repo is healthy.
# Lean by design: tracker structure, spec gates, UI, Rust, and formal checks.
# No PRD gate.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -d "$HOME/.npm-global/bin" ]; then
  export PATH="$HOME/.npm-global/bin:$PATH"
fi

echo "== tracker =="
python3 tools/project/cli.py doctor
python3 -m unittest discover -s tests -p 'test_tracker_smoke.py' -v

echo "== spec gates =="
node --test scripts/tests/
node scripts/ste-lint.mjs
node scripts/trace-scenarios.mjs --check

echo "== ui =="
if ! command -v pnpm >/dev/null 2>&1; then
  echo "pnpm is required (corepack enable or npm i -g pnpm@12)" >&2
  exit 1
fi
pnpm --dir ui typecheck
pnpm --dir ui test
# src-tauri embeds ui/dist at compile time through generate_context!.
pnpm --dir ui build

echo "== design =="
scripts/design-check.sh

echo "== rust =="
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test -p swap-discord --features test-proxy --test setup_http

echo "== formal =="
skipped=()
if command -v java >/dev/null 2>&1; then
  scripts/formal/check.sh
else
  echo "formal: SKIPPED (no java)"
  skipped+=("formal — no java")
fi

if [ "${#skipped[@]}" -eq 0 ]; then
  echo "== check OK (skipped: none) =="
else
  joined="$(printf '%s; ' "${skipped[@]}")"
  echo "== check OK (skipped: ${joined%; }) =="
fi
