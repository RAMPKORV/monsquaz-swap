#!/usr/bin/env python3
"""Generate and check the requirement consistency report (D-076).

Run this script through `scripts/requirements-check.sh` from the repository
root. The gate checks the finite formal model, the known-bad corpus, the
reachable premises, scenario entailment and witnesses, rule necessity, and the
coverage links. It then writes
`docs/generated/REQUIREMENT_CONSISTENCY.md`.

A solver `unknown` is a failure. The gate fails closed.
"""

from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from specs.requirements.checker import ModelResult, check_model  # noqa: E402
from specs.requirements.model import build_premises, build_rules, build_scenarios, World  # noqa: E402
from specs.requirements.schema import (  # noqa: E402
    INVITATIONS,
    PARTICIPANTS,
    ROUNDS,
    TRACKS,
    TURNS,
    Premise,
    Rule,
    Scenario,
    Source,
)
from specs.requirements.tests.known_bad import KnownBadResult, run_corpus  # noqa: E402

REPORT_PATH = Path("docs/generated/REQUIREMENT_CONSISTENCY.md")
REPORT_PREFIX = "../../"


def _link(source: Source) -> str:
    return f"[{source.ref}]({REPORT_PREFIX}{source.path})"


def _source_cell(sources: tuple[Source, ...]) -> str:
    return ", ".join(_link(source) for source in sources)


def _table_cell(text: str) -> str:
    return text.replace("|", "\\|")


def _rule_row(rule: Rule, scenarios: list[Scenario]) -> str:
    linked = [scenario.id for scenario in scenarios if rule.id in scenario.rules]
    links = ", ".join(f"`{item}`" for item in linked) if linked else "**none**"
    return (
        f"| `{rule.id}` | `{rule.kind}` | {_source_cell(rule.sources)} | "
        f"{_table_cell(rule.paraphrase)} | {links} |"
    )


def _scenario_row(scenario: Scenario, result: ModelResult) -> str:
    scenario_result = next(
        item for item in result.scenarios if item.scenario_id == scenario.id
    )
    witness = (
        "PASS"
        if scenario_result.satisfiable
        else "UNKNOWN"
        if scenario_result.status == "unknown"
        else "FAIL"
    )
    entailment = (
        "PASS"
        if scenario_result.entailed
        else "UNKNOWN"
        if scenario_result.status == "unknown"
        else "FAIL"
    )
    pending = "yes" if scenario.pending else "no"
    rules = ", ".join(f"`{item}`" for item in scenario.rules) if scenario.rules else "**none**"
    feature = f"[`{scenario.feature}`]({REPORT_PREFIX}{scenario.feature})"
    return (
        f"| `{scenario.id}` | {_table_cell(scenario.name)} | {feature} | {pending} | "
        f"{witness} | {entailment} | {rules} |"
    )


def _premise_row(premise: Premise, result: ModelResult) -> str:
    premise_result = next(
        item for item in result.premises if item.premise_id == premise.id
    )
    outcome = premise_result.status.upper()
    core = ", ".join(premise_result.core) if premise_result.core else "none"
    return (
        f"| `{premise.id}` | {_table_cell(premise.description)} | "
        f"{outcome} | {core} |"
    )


def _contradiction_rows(result: ModelResult, rules: list[Rule]) -> list[str]:
    by_id = {rule.id: rule for rule in rules}
    rows: list[str] = []
    for item in result.contradictions:
        left = by_id[item.left_id]
        right = by_id[item.right_id]
        rows.append(
            f"| `{item.left_id}` + `{item.right_id}` | `{item.premise}` | "
            f"{_source_cell(left.sources)}; {_source_cell(right.sources)} | "
            f"{', '.join(item.core) if item.core else 'domain'} |"
        )
    return rows


def _necessity_rows(result: ModelResult) -> list[str]:
    rows: list[str] = []
    for item in result.necessity:
        exercised = "yes" if item.exercised else "**NO**"
        linked = ", ".join(f"`{sid}`" for sid in item.linked_scenarios) if item.linked_scenarios else "none"
        failing = ", ".join(f"`{sid}`" for sid in item.failing_scenarios) if item.failing_scenarios else "none"
        rows.append(f"| `{item.rule_id}` | {exercised} | {linked} | {failing} |")
    return rows


