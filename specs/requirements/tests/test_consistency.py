"""Tests for the requirement consistency gate (D-076).

The gate must fail closed, detect ineligible tier offers, require scenario
entailment and witnesses, prove rule necessity, and allow host-set settings
that differ from the defaults.
"""

from __future__ import annotations

import unittest
from dataclasses import replace
from pathlib import Path

import z3

from specs.requirements.check import render_report
from specs.requirements.checker import check_coverage, check_model, check_premises
from specs.requirements.model import build_premises, build_rules, build_scenarios, World
from specs.requirements.schema import Premise, Rule, Source
from specs.requirements.tests.known_bad import run_corpus

REPO_ROOT = Path(__file__).resolve().parents[3]

LINK_MANIFEST = {
    "R-CLAIM-WINDOW-REGULAR": ("S-CLAIM-24",),
    "R-CLAIM-WINDOW-FILLIN": ("S-FILL-SEQUENCE",),
    "R-CLAIM-REMINDER": ("S-CLAIM-REMINDER",),
    "R-EDIT-WINDOW": ("S-FILL-DUE-CLAIM", "S-FILL-DUE-APPOINT", "S-CLAIM-24"),
    "R-CLAIM-EDIT-DUE": ("S-FILL-DUE-CLAIM", "S-CLAIM-24"),
    "R-FILL-AUTO-CHAIN": ("S-FILL-SEQUENCE",),
    "R-FILL-HOST-LIST": ("S-FILL-SEQUENCE",),
    "R-FILL-SEQUENTIAL": ("S-FILL-SEQUENCE",),
    "R-FILL-FIRST-ACCEPT": ("S-FILL-SEQUENCE",),
    "R-FILL-DUE-APPOINT": ("S-FILL-DUE-APPOINT",),
    "R-FILL-DUE-CLAIM": ("S-FILL-DUE-CLAIM",),
    "R-TIER1-OFFER-ELIGIBLE": ("S-FILL-POOL",),
    "R-TIER2-OFFER-ELIGIBLE": ("S-TIER2-EXTRA-TURN",),
    "R-TIER2-ONLY-NO-TIER1": ("S-TIER2-EXTRA-TURN",),
    "R-TIER2-ONE-PER-ROUND": ("S-TIER2-EXTRA-TURN",),
    "R-TIER2-NO-EVENT-CAP": ("S-TIER2-EXTRA-TURN",),
    "R-BUDGET-REGULAR": ("S-TIER2-EXTRA-TURN",),
    "R-BUDGET-TIER2-EXTRA": ("S-TIER2-EXTRA-TURN",),
    "R-FILL-PASS-UNCHANGED": ("S-FILL-NO-ELIGIBLE", "S-FILL-PASS-ASYNC"),
    "R-NO-REPEAT": ("S-SCHEDULE-ROUND",),
    "R-NO-OWN-TRACK": ("S-SCHEDULE-ROUND",),
    "R-ONE-ASSIGNMENT-PER-PERSON-ROUND": ("S-SCHEDULE-ROUND",),
    "R-ONE-ASSIGNMENT-PER-TRACK-ROUND": ("S-SCHEDULE-ROUND",),
    "R-CLASSIC-LENGTH": ("S-CLASSIC-LENGTH",),
    "R-ROUNDS-HOST-OPENED": ("S-CLASSIC-LENGTH",),
    "R-NO-AUTO-ADVANCE": ("S-SOFT-DEADLINE",),
    "R-ONE-OPEN-ROUND": ("S-ONE-OPEN-ROUND",),
    "R-STRICT-REFUSAL": ("S-STRICT-REFUSAL",),
    "R-REVEAL-AFTER-HOST": ("S-REVEAL-NO-HOST",),
    "R-REVEAL-HOST-TRIGGER": ("S-REVEAL-HOST",),
    "R-REVEAL-NO-ACTIVE": ("S-REVEAL-PENDING",),
    "R-LEAVE-SEEN": ("S-LEAVE-HOLDING",),
    "R-LEAVE-NO-FUTURE": ("S-LEAVE-HOLDING",),
    "R-RESTORE-FILLIN-ONLY": ("S-LEAVER-RETURN",),
    "R-RESTORE-EXPIRY": ("S-LEAVER-RETURN",),
    "R-RESTORE-EXPIRY-SETTING": ("S-LEAVER-RETURN", "S-RESTORE-EXPIRY"),
    "R-RESTORE-EXPIRED-BLOCKS": ("S-RESTORE-EXPIRY",),
}


