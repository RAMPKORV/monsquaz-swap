#!/usr/bin/env bash
# Prepare a freshly created git worktree for development.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -d "$HOME/.npm-global/bin" ]; then
  export PATH="$HOME/.npm-global/bin:$PATH"
fi

if [ -f .env.example ] && [ ! -f .env ]; then
  cp .env.example .env
fi

if command -v pnpm >/dev/null 2>&1; then
  pnpm install --frozen-lockfile
fi

cargo fetch
echo "worktree ready: $(pwd)"
