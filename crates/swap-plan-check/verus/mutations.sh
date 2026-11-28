#!/usr/bin/env bash
# Proof mutations for the flat verified core. Every mutation must make Verus
# fail. A mutation that verifies is a failed pilot.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
verus="${VERUS:-verus}"
work="${TMPDIR:-/tmp}/swap-plan-check-proof-mutations"
rm -rf "$work"
mkdir -p "$work"

run_mutation() {
  local label="$1"
  local old="$2"
  local new="$3"
  local dir="$work/$label"
  mkdir -p "$dir"
  cp "$here/flat_plan_check.rs" "$dir/flat_plan_check.rs"
  python3 - "$old" "$new" "$dir/flat_plan_check.rs" <<'PY'
import pathlib
import sys

old, new, path = sys.argv[1], sys.argv[2], pathlib.Path(sys.argv[3])
text = path.read_text()
if old not in text:
    raise SystemExit(f"mutation pattern not found: {old}")
path.write_text(text.replace(old, new, 1))
PY
  if (cd "$dir" && "$verus" --no-cheating --triggers-mode silent flat_plan_check.rs >verus.log 2>&1); then
    echo "PROOF MUTATION VERIFIED (failed pilot): $label"
    return 1
  fi
  if grep -q "error:.*parse\|error: expected" "$dir/verus.log"; then
    echo "mutation failed for a parse error, not a proof error: $label"
    sed -n '1,20p' "$dir/verus.log"
    return 1
  fi
  echo "proof mutation failed as required: $label"
  grep -m1 -E "postcondition not satisfied|assertion failed|precondition not satisfied" "$dir/verus.log" | sed 's/^/  /' || true
  return 0
}

status=0
run_mutation \
  "accept-seen-pair" \
  "if seen.contains(&(p, t)) {" \
  "if false && seen.contains(&(p, t)) {" || status=1
run_mutation \
  "accept-own-track" \
  "if input.owners[t as usize] == p {" \
  "if false && input.owners[t as usize] == p {" || status=1
run_mutation \
  "accept-incomplete-round" \
  "if !round_has_person_exec(input, r, p) {" \
  "if false && !round_has_person_exec(input, r, p) {" || status=1
run_mutation \
  "accept-invalid-hall-violator" \
  "&&& cert.neighbors_len < cert.tracks_len" \
  "&&& true" || status=1

if [ "$status" -ne 0 ]; then
  echo "one or more proof mutations verified; the pilot fails" >&2
  exit 1
fi
echo "all four proof mutations failed as required"
