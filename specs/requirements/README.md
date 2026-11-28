# Requirement consistency model

This directory holds the finite Z3 model for the normative rules (D-076).
`scripts/requirements-check.sh` is the gate. The generated report is
`docs/generated/REQUIREMENT_CONSISTENCY.md`.

## Files

| File | Purpose |
|---|---|
| `schema.py` | Typed schema, finite domain sizes, `Source`, `Rule`, and `Scenario`. |
| `model.py` | The world variables, the domain definitions, the must rules, and the formalized scenarios. |
| `checker.py` | The satisfiability, contradiction, scenario, and coverage checks. |
| `check.py` | The report generator and the command-line entry point. |
| `tests/known_bad.py` | The three historical contradictions. The gate must detect each one. |
| `tests/test_consistency.py` | The gate tests, the seeded mutation, and the coverage-gap tests. |

## Install

```bash
python3 -m pip install -r specs/requirements/requirements.txt
```

CI installs the pinned wheel before the spec gates. The gate checks the import
and prints the solver version. The report includes the solver version.

## Run

```bash
scripts/requirements-check.sh           # generate the report
scripts/requirements-check.sh --check   # fail when the report is stale
```

The gate fails when:

- a must rule is unsatisfiable with the other rules;
- a rule pair is unsatisfiable;
- a formalized scenario is unsatisfiable;
- a rule has no scenario or a scenario has no rule;
- a known-bad case is not detected;
- the report is stale in `--check` mode.

## Add a rule

1. Add the rule to `build_rules` with a stable id, the source links, a Z3
   formula, and a short paraphrase in ASD-STE100.
2. Add or update a formalized scenario in `build_scenarios` and link the rule.
3. Run `scripts/requirements-check.sh`.
4. Review the formula like a test. The natural-language source wins.
