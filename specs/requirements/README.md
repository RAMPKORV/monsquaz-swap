# Requirement consistency model

This directory holds the finite Z3 model for the normative rules (D-076).
`scripts/requirements-check.sh` is the gate. The generated report is
`docs/generated/REQUIREMENT_CONSISTENCY.md`.

The pilot scope is the fill-in, leave, handoff, and scheduling rules. The
model has 20 formalized scenarios from 19 source scenarios. The other scenarios are
outside the modeled scope. PASS means model satisfiability and entailment, not
implementation.

## Files

| File | Purpose |
|---|---|
| `schema.py` | Typed schema, finite domain sizes, defaults, `Premise`, `Rule`, and `Scenario`. |
| `model.py` | The world variables, the domain definitions, the must rules, the formalized scenarios, and the reachable premises. |
| `checker.py` | The fail-closed solver checks: base, pairs, witnesses, entailment, premises, necessity, and coverage. |
| `check.py` | The report generator and the command-line entry point. |
| `tests/known_bad.py` | The three historical contradictions. Each case has a separate premise. |
| `tests/test_consistency.py` | The gate tests, the negative tier tests, the solver-unknown test, the necessity test, and the link manifest. |

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
- a rule pair is unsatisfiable under a reachable premise;
- a formalized scenario has no witness (the scenario is impossible);
- a formalized scenario does not entail its postcondition;
- a reachable premise is unsatisfiable;
- a rule is not necessary for any linked scenario;
- a rule has no scenario or a scenario has no rule;
- a known-bad case is not detected;
- a solver query returns `unknown` or times out;
- the report is stale in `--check` mode.

## Checks

1. **Base conjunction.** All must rules are satisfiable together.
2. **Pairs.** No two rules have an unsatisfiable conjunction. The premise is
   tracked separately from `domain`.
3. **Witness.** `rules and pre and action and post` is satisfiable.
4. **Entailment.** `rules and pre and action and not post` is unsatisfiable.
5. **Premises.** Each reachable mode is satisfiable with all rules.
6. **Necessity.** For each rule, remove it and find a linked scenario whose
   postcondition no longer follows. A rule without such a scenario fails.
7. **Coverage.** Every rule links a scenario and every scenario links a rule.
8. **Unknown.** A solver `unknown` or timeout fails the gate.

The replan opt-in in REQ-JOIN-006 is a documented exception. The rule
`R-RESTORE-FILLIN-ONLY` applies only when `replan_allowed` is false.

## Add a rule

1. Add the rule to `build_rules` with a stable id, the source links, a Z3
   formula, a short paraphrase in ASD-STE100, and a kind
   (`behaviour`, `setting`, `invariant`, or `definition`).
2. Add or update a formalized scenario in `build_scenarios`. Set the source
   `@pending` flag. Link the rule.
3. Add the rule to `LINK_MANIFEST` in `tests/test_consistency.py`.
4. Run `scripts/requirements-check.sh`.
5. Review the formula like a test. The natural-language source wins.
