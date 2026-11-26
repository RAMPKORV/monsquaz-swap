#!/usr/bin/env bash
# Build the pinned Refract CLI, start the demo host, and run the design gate.
# Usage: scripts/design-check.sh [--update-baseline] [--baseline-issue MSW-###]
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -d "$HOME/.npm-global/bin" ]; then
  export PATH="$HOME/.npm-global/bin:$PATH"
fi

if ! command -v pnpm >/dev/null 2>&1; then
  echo "pnpm is required for the design gate" >&2
  exit 1
fi

CACHE="$(scripts/install-refract.sh)"
BASE_PORT="${DESIGN_PORT:-4199}"
PORT=""
for offset in 0 1 2 3 4 5 6 7 8 9; do
  candidate=$((BASE_PORT + offset))
  if ! curl -s -o /dev/null "http://127.0.0.1:$candidate/"; then
    PORT="$candidate"
    break
  fi
done
if [ -z "$PORT" ]; then
  echo "design: no free port near $BASE_PORT" >&2
  exit 1
fi
LOG="${TMPDIR:-/tmp}/monsquaz-design-preview-$PORT.log"

(
  cd ui
  VITE_DEMO=1 exec ./node_modules/.bin/vite \
    --host 127.0.0.1 \
    --port "$PORT" \
    --strictPort
) >"$LOG" 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

for _ in $(seq 1 60); do
  if curl -s -o /dev/null "http://127.0.0.1:$PORT/"; then
    break
  fi
  sleep 0.5
done

if ! curl -s -o /dev/null "http://127.0.0.1:$PORT/"; then
  echo "design: the demo server did not start. See $LOG" >&2
  exit 1
fi

REFRACT_CACHE="$CACHE" DESIGN_BASE_URL="http://127.0.0.1:$PORT" \
  node scripts/design-check.mjs "$@"
