#!/usr/bin/env bash
# Mutation testing for swap-core and the classic scheduler.
#
# The nightly workflow (.github/workflows/mutants.yml) runs `--core` and then
# `--scheduler`. Use a local run to check a change before the nightly job.
#
# cargo-mutants exit codes: 2 means a mutant survived, 3 means a mutant
# timed out. Both fail the run. `--summary` never fails; the workflow uses the
# step outcome to fail the job after it uploads the results.
set -euo pipefail
cd "$(dirname "$0")/.."

PINNED_VERSION="27.1.0"
CORE_OUT="mutants.out/core"
SCHEDULER_OUT="mutants.out/scheduler"
CORE_GLOB="crates/swap-core/src/**/*.rs"
SCHEDULER_GLOB="crates/swap-core/src/schedule.rs"

usage() {
  cat <<'EOF'
Usage: scripts/mutants.sh [MODE]

Modes:
  --check       Verify the pinned cargo-mutants version and the reviewed scope.
  --list        List the mutants that the nightly job will test.
  --core        Test mutants in swap-core, excluding the scheduler.
  --scheduler   Test mutants in the scheduler module.
  --summary     Print the outcome counts and the survivors in mutants.out/.
  --all         Run --core, then --scheduler. This is the default.
  -h, --help    Show this help.

Install cargo-mutants 27.1.0 when the check reports that it is missing:
  mkdir -p /tmp/cargo-mutants
  curl -sSLf -o /tmp/cargo-mutants/cargo-mutants.tar.gz \
    https://github.com/sourcefrog/cargo-mutants/releases/download/v27.1.0/cargo-mutants-x86_64-unknown-linux-gnu.tar.gz
  tar xzf /tmp/cargo-mutants/cargo-mutants.tar.gz -C /tmp/cargo-mutants
  install -m 755 /tmp/cargo-mutants/cargo-mutants ~/.cargo/bin/cargo-mutants
EOF
}

require_cargo_mutants() {
  if ! command -v cargo-mutants >/dev/null 2>&1; then
    echo "cargo-mutants is not installed." >&2
    echo "This repo pins cargo-mutants $PINNED_VERSION. Run scripts/mutants.sh --help for the install command." >&2
    exit 1
  fi
  local installed
  installed="$(cargo mutants --version | awk '{print $2}')"
  if [ "$installed" != "$PINNED_VERSION" ]; then
    echo "cargo-mutants $installed is installed; this repo pins $PINNED_VERSION." >&2
    echo "Install the pinned release before you run the job. See scripts/mutants.sh --help." >&2
    exit 1
  fi
}

list_mutants() {
  local glob=$1
  shift
  cargo mutants -p swap-core --file "$glob" "$@" --list
}

run_scope() {
  local label=$1
  local glob=$2
  local output=$3
  shift 3
  echo "== mutants: $label =="
  rm -rf "$output"
  mkdir -p "$(dirname "$output")"
  cargo mutants -p swap-core --file "$glob" "$@" --output "$output"
}

summary() {
  local scope dir missed timeouts caught unviable
  for dir in mutants.out/*/; do
    [ -d "$dir" ] || continue
    scope="$(basename "$dir")"
    dir="${dir%/}/mutants.out"
    if [ ! -d "$dir" ]; then
      echo "$scope: no result directory ($dir)"
      continue
    fi
    missed="$(wc -l < "$dir/missed.txt" 2>/dev/null || echo 0)"
    timeouts="$(wc -l < "$dir/timeout.txt" 2>/dev/null || echo 0)"
    caught="$(wc -l < "$dir/caught.txt" 2>/dev/null || echo 0)"
    unviable="$(wc -l < "$dir/unviable.txt" 2>/dev/null || echo 0)"
    echo "== $scope: caught=$caught missed=$missed timeout=$timeouts unviable=$unviable =="
    if [ -s "$dir/missed.txt" ]; then
      echo "-- survivors (test gaps; read the diff and the log before you act) --"
      sed 's/^/  /' "$dir/missed.txt"
    fi
    if [ -s "$dir/timeout.txt" ]; then
      echo "-- timeouts (raise the timeout or inspect a possible hang) --"
      sed 's/^/  /' "$dir/timeout.txt"
    fi
  done
}

mode="${1:---all}"
case "$mode" in
  -h|--help)
    usage
    ;;
  --check)
    require_cargo_mutants
    echo "cargo-mutants $PINNED_VERSION"
    core_count="$(list_mutants "$CORE_GLOB" --exclude "$SCHEDULER_GLOB" | wc -l)"
    scheduler_count="$(list_mutants "$SCHEDULER_GLOB" | wc -l)"
    if [ "$core_count" -eq 0 ] || [ "$scheduler_count" -eq 0 ]; then
      echo "The reviewed scope is empty. Check .cargo/mutants.toml and the globs in this script." >&2
      exit 1
    fi
    echo "scope: $core_count swap-core mutants, $scheduler_count scheduler mutants"
    ;;
  --list)
    require_cargo_mutants
    echo "== swap-core (scheduler excluded) =="
    list_mutants "$CORE_GLOB" --exclude "$SCHEDULER_GLOB"
    echo "== scheduler =="
    list_mutants "$SCHEDULER_GLOB"
    ;;
  --core)
    require_cargo_mutants
    run_scope "swap-core" "$CORE_GLOB" "$CORE_OUT" --exclude "$SCHEDULER_GLOB"
    ;;
  --scheduler)
    require_cargo_mutants
    run_scope "scheduler" "$SCHEDULER_GLOB" "$SCHEDULER_OUT"
    ;;
  --summary)
    summary
    ;;
  --all)
    require_cargo_mutants
    run_scope "swap-core" "$CORE_GLOB" "$CORE_OUT" --exclude "$SCHEDULER_GLOB"
    run_scope "scheduler" "$SCHEDULER_GLOB" "$SCHEDULER_OUT"
    ;;
  *)
    echo "Unknown mode: $mode" >&2
    usage >&2
    exit 1
    ;;
esac
