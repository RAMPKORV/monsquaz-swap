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
  if [ -n "${ONLY:-}" ] && [[ "$name" != *"$ONLY"* ]]; then
    return
  fi
  rm -rf "$PROFILE"
  mkdir -p "$PROFILE"
  if [[ "$name" == *-light-* ]]; then
    THEME=light
  else
    THEME=dark
  fi
  local url="http://localhost:$PORT/?demo=1&static=1&theme=$THEME&$query"
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
    "$url" >/dev/null 2>&1
  if [[ "$name" == plan-* && "$name" != *-error-* ]]; then
    local dom
    dom="$("$CHROMIUM" \
      --headless=new \
      --no-sandbox \
      --disable-gpu \
      --hide-scrollbars \
      --run-all-compositor-stages-before-draw \
      --user-data-dir="$PROFILE" \
      --virtual-time-budget=4000 \
      --dump-dom \
      "$url" 2>/dev/null)"
    if [[ "$dom" == *"The local service did not answer"* || "$dom" == *"Local service unavailable"* ]]; then
      echo "capture $name rendered the global error panel" >&2
      exit 1
    fi
    case "$name" in
      plan-participant-view-*)
        if [[ "$dom" != *"Redaction preview</h1>"* ]]; then
          echo "capture $name did not render the redaction preview" >&2
          exit 1
        fi
        ;;
      plan-full-*|plan-graph-*|plan-populated-dark-zoom200)
        if [[ "$dom" != *"Schedule planner</h1>"* ]]; then
          echo "capture $name did not render the planner" >&2
          exit 1
        fi
        ;;
    esac
  fi
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
shot shell-command-palette-dark-1280 "view=mission&palette=1" 1280 800

shot setup-first-run-dark-1280 "view=setup&state=empty" 1280 800
shot setup-loading-dark-1280 "view=setup&state=loading" 1280 800
shot setup-populated-dark-1280 "view=setup" 1280 800
shot setup-error-dark-1280 "view=setup&state=error" 1280 800
shot setup-long-dark-1280 "view=setup&state=long" 1280 800
shot setup-populated-light-1280 "view=setup&theme=light" 1280 800
shot setup-populated-dark-1024 "view=setup" 1024 700
shot setup-review-dark-1280 "view=setup&step=review" 1280 800
shot setup-review-dark-1024 "view=setup&step=review" 1024 700
shot setup-review-light-1280 "view=setup&step=review&theme=light" 1280 800
shot setup-review-dark-1280x1400 "view=setup&step=review" 1280 1400
shot setup-review-dark-1024x1000 "view=setup&step=review" 1024 1000
shot setup-review-dark-1024x1100 "view=setup&step=review" 1024 1100
shot setup-review-light-1280x1400 "view=setup&step=review&theme=light" 1280 1400
shot mission-join-dark-1280 "view=mission&state=join-phase" 1280 800
shot mission-join-dark-1024 "view=mission&state=join-phase" 1024 700
shot mission-join-light-1280 "view=mission&state=join-phase&theme=light" 1280 800
shot mission-join-light-1024 "view=mission&state=join-phase&theme=light" 1024 700
shot mission-join-dark-zoom200 "view=mission&state=join-phase" 640 400
shot mission-join-long-zone-dark-1280 "view=mission&state=join-phase&joinTimezone=America/Argentina/Buenos_Aires" 1280 800
shot mission-join-late-dark-1280 "view=mission&state=join-late" 1280 800
shot mission-join-late-dark-1024 "view=mission&state=join-late" 1024 700
shot mission-join-late-light-1280 "view=mission&state=join-late&theme=light" 1280 800
shot mission-seed-setup-dark-1280 "view=mission&state=seed-setup" 1280 800
shot mission-seed-setup-dark-1024 "view=mission&state=seed-setup" 1024 700
shot mission-seed-setup-light-1280 "view=mission&state=seed-setup&theme=light" 1280 800
shot mission-seed-error-dark-1280 "view=mission&state=seed-setup&seedError=1" 1280 800