def _coverage_rows(result: ModelResult, rules: list[Rule]) -> list[str]:
    by_id = {rule.id: rule for rule in rules}
    rows: list[str] = []
    for gap in result.coverage:
        sources = ""
        if gap.kind in {"rule_without_scenario", "unknown_rule"} and gap.subject in by_id:
            sources = _source_cell(by_id[gap.subject].sources)
        rows.append(f"| `{gap.kind}` | `{gap.subject}` | {_table_cell(gap.detail)} | {sources} |")
    return rows


def _known_ref_link(ref: str) -> str:
    if ref.startswith("D-"):
        path = "ai-rules/DECISIONS.md"
    elif ref.startswith("REQ-"):
        path = "docs/PRD.md"
    else:
        path = "docs/EVENT_SETTINGS.md"
    return f"[{ref}]({REPORT_PREFIX}{path})"


def _known_bad_rows(known_bad: list[KnownBadResult]) -> list[str]:
    rows: list[str] = []
    for item in known_bad:
        outcome = "DETECTED" if item.detected else "**MISSED**"
        expected = ", ".join(_known_ref_link(ref) for ref in item.expected_sources)
        actual = (
            ", ".join(_known_ref_link(ref) for ref in item.actual_sources)
            if item.actual_sources
            else "none"
        )
        core = ", ".join(item.core) if item.core else "none"
        rows.append(
            f"| `{item.case_id}` | `{item.premise}` | {expected} | {actual} | "
            f"{outcome} | {core} |"
        )
    return rows


