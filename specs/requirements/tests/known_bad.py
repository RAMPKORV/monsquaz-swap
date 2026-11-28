"""Known-bad corpus for the requirement consistency gate (D-076).

Each case is a small, self-contained rule set that reproduces one contradiction
from the 2026-11-28 reviews. The gate must detect each case and name both
sources. The corrected model in `model.py` must not contain these pairs.

The premise is separate from the domain definitions. A contradiction under a
reachable premise is still a contradiction.
"""

from __future__ import annotations

from dataclasses import dataclass

import z3

from ..checker import find_contradictions
from ..model import World
from ..schema import Flow, OfferKind, Rule, Source


def _decision(ref: str) -> Source:
    return Source("decision", ref, "ai-rules/DECISIONS.md")


def _setting(ref: str) -> Source:
    return Source("setting", ref, "docs/EVENT_SETTINGS.md")


@dataclass(frozen=True)
class KnownBadResult:
    case_id: str
    title: str
    expected_sources: tuple[str, str]
    premise: str
    detected: bool
    actual_sources: tuple[str, ...]
    core: tuple[str, ...]
    rule_ids: tuple[str, str]


def _result(case_id, title, expected, premise, contradictions, rule_ids):
    expected_set = set(expected)
    for contradiction in contradictions:
        actual = tuple(
            dict.fromkeys(
                tuple(contradiction.left_sources) + tuple(contradiction.right_sources)
            )
        )
        if expected_set <= set(actual):
            return KnownBadResult(
                case_id=case_id,
                title=title,
                expected_sources=expected,
                premise=premise,
                detected=True,
                actual_sources=actual,
                core=contradiction.core,
                rule_ids=rule_ids,
            )
    if contradictions:
        first = contradictions[0]
        return KnownBadResult(
            case_id=case_id,
            title=title,
            expected_sources=expected,
            premise=premise,
            detected=False,
            actual_sources=tuple(
                dict.fromkeys(
                    tuple(first.left_sources) + tuple(first.right_sources)
                )
            ),
            core=first.core,
            rule_ids=rule_ids,
        )
    return KnownBadResult(
        case_id=case_id,
        title=title,
        expected_sources=expected,
        premise=premise,
        detected=False,
        actual_sources=(),
        core=(),
        rule_ids=rule_ids,
    )


def claim_window_case(world: World) -> KnownBadResult:
    """D-014 (12 hours) and D-069 (6 hours) apply to one default fill-in window."""
    twelve = Rule(
        id="KB-CLAIM-12",
        sources=(
            _decision("D-014"),
            _setting("Fill-in offer claim window"),
        ),
        paraphrase="The default fill-in claim window must be 12 hours.",
        formula=world.default_fill_in_claim_window_h == 12,
    )
    six = Rule(
        id="KB-CLAIM-6",
        sources=(
            _decision("D-069"),
            _setting("Fill-in offer claim window"),
        ),
        paraphrase="The default fill-in claim window must be 6 hours.",
        formula=world.default_fill_in_claim_window_h == 6,
    )
    premise = world.offer_kind == OfferKind.FILL_IN
    contradictions, _ = find_contradictions(
        world.core,
        [twelve, six],
        premise=premise,
        premise_id="premise:fill-in-offer",
    )
    return _result(
        "claim-window-12-vs-6",
        "A 12-hour fill-in claim window contradicts a 6-hour window",
        ("D-014", "D-069"),
        "premise:fill-in-offer",
        contradictions,
        (twelve.id, six.id),
    )


def host_list_case(world: World) -> KnownBadResult:
    """D-014 (max 3) and D-070 (no limit) apply to one global offer cap."""
    global_cap = z3.Bool("legacy_global_fill_in_cap")
    capped = Rule(
        id="KB-GLOBAL-CAP-3",
        sources=(
            _decision("D-014"),
            _setting("Fill-in flow"),
        ),
        paraphrase="The fill-in flow must enforce a global 3-offer cap.",
        formula=z3.And(global_cap, world.max_auto_chain_offers == 3),
    )
    unlimited = Rule(
        id="KB-HOST-LIST-UNLIMITED",
        sources=(
            _decision("D-070"),
            _setting("Fill-in flow"),
        ),
        paraphrase="The host-ordered fill-in list must have no size limit.",
        formula=z3.Not(global_cap),
    )
    premise = world.fill_in_flow == Flow.AUTO_CHAIN
    contradictions, _ = find_contradictions(
        world.core,
        [capped, unlimited],
        premise=premise,
        premise_id="premise:auto-chain",
    )
    return _result(
        "max-3-vs-no-cap",
        "A global 3-offer cap contradicts an unlimited host list",
        ("D-014", "D-070"),
        "premise:auto-chain",
        contradictions,
        (capped.id, unlimited.id),
    )


def shared_budget_case(world: World) -> KnownBadResult:
    """D-014 (shared budget) and D-050 (extra turn) count one rescue both ways."""
    shared = Rule(
        id="KB-SHARED-BUDGET",
        sources=(
            _decision("D-014"),
            _setting("Fill-in flow"),
        ),
        paraphrase="A tier-2 rescue must use the shared handoff budget.",
        formula=world.budget_after_tier2[0]
        == world.budget_used[0] + world.tier2_rescues[0],
    )
    extra = Rule(
        id="KB-EXTRA-TURN",
        sources=(
            _decision("D-050"),
            _setting("Fill-in flow"),
        ),
        paraphrase="A tier-2 rescue must be outside the shared handoff budget.",
        formula=world.budget_after_tier2[0] == world.budget_used[0],
    )
    premise = world.tier2_rescues[0] == 1
    contradictions, _ = find_contradictions(
        world.core,
        [shared, extra],
        premise=premise,
        premise_id="premise:rescue",
    )
    return _result(
        "shared-budget-vs-extra-turn",
        "A shared handoff budget contradicts an extra tier-2 turn",
        ("D-014", "D-050"),
        "premise:rescue",
        contradictions,
        (shared.id, extra.id),
    )


def run_corpus(world: World) -> list[KnownBadResult]:
    return [
        claim_window_case(world),
        host_list_case(world),
        shared_budget_case(world),
    ]
