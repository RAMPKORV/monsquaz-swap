#!/usr/bin/env bash
# Build the pinned Refract CLI into a local cache. Refract is pre-release, so the
# repository is pinned by commit in design/refract.lock.json.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -d "$HOME/.npm-global/bin" ]; then
  export PATH="$HOME/.npm-global/bin:$PATH"
fi

if ! command -v pnpm >/dev/null 2>&1; then
  echo "pnpm is required to build Refract" >&2
  exit 1
fi

COMMIT="$(node -p "require('./design/refract.lock.json').commit")"
REPOSITORY="$(node -p "require('./design/refract.lock.json').repository")"
CACHE="${REFRACT_CACHE:-$PWD/.cache/refract/$COMMIT}"

if [ ! -f "$CACHE/packages/cli/dist/index.js" ]; then
  echo "refract: building $COMMIT into $CACHE" >&2
  rm -rf "$CACHE" "$CACHE.tmp"
  mkdir -p "$(dirname "$CACHE")"
  git init -q "$CACHE.tmp"
  git -C "$CACHE.tmp" remote add origin "$REPOSITORY"
  git -C "$CACHE.tmp" fetch -q --depth 1 origin "$COMMIT"
  git -C "$CACHE.tmp" checkout -q FETCH_HEAD
  (
    cd "$CACHE.tmp"
    pnpm install --frozen-lockfile >&2
    pnpm build >&2
  )
  mv "$CACHE.tmp" "$CACHE"
fi

# One-time browser download. The Playwright cache is shared with other tools.
if ! node "$CACHE/packages/core/node_modules/playwright/cli.js" install chromium \
  >/dev/null 2>"$CACHE/playwright-install.log"; then
  cat "$CACHE/playwright-install.log" >&2
  exit 1
fi

echo "$CACHE"
