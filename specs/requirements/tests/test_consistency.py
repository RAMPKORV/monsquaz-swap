"""Tests for the requirement consistency gate (D-076)."""

from __future__ import annotations

import unittest
from dataclasses import replace

import z3

from specs.requirements.check import render_report
from specs.requirements.checker import check_coverage, check_model, find_contradictions
from specs.requirements.model import build_rules, build_scenarios, World
from specs.requirements.tests.known_bad import run_corpus


class RequirementConsistencyTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.world = World()
        cls.rules = build_rules(cls.world)
        cls.scenarios = build_scenarios(cls.world)
        cls.result = check_model(cls.world.core, cls.rules, cls.scenarios)
        cls.known_bad = run_corpus(cls.world)

    def test_all_must_rules_are_satisfiable(self):
        self.assertTrue(
            self.result.satisfiable,
            f"unsatisfiable must rules: {self.result.base_core}",
        )

    def test_no_rule_pair_contradicts(self):
        self.assertEqual(
            [],
            list(self.result.contradictions),
            "a rule pair has an unsatisfiable conjunction",
        )

    def test_every_formalized_scenario_is_satisfiable(self):
        failed = [item for item in self.result.scenarios if not item.satisfiable]
        self.assertEqual([], failed, "a formalized scenario is impossible under the rules")

    def test_coverage_has_no_gaps(self):
        self.assertEqual([], list(self.result.coverage), "the model has a coverage gap")

    def test_known_bad_corpus_is_detected(self):
        self.assertEqual(3, len(self.known_bad))
        for item in self.known_bad:
            with self.subTest(case=item.case_id):
                self.assertTrue(item.detected, f"{item.case_id} was not detected")
                self.assertIsNotNone(item.actual_sources)
                self.assertEqual(set(item.expected_sources), set(item.actual_sources))
                self.assertTrue(
                    set(item.rule_ids) <= set(item.core),
                    f"{item.case_id} core does not name both rules: {item.core}",
                )

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

    def test_seeded_mutation_is_caught(self):
        # Mutation: remove the offer-kind scope from the regular claim window.
        # The 24-hour rule then applies to fill-in offers too and contradicts
        # the 6-hour fill-in rule.
        mutated: list = []
        for rule in self.rules:
            if rule.id == "R-CLAIM-WINDOW-REGULAR":
                formula = z3.And(
                    [self.world.claim_window_h[kind] == 24 for kind in range(2)]
                )
                mutated.append(replace(rule, formula=formula))
            else:
                mutated.append(rule)
        contradictions = find_contradictions(self.world.core, mutated)
        pair = {
            "R-CLAIM-WINDOW-REGULAR",
            "R-CLAIM-WINDOW-FILLIN",
        }
        detected = [
            item
            for item in contradictions
            if pair <= {item.left_id, item.right_id}
        ]
        self.assertEqual(1, len(detected), "the seeded mutation was not caught")
        actual = set(detected[0].left_sources) | set(detected[0].right_sources)
        self.assertIn("D-025", actual)
        self.assertIn("D-069", actual)

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
        report = render_report(self.result, self.rules, self.scenarios, self.known_bad)
        for heading in (
            "# Requirement consistency",
            "## Rules",
            "## Scenario results",
            "## Contradiction pairs",
            "## Unsat cores",
            "## Coverage gaps",
            "## Known-bad corpus",
            "## Trusted gap",
        ):
            self.assertIn(heading, report)
        self.assertIn(f"z3 {z3.get_version_string()}", report)
        self.assertIn("Status: **PASS**", report)


if __name__ == "__main__":
    unittest.main()
