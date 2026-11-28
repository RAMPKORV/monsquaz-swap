#!/usr/bin/env python3
"""Generate and check the requirement consistency report (D-076).

Run this script through `scripts/requirements-check.sh` from the repository
root. The script checks the finite formal model, the known-bad corpus, and the
coverage links, then writes `docs/generated/REQUIREMENT_CONSISTENCY.md`.
"""

from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from specs.requirements.checker import ModelResult, check_model  # noqa: E402
from specs.requirements.model import build_rules, build_scenarios, World  # noqa: E402
from specs.requirements.schema import (  # noqa: E402
    INVITATIONS,
    PARTICIPANTS,
    ROUNDS,
    TRACKS,
    TURNS,
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
        f"| `{rule.id}` | {_source_cell(rule.sources)} | "
        f"{_table_cell(rule.paraphrase)} | {links} |"
    )


def _scenario_row(scenario: Scenario, result: ModelResult) -> str:
    scenario_result = next(
        item for item in result.scenarios if item.scenario_id == scenario.id
    )
    outcome = "PASS" if scenario_result.satisfiable else "FAIL"
    rules = ", ".join(f"`{item}`" for item in scenario.rules) if scenario.rules else "**none**"
    feature = f"[`{scenario.feature}`]({REPORT_PREFIX}{scenario.feature})"
    return (
        f"| `{scenario.id}` | {_table_cell(scenario.name)} | "
        f"{feature} | {outcome} | {rules} |"
    )


def _contradiction_rows(result: ModelResult, rules: list[Rule]) -> list[str]:
    by_id = {rule.id: rule for rule in rules}
    rows: list[str] = []
    for item in result.contradictions:
        left = by_id[item.left_id]
        right = by_id[item.right_id]
        rows.append(
            f"| `{item.left_id}` + `{item.right_id}` | "
            f"{_source_cell(left.sources)}; {_source_cell(right.sources)} | "
            f"{', '.join(item.core) if item.core else 'domain'} |"
        )
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


def _known_bad_rows(known_bad: list[KnownBadResult]) -> list[str]:
    rows: list[str] = []
    for item in known_bad:
        outcome = "DETECTED" if item.detected else "**MISSED**"
        expected = ", ".join(
            f"[{ref}]({REPORT_PREFIX}ai-rules/DECISIONS.md)" for ref in item.expected_sources
        )
        actual = (
            ", ".join(
                f"[{ref}]({REPORT_PREFIX}ai-rules/DECISIONS.md)"
                for ref in item.actual_sources
            )
            if item.actual_sources
            else "none"
        )
        core = ", ".join(item.core) if item.core else "none"
        rows.append(
            f"| `{item.case_id}` | {expected} | {actual} | {outcome} | {core} |"
        )
    return rows


def render_report(
    result: ModelResult,
    rules: list[Rule],
    scenarios: list[Scenario],
    known_bad: list[KnownBadResult],
) -> str:
    failures = (
        not result.satisfiable
        or bool(result.contradictions)
        or any(not item.satisfiable for item in result.scenarios)
        or bool(result.coverage)
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
        "## Model",
        "",
        f"- Solver: `z3 {result.solver_version}`",
        f"- Rules: **{result.rule_count}** must rules.",
        f"- Formalized scenarios: **{len(scenarios)}**.",
        f"- Rule pairs checked: **{result.pair_count}**.",
        f"- Participants: **{PARTICIPANTS}**, tracks: **{TRACKS}**, "
        f"rounds: **{ROUNDS}**, turns: **{TURNS}**, invitations: **{INVITATIONS}**.",
        "",
        "## Rules",
        "",
        "| ID | Source | Rule (ASD-STE100) | Scenarios |",
        "|---|---|---|---|",
        *[_rule_row(rule, scenarios) for rule in rules],
        "",
        "## Scenario results",
        "",
        "| ID | Scenario | Feature | Result | Rules |",
        "|---|---|---|---|---|",
        *[_scenario_row(scenario, result) for scenario in scenarios],
        "",
        "## Contradiction pairs",
        "",
    ]
    if result.contradictions:
        lines.extend(
            [
                "| Rule pair | Sources | Unsat core |",
                "|---|---|---|",
                *_contradiction_rows(result, rules),
            ]
        )
    else:
        lines.append("None. Every rule pair has a satisfiable conjunction.")

    lines.extend(
        [
            "",
            "## Base conjunction",
            "",
            "| Check | Result | Unsat core |",
            "|---|---|---|",
            f"| All must rules together | {'PASS' if result.satisfiable else 'FAIL'} | "
            f"{', '.join(result.base_core) if result.base_core else 'none'} |",
            "",
            "## Unsat cores",
            "",
        ]
    )
    scenario_failures = [item for item in result.scenarios if not item.satisfiable]
    if result.base_core or scenario_failures:
        if result.base_core:
            lines.append(f"- All must rules: `{', '.join(result.base_core)}`.")
        for item in scenario_failures:
            core = ", ".join(item.core) if item.core else "domain"
            lines.append(f"- Scenario `{item.scenario_id}`: `{core}`.")
    else:
        lines.append("None. Every scenario is satisfiable under all must rules.")

    lines.extend(
        [
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
            "The gate must detect each case and name both sources.",
            "",
            "| Case | Expected sources | Actual sources | Result | Unsat core |",
            "|---|---|---|---|---|",
            *_known_bad_rows(known_bad),
            "",
            "## Trusted gap",
            "",
            "The model is a finite formalization. A wrong or incomplete model can",
            "miss a contradiction in the natural-language source. The source links",
            "and the review of the model are mandatory. The pair check covers two",
            "rules at a time; the base conjunction check covers all must rules",
            "together. The model does not prove",
            "the product code, the SQLite schema, Discord delivery, or wall-clock",
            "time. The model uses a finite domain and integer hours.",
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
    result = check_model(world.core, rules, scenarios)
    known_bad = run_corpus(world)
    elapsed = time.monotonic() - started

    report = render_report(result, rules, scenarios, known_bad)
    failures: list[str] = []
    if not result.satisfiable:
        failures.append(
            "The conjunction of all must rules is unsatisfiable: "
            + ", ".join(result.base_core)
        )
    for item in result.contradictions:
        failures.append(
            f"Rule {item.left_id} contradicts rule {item.right_id}: "
            f"{', '.join(item.left_sources)}; {', '.join(item.right_sources)}"
        )
    for item in result.scenarios:
        if not item.satisfiable:
            failures.append(
                f"Scenario {item.scenario_id} is unsatisfiable: "
                + (", ".join(item.core) if item.core else "domain")
            )
    for gap in result.coverage:
        failures.append(f"Coverage gap: {gap.detail}")
    for item in known_bad:
        if not item.detected:
            failures.append(
                f"Known-bad case {item.case_id} was not detected. "
                f"Expected sources: {', '.join(item.expected_sources)}."
            )

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
        f"{len(scenarios)} scenarios, {len(result.contradictions)} contradictions, "
        f"{len(result.coverage)} coverage gaps, {len(known_bad)} known-bad cases; "
        f"z3 {result.solver_version}; {elapsed:.2f}s"
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
