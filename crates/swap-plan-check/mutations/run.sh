#!/usr/bin/env bash
# Rust mutation runner for the plan checker.
#
# Each mutation must make at least one test fail. A mutation that passes is a
# failed pilot for that property. The verified twin is separate; see README.md.
set -euo pipefail

crate_dir="$(cd "$(dirname "$0")/.." && pwd)"
work_root="${TMPDIR:-/tmp}/swap-plan-check-mutations"
rm -rf "$work_root"
mkdir -p "$work_root"

run_mutation() {
  local label="$1"
  local old="$2"
  local new="$3"
  local dir="$work_root/$label"
  mkdir -p "$dir/src" "$dir/tests"
  cp "$crate_dir/src/lib.rs" "$dir/src/lib.rs"
  cp "$crate_dir/src/corpus.rs" "$dir/src/corpus.rs"
  mkdir -p "$dir/src/bin"
  cp "$crate_dir/src/bin/gen_corpus.rs" "$dir/src/bin/gen_corpus.rs"
  cp "$crate_dir/tests/plan_check.rs" "$dir/tests/plan_check.rs"
  cat > "$dir/Cargo.toml" <<'EOF'
[package]
name = "swap-plan-check"
version = "0.1.1"
edition = "2024"

[dependencies]
EOF
  python3 - "$old" "$new" "$dir/src/lib.rs" <<'PY'
import pathlib
import sys

old, new, path = sys.argv[1], sys.argv[2], pathlib.Path(sys.argv[3])
text = path.read_text()
if old not in text:
    raise SystemExit(f"mutation pattern not found: {old}")
path.write_text(text.replace(old, new, 1))
PY
  if (cd "$dir" && cargo test --quiet >"$dir/test.log" 2>&1); then
    echo "MUTATION PASSED (failed pilot): $label"
    return 1
  fi
  echo "mutation failed as required: $label"
  grep -E "^test .* FAILED|panicked at" "$dir/test.log" | head -3 | sed 's/^/  /'
}

status=0
run_mutation \
  "accept-seen-pair" \
  "if seen_in_previous_round(input, round_index, assignment) {" \
  "if false && seen_in_previous_round(input, round_index, assignment) {" || status=1
run_mutation \
  "accept-own-track" \
  "if input.owners[assignment.track as usize] == assignment.participant {" \
  "if false && input.owners[assignment.track as usize] == assignment.participant {" || status=1
run_mutation \
  "accept-incomplete-round" \
  "if input.policy == Policy::Strict {" \
  "if false && input.policy == Policy::Strict {" || status=1
run_mutation \
  "accept-invalid-hall-violator" \
  "if participant != owner
                && !seen_in_previous_rounds(input, cert.round, participant, track)
                && !contains(&cert.neighbors[..cert.neighbors_len], participant)" \
  "if false
                && participant != owner
                && !seen_in_previous_rounds(input, cert.round, participant, track)
                && !contains(&cert.neighbors[..cert.neighbors_len], participant)" || status=1

if [ "$status" -ne 0 ]; then
  echo "one or more mutations passed; the pilot fails" >&2
  exit 1
fi
echo "all four mutations failed as required"