shot people-empty-dark-1280 "view=people&state=empty" 1280 800
shot people-loading-dark-1280 "view=people&state=loading" 1280 800
shot people-populated-dark-1280 "view=people" 1280 800
shot people-error-dark-1280 "view=people&state=error" 1280 800
shot people-long-dark-1280 "view=people&state=long" 1280 800
shot people-dense-dark-1280 "view=people&state=dense" 1280 800
shot people-populated-light-1280 "view=people&theme=light" 1280 800
shot people-populated-dark-1024 "view=people" 1024 700
shot people-populated-dark-zoom200 "view=people" 640 800

shot plan-empty-dark-1280 "view=plan&state=empty" 1280 800
shot plan-loading-dark-1280 "view=plan&state=loading" 1280 800
shot plan-populated-dark-1280 "view=plan" 1280 800
shot plan-error-dark-1280 "view=plan&state=error" 1280 800
shot plan-infeasible-dark-1280 "view=plan&state=infeasible" 1280 800
shot plan-long-dark-1280 "view=plan&state=long" 1280 800
shot plan-dense-dark-1280 "view=plan&state=dense" 1280 800
shot plan-edit-dark-1280 "view=plan&state=dense&dialog=edit" 1280 800
shot plan-edit-selected-dark-1280 "view=plan&state=dense&dialog=edit&candidate=first" 1280 800
shot plan-cell-focus-dark-1280 "view=plan&state=dense&focus=cell" 1280 800
shot plan-edit-dark-1024 "view=plan&state=dense&dialog=edit" 1024 700
shot plan-edit-light-1280 "view=plan&state=dense&dialog=edit&theme=light" 1280 800
shot plan-edit-no-swap-dark-1280 "view=plan&state=dense&dialog=edit&nocandidates=1" 1280 800
shot plan-edit-no-swap-dark-1024 "view=plan&state=dense&dialog=edit&nocandidates=1" 1024 700
shot plan-edit-dark-zoom200 "view=plan&state=dense&dialog=edit" 640 400
shot plan-populated-light-1280 "view=plan&theme=light" 1280 800
shot plan-populated-dark-1024 "view=plan" 1024 700
shot plan-participant-view-dark-1280 "view=plan&participant=1" 1280 800
shot plan-full-dark-1280x1600 "view=plan" 1280 1600
shot plan-graph-dark-1280x1200 "view=plan" 1280 1200
shot plan-populated-dark-zoom200 "view=plan" 640 400
shot plan-groups-dark-1280 "view=plan" 1280 800
shot plan-groups-dark-1024 "view=plan" 1024 700
shot plan-groups-light-1280 "view=plan&theme=light" 1280 800
shot plan-group-filter-dark-1280 "view=plan&group=A" 1280 800
shot plan-group-filter-dark-1024 "view=plan&group=A" 1024 700
shot plan-dense-groups-dark-1280 "view=plan&state=dense" 1280 800
shot plan-dense-groups-dark-1024 "view=plan&state=dense" 1024 700
shot plan-participant-view-groups-dark-1280 "view=plan&participant=1" 1280 800
shot plan-participant-view-groups-dark-1024 "view=plan&participant=1" 1024 700

shot settings-first-run-dark-1280 "view=settings&state=empty" 1280 800
shot settings-populated-dark-1280 "view=settings" 1280 800
shot settings-populated-light-1280 "view=settings&theme=light" 1280 800
shot settings-error-dark-1280 "view=settings&state=error" 1280 800
shot settings-long-dark-1280 "view=settings&state=long" 1280 800
shot settings-populated-dark-1024 "view=settings" 1024 700
shot settings-full-dark-1280x2000 "view=settings" 1280 2000
shot settings-full-light-1280x2000 "view=settings&theme=light" 1280 2000
shot settings-delete-dialog-dark-1280 "view=settings&dialog=delete" 1280 800
shot settings-bot-dialog-dark-1280 "view=settings&dialog=bot" 1280 800
shot settings-bot-dialog-light-1280 "view=settings&dialog=bot&theme=light" 1280 800
shot settings-bot-dialog-dark-1024 "view=settings&dialog=bot" 1024 700
shot settings-bot-dialog-empty-dark-1280 "view=settings&dialog=bot&state=empty" 1280 800
shot settings-bot-dialog-token-error-dark-1280 "view=settings&dialog=bot&botError=token" 1280 800
shot settings-bot-dialog-test-error-dark-1280 "view=settings&dialog=bot&botError=test" 1280 800
shot settings-bot-dialog-keychain-dark-1280 "view=settings&dialog=bot&botError=keychain" 1280 800
shot settings-bot-dialog-confirm-dark-1280 "view=settings&dialog=bot&botError=confirm" 1280 800
shot settings-dirty-dark-1280 "view=settings&dirty=1" 1280 800
shot settings-dirty-dark-1024 "view=settings&dirty=1" 1024 700