class UnknownSolver:
    """A solver that always returns unknown, to prove the fail-closed path."""

    def assert_and_track(self, *args, **kwargs):
        return None

    def check(self):
        return z3.unknown

    def reason_unknown(self):
        return "mock solver timeout"

    def unsat_core(self):
        return []


class RequirementConsistencyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.world = World()
        cls.rules = build_rules(cls.world)
        cls.scenarios = build_scenarios(cls.world)
        cls.premises = build_premises(cls.world)
        cls.result = check_model(
            cls.world.core, cls.rules, cls.scenarios, cls.premises
        )
        cls.known_bad = run_corpus(cls.world)

    def _check_sat(self, extra):
        solver = z3.Solver()
        solver.add(self.world.core)
        for rule in self.rules:
            solver.add(rule.formula)
        solver.add(extra)
        return solver.check()

    def test_all_must_rules_are_satisfiable(self):
        self.assertTrue(
            self.result.satisfiable,
            f"unsatisfiable must rules: {self.result.base_core}",
        )
        self.assertEqual("sat", self.result.base_status)

    def test_no_rule_pair_contradicts(self):
        self.assertEqual(
            [],
            list(self.result.contradictions),
            "a rule pair has an unsatisfiable conjunction",
        )

    def test_every_formalized_scenario_has_a_witness_and_entailment(self):
        failed = [item for item in self.result.scenarios if item.status != "pass"]
        self.assertEqual([], failed, "a scenario is impossible or its postcondition does not follow")

    def test_every_reachable_premise_is_satisfiable(self):
        failed = [item for item in self.result.premises if item.status != "sat"]
        self.assertEqual([], failed, "a reachable premise contradicts the rules")

    def test_every_rule_is_independently_exercised(self):
        failed = [item.rule_id for item in self.result.necessity if not item.exercised]
        self.assertEqual([], failed, "a rule is not necessary for any linked scenario")

    def test_coverage_has_no_gaps(self):
        self.assertEqual([], list(self.result.coverage), "the model has a coverage gap")

    def test_expected_links_match_manifest(self):
        actual = {
            rule.id: tuple(
                scenario.id for scenario in self.scenarios if rule.id in scenario.rules
            )
            for rule in self.rules
        }
        self.assertEqual(set(LINK_MANIFEST), set(actual))
        for rule_id, expected in LINK_MANIFEST.items():
            self.assertEqual(expected, actual[rule_id], f"link drift for {rule_id}")

    def test_pending_flags_match_the_feature_tags(self):
        for scenario in self.scenarios:
            with self.subTest(scenario=scenario.id):
                pending = _feature_pending(scenario.feature, scenario.name)
                self.assertIsNotNone(pending, "source scenario is missing from the feature file")
                self.assertEqual(scenario.pending, pending)

    def test_known_bad_corpus_is_detected(self):
        self.assertEqual(3, len(self.known_bad))
        for item in self.known_bad:
            with self.subTest(case=item.case_id):
                self.assertTrue(item.detected, f"{item.case_id} was not detected")
                self.assertTrue(
                    set(item.expected_sources) <= set(item.actual_sources),
                    f"{item.case_id} did not name both sources: {item.actual_sources}",
                )
                self.assertTrue(
                    set(item.rule_ids) <= set(item.core),
                    f"{item.case_id} core does not name both rules: {item.core}",
                )
                self.assertTrue(item.premise.startswith("premise:"))
                self.assertNotIn("domain", item.premise)

    def test_corrected_model_does_not_contain_the_historical_pairs(self):
        historical = {
            frozenset(("D-014", "D-069")),
            frozenset(("D-014", "D-070")),
            frozenset(("D-014", "D-050")),
        }
        for contradiction in self.result.contradictions:
            sources = frozenset(
                set(contradiction.left_sources) | set(contradiction.right_sources)
            )
            self.assertNotIn(sources, historical)

    def test_tier1_offer_to_a_removed_participant_is_unsat(self):
        extra = z3.And(
            self.world.tier1_offer[0][0],
            self.world.role[0] == 1,  # FILL_IN
            self.world.membership[0] == 4,  # REMOVED
            self.world.ready[0],
            z3.Not(self.world.seen[0][0]),
            self.world.owner[0] == 1,
            self.world.fill_in_source[0] == 0,
        )
        self.assertEqual(z3.unsat, self._check_sat(extra))

    def test_tier2_offer_without_a_submitted_own_turn_is_unsat(self):
        extra = z3.And(
            self.world.tier2_offer[1][0],
            self.world.role[1] == 0,  # PARTICIPANT
            self.world.membership[1] == 2,  # ACTIVE
            self.world.ready[1],
            z3.Not(self.world.submitted_own[1]),
            z3.Not(self.world.seen[1][0]),
            self.world.owner[0] == 0,
            self.world.plan_never_gives_track[1][0],
        )
        self.assertEqual(z3.unsat, self._check_sat(extra))

    def test_tier2_offer_without_a_plan_gap_is_unsat(self):
        extra = z3.And(
            self.world.tier2_offer[1][0],
            self.world.role[1] == 0,  # PARTICIPANT
            self.world.membership[1] == 2,  # ACTIVE
            self.world.ready[1],
            self.world.submitted_own[1],
            z3.Not(self.world.seen[1][0]),
            self.world.owner[0] == 0,
            z3.Not(self.world.plan_never_gives_track[1][0]),
        )
        self.assertEqual(z3.unsat, self._check_sat(extra))

    def test_tier2_offer_with_two_turns_in_the_offer_round_is_unsat(self):
        extra = z3.And(
            self.world.tier2_offer[1][0],
            self.world.tier2_offer_round[1][0] == 0,
            self.world.tier2_turns[1][0] == 2,
            self.world.role[1] == 0,  # PARTICIPANT
            self.world.membership[1] == 2,  # ACTIVE
            self.world.ready[1],
            self.world.submitted_own[1],
            z3.Not(self.world.seen[1][0]),
            self.world.owner[0] == 0,
            self.world.plan_never_gives_track[1][0],
        )
        self.assertEqual(z3.unsat, self._check_sat(extra))

    def test_solver_unknown_fails_closed(self):
        result = check_model(
            self.world.core,
            self.rules[:2],
            [],
            [],
            solver_factory=lambda core: UnknownSolver(),
        )
        self.assertEqual("unknown", result.base_status)
        self.assertFalse(result.satisfiable)
        self.assertTrue(
            any("base conjunction" in item for item in result.unknown_queries),
            f"unknown was not reported: {result.unknown_queries}",
        )

    def test_premise_dependent_contradiction_is_caught(self):
        value = z3.Int("premise_test_value")
        rules = [
            Rule(
                id="KB-PREMISE-ONE",
                sources=(Source("decision", "D-TEST", "ai-rules/DECISIONS.md"),),
                paraphrase="The test value must be one in classic mode.",
                formula=z3.Implies(self.world.mode == 0, value == 1),
            ),
            Rule(
                id="KB-PREMISE-TWO",
                sources=(Source("decision", "D-TEST", "ai-rules/DECISIONS.md"),),
                paraphrase="The test value must be two in classic mode.",
                formula=z3.Implies(self.world.mode == 0, value == 2),
            ),
        ]
        premise = Premise(
            "P-TEST-CLASSIC",
            "A test premise for classic mode.",
            self.world.mode == 0,
        )
        results, unknowns = check_premises(self.world.core, rules, [premise])
        self.assertEqual([], unknowns)
        self.assertEqual("unsat", results[0].status)
        self.assertIn("premise:P-TEST-CLASSIC", results[0].core)
        self.assertIn("KB-PREMISE-ONE", results[0].core)
        self.assertIn("KB-PREMISE-TWO", results[0].core)

    def test_host_set_12h_regular_claim_window_is_sat(self):
        extra = z3.And(
            self.world.claim_window_h[0] == 12,
            self.world.claim_reminder_h == 6,
        )
        self.assertEqual(z3.sat, self._check_sat(extra))

    def test_replan_opt_in_allows_a_personal_track(self):
        extra = z3.And(
            self.world.replan_allowed,
            self.world.restored[0],
            self.world.role[0] == 0,  # PARTICIPANT
            self.world.has_personal_track[0],
        )
        self.assertEqual(z3.sat, self._check_sat(extra))

    def test_restore_boundary_is_exclusive(self):
        at_boundary = z3.And(
            self.world.invitation_action[0],
            self.world.invitation_sent_h[0] == 100,
            self.world.invitation_action_h[0] == 268,
        )
        before_boundary = z3.And(
            self.world.invitation_action[0],
            self.world.invitation_sent_h[0] == 100,
            self.world.invitation_action_h[0] == 267,
        )
        self.assertEqual(z3.unsat, self._check_sat(at_boundary))
        self.assertEqual(z3.sat, self._check_sat(before_boundary))

    def test_seeded_deletion_of_no_repeat_is_caught(self):
        mutated = [
            replace(rule, formula=z3.BoolVal(True)) if rule.id == "R-NO-REPEAT" else rule
            for rule in self.rules
        ]
        result = check_model(self.world.core, mutated, self.scenarios, self.premises)
        failed = [item for item in result.scenarios if item.status != "pass"]
        self.assertTrue(failed, "deleting R-NO-REPEAT was not caught")
        self.assertIn("S-SCHEDULE-ROUND", {item.scenario_id for item in failed})

    def test_seeded_claim_window_mutation_is_caught(self):
        mutated = [
            replace(rule, formula=self.world.default_fill_in_claim_window_h == 12)
            if rule.id == "R-CLAIM-WINDOW-FILLIN"
            else rule
            for rule in self.rules
        ]
        result = check_model(self.world.core, mutated, self.scenarios, self.premises)
        failed = [item for item in result.scenarios if item.status != "pass"]
        self.assertIn("S-FILL-SEQUENCE", {item.scenario_id for item in failed})

    def test_removed_rule_reports_an_unknown_link(self):
        rules = [rule for rule in self.rules if rule.id != "R-FILL-FIRST-ACCEPT"]
        gaps = check_coverage(rules, self.scenarios)
        self.assertTrue(
            any(
                gap.kind == "unknown_rule" and gap.subject == "R-FILL-FIRST-ACCEPT"
                for gap in gaps
            ),
            "a removed rule left no coverage gap",
        )

    def test_removed_rule_and_link_reports_a_scenario_without_a_rule(self):
        rules = [rule for rule in self.rules if rule.id != "R-ONE-OPEN-ROUND"]
        scenarios = [
            replace(
                scenario,
                rules=tuple(
                    rule_id
                    for rule_id in scenario.rules
                    if rule_id != "R-ONE-OPEN-ROUND"
                ),
            )
            if scenario.id == "S-ONE-OPEN-ROUND"
            else scenario
            for scenario in self.scenarios
        ]
        gaps = check_coverage(rules, scenarios)
        self.assertTrue(
            any(
                gap.kind == "scenario_without_rule" and gap.subject == "S-ONE-OPEN-ROUND"
                for gap in gaps
            ),
            "a scenario without a rule left no coverage gap",
        )

    def test_report_has_the_required_sections(self):
        report = render_report(
            self.result, self.rules, self.scenarios, self.premises, self.known_bad
        )
        for heading in (
            "# Requirement consistency",
            "## Rules",
            "## Scenario results",
            "## Premises",
            "## Contradiction pairs",
            "## Base conjunction",
            "## Unknown results",
            "## Unsat cores",
            "## Rule necessity",
            "## Coverage gaps",
            "## Known-bad corpus",
            "## Trusted gap",
        ):
            self.assertIn(heading, report)
        self.assertIn(f"z3 {z3.get_version_string()}", report)
        self.assertIn("Status: **PASS**", report)
        self.assertIn("PASS means model satisfiability and entailment, not implementation.", report)


def _feature_pending(feature: str, name: str):
    """Return the @pending state of a source scenario, or None if missing."""
    tags: list[str] = []
    for raw in (REPO_ROOT / feature).read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line.startswith("@"):
            tags.extend(line.split())
            continue
        if line.startswith("Feature:"):
            tags = []
            continue
        if line.startswith("Scenario:") or line.startswith("Scenario Outline:"):
            current = line.split(":", 1)[1].strip()
            if current == name:
                return "@pending" in tags
            tags = []
    return None


if __name__ == "__main__":
    unittest.main()
