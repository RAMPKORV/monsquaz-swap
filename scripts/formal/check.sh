#!/usr/bin/env bash
# Reproducible formal checks. The TLA+ jar is downloaded with a pinned
# checksum and is never committed.
set -euo pipefail
cd "$(dirname "$0")/../.."

TLA_VERSION="v1.8.0"
# D-062: download from our own release asset, not the mutable upstream asset.
# Provenance: https://github.com/tlaplus/tlaplus/releases/download/v1.8.0/tla2tools.jar
# (upstream build of 2026-11-28, GitHub digest sha256:df7cef5d...).
# The update procedure is in docs/FORMAL_METHODS.md, section "Tools".
TLA_SHA256="df7cef5d37bcca879817424c001696e07fd094d68a72690449af3045fd972469"
TLA_URL="https://github.com/RAMPKORV/monsquaz-swap/releases/download/tools-tla2tools-1.8.0/tla2tools.jar"
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
run_named_mutation() {
  local label="$1"
  local expected="$2"
  local source="$3"
  local config="$4"
  local module="$5"
  local old="$6"
  local new="$7"
  local work
  work="$(mktemp -d)"
  python3 - "$old" "$new" "$source" "$work/$(basename "$source")" <<'PY'
import pathlib
import sys

old = sys.argv[1]
new = sys.argv[2]
source = pathlib.Path(sys.argv[3]).read_text()
if old not in source:
    raise SystemExit(f"mutation pattern not found: {old}")
pathlib.Path(sys.argv[4]).write_text(source.replace(old, new, 1))
PY
  cp "$module" "$work/$(basename "$module")"
  cp "$config" "$work/$(basename "$config")"
  if java -XX:+UseParallelGC -cp "$TLA_JAR" tlc2.TLC \
      -workers auto -config "$work/$(basename "$config")" "$work/$(basename "$module")" \
      >"$work/tlc.log" 2>&1; then
    echo "mutation '$label' did not fail" >&2
    cat "$work/tlc.log" >&2
    rm -rf "$work"
    return 1
  fi
  if ! grep -q "Invariant $expected is violated" "$work/tlc.log"; then
    echo "mutation '$label' failed without violating $expected" >&2
    cat "$work/tlc.log" >&2
    rm -rf "$work"
    return 1
  fi
  echo "mutation detected: $label ($expected)"
  rm -rf "$work"
}

run_claim_mutation() {
  run_named_mutation "$1" "$2" \
    specs/tla/SwapClaim.tla \
    specs/tla/SwapClaimMutation.cfg \
    specs/tla/SwapClaimNightly.tla \
    "$3" "$4"
}

run_delivery_mutation() {
  run_named_mutation "$1" "$2" \
    specs/tla/SwapDelivery.tla \
    specs/tla/SwapDeliveryMutation.cfg \
    specs/tla/SwapDeliveryMutation.tla \
    "$3" "$4"
}

echo "TLA+ PR model"
run_tlc specs/tla/SwapClaimPR.cfg specs/tla/SwapClaimPR.tla
run_claim_mutation "clock leaves the natural numbers" "TypeOK" "now' = now + 1" "now' = -1"
run_claim_mutation "offer allows own track" "NoOwnOffer" '/\ p /= Owner[t]' '/\ TRUE'
run_claim_mutation "claim skips the offer guard" "NoOwnTrack" \
  '/\ phase[t][p] = "offered"' '/\ TRUE'
run_claim_mutation "offer allows two people for one track" "AtMostOneOfferPerTrack" \
  '/\ \A other \in Participants : phase[t][other] /= "offered"' '/\ TRUE'
run_claim_mutation "offer allows two tracks for one person" "AtMostOneOfferPerPerson" \
  '/\ \A other \in Tracks : phase[other][p] /= "offered"' '/\ TRUE'
run_claim_mutation "offer ignores the holder guard" "AtMostOneActivePerPerson" \
  '/\ \A other \in Tracks : holder[other] /= p' '/\ TRUE'
run_claim_mutation "claim leaves a seen pair offered" "NoRepeat" \
  '[phase[t] EXCEPT ![p] = "claimed"]' '[phase[t] EXCEPT ![p] = "offered"]'
run_claim_mutation "expiry keeps a banned pair offered" "NoExpiredReoffer" \
  '[phase[t] EXCEPT ![p] = "idle"]' '[phase[t] EXCEPT ![p] = "offered"]'

echo "TLA+ delivery PR model"
run_tlc specs/tla/SwapDeliveryPR.cfg specs/tla/SwapDeliveryPR.tla
run_delivery_mutation "ack an unsent effect" "AckedWereSent" '/\ e \in sent' '/\ TRUE'
run_delivery_mutation "send an unenqueued effect" "SentWereEnqueued" '/\ e \in pending' '/\ TRUE'
run_delivery_mutation "allow duplicate keys" "UniqueKeys" \
  '/\ \A other \in enqueued : Key[other] /= Key[e]' '/\ TRUE'
run_delivery_mutation "ack drops the effect" "NoLostEffect" \
  "acked' = acked \\cup {e}" "acked' = acked"
run_delivery_mutation "attempt leaves the counter unbounded" "AttemptsBounded" \
  "attempts' = [attempts EXCEPT ![e] = attempts[e] + 1]" \
  "attempts' = [attempts EXCEPT ![e] = MaxAttempts + 1]"
run_delivery_mutation "attempt count leaves the naturals" "TypeOK" \
  "attempts' = [attempts EXCEPT ![e] = attempts[e] + 1]" \
  "attempts' = [attempts EXCEPT ![e] = -1]"
run_delivery_mutation "failure drops the host alert" "FailedWereAlerted" \
  "alerted' = alerted \\cup {e}" \
  "UNCHANGED alerted"

if [ "$MODE" = "nightly" ]; then
  echo "TLA+ claim nightly model"
  run_tlc specs/tla/SwapClaimNightly.cfg specs/tla/SwapClaimNightly.tla
  echo "TLA+ delivery nightly model"
  run_tlc specs/tla/SwapDeliveryNightly.cfg specs/tla/SwapDeliveryNightly.tla
fi
