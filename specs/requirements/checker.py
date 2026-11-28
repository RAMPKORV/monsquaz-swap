"""Solver checks for the requirement consistency model (D-076).

The gate fails closed: a solver `unknown` or timeout is a failure, never a
pass. Scenario checks prove both a witness (the scenario is possible) and
entailment (the postcondition follows from the rules). A necessity check
removes each rule and looks for a linked scenario that no longer entails its
postcondition.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable

import z3

from .schema import Premise, Rule, Scenario

SolverFactory = Callable[[object], object]


@dataclass(frozen=True)
class QueryResult:
    status: str  # "sat", "unsat", or "unknown"
    core: tuple[str, ...]
    reason: str


@dataclass(frozen=True)
class Contradiction:
    left_id: str
    right_id: str
    left_sources: tuple[str, ...]
    right_sources: tuple[str, ...]
    core: tuple[str, ...]
    premise: str


@dataclass(frozen=True)
class ScenarioResult:
    scenario_id: str
    status: str  # "pass", "fail", or "unknown"
    satisfiable: bool
    entailed: bool
    witness_core: tuple[str, ...]
    entailment_core: tuple[str, ...]
    unknown: tuple[str, ...]


@dataclass(frozen=True)
class PremiseResult:
    premise_id: str
    description: str
    status: str  # "sat", "unsat", or "unknown"
    core: tuple[str, ...]
    reason: str


@dataclass(frozen=True)
class NecessityResult:
    rule_id: str
    exercised: bool
    linked_scenarios: tuple[str, ...]
    failing_scenarios: tuple[str, ...]
    unknown: tuple[str, ...]


@dataclass(frozen=True)
class CoverageGap:
    kind: str
    subject: str
    detail: str


@dataclass(frozen=True)
class ModelResult:
    satisfiable: bool
    base_status: str
    base_core: tuple[str, ...]
    contradictions: tuple[Contradiction, ...]
    scenarios: tuple[ScenarioResult, ...]
    premises: tuple[PremiseResult, ...]
    necessity: tuple[NecessityResult, ...]
    coverage: tuple[CoverageGap, ...]
    unknown_queries: tuple[str, ...]
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


def _query(solver) -> QueryResult:
    result = solver.check()
    if result == z3.sat:
        return QueryResult("sat", (), "")
    if result == z3.unsat:
        return QueryResult("unsat", _core_names(solver), "")
    reason = ""
    if hasattr(solver, "reason_unknown"):
        try:
            reason = str(solver.reason_unknown())
        except Exception:  # pragma: no cover - defensive only
            reason = "unknown"
    return QueryResult("unknown", (), reason or "unknown")


def find_contradictions(
    core,
    rules: list[Rule],
    premise=None,
    premise_id: str = "premise",
    solver_factory: SolverFactory | None = None,
) -> tuple[list[Contradiction], list[str]]:
    """Return every rule pair whose conjunction is unsatisfiable.

    A solver unknown is returned separately and makes the gate fail.
    """
    factory = solver_factory or _solver
    contradictions: list[Contradiction] = []
    unknowns: list[str] = []
    for index, left in enumerate(rules):
        for right in rules[index + 1 :]:
            solver = factory(core)
            if premise is not None:
                solver.assert_and_track(premise, premise_id)
            solver.assert_and_track(left.formula, left.id)
            solver.assert_and_track(right.formula, right.id)
            result = _query(solver)
            if result.status == "unsat":
                contradictions.append(
                    Contradiction(
                        left_id=left.id,
                        right_id=right.id,
                        left_sources=left.source_refs(),
                        right_sources=right.source_refs(),
                        core=result.core,
                        premise=premise_id if premise is not None else "domain",
                    )
                )
            elif result.status == "unknown":
                where = f" under {premise_id}" if premise is not None else ""
                unknowns.append(
                    f"pair {left.id} + {right.id}{where}: {result.reason}"
                )
    return contradictions, unknowns


def check_scenarios(
    core,
    rules: list[Rule],
    scenarios: list[Scenario],
    solver_factory: SolverFactory | None = None,
) -> tuple[list[ScenarioResult], list[str]]:
    """Check a witness and entailment for every formalized scenario."""
    factory = solver_factory or _solver
    results: list[ScenarioResult] = []
    unknowns: list[str] = []
    for scenario in scenarios:
        witness = factory(core)
        for rule in rules:
            witness.assert_and_track(rule.formula, rule.id)
        witness.assert_and_track(scenario.body, f"{scenario.id}:body")
        witness.assert_and_track(scenario.postcondition, f"{scenario.id}:post")
        witness_result = _query(witness)

        entailment = factory(core)
        for rule in rules:
            entailment.assert_and_track(rule.formula, rule.id)
        entailment.assert_and_track(scenario.body, f"{scenario.id}:body")
        entailment.assert_and_track(z3.Not(scenario.postcondition), f"{scenario.id}:not_post")
        entailment_result = _query(entailment)

        if witness_result.status == "unknown":
            unknowns.append(
                f"scenario {scenario.id} witness: {witness_result.reason}"
            )
        if entailment_result.status == "unknown":
            unknowns.append(
                f"scenario {scenario.id} entailment: {entailment_result.reason}"
            )
        if witness_result.status == "unknown" or entailment_result.status == "unknown":
            status = "unknown"
        elif witness_result.status == "sat" and entailment_result.status == "unsat":
            status = "pass"
        else:
            status = "fail"
        results.append(
            ScenarioResult(
                scenario_id=scenario.id,
                status=status,
                satisfiable=witness_result.status == "sat",
                entailed=entailment_result.status == "unsat",
                witness_core=witness_result.core,
                entailment_core=entailment_result.core,
                unknown=tuple(
                    item
                    for item in (
                        witness_result.reason if witness_result.status == "unknown" else "",
                        entailment_result.reason
                        if entailment_result.status == "unknown"
                        else "",
                    )
                    if item
                ),
            )
        )
    return results, unknowns


def check_premises(
    core,
    rules: list[Rule],
    premises: list[Premise],
    solver_factory: SolverFactory | None = None,
) -> tuple[list[PremiseResult], list[str]]:
    """Check that every reachable premise is consistent with all must rules."""
    factory = solver_factory or _solver
    results: list[PremiseResult] = []
    unknowns: list[str] = []
    for premise in premises:
        solver = factory(core)
        solver.assert_and_track(premise.formula, f"premise:{premise.id}")
        for rule in rules:
            solver.assert_and_track(rule.formula, rule.id)
        result = _query(solver)
        results.append(
            PremiseResult(
                premise_id=premise.id,
                description=premise.description,
                status=result.status,
                core=result.core,
                reason=result.reason,
            )
        )
        if result.status == "unknown":
            unknowns.append(f"premise {premise.id}: {result.reason}")
    return results, unknowns


def check_necessity(
    core,
    rules: list[Rule],
    scenarios: list[Scenario],
    solver_factory: SolverFactory | None = None,
) -> tuple[list[NecessityResult], list[str]]:
    """Remove one rule and look for a linked scenario that loses entailment."""
    factory = solver_factory or _solver
    results: list[NecessityResult] = []
    unknowns: list[str] = []
    for rule in rules:
        linked = [scenario for scenario in scenarios if rule.id in scenario.rules]
        failing: list[str] = []
        if linked:
            without = [other for other in rules if other.id != rule.id]
            for scenario in linked:
                solver = factory(core)
                for other in without:
                    solver.assert_and_track(other.formula, other.id)
                solver.assert_and_track(scenario.body, f"{scenario.id}:body")
                solver.assert_and_track(
                    z3.Not(scenario.postcondition), f"{scenario.id}:not_post"
                )
                result = _query(solver)
                if result.status == "sat":
                    failing.append(scenario.id)
                elif result.status == "unknown":
                    unknowns.append(
                        f"necessity {rule.id} on {scenario.id}: {result.reason}"
                    )
        results.append(
            NecessityResult(
                rule_id=rule.id,
                exercised=bool(failing),
                linked_scenarios=tuple(scenario.id for scenario in linked),
                failing_scenarios=tuple(failing),
                unknown=tuple(
                    item for item in unknowns if item.startswith(f"necessity {rule.id} ")
                ),
            )
        )
    return results, unknowns


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


def check_model(
    core,
    rules: list[Rule],
    scenarios: list[Scenario],
    premises: list[Premise] | None = None,
    solver_factory: SolverFactory | None = None,
) -> ModelResult:
    """Run all D-076 checks, including premises, entailment, and necessity."""
    factory = solver_factory or _solver
    must_rules = [rule for rule in rules if rule.severity == "must"]
    unknowns: list[str] = []

    solver = factory(core)
    for rule in must_rules:
        solver.assert_and_track(rule.formula, rule.id)
    base = _query(solver)
    if base.status == "unknown":
        unknowns.append(f"base conjunction: {base.reason}")

    contradictions, pair_unknowns = find_contradictions(
        core, must_rules, solver_factory=factory
    )
    unknowns.extend(pair_unknowns)
    scenario_results, scenario_unknowns = check_scenarios(
        core, must_rules, scenarios, solver_factory=factory
    )
    unknowns.extend(scenario_unknowns)
    premise_results, premise_unknowns = check_premises(
        core, must_rules, premises or [], solver_factory=factory
    )
    unknowns.extend(premise_unknowns)
    necessity_results, necessity_unknowns = check_necessity(
        core, must_rules, scenarios, solver_factory=factory
    )
    unknowns.extend(necessity_unknowns)
    coverage = check_coverage(must_rules, scenarios)
    pair_count = len(must_rules) * (len(must_rules) - 1) // 2
    return ModelResult(
        satisfiable=base.status == "sat",
        base_status=base.status,
        base_core=base.core,
        contradictions=tuple(contradictions),
        scenarios=tuple(scenario_results),
        premises=tuple(premise_results),
        necessity=tuple(necessity_results),
        coverage=tuple(coverage),
        unknown_queries=tuple(unknowns),
        rule_count=len(must_rules),
        pair_count=pair_count,
        solver_version=z3.get_version_string(),
    )
