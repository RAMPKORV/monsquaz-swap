#!/usr/bin/env bash
# Run every end-to-end layer and keep the evidence in target/e2e.
#
# Layers:
#   1. Rust: harness, simulator, classic journey, async journey.
#   2. Browser: the real React UI, the HTTP bridge, and the simulator.
#
# Requirements: pnpm, cargo, openmpt123, a Chromium build (or Playwright's).
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -d "$HOME/.npm-global/bin" ]; then
  export PATH="$HOME/.npm-global/bin:$PATH"
fi

if ! command -v openmpt123 >/dev/null 2>&1; then
  echo "openmpt123 is required. Install it, then run this script again." >&2
  exit 1
fi

OUT="target/e2e"
mkdir -p "$OUT/screens"
rm -f "$OUT"/*.log "$OUT/summary.md"
STARTED="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

echo "== build the UI =="
pnpm --dir ui build >"$OUT/ui-build.log" 2>&1

echo "== build the browser bridge =="
cargo build -p swap-e2e --features bridge --bin e2e-bridge >"$OUT/bridge-build.log" 2>&1

echo "== rust end-to-end journeys =="
cargo test -p swap-e2e >"$OUT/rust-e2e.log" 2>&1

echo "== browser end-to-end journeys =="
MSW_E2E_BRIDGE_BIN="$PWD/target/debug/e2e-bridge" \
  pnpm --dir ui e2e >"$OUT/browser-e2e.log" 2>&1

FINISHED="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
cat >"$OUT/summary.md" <<SUMMARY
# E2E run

- Started: $STARTED
- Finished: $FINISHED
- Rust journeys: pass (\`rust-e2e.log\`)
- Browser journeys: pass (\`browser-e2e.log\`)
- Playwright report: \`playwright-report/index.html\`
- Screenshots: \`screens/\`
SUMMARY

echo
echo "E2E passed."
echo "Report: $OUT/playwright-report/index.html"
echo "Logs:   $OUT/rust-e2e.log, $OUT/browser-e2e.log"
