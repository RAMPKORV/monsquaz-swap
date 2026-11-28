"""Solver checks for the requirement consistency model (D-076)."""

from __future__ import annotations

from dataclasses import dataclass

import z3

from .schema import Rule, Scenario


@dataclass(frozen=True)
class Contradiction:
    left_id: str
    right_id: str
    left_sources: tuple[str, ...]
    right_sources: tuple[str, ...]
    core: tuple[str, ...]


@dataclass(frozen=True)
class ScenarioResult:
    scenario_id: str
    satisfiable: bool
    core: tuple[str, ...]


@dataclass(frozen=True)
class CoverageGap:
    kind: str
    subject: str
    detail: str


@dataclass(frozen=True)
class ModelResult:
    satisfiable: bool
    base_core: tuple[str, ...]
    contradictions: tuple[Contradiction, ...]
    scenarios: tuple[ScenarioResult, ...]
    coverage: tuple[CoverageGap, ...]
    rule_count: int
    pair_count: int
    solver_version: str


def _solver(core):
    solver = z3.Solver()
    # Every check is small. The timeout is a safety net for a bad edit.
    solver.set("timeout", 10_000)
    solver.assert_and_track(core, "domain")
    return solver


def _core_names(solver) -> tuple[str, ...]:
    return tuple(str(item) for item in solver.unsat_core())


def find_contradictions(core, rules: list[Rule]) -> list[Contradiction]:
    """Return every rule pair whose conjunction is unsatisfiable."""
    contradictions: list[Contradiction] = []
    for index, left in enumerate(rules):
        for right in rules[index + 1 :]:
            solver = _solver(core)
            solver.assert_and_track(left.formula, left.id)
            solver.assert_and_track(right.formula, right.id)
            if solver.check() == z3.unsat:
                contradictions.append(
                    Contradiction(
                        left_id=left.id,
                        right_id=right.id,
                        left_sources=left.source_refs(),
                        right_sources=right.source_refs(),
                        core=_core_names(solver),
                    )
                )
    return contradictions


def check_scenarios(core, rules: list[Rule], scenarios: list[Scenario]) -> list[ScenarioResult]:
    """Check that every formalized scenario is satisfiable under all rules."""
    results: list[ScenarioResult] = []
    for scenario in scenarios:
        solver = _solver(core)
        for rule in rules:
            solver.assert_and_track(rule.formula, rule.id)
        solver.assert_and_track(scenario.formula, scenario.id)
        satisfiable = solver.check() != z3.unsat
        results.append(
            ScenarioResult(
                scenario_id=scenario.id,
                satisfiable=satisfiable,
                core=() if satisfiable else _core_names(solver),
            )
        )
    return results


def check_coverage(rules: list[Rule], scenarios: list[Scenario]) -> list[CoverageGap]:
    """Report a rule without a scenario and a scenario without a rule."""
    known = {rule.id for rule in rules}
    linked = {rule.id: [] for rule in rules}  # type: dict[str, list[str]]
    gaps: list[CoverageGap] = []
    for scenario in scenarios:
        valid_links: list[str] = []
        for rule_id in scenario.rules:
            if rule_id in known:
                linked[rule_id].append(scenario.id)
                valid_links.append(rule_id)
            else:
                gaps.append(
                    CoverageGap(
                        kind="unknown_rule",
                        subject=rule_id,
                        detail=f"Scenario {scenario.id} links rule {rule_id}, but the rule is missing.",
                    )
                )
        if not valid_links:
            gaps.append(
                CoverageGap(
                    kind="scenario_without_rule",
                    subject=scenario.id,
                    detail=f"Scenario {scenario.id} has no link to a rule in the model.",
                )
            )
    for rule in rules:
        if not linked[rule.id]:
            gaps.append(
                CoverageGap(
                    kind="rule_without_scenario",
                    subject=rule.id,
                    detail=f"Rule {rule.id} has no formalized scenario link.",
                )
            )
    order = {"rule_without_scenario": 0, "scenario_without_rule": 1, "unknown_rule": 2}
    return sorted(gaps, key=lambda gap: (order.get(gap.kind, 9), gap.subject))


def check_model(core, rules: list[Rule], scenarios: list[Scenario]) -> ModelResult:
    """Run all four checks from D-076."""
    must_rules = [rule for rule in rules if rule.severity == "must"]
    solver = _solver(core)
    for rule in must_rules:
        solver.assert_and_track(rule.formula, rule.id)
    satisfiable = solver.check() != z3.unsat
    base_core = () if satisfiable else _core_names(solver)
    contradictions = find_contradictions(core, must_rules)
    scenario_results = check_scenarios(core, must_rules, scenarios)
    coverage = check_coverage(rules, scenarios)
    pair_count = len(must_rules) * (len(must_rules) - 1) // 2
    return ModelResult(
        satisfiable=satisfiable,
        base_core=base_core,
        contradictions=tuple(contradictions),
        scenarios=tuple(scenario_results),
        coverage=tuple(coverage),
        rule_count=len(must_rules),
        pair_count=pair_count,
        solver_version=z3.get_version_string(),
    )
