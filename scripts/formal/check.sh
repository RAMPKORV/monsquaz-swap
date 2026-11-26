#!/usr/bin/env bash
# Reproducible formal checks. The TLA+ jar is downloaded with a pinned
# checksum and is never committed.
set -euo pipefail
cd "$(dirname "$0")/../.."

TLA_VERSION="v1.8.0"
TLA_SHA256="fdffef247f563ef6edbfd44958d2a82db79e7564e07cb673262d896353fa6713"
TLA_URL="https://github.com/tlaplus/tlaplus/releases/download/${TLA_VERSION}/tla2tools.jar"
TLA_JAR="target/formal/tla2tools.jar"
MODE="${1:-pr}"

mkdir -p target/formal
if [ ! -f "$TLA_JAR" ] || ! echo "${TLA_SHA256}  ${TLA_JAR}" | sha256sum -c - >/dev/null 2>&1; then
  echo "downloading TLA+ ${TLA_VERSION}"
  curl -fsSL -o "$TLA_JAR.tmp" "$TLA_URL"
  echo "${TLA_SHA256}  ${TLA_JAR}.tmp" | sha256sum -c -
  mv "$TLA_JAR.tmp" "$TLA_JAR"
fi

run_tlc() {
  local config="$1"
  local module="$2"
  java -XX:+UseParallelGC -cp "$TLA_JAR" tlc2.TLC \
    -workers auto -config "$config" "$module"
}

# Replace one source fragment and require TLC to report the named invariant.
# A parse error or an unrelated failure is not a detected mutation.
run_mutation() {
  local label="$1"
  local expected="$2"
  local old="$3"
  local new="$4"
  local work
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' RETURN
  python3 - "$old" "$new" "$work/SwapClaim.tla" <<'PY'
import pathlib
import sys

old = sys.argv[1]
new = sys.argv[2]
source = pathlib.Path("specs/tla/SwapClaim.tla").read_text()
if old not in source:
    raise SystemExit(f"mutation pattern not found: {old}")
pathlib.Path(sys.argv[3]).write_text(source.replace(old, new, 1))
PY
  cp specs/tla/SwapClaimNightly.tla "$work/SwapClaimNightly.tla"
  cp specs/tla/SwapClaimMutation.cfg "$work/SwapClaimMutation.cfg"
  if java -XX:+UseParallelGC -cp "$TLA_JAR" tlc2.TLC \
      -workers auto -config "$work/SwapClaimMutation.cfg" "$work/SwapClaimNightly.tla" \
      >"$work/tlc.log" 2>&1; then
    echo "mutation '$label' did not fail" >&2
    cat "$work/tlc.log" >&2
    return 1
  fi
  if ! grep -q "Invariant $expected is violated" "$work/tlc.log"; then
    echo "mutation '$label' failed without violating $expected" >&2
    cat "$work/tlc.log" >&2
    return 1
  fi
  echo "mutation detected: $label ($expected)"
}

echo "TLA+ PR model"
run_tlc specs/tla/SwapClaimPR.cfg specs/tla/SwapClaimPR.tla
run_mutation "clock leaves the natural numbers" "TypeOK" "now' = now + 1" "now' = -1"
run_mutation "offer allows own track" "NoOwnTrack" '/\ p /= Owner[t]' '/\ TRUE'
run_mutation "claim allows two held tracks" "OneTrackPerPerson" \
  '/\ \A other \in Tracks : holder[other] /= p' '/\ TRUE'
run_mutation "claim leaves a seen pair offered" "NoRepeat" \
  '[phase[t] EXCEPT ![p] = "claimed"]' '[phase[t] EXCEPT ![p] = "offered"]'
run_mutation "expiry keeps a banned pair offered" "NoExpiredReoffer" \
  '[phase[t] EXCEPT ![p] = "idle"]' '[phase[t] EXCEPT ![p] = "offered"]'

if [ "$MODE" = "nightly" ]; then
  echo "TLA+ nightly model"
  run_tlc specs/tla/SwapClaimNightly.cfg specs/tla/SwapClaimNightly.tla
fi
