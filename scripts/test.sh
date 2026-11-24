#!/usr/bin/env bash
# Fast test entry point used by scripts/check.sh and CI.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -d "$HOME/.npm-global/bin" ]; then
  export PATH="$HOME/.npm-global/bin:$PATH"
fi

echo "== tracker smoke =="
python3 -m unittest discover -s tests -p 'test_tracker_smoke.py' -v

if command -v pnpm >/dev/null 2>&1; then
  echo "== ui =="
  pnpm --dir ui typecheck
  pnpm --dir ui test
  # src-tauri embeds ui/dist at compile time.
  pnpm --dir ui build
else
  echo "pnpm not found; UI checks skipped" >&2
fi

echo "== rust workspace =="
cargo test --workspace