shot reveal-populated-dark-1280 "view=reveal&stream=0" 1280 800
shot reveal-stream-safe-dark-1280 "view=reveal" 1280 800
shot reveal-populated-light-1280 "view=reveal&stream=0" 1280 800
shot reveal-populated-dark-1024 "view=reveal&stream=0" 1024 700
shot reveal-stream-safe-dark-1024 "view=reveal" 1024 700
shot reveal-populated-light-1024 "view=reveal&stream=0" 1024 700
shot reveal-dense-dark-1280 "view=reveal&stream=0&state=dense" 1280 800
shot reveal-dense-dark-1024 "view=reveal&stream=0&state=dense" 1024 700
shot reveal-long-dark-1280 "view=reveal&stream=0&state=long" 1280 800
shot reveal-long-dark-1024 "view=reveal&stream=0&state=long" 1024 700
shot reveal-gap-dark-1280 "view=reveal&stream=0&track=S1P7J&section=lineage" 1280 800
shot reveal-empty-dark-1280 "view=reveal&state=empty" 1280 800
shot reveal-loading-dark-1280 "view=reveal&state=loading" 1280 800
shot reveal-error-dark-1280 "view=reveal&state=error" 1280 800
shot reveal-credits-dark-1280 "view=reveal&stream=0&section=credits" 1280 800
shot reveal-credits-light-1280 "view=reveal&stream=0&section=credits" 1280 800
shot reveal-credits-dark-1024 "view=reveal&stream=0&section=credits" 1024 700
shot reveal-comments-dark-1280 "view=reveal&stream=0&section=comments" 1280 800
shot reveal-comments-dark-1024 "view=reveal&stream=0&section=comments" 1024 700
shot reveal-tools-dark-1280 "view=reveal&stream=0&section=tools" 1280 800
shot reveal-tools-light-1280 "view=reveal&stream=0&section=tools" 1280 800
shot reveal-publish-dark-1280 "view=reveal&stream=0&dialog=publish" 1280 800
shot reveal-publish-dark-1024 "view=reveal&stream=0&dialog=publish" 1024 700
shot reveal-publish-light-1280 "view=reveal&stream=0&dialog=publish" 1280 800
shot reveal-redact-dark-1280 "view=reveal&stream=0&dialog=redact" 1280 800
shot reveal-redact-dark-1024 "view=reveal&stream=0&dialog=redact" 1024 700
shot reveal-blocked-dark-1280 "view=reveal&stream=0&dialog=publish&blocked=1" 1280 800
shot reveal-published-dark-1280 "view=reveal&stream=0&published=1" 1280 800
shot reveal-published-light-1280 "view=reveal&stream=0&published=1" 1280 800
shot reveal-published-dark-1024 "view=reveal&stream=0&published=1" 1024 700
shot reveal-populated-dark-zoom200 "view=reveal&stream=0" 640 400
shot reveal-stream-safe-dark-zoom200 "view=reveal" 640 400

shot audit-empty-dark-1280 "view=audit&state=empty" 1280 800
shot audit-loading-dark-1280 "view=audit&state=loading" 1280 800
shot audit-populated-dark-1280 "view=audit" 1280 800
shot audit-populated-light-1280 "view=audit&theme=light" 1280 800
shot audit-error-dark-1280 "view=audit&state=error" 1280 800
shot audit-long-dark-1280 "view=audit&state=long" 1280 800
shot audit-dense-dark-1280 "view=audit&state=dense" 1280 800
shot audit-populated-dark-1024 "view=audit" 1024 700
shot audit-populated-dark-zoom200 "view=audit" 640 1000
