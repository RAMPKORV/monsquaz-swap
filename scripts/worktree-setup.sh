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

# The local agent config is ignored because it contains machine-specific
# paths. Copy it from the main checkout so every worktree keeps the same
# permission profile.
if [ -n "${PROJECT_WORKSPACE:-}" ] && [ -f "$PROJECT_WORKSPACE/opencode.json" ] && [ ! -e opencode.json ]; then
  cp "$PROJECT_WORKSPACE/opencode.json" opencode.json
fi
if [ -n "${PROJECT_WORKSPACE:-}" ] && [ -f "$PROJECT_WORKSPACE/opencode.jsonc" ] && [ ! -e opencode.jsonc ]; then
  cp "$PROJECT_WORKSPACE/opencode.jsonc" opencode.jsonc
fi

if command -v pnpm >/dev/null 2>&1; then
  pnpm install --frozen-lockfile
fi

cargo fetch
echo "worktree ready: $(pwd)"
