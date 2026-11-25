#!/usr/bin/env bash
# Capture the host UI review matrix with deterministic demo data.
# Requires: pnpm, chromium, curl. Run from anywhere in the repo.
set -euo pipefail
cd "$(dirname "$0")/../.."

if [ -d "$HOME/.npm-global/bin" ]; then
  export PATH="$HOME/.npm-global/bin:$PATH"
fi

CHROMIUM="${CHROMIUM:-chromium}"
PORT="${PORT:-4173}"
OUT="docs/ui/screens"
mkdir -p "$OUT"

VITE_DEMO=1 pnpm --dir ui build >/dev/null
if curl -s -o /dev/null "http://localhost:$PORT/"; then
  echo "Port $PORT is already in use. Stop that server first." >&2
  exit 1
fi

( cd ui && exec ./node_modules/.bin/vite preview --port "$PORT" --strictPort ) >/tmp/monsquaz-ui-preview.log 2>&1 &
PREVIEW_PID=$!
trap 'kill "$PREVIEW_PID" 2>/dev/null || true' EXIT

for _ in $(seq 1 40); do
  if curl -s -o /dev/null "http://localhost:$PORT/"; then
    break
  fi
  sleep 0.25
done

BUILT_JS="$(grep -o 'index-[A-Za-z0-9_-]*\.js' ui/dist/index.html | head -1)"
SERVED_HTML="$(curl -s "http://localhost:$PORT/")"
if [[ "$SERVED_HTML" != *"$BUILT_JS"* ]]; then
  echo "Preview server is not serving this checkout's build." >&2
  exit 1
fi

PROFILE="$(mktemp -d)"
trap 'kill "$PREVIEW_PID" 2>/dev/null || true; rm -rf "$PROFILE"' EXIT

shot() {
  local name="$1"
  local query="$2"
  local width="$3"
  local height="$4"
  rm -rf "$PROFILE"
  mkdir -p "$PROFILE"
  "$CHROMIUM" \
    --headless=new \
    --no-sandbox \
    --disable-gpu \
    --hide-scrollbars \
    --run-all-compositor-stages-before-draw \
    --user-data-dir="$PROFILE" \
    --window-size="$width,$height" \
    --virtual-time-budget=4000 \
    --screenshot="$OUT/$name.png" \
    "http://localhost:$PORT/?demo=1&static=1&$query" >/dev/null 2>&1
  echo "wrote $OUT/$name.png"
}

shot mission-first-run-dark-1280 "view=mission&state=empty" 1280 800
shot mission-loading-dark-1280 "view=mission&state=loading" 1280 800
shot mission-populated-dark-1280 "view=mission" 1280 800
shot mission-error-dark-1280 "view=mission&state=error" 1280 800
shot mission-long-dark-1280 "view=mission&state=long" 1280 800
shot mission-bot-offline-dark-1280 "view=mission&state=offline" 1280 800
shot mission-populated-light-1280 "view=mission&theme=light" 1280 800
shot mission-populated-dark-1024 "view=mission" 1024 700
shot mission-populated-light-1024 "view=mission&theme=light" 1024 700
shot mission-populated-dark-zoom200 "view=mission" 640 400

shot settings-first-run-dark-1280 "view=settings&state=empty" 1280 800
shot settings-populated-dark-1280 "view=settings" 1280 800
shot settings-populated-light-1280 "view=settings&theme=light" 1280 800
shot settings-error-dark-1280 "view=settings&state=error" 1280 800
shot settings-long-dark-1280 "view=settings&state=long" 1280 800
shot settings-populated-dark-1024 "view=settings" 1024 700
shot settings-full-dark-1280x2000 "view=settings" 1280 2000
shot settings-full-light-1280x2000 "view=settings&theme=light" 1280 2000
shot settings-delete-dialog-dark-1280 "view=settings&dialog=delete" 1280 800

shot audit-empty-dark-1280 "view=audit&state=empty" 1280 800
shot audit-loading-dark-1280 "view=audit&state=loading" 1280 800
shot audit-populated-dark-1280 "view=audit" 1280 800
shot audit-populated-light-1280 "view=audit&theme=light" 1280 800
shot audit-error-dark-1280 "view=audit&state=error" 1280 800
shot audit-long-dark-1280 "view=audit&state=long" 1280 800
shot audit-dense-dark-1280 "view=audit&state=dense" 1280 800
shot audit-populated-dark-1024 "view=audit" 1024 700
shot audit-populated-dark-zoom200 "view=audit" 640 1000