def render_report(
    result: ModelResult,
    rules: list[Rule],
    scenarios: list[Scenario],
    premises: list[Premise],
    known_bad: list[KnownBadResult],
) -> str:
    pending = sum(1 for scenario in scenarios if scenario.pending)
    necessity_failures = [item for item in result.necessity if not item.exercised]
    failures = (
        result.base_status != "sat"
        or bool(result.contradictions)
        or any(item.status != "pass" for item in result.scenarios)
        or any(item.status != "sat" for item in result.premises)
        or bool(necessity_failures)
        or bool(result.coverage)
        or bool(result.unknown_queries)
        or any(not item.detected for item in known_bad)
    )
    status = "FAIL" if failures else "PASS"
    lines: list[str] = [
        "# Requirement consistency",
        "",
        "> <!-- GENERATED FILE - run `scripts/requirements-check.sh`; do not edit by hand. -->",
        "",
        f"Status: **{status}**",
        "",
        "This report is the output of the finite requirement model in",
        "`specs/requirements/`. The model formalizes the normative fill-in,",
        "leave, handoff, and scheduling rules. The natural-language source wins.",
        "A stale model is a defect.",
        "",
        "**PASS means model satisfiability and entailment, not implementation.**",
        f"Formalized scenarios marked `@pending`: **{pending}/{len(scenarios)}**.",
        f"The pilot has {len(scenarios)} formalized scenarios from "
        f"{len({(scenario.feature, scenario.name) for scenario in scenarios})} source scenarios.",
        "The other Gherkin scenarios are outside the modeled scope.",
        "",
        "The base conjunction is the authoritative global check. Pair checks",
        "localize a conflict and are subsumed by the base check. The gate fails",
        "closed: a solver `unknown` or timeout is a failure.",
        "",
        "Defaults are separate from the current event settings. A host-set",
        "12-hour regular claim window is allowed; only the default is 24 hours.",
        "",
        "## Model",
        "",
        f"- Solver: `z3 {result.solver_version}`",
        f"- Rules: **{result.rule_count}** must rules.",
        f"- Formalized scenarios: **{len(scenarios)}** ({pending} pending).",
        f"- Reachable premises: **{len(premises)}**.",
        f"- Rule pairs checked: **{result.pair_count}**.",
        f"- Participants: **{PARTICIPANTS}**, tracks: **{TRACKS}**, "
        f"rounds: **{ROUNDS}**, turns: **{TURNS}**, invitations: **{INVITATIONS}**.",
        "",
        "## Rules",
        "",
        "| ID | Kind | Source | Rule (ASD-STE100) | Scenarios |",
        "|---|---|---|---|---|",
        *[_rule_row(rule, scenarios) for rule in rules],
        "",
        "## Scenario results",
        "",
        "A scenario must have a witness (the pre/action/post conjunction is",
        "satisfiable) and entailment (the rules plus pre/action imply the",
        "postcondition). `FAIL` means an impossible scenario or a missing",
        "postcondition.",
        "",
        "| ID | Scenario | Feature | Pending | Witness | Entailment | Rules |",
        "|---|---|---|---|---|---|---|",
        *[_scenario_row(scenario, result) for scenario in scenarios],
        "",
        "## Premises",
        "",
        "A premise is a reachable mode. The rules must be satisfiable under",
        "each premise. The premise name is tracked separately from `domain` in",
        "the unsat core.",
        "",
        "| Premise | Description | Result | Unsat core |",
        "|---|---|---|---|",
        *[_premise_row(premise, result) for premise in premises],
        "",
        "## Contradiction pairs",
        "",
    ]
    if result.contradictions:
        lines.extend(
            [
                "| Rule pair | Premise | Sources | Unsat core |",
                "|---|---|---|---|",
                *_contradiction_rows(result, rules),
            ]
        )
    else:
        lines.append("None. Every rule pair has a satisfiable conjunction under the domain.")

    lines.extend(
        [
            "",
            "## Base conjunction",
            "",
            "| Check | Result | Unsat core |",
            "|---|---|---|",
            f"| All must rules together | {result.base_status.upper()} | "
            f"{', '.join(result.base_core) if result.base_core else 'none'} |",
            "",
            "## Unknown results",
            "",
        ]
    )
    if result.unknown_queries:
        lines.extend(f"- {item}" for item in result.unknown_queries)
    else:
        lines.append("None. Every solver query returned sat or unsat.")

    lines.extend(
        [
            "",
            "## Unsat cores",
            "",
        ]
    )
    unsat_lines: list[str] = []
    if result.base_core:
        unsat_lines.append(f"- All must rules: `{', '.join(result.base_core)}`.")
    for item in result.scenarios:
        if not item.satisfiable:
            core = ", ".join(item.witness_core) if item.witness_core else "none"
            unsat_lines.append(f"- Scenario `{item.scenario_id}` witness: `{core}`.")
        if not item.entailed:
            unsat_lines.append(
                f"- Scenario `{item.scenario_id}` entailment has a counterexample."
            )
    for item in result.premises:
        if item.status == "unsat":
            core = ", ".join(item.core) if item.core else "none"
            unsat_lines.append(f"- Premise `{item.premise_id}`: `{core}`.")
    if unsat_lines:
        lines.extend(unsat_lines)
    else:
        lines.append("None. The base conjunction, scenarios, and premises are consistent.")

    lines.extend(
        [
            "",
            "## Rule necessity",
            "",
            "For each rule, the gate removes it and looks for a linked scenario",
            "whose postcondition no longer follows. `NO` means the rule is not",
            "independently exercised by its scenarios.",
            "",
            "| Rule | Exercised | Linked scenarios | Counterexample scenario |",
            "|---|---|---|---|",
            *_necessity_rows(result),
            "",
            "## Coverage gaps",
            "",
        ]
    )
    coverage_rows = _coverage_rows(result, rules)
    if coverage_rows:
        lines.extend(
            [
                "| Gap | Subject | Detail | Sources |",
                "|---|---|---|---|",
                *coverage_rows,
            ]
        )
    else:
        lines.append("None. Every rule has a scenario and every scenario has a rule.")

    lines.extend(
        [
            "",
            "## Known-bad corpus",
            "",
            "The corpus reproduces the contradictions from the 2026-11-28 reviews.",
            "The gate must detect each case and name both sources. The premise is",
            "separate from the domain definitions.",
            "",
            "| Case | Premise | Expected sources | Actual sources | Result | Unsat core |",
            "|---|---|---|---|---|---|",
            *_known_bad_rows(known_bad),
            "",
            "## Trusted gap",
            "",
            "The model is a finite formalization. A wrong or incomplete model can",
            "miss a contradiction in the natural-language source. The source links",
            "and the review of the model are mandatory. The base conjunction check",
            "covers all must rules together; pair checks localize a conflict and",
            "are subsumed by the base check. The model does not prove the product",
            "code, the SQLite schema, Discord delivery, or wall-clock time. The",
            "model uses a finite domain and integer hours.",
            "",
            "## Commands",
            "",
            "```bash",
            "scripts/requirements-check.sh           # generate the report",
            "scripts/requirements-check.sh --check   # fail when the report is stale",
            "python3 -m unittest discover -s specs/requirements/tests -t . -p 'test_*.py'",
            "```",
            "",
            "## Formula appendix",
            "",
        ]
    )
    for rule in rules:
        lines.extend(
            [
                f"### `{rule.id}`",
                "",
                "```text",
                rule.formula.sexpr(),
                "```",
                "",
            ]
        )
    return "\n".join(lines).rstrip() + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="do not write the report; fail when the report is stale",
    )
    args = parser.parse_args(argv)

    started = time.monotonic()
    world = World()
    rules = build_rules(world)
    scenarios = build_scenarios(world)
    premises = build_premises(world)
    result = check_model(world.core, rules, scenarios, premises)
    known_bad = run_corpus(world)
    elapsed = time.monotonic() - started

    report = render_report(result, rules, scenarios, premises, known_bad)
    failures: list[str] = []
    if result.base_status != "sat":
        label = "unknown" if result.base_status == "unknown" else "unsatisfiable"
        failures.append(
            f"The conjunction of all must rules is {label}: "
            + (", ".join(result.base_core) if result.base_core else "no core")
        )
    for item in result.contradictions:
        failures.append(
            f"Rule {item.left_id} contradicts rule {item.right_id} under "
            f"{item.premise}: {', '.join(item.left_sources)}; {', '.join(item.right_sources)}"
        )
    for item in result.scenarios:
        if item.status != "pass":
            failures.append(
                f"Scenario {item.scenario_id} is {item.status}: "
                f"witness={item.satisfiable}, entailed={item.entailed}"
            )
    for item in result.premises:
        if item.status != "sat":
            failures.append(
                f"Premise {item.premise_id} is {item.status}: "
                + (", ".join(item.core) if item.core else item.reason)
            )
    for item in result.necessity:
        if not item.exercised:
            failures.append(
                f"Rule {item.rule_id} is not exercised by its linked scenarios: "
                + (", ".join(item.linked_scenarios) if item.linked_scenarios else "none")
            )
    for gap in result.coverage:
        failures.append(f"Coverage gap: {gap.detail}")
    for item in known_bad:
        if not item.detected:
            failures.append(
                f"Known-bad case {item.case_id} was not detected. "
                f"Expected sources: {', '.join(item.expected_sources)}."
            )
    for unknown in result.unknown_queries:
        failures.append(f"Solver unknown: {unknown}")

    if args.check:
        current = REPORT_PATH.read_text(encoding="utf-8") if REPORT_PATH.exists() else None
        if current is None:
            print(
                f"requirements: FAILED (the report {REPORT_PATH} is missing; "
                "run scripts/requirements-check.sh)",
                file=sys.stderr,
            )
            return 2
        if current != report:
            print(
                f"requirements: FAILED (the report {REPORT_PATH} is stale; "
                "run scripts/requirements-check.sh)",
                file=sys.stderr,
            )
            return 2
    else:
        REPORT_PATH.parent.mkdir(parents=True, exist_ok=True)
        REPORT_PATH.write_text(report, encoding="utf-8")

    summary = (
        f"requirements: {result.rule_count} rules, {result.pair_count} pairs, "
        f"{len(scenarios)} scenarios, {len(premises)} premises, "
        f"{len(result.contradictions)} contradictions, {len(result.coverage)} coverage gaps, "
        f"{len(known_bad)} known-bad cases; z3 {result.solver_version}; {elapsed:.2f}s"
    )
    if failures:
        print(f"{summary} -> FAIL", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1
    print(f"{summary} -> PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
