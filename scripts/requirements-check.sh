#!/usr/bin/env bash
# Requirement consistency gate (D-076).
# Checks z3, runs the known-bad corpus and the model tests, then writes or
# checks docs/generated/REQUIREMENT_CONSISTENCY.md.
# Usage: scripts/requirements-check.sh [--check]
#   --check  do not write the report; fail when the report is stale
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v python3 >/dev/null 2>&1; then
  echo "requirements: FAILED (python3 is not on PATH)" >&2
  exit 1
fi

if ! python3 -c "import z3" >/dev/null 2>&1; then
  echo "requirements: FAILED (the z3-solver Python package is not installed)" >&2
  echo "Install it with: python3 -m pip install -r specs/requirements/requirements.txt" >&2
  exit 1
fi

echo "== requirement self-tests =="
python3 -m unittest discover -s specs/requirements/tests -t . -p 'test_*.py' -v

echo "== requirement consistency =="
python3 specs/requirements/check.py "$@"
