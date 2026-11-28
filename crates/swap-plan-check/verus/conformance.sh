#!/usr/bin/env bash
# Conformance: compare the verified flat checker with the Rust round checker.
#
# The verified contract is acceptance soundness. The script fails on any
# acceptance disagreement. It reports reason-class differences separately; the
# flat checker checks the global seen set before the round shape, so a corrupt
# round can report RepeatedPair where the round checker reports a duplicate
# participant or duplicate track. Both reject.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
verus="${VERUS:-verus}"
out="${TMPDIR:-/tmp}/swap-plan-check-conformance"
mkdir -p "$out"

cd "$root"
cargo run -q -p swap-plan-check --bin gen_corpus -- \
  --plans 1200 --certs 600 \
  --corpus target/plan-check/corpus.txt \
  --expected target/plan-check/expected.txt >/dev/null

(cd "$here" && "$verus" --no-cheating --compile flat_plan_check.rs >/dev/null)
"$here/flat_plan_check" target/plan-check/corpus.txt > "$out/verus.txt"
grep '^PLAN ' target/plan-check/expected.txt > "$out/rust.txt"

python3 - "$out/rust.txt" "$out/verus.txt" <<'PY'
import sys
from collections import Counter

def read(path):
    result = {}
    for line in open(path):
        _, ident, code = line.split()
        result[int(ident)] = int(code)
    return result

rust = read(sys.argv[1])
verus = read(sys.argv[2])
if set(rust) != set(verus):
    raise SystemExit("case id sets differ")
acceptance = [i for i in rust if (rust[i] == 0) != (verus[i] == 0)]
reasons = [i for i in rust if rust[i] != verus[i]]
print(f"plans compared: {len(rust)}")
print(f"acceptance disagreements: {len(acceptance)}")
print(f"reason-class differences: {len(reasons)}")
if reasons:
    print("reason pairs:", dict(Counter((rust[i], verus[i]) for i in reasons)))
if acceptance:
    print("first acceptance disagreements:", acceptance[:20])
    raise SystemExit(1)
PY

echo "conformance OK: no acceptance disagreement"
