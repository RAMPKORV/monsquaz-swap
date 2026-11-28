"""Z3 model of the normative fill-in, leave, handoff, and scheduling rules.

Every rule has a stable id, one or more source links, a Z3 formula, and an
ASD-STE100 paraphrase. The model is finite and explicit. The natural-language
source wins; this file is a reviewed formalization (D-076).
"""

from __future__ import annotations

from dataclasses import dataclass

import z3

from .schema import (
    AUTO_CHAIN_MAX_OFFERS,
    EDIT_WINDOW_H,
    FILL_IN_CLAIM_WINDOW_H,
    HOST_APPOINT_EDIT_WINDOW_H,
    INVITATIONS,
    PARTICIPANTS,
    REGULAR_CLAIM_WINDOW_H,
    RESTORE_INVITATION_HOURS,
    ROUNDS,
    TRACKS,
    TURNS,
    Flow,
    Membership,
    Mode,
    OfferKind,
    Role,
    Rule,
    Scenario,
    Source,
    TurnKind,
)


def _select_int(index, values: list):
    """Return values[index] for a symbolic index in the finite domain."""
    result = values[-1]
    for position in range(len(values) - 2, -1, -1):
        result = z3.If(index == position, values[position], result)
    return result


def _select_bool(index, values: list):
    """Return values[index] for a symbolic index in the finite domain."""
    return z3.Or([z3.And(index == position, value) for position, value in enumerate(values)])


def _sum_bools(items: list):
    return z3.Sum([z3.If(item, 1, 0) for item in items])


def _max_int(left, right):
    """z3.Max is not available in every z3 build; use an if-then-else."""
    return z3.If(left >= right, left, right)


def _decision(ref: str) -> Source:
    return Source("decision", ref, "ai-rules/DECISIONS.md")


def _prd(ref: str) -> Source:
    return Source("prd", ref, "docs/PRD.md")


def _setting(ref: str) -> Source:
    return Source("setting", ref, "docs/EVENT_SETTINGS.md")


def _scenario(name: str, feature: str) -> Source:
    return Source("scenario", name, f"specs/features/{feature}")


@dataclass
class World:
    """The finite symbolic world and its domain definitions."""

    def __init__(self) -> None:
        # Event and roster.
        self.mode = z3.Int("mode")
        self.owner = [z3.Int(f"owner_{track}") for track in range(TRACKS)]
        self.role = [z3.Int(f"role_{person}") for person in range(PARTICIPANTS)]
        self.membership = [z3.Int(f"membership_{person}") for person in range(PARTICIPANTS)]
        self.ready = [z3.Bool(f"ready_{person}") for person in range(PARTICIPANTS)]
        self.max_handoffs = [z3.Int(f"max_handoffs_{person}") for person in range(PARTICIPANTS)]
        self.planned_edits = [z3.Int(f"planned_edits_{person}") for person in range(PARTICIPANTS)]
        self.tier2_rescues = [z3.Int(f"tier2_rescues_{person}") for person in range(PARTICIPANTS)]
        self.budget_used = [z3.Int(f"budget_used_{person}") for person in range(PARTICIPANTS)]
        self.budget_after_tier2 = [
            z3.Int(f"budget_after_tier2_{person}") for person in range(PARTICIPANTS)
        ]
        self.submitted_own = [z3.Bool(f"submitted_own_{person}") for person in range(PARTICIPANTS)]
        self.leave = [z3.Bool(f"leave_{person}") for person in range(PARTICIPANTS)]
        self.leave_after_claim = [
            z3.Bool(f"leave_after_claim_{person}") for person in range(PARTICIPANTS)
        ]
        self.leave_held_track = [
            z3.Int(f"leave_held_track_{person}") for person in range(PARTICIPANTS)
        ]
        self.held_track_seen = [
            z3.Bool(f"held_track_seen_{person}") for person in range(PARTICIPANTS)
        ]
        self.has_future_assignment = [
            z3.Bool(f"has_future_assignment_{person}") for person in range(PARTICIPANTS)
        ]
        self.restored = [z3.Bool(f"restored_{person}") for person in range(PARTICIPANTS)]
        self.has_personal_track = [
            z3.Bool(f"has_personal_track_{person}") for person in range(PARTICIPANTS)
        ]

        # Seen ledger and fill-in eligibility.
        self.seen = [
            [z3.Bool(f"seen_{person}_{track}") for track in range(TRACKS)]
            for person in range(PARTICIPANTS)
        ]
        self.tier1_eligible = [
            [z3.Bool(f"tier1_eligible_{person}_{track}") for track in range(TRACKS)]
            for person in range(PARTICIPANTS)
        ]
        self.tier2_eligible = [
            [z3.Bool(f"tier2_eligible_{person}_{track}") for track in range(TRACKS)]
            for person in range(PARTICIPANTS)
        ]
        self.tier1_offer = [
            [z3.Bool(f"tier1_offer_{person}_{track}") for track in range(TRACKS)]
            for person in range(PARTICIPANTS)
        ]
        self.tier2_offer = [
            [z3.Bool(f"tier2_offer_{person}_{track}") for track in range(TRACKS)]
            for person in range(PARTICIPANTS)
        ]
        self.fill_in_offer = [
            [z3.Bool(f"fill_in_offer_{person}_{track}") for track in range(TRACKS)]
            for person in range(PARTICIPANTS)
        ]
        self.plan_leaves_gap = [
            [z3.Bool(f"plan_leaves_gap_{person}_{track}") for track in range(TRACKS)]
            for person in range(PARTICIPANTS)
        ]
        self.tier2_turns = [
            [z3.Int(f"tier2_turns_{person}_{round}") for round in range(ROUNDS)]
            for person in range(PARTICIPANTS)
        ]

        # Settings.
        self.claim_window_h = [z3.Int(f"claim_window_h_{kind}") for kind in range(2)]
        self.claim_reminder_h = z3.Int("claim_reminder_h")
        self.edit_window_h = z3.Int("edit_window_h")
        self.fill_in_flow = z3.Int("fill_in_flow")
        self.fill_in_offer_cap = [z3.Bool(f"fill_in_offer_cap_{flow}") for flow in range(3)]
        self.max_auto_chain_offers = z3.Int("max_auto_chain_offers")
        self.open_fill_in_offers = z3.Int("open_fill_in_offers")
        self.first_accept_wins = z3.Bool("first_accept_wins")
        self.event_wide_tier2_cap = z3.Bool("event_wide_tier2_cap")
        self.invitation_valid_h = z3.Int("invitation_valid_h")

        # Assignment turns. A turn is a static record; the model does not
        # model wall-clock transitions.
        self.turn_holder = [z3.Int(f"turn_holder_{turn}") for turn in range(TURNS)]
        self.turn_track = [z3.Int(f"turn_track_{turn}") for turn in range(TURNS)]
        self.turn_round = [z3.Int(f"turn_round_{turn}") for turn in range(TURNS)]
        self.turn_kind = [z3.Int(f"turn_kind_{turn}") for turn in range(TURNS)]
        self.turn_assigned = [z3.Bool(f"turn_assigned_{turn}") for turn in range(TURNS)]
        self.turn_active = [z3.Bool(f"turn_active_{turn}") for turn in range(TURNS)]
        self.turn_offered = [z3.Bool(f"turn_offered_{turn}") for turn in range(TURNS)]
        self.turn_claimed = [z3.Bool(f"turn_claimed_{turn}") for turn in range(TURNS)]
        self.turn_submitted = [z3.Bool(f"turn_submitted_{turn}") for turn in range(TURNS)]
        self.turn_skip_before_claim = [
            z3.Bool(f"turn_skip_before_claim_{turn}") for turn in range(TURNS)
        ]
        self.turn_skip_after_claim = [
            z3.Bool(f"turn_skip_after_claim_{turn}") for turn in range(TURNS)
        ]
        self.turn_expired = [z3.Bool(f"turn_expired_{turn}") for turn in range(TURNS)]
        self.turn_seen = [z3.Bool(f"turn_seen_{turn}") for turn in range(TURNS)]
        self.repeat_pair = [z3.Bool(f"repeat_pair_{turn}") for turn in range(TURNS)]
        self.own_track_turn = [z3.Bool(f"own_track_turn_{turn}") for turn in range(TURNS)]
        self.is_fill_in_turn = [z3.Bool(f"is_fill_in_turn_{turn}") for turn in range(TURNS)]
        self.appoint_time_h = [z3.Int(f"appoint_time_h_{turn}") for turn in range(TURNS)]
        self.claim_time_h = [z3.Int(f"claim_time_h_{turn}") for turn in range(TURNS)]
        self.fill_in_due_h = [z3.Int(f"fill_in_due_h_{turn}") for turn in range(TURNS)]

        # Rounds and reveal.
        self.round_open = [z3.Bool(f"round_open_{round}") for round in range(ROUNDS)]
        self.host_opened_round = [
            z3.Bool(f"host_opened_round_{round}") for round in range(ROUNDS)
        ]
        self.round_deadline_h = [z3.Int(f"round_deadline_h_{round}") for round in range(ROUNDS)]
        self.deadline_passed = [z3.Bool(f"deadline_passed_{round}") for round in range(ROUNDS)]
        self.auto_advanced = [z3.Bool(f"auto_advanced_{round}") for round in range(ROUNDS)]
        self.open_round_count = z3.Int("open_round_count")
        self.host_reveal_action = z3.Bool("host_reveal_action")
        self.reveal_published = z3.Bool("reveal_published")
        self.active_assignments_count = z3.Int("active_assignments_count")
        self.no_active_assignments = z3.Bool("no_active_assignments")

        # Leave and restore.
        self.invitation_sent_h = [z3.Int(f"invitation_sent_h_{item}") for item in range(INVITATIONS)]
        self.invitation_action_h = [
            z3.Int(f"invitation_action_h_{item}") for item in range(INVITATIONS)
        ]
        self.invitation_action = [z3.Bool(f"invitation_action_{item}") for item in range(INVITATIONS)]
        self.invitation_expired = [
            z3.Bool(f"invitation_expired_{item}") for item in range(INVITATIONS)
        ]

        # Fill-in flow outcomes.
        self.no_eligible_fill_in = z3.Bool("no_eligible_fill_in")
        self.pass_unchanged = z3.Bool("pass_unchanged")
        self.return_to_pool = z3.Bool("return_to_pool")

        self.core = z3.And(*self._domain_constraints())

    def _domain_constraints(self) -> list:
        constraints: list = [
            self.mode >= Mode.CLASSIC,
            self.mode <= Mode.ASYNC,
            self.fill_in_flow >= Flow.HOST_APPOINTS,
            self.fill_in_flow <= Flow.HOST_LIST,
            self.max_auto_chain_offers >= 0,
            self.open_fill_in_offers >= 0,
            self.invitation_valid_h >= 0,
            self.edit_window_h > 0,
            self.claim_reminder_h >= 0,
        ]
        for kind in range(2):
            constraints.append(self.claim_window_h[kind] > 0)

        for track in range(TRACKS):
            constraints.extend([self.owner[track] >= 0, self.owner[track] < PARTICIPANTS])
        for person in range(PARTICIPANTS):
            constraints.extend(
                [
                    self.role[person] >= Role.PARTICIPANT,
                    self.role[person] <= Role.FILL_IN,
                    self.membership[person] >= Membership.SIGNED_UP,
                    self.membership[person] <= Membership.REMOVED,
                    self.max_handoffs[person] >= 0,
                    self.planned_edits[person] >= 0,
                    self.tier2_rescues[person] >= 0,
                    self.budget_used[person] >= 0,
                    self.budget_after_tier2[person] >= 0,
                    self.leave_held_track[person] >= 0,
                    self.leave_held_track[person] < TRACKS,
                    self.held_track_seen[person]
                    == _select_bool(self.leave_held_track[person], self.seen[person]),
                ]
            )
            for round in range(ROUNDS):
                constraints.extend(
                    [self.tier2_turns[person][round] >= 0, self.tier2_turns[person][round] <= TURNS]
                )
            for track in range(TRACKS):
                constraints.append(
                    self.fill_in_offer[person][track]
                    == z3.Or(self.tier1_offer[person][track], self.tier2_offer[person][track])
                )

        for turn in range(TURNS):
            constraints.extend(
                [
                    self.turn_holder[turn] >= 0,
                    self.turn_holder[turn] < PARTICIPANTS,
                    self.turn_track[turn] >= 0,
                    self.turn_track[turn] < TRACKS,
                    self.turn_round[turn] >= 0,
                    self.turn_round[turn] < ROUNDS,
                    self.turn_kind[turn] >= TurnKind.PLANNED,
                    self.turn_kind[turn] <= TurnKind.FILL_IN,
                    self.is_fill_in_turn[turn] == (self.turn_kind[turn] == TurnKind.FILL_IN),
                    self.turn_assigned[turn]
                    == z3.Or(
                        self.turn_active[turn],
                        self.turn_offered[turn],
                        self.turn_submitted[turn],
                        self.turn_skip_before_claim[turn],
                        self.turn_skip_after_claim[turn],
                        self.turn_expired[turn],
                    ),
                    z3.Implies(self.turn_active[turn], self.turn_assigned[turn]),
                    z3.Implies(self.turn_submitted[turn], z3.Not(self.turn_active[turn])),
                    z3.Implies(self.turn_skip_before_claim[turn], z3.Not(self.turn_active[turn])),
                    z3.Implies(self.turn_skip_after_claim[turn], z3.Not(self.turn_active[turn])),
                    z3.Implies(self.turn_expired[turn], z3.Not(self.turn_active[turn])),
                    self.turn_seen[turn]
                    == z3.Or(
                        self.turn_claimed[turn],
                        self.turn_submitted[turn],
                        self.turn_skip_after_claim[turn],
                    ),
                    z3.Implies(self.turn_submitted[turn], self.turn_claimed[turn]),
                    z3.Implies(self.turn_skip_after_claim[turn], self.turn_claimed[turn]),
                    z3.Implies(self.turn_skip_before_claim[turn], z3.Not(self.turn_claimed[turn])),
                    z3.Implies(self.turn_expired[turn], z3.Not(self.turn_claimed[turn])),
                    self.own_track_turn[turn]
                    == (self.turn_holder[turn] == _select_int(self.turn_track[turn], self.owner)),
                    self.repeat_pair[turn]
                    == z3.Or(
                        [
                            z3.And(
                                self.turn_seen[earlier],
                                self.turn_holder[earlier] == self.turn_holder[turn],
                                self.turn_track[earlier] == self.turn_track[turn],
                            )
                            for earlier in range(turn)
                        ]
                    ),
                ]
            )

        constraints.extend(
            [
                self.active_assignments_count == _sum_bools(self.turn_active),
                self.no_active_assignments == (self.active_assignments_count == 0),
                self.open_round_count == _sum_bools(self.round_open),
            ]
        )
        for round in range(ROUNDS):
            constraints.extend(
                [
                    z3.Implies(self.auto_advanced[round], self.deadline_passed[round]),
                    z3.Implies(self.auto_advanced[round], z3.Not(self.host_opened_round[round])),
                ]
            )
        return constraints


def build_rules(world: World) -> list[Rule]:
    """Build the must rules in a stable order."""

    rules: list[Rule] = []

    rules.append(
        Rule(
            id="R-CLAIM-WINDOW-REGULAR",
            sources=(
                _decision("D-025"),
                _prd("REQ-DELIVERY-002"),
                _setting("Claim window"),
                _scenario("A claim delivers the file and starts the deadline", "async_queue.feature"),
            ),
            paraphrase="A regular offer must expire after the 24-hour claim window.",
            formula=world.claim_window_h[OfferKind.REGULAR] == REGULAR_CLAIM_WINDOW_H,
            tags=("claim", "setting"),
        )
    )
    rules.append(
        Rule(
            id="R-CLAIM-WINDOW-FILLIN",
            sources=(
                _decision("D-069"),
                _prd("REQ-FILL-003"),
                _setting("Fill-in offer claim window"),
                _scenario("A fill-in offer uses the standard sequence", "fill_ins.feature"),
            ),
            paraphrase="A fill-in offer must expire after the 6-hour claim window.",
            formula=world.claim_window_h[OfferKind.FILL_IN] == FILL_IN_CLAIM_WINDOW_H,
            tags=("claim", "fill-in", "setting"),
        )
    )
    rules.append(
        Rule(
            id="R-CLAIM-REMINDER",
            sources=(
                _decision("D-041"),
                _setting("Claim reminder"),
                _scenario("The bot reminds the participant to claim", "async_queue.feature"),
            ),
            paraphrase="The claim reminder must occur before the claim window ends.",
            formula=z3.And(
                world.claim_reminder_h == 12,
                world.claim_reminder_h < world.claim_window_h[OfferKind.REGULAR],
            ),
            tags=("claim", "setting"),
        )
    )
    rules.append(
        Rule(
            id="R-EDIT-WINDOW",
            sources=(
                _decision("D-033"),
                _decision("D-069"),
                _setting("Async handoff deadline"),
                _scenario("A claimed fill-in offer starts the 24-hour edit time", "fill_ins.feature"),
            ),
            paraphrase="The default edit window must be 24 hours.",
            formula=world.edit_window_h == EDIT_WINDOW_H,
            tags=("deadline", "setting"),
        )
    )
    rules.append(
        Rule(
            id="R-FILL-AUTO-CHAIN",
            sources=(
                _decision("D-014"),
                _decision("D-064"),
                _decision("D-069"),
                _prd("REQ-FILL-003"),
                _setting("Fill-in flow"),
                _scenario("A fill-in offer uses the standard sequence", "fill_ins.feature"),
            ),
            paraphrase="An auto-chain must make at most three offers before the host decides.",
            formula=z3.And(
                world.fill_in_offer_cap[Flow.AUTO_CHAIN],
                world.max_auto_chain_offers == AUTO_CHAIN_MAX_OFFERS,
            ),
            tags=("fill-in", "setting"),
        )
    )
    rules.append(
        Rule(
            id="R-FILL-HOST-LIST",
            sources=(
                _decision("D-070"),
                _prd("REQ-FILL-003"),
                _setting("Fill-in flow"),
                _scenario("A fill-in offer uses the standard sequence", "fill_ins.feature"),
            ),
            paraphrase="A host-ordered fill-in list must have no size limit.",
            formula=z3.Not(world.fill_in_offer_cap[Flow.HOST_LIST]),
            tags=("fill-in", "setting"),
        )
    )
    rules.append(
        Rule(
            id="R-FILL-SEQUENTIAL",
            sources=(
                _decision("D-014"),
                _prd("REQ-FILL-003"),
                _scenario("A fill-in offer uses the standard sequence", "fill_ins.feature"),
            ),
            paraphrase="The bot must offer a fill-in slot to one person at a time.",
            formula=world.open_fill_in_offers <= 1,
            tags=("fill-in",),
        )
    )
    rules.append(
        Rule(
            id="R-FILL-FIRST-ACCEPT",
            sources=(
                _decision("D-014"),
                _prd("REQ-FILL-003"),
                _scenario("A fill-in offer uses the standard sequence", "fill_ins.feature"),
            ),
            paraphrase="The first accept must win the fill-in offer.",
            formula=world.first_accept_wins,
            tags=("fill-in",),
        )
    )
    rules.append(
        Rule(
            id="R-FILL-DUE-APPOINT",
            sources=(
                _decision("D-040"),
                _decision("D-069"),
                _setting("Fill-in due time"),
                _scenario("A host-appointed fill-in gets the later due time", "fill_ins.feature"),
            ),
            paraphrase=(
                "A host-appointed fill-in must have a due time at the later of two times. "
                "The two times are the round deadline and 24 hours after the appointment."
            ),
            formula=z3.And(
                [
                    z3.Implies(
                        z3.And(
                            world.is_fill_in_turn[turn],
                            world.fill_in_flow == Flow.HOST_APPOINTS,
                        ),
                        world.fill_in_due_h[turn]
                        == _max_int(
                            _select_int(world.turn_round[turn], world.round_deadline_h),
                            world.appoint_time_h[turn] + HOST_APPOINT_EDIT_WINDOW_H,
                        ),
                    )
                    for turn in range(TURNS)
                ]
            ),
            tags=("fill-in", "deadline", "setting"),
        )
    )
    rules.append(
        Rule(
            id="R-FILL-DUE-CLAIM",
            sources=(
                _decision("D-069"),
                _prd("REQ-FILL-003"),
                _setting("Fill-in offer claim window"),
                _scenario("A claimed fill-in offer starts the 24-hour edit time", "fill_ins.feature"),
            ),
            paraphrase="A claimed fill-in offer must start the 24-hour edit time at the claim.",
            formula=z3.And(
                [
                    z3.Implies(
                        z3.And(
                            world.is_fill_in_turn[turn],
                            z3.Or(
                                world.fill_in_flow == Flow.AUTO_CHAIN,
                                world.fill_in_flow == Flow.HOST_LIST,
                            ),
                        ),
                        world.fill_in_due_h[turn] == world.claim_time_h[turn] + world.edit_window_h,
                    )
                    for turn in range(TURNS)
                ]
            ),
            tags=("fill-in", "deadline", "setting"),
        )
    )
    rules.append(
        Rule(
            id="R-FILL-ELIGIBLE-NEVER-SEEN",
            sources=(
                _decision("D-007"),
                _decision("D-050"),
                _prd("REQ-FILL-002"),
                _scenario("A fill-in has never seen the track", "fill_ins.feature"),
            ),
            paraphrase="The app must not offer a fill-in slot to a person who has seen the track.",
            formula=z3.And(
                [
                    z3.Implies(world.fill_in_offer[person][track], z3.Not(world.seen[person][track]))
                    for person in range(PARTICIPANTS)
                    for track in range(TRACKS)
                ]
            ),
            tags=("fill-in", "seen"),
        )
    )
    rules.append(
        Rule(
            id="R-FILL-ELIGIBLE-NOT-OWNER",
            sources=(
                _decision("D-007"),
                _decision("D-050"),
                _prd("REQ-FILL-002"),
                _scenario("A fill-in has never seen the track", "fill_ins.feature"),
            ),
            paraphrase="The app must not offer a fill-in slot to the track owner.",
            formula=z3.And(
                [
                    z3.Implies(world.fill_in_offer[person][track], world.owner[track] != person)
                    for person in range(PARTICIPANTS)
                    for track in range(TRACKS)
                ]
            ),
            tags=("fill-in", "own-track"),
        )
    )
    rules.append(
        Rule(
            id="R-TIER1-ELIGIBLE",
            sources=(
                _decision("D-050"),
                _prd("REQ-FILL-002"),
                _setting("Fill-in flow"),
                _scenario("The fill-in pool has three sources", "fill_ins.feature"),
            ),
            paraphrase=(
                "A tier-1 fill-in must have the fill-in role, be active, and be ready. "
                "A tier-1 fill-in must not have seen the track or own the track."
            ),
            formula=z3.And(
                [
                    world.tier1_eligible[person][track]
                    == z3.And(
                        world.role[person] == Role.FILL_IN,
                        world.membership[person] == Membership.ACTIVE,
                        world.ready[person],
                        z3.Not(world.seen[person][track]),
                        world.owner[track] != person,
                    )
                    for person in range(PARTICIPANTS)
                    for track in range(TRACKS)
                ]
            ),
            tags=("fill-in", "tier"),
        )
    )
    rules.append(
        Rule(
            id="R-TIER2-ELIGIBLE",
            sources=(
                _decision("D-050"),
                _prd("REQ-FILL-002"),
                _scenario("The fill-in pool has three sources", "fill_ins.feature"),
            ),
            paraphrase=(
                "A tier-2 fill-in must be an active seeded participant who submitted their own "
                "track in the round. The plan must leave a gap for that track."
            ),
            formula=z3.And(
                [
                    world.tier2_eligible[person][track]
                    == z3.And(
                        world.role[person] == Role.PARTICIPANT,
                        world.membership[person] == Membership.ACTIVE,
                        world.ready[person],
                        world.submitted_own[person],
                        z3.Not(world.seen[person][track]),
                        world.owner[track] != person,
                        world.plan_leaves_gap[person][track],
                    )
                    for person in range(PARTICIPANTS)
                    for track in range(TRACKS)
                ]
            ),
            tags=("fill-in", "tier"),
        )
    )
    rules.append(
        Rule(
            id="R-TIER1-BEFORE-TIER2",
            sources=(
                _decision("D-050"),
                _prd("REQ-FILL-002"),
                _scenario("The fill-in pool has three sources", "fill_ins.feature"),
            ),
            paraphrase="A tier-2 offer must not go to a person who is eligible as tier 1.",
            formula=z3.And(
                [
                    z3.Implies(
                        world.tier2_offer[person][track],
                        z3.Not(world.tier1_eligible[person][track]),
                    )
                    for person in range(PARTICIPANTS)
                    for track in range(TRACKS)
                ]
            ),
            tags=("fill-in", "tier"),
        )
    )
    rules.append(
        Rule(
            id="R-TIER2-ONLY-NO-TIER1",
            sources=(
                _decision("D-050"),
                _prd("REQ-FILL-002"),
                _scenario("The fill-in pool has three sources", "fill_ins.feature"),
            ),
            paraphrase="A tier-2 offer must not occur while a tier-1 person is eligible for the track.",
            formula=z3.And(
                [
                    z3.Implies(
                        world.tier2_offer[person][track],
                        z3.Not(
                            z3.Or(
                                [
                                    world.tier1_eligible[other][track]
                                    for other in range(PARTICIPANTS)
                                ]
                            )
                        ),
                    )
                    for person in range(PARTICIPANTS)
                    for track in range(TRACKS)
                ]
            ),
            tags=("fill-in", "tier"),
        )
    )
    rules.append(
        Rule(
            id="R-TIER2-ONE-PER-ROUND",
            sources=(
                _decision("D-063"),
                _prd("REQ-FILL-003"),
                _scenario("A tier-2 fill-in rescue is an extra turn", "fill_ins.feature"),
            ),
            paraphrase="A person must take at most one tier-2 fill-in turn in one round.",
            formula=z3.And(
                [world.tier2_turns[person][round] <= 1 for person in range(PARTICIPANTS) for round in range(ROUNDS)]
            ),
            tags=("fill-in", "tier"),
        )
    )
    rules.append(
        Rule(
            id="R-TIER2-NO-EVENT-CAP",
            sources=(
                _decision("D-063"),
                _scenario("A tier-2 fill-in rescue is an extra turn", "fill_ins.feature"),
            ),
            paraphrase="The event must not cap the total number of tier-2 fill-in turns.",
            formula=z3.Not(world.event_wide_tier2_cap),
            tags=("fill-in", "tier"),
        )
    )
    rules.append(
        Rule(
            id="R-BUDGET-REGULAR",
            sources=(
                _decision("D-014"),
                _decision("D-034"),
                _prd("REQ-FILL-004"),
                _scenario("A tier-2 fill-in rescue is an extra turn", "fill_ins.feature"),
            ),
            paraphrase="A planned edit must count toward the shared handoff budget.",
            formula=z3.And(
                [world.budget_used[person] == world.planned_edits[person] for person in range(PARTICIPANTS)]
            ),
            tags=("budget", "handoff"),
        )
    )
    rules.append(
        Rule(
            id="R-BUDGET-TIER2-EXTRA",
            sources=(
                _decision("D-050"),
                _prd("REQ-FILL-004"),
                _scenario("A tier-2 fill-in rescue is an extra turn", "fill_ins.feature"),
            ),
            paraphrase="A tier-2 rescue must be an extra turn outside the shared handoff budget.",
            formula=z3.And(
                [
                    world.budget_after_tier2[person] == world.budget_used[person]
                    for person in range(PARTICIPANTS)
                ]
            ),
            tags=("budget", "handoff", "tier"),
        )
    )
    rules.append(
        Rule(
            id="R-NO-REPEAT",
            sources=(
                _decision("D-007"),
                _prd("REQ-SCHED-001"),
                _scenario("Every participant receives one unseen track per round", "scheduling.feature"),
            ),
            paraphrase="A participant must never work on the same track twice.",
            formula=z3.And(
                [
                    z3.Implies(world.turn_assigned[turn], z3.Not(world.repeat_pair[turn]))
                    for turn in range(TURNS)
                ]
            ),
            tags=("scheduling", "invariant"),
        )
    )
    rules.append(
        Rule(
            id="R-NO-OWN-TRACK",
            sources=(
                _decision("D-007"),
                _prd("REQ-SCHED-001"),
                _scenario("Every participant receives one unseen track per round", "scheduling.feature"),
            ),
            paraphrase="The app must never assign a participant their own track.",
            formula=z3.And(
                [
                    z3.Implies(world.turn_assigned[turn], z3.Not(world.own_track_turn[turn]))
                    for turn in range(TURNS)
                ]
                + [
                    z3.Implies(world.turn_offered[turn], z3.Not(world.own_track_turn[turn]))
                    for turn in range(TURNS)
                ]
            ),
            tags=("scheduling", "invariant"),
        )
    )
    rules.append(
        Rule(
            id="R-ONE-ASSIGNMENT-PER-PERSON-ROUND",
            sources=(
                _decision("D-008"),
                _prd("REQ-SCHED-003"),
                _scenario("Every participant receives one unseen track per round", "scheduling.feature"),
            ),
            paraphrase="A round must assign at most one track to each participant.",
            formula=z3.And(
                [
                    _sum_bools(
                        [
                            z3.And(
                                world.turn_assigned[turn],
                                world.turn_holder[turn] == person,
                                world.turn_round[turn] == round,
                            )
                            for turn in range(TURNS)
                        ]
                    )
                    <= 1
                    for person in range(PARTICIPANTS)
                    for round in range(ROUNDS)
                ]
            ),
            tags=("scheduling",),
        )
    )
    rules.append(
        Rule(
            id="R-ONE-ASSIGNMENT-PER-TRACK-ROUND",
            sources=(
                _decision("D-008"),
                _prd("REQ-SCHED-003"),
                _scenario("Every participant receives one unseen track per round", "scheduling.feature"),
            ),
            paraphrase="A round must assign a track to at most one participant.",
            formula=z3.And(
                [
                    _sum_bools(
                        [
                            z3.And(
                                world.turn_assigned[turn],
                                world.turn_track[turn] == track,
                                world.turn_round[turn] == round,
                            )
                            for turn in range(TURNS)
                        ]
                    )
                    <= 1
                    for track in range(TRACKS)
                    for round in range(ROUNDS)
                ]
            ),
            tags=("scheduling",),
        )
    )
    rules.append(
        Rule(
            id="R-ROUNDS-HOST-OPENED",
            sources=(
                _decision("D-031"),
                _decision("D-010"),
                _prd("REQ-DEADLINE-001"),
                _scenario("A default classic event runs N total rounds", "scheduling.feature"),
            ),
            paraphrase="The host must open every round. A round must not open automatically.",
            formula=z3.And(
                [
                    z3.Implies(world.round_open[round], world.host_opened_round[round])
                    for round in range(ROUNDS)
                ]
            ),
            tags=("scheduling", "deadline", "invariant"),
        )
    )
    rules.append(
        Rule(
            id="R-NO-AUTO-ADVANCE",
            sources=(
                _decision("D-010"),
                _decision("D-031"),
                _prd("REQ-ASYNC-008"),
                _scenario("A soft deadline does not move a track", "deadlines_and_extensions.feature"),
            ),
            paraphrase="A passed deadline must not advance a round without a host action.",
            formula=z3.And([z3.Not(world.auto_advanced[round]) for round in range(ROUNDS)]),
            tags=("deadline", "invariant"),
        )
    )
    rules.append(
        Rule(
            id="R-ONE-OPEN-ROUND",
            sources=(
                _decision("D-031"),
                _scenario("The host opens at most one round at a time", "deadlines_and_extensions.feature"),
            ),
            paraphrase="The host must open at most one round at a time.",
            formula=world.open_round_count <= 1,
            tags=("scheduling", "deadline"),
        )
    )
    rules.append(
        Rule(
            id="R-REVEAL-AFTER-HOST",
            sources=(
                _decision("D-017"),
                _prd("REQ-REVEAL-001"),
                _setting("Reveal trigger"),
                _scenario("The host triggers the reveal", "reveal.feature"),
            ),
            paraphrase="The host must trigger the reveal.",
            formula=z3.Implies(world.reveal_published, world.host_reveal_action),
            tags=("reveal",),
        )
    )
    rules.append(
        Rule(
            id="R-REVEAL-NO-ACTIVE",
            sources=(
                _decision("D-017"),
                _decision("D-031"),
                _prd("REQ-REVEAL-001"),
                _scenario("The reveal is refused while work is pending", "reveal.feature"),
            ),
            paraphrase="The app must not publish the reveal while an assignment is active.",
            formula=z3.Implies(world.reveal_published, world.no_active_assignments),
            tags=("reveal", "invariant"),
        )
    )
    rules.append(
        Rule(
            id="R-LEAVE-SEEN",
            sources=(
                _decision("D-051"),
                _prd("REQ-JOIN-005"),
                _scenario("A participant leaves while holding a track", "join_and_seed.feature"),
            ),
            paraphrase="A leave after a claim must mark the held track as seen.",
            formula=z3.And(
                [
                    z3.Implies(world.leave_after_claim[person], world.held_track_seen[person])
                    for person in range(PARTICIPANTS)
                ]
            ),
            tags=("leave", "seen"),
        )
    )
    rules.append(
        Rule(
            id="R-LEAVE-NO-FUTURE",
            sources=(
                _decision("D-051"),
                _prd("REQ-JOIN-005"),
                _scenario("A participant leaves while holding a track", "join_and_seed.feature"),
            ),
            paraphrase="A leave must remove the future assignments of the person.",
            formula=z3.And(
                [
                    z3.Implies(world.leave[person], z3.Not(world.has_future_assignment[person]))
                    for person in range(PARTICIPANTS)
                ]
            ),
            tags=("leave",),
        )
    )
    rules.append(
        Rule(
            id="R-RESTORE-FILLIN-ONLY",
            sources=(
                _decision("D-051"),
                _prd("REQ-JOIN-006"),
                _scenario("A leaver can return as a fill-in", "join_and_seed.feature"),
            ),
            paraphrase="A leaver must return as a fill-in only.",
            formula=z3.And(
                [
                    z3.Implies(
                        world.restored[person],
                        z3.And(
                            world.role[person] == Role.FILL_IN,
                            z3.Not(world.has_personal_track[person]),
                        ),
                    )
                    for person in range(PARTICIPANTS)
                ]
            ),
            tags=("leave", "restore"),
        )
    )
    rules.append(
        Rule(
            id="R-RESTORE-EXPIRY",
            sources=(
                _decision("D-068"),
                _scenario("A restore invitation expires after 7 days", "join_and_seed.feature"),
            ),
            paraphrase="A restore invitation must stop working 7 days after the bot sends it.",
            formula=z3.And(
                [
                    z3.Implies(
                        world.invitation_action[item],
                        world.invitation_action_h[item]
                        <= world.invitation_sent_h[item] + RESTORE_INVITATION_HOURS,
                    )
                    for item in range(INVITATIONS)
                ]
            ),
            tags=("restore",),
        )
    )
    rules.append(
        Rule(
            id="R-RESTORE-EXPIRY-SETTING",
            sources=(
                _decision("D-068"),
                _scenario("A restore invitation expires after 7 days", "join_and_seed.feature"),
            ),
            paraphrase="The restore invitation must expire after 168 hours.",
            formula=world.invitation_valid_h == RESTORE_INVITATION_HOURS,
            tags=("restore", "setting"),
        )
    )
    rules.append(
        Rule(
            id="R-RESTORE-EXPIRED-BLOCKS",
            sources=(
                _decision("D-068"),
                _scenario("A restore invitation expires after 7 days", "join_and_seed.feature"),
            ),
            paraphrase="An expired restore invitation must not accept a Confirm or Decline action.",
            formula=z3.And(
                [
                    z3.Implies(
                        world.invitation_expired[item],
                        z3.Not(world.invitation_action[item]),
                    )
                    for item in range(INVITATIONS)
                ]
            ),
            tags=("restore",),
        )
    )
    rules.append(
        Rule(
            id="R-FILL-PASS-UNCHANGED",
            sources=(
                _decision("D-052"),
                _prd("REQ-FILL-005"),
                _scenario("No eligible fill-in exists", "fill_ins.feature"),
            ),
            paraphrase=(
                "When no eligible fill-in exists, classic must pass the track unchanged. "
                "Async must return the track to the pool."
            ),
            formula=z3.And(
                z3.Implies(
                    z3.And(world.no_eligible_fill_in, world.mode == Mode.CLASSIC),
                    world.pass_unchanged,
                ),
                z3.Implies(
                    z3.And(world.no_eligible_fill_in, world.mode == Mode.ASYNC),
                    world.return_to_pool,
                ),
            ),
            tags=("fill-in",),
        )
    )
    return rules


def _turn(world: World, turn: int, holder: int, track: int, round_number: int, active: bool = True):
    return z3.And(
        world.turn_holder[turn] == holder,
        world.turn_track[turn] == track,
        world.turn_round[turn] == round_number,
        world.turn_active[turn] == active,
        world.turn_offered[turn] == active,
        world.turn_claimed[turn] == active,
    )


def build_scenarios(world: World) -> list[Scenario]:
    """Build the formalized scenarios and their rule links."""

    scenarios: list[Scenario] = []

    scenarios.append(
        Scenario(
            id="S-FILL-SEQUENCE",
            name="A fill-in offer uses the standard sequence",
            feature="specs/features/fill_ins.feature",
            precondition=z3.And(
                world.fill_in_flow == Flow.AUTO_CHAIN,
                world.turn_kind[0] == TurnKind.FILL_IN,
                world.claim_time_h[0] == 100,
                world.fill_in_due_h[0] == 124,
                world.max_auto_chain_offers == AUTO_CHAIN_MAX_OFFERS,
                world.fill_in_offer_cap[Flow.AUTO_CHAIN],
                z3.Not(world.fill_in_offer_cap[Flow.HOST_LIST]),
                world.open_fill_in_offers == 1,
                world.first_accept_wins,
            ),
            action=world.turn_offered[0],
            postcondition=z3.And(
                world.turn_offered[0],
                world.fill_in_due_h[0] == world.claim_time_h[0] + EDIT_WINDOW_H,
            ),
            rules=(
                "R-CLAIM-WINDOW-FILLIN",
                "R-EDIT-WINDOW",
                "R-FILL-AUTO-CHAIN",
                "R-FILL-HOST-LIST",
                "R-FILL-SEQUENTIAL",
                "R-FILL-FIRST-ACCEPT",
                "R-FILL-DUE-CLAIM",
            ),
        )
    )
    scenarios.append(
        Scenario(
            id="S-TIER2-EXTRA-TURN",
            name="A tier-2 fill-in rescue is an extra turn",
            feature="specs/features/fill_ins.feature",
            precondition=z3.And(
                world.role[0] == Role.PARTICIPANT,
                world.membership[0] == Membership.ACTIVE,
                world.ready[0],
                world.max_handoffs[0] == 4,
                world.planned_edits[0] == 2,
                world.tier2_rescues[0] == 1,
                world.tier2_turns[0][1] == 1,
                z3.Not(world.event_wide_tier2_cap),
            ),
            action=world.tier2_turns[0][1] == 1,
            postcondition=z3.And(
                world.budget_used[0] == world.planned_edits[0],
                world.budget_after_tier2[0] == world.budget_used[0],
            ),
            rules=(
                "R-TIER2-ONE-PER-ROUND",
                "R-TIER2-NO-EVENT-CAP",
                "R-BUDGET-REGULAR",
                "R-BUDGET-TIER2-EXTRA",
            ),
        )
    )
    scenarios.append(
        Scenario(
            id="S-FILL-NEVER-SEEN",
            name="A fill-in has never seen the track",
            feature="specs/features/fill_ins.feature",
            precondition=z3.And(
                world.owner[0] == 0,
                world.role[1] == Role.FILL_IN,
                world.membership[1] == Membership.ACTIVE,
                world.ready[1],
                z3.Not(world.seen[1][0]),
            ),
            action=world.tier1_offer[1][0],
            postcondition=z3.And(
                world.fill_in_offer[1][0],
                z3.Not(world.seen[1][0]),
                world.owner[0] != 1,
            ),
            rules=(
                "R-FILL-ELIGIBLE-NEVER-SEEN",
                "R-FILL-ELIGIBLE-NOT-OWNER",
                "R-TIER1-ELIGIBLE",
            ),
        )
    )
    scenarios.append(
        Scenario(
            id="S-FILL-POOL",
            name="The fill-in pool has three sources",
            feature="specs/features/fill_ins.feature",
            precondition=z3.And(
                world.owner[0] == 0,
                world.owner[1] == 0,
                world.role[1] == Role.FILL_IN,
                world.membership[1] == Membership.ACTIVE,
                world.ready[1],
                z3.Not(world.seen[1][0]),
                world.seen[1][1],
                world.role[2] == Role.PARTICIPANT,
                world.membership[2] == Membership.ACTIVE,
                world.ready[2],
                world.submitted_own[2],
                z3.Not(world.seen[2][1]),
                world.plan_leaves_gap[2][1],
            ),
            action=z3.And(world.tier1_offer[1][0], world.tier2_offer[2][1]),
            postcondition=z3.And(
                world.tier1_eligible[1][0],
                world.tier2_eligible[2][1],
                z3.Not(
                    z3.Or([world.tier1_eligible[person][1] for person in range(PARTICIPANTS)])
                ),
            ),
            rules=(
                "R-TIER1-ELIGIBLE",
                "R-TIER2-ELIGIBLE",
                "R-TIER1-BEFORE-TIER2",
                "R-TIER2-ONLY-NO-TIER1",
            ),
        )
    )
    scenarios.append(
        Scenario(
            id="S-FILL-NO-ELIGIBLE",
            name="No eligible fill-in exists",
            feature="specs/features/fill_ins.feature",
            precondition=z3.And(world.mode == Mode.CLASSIC, world.no_eligible_fill_in),
            action=z3.BoolVal(True),
            postcondition=world.pass_unchanged,
            rules=("R-FILL-PASS-UNCHANGED",),
        )
    )
    scenarios.append(
        Scenario(
            id="S-SCHEDULE-ROUND",
            name="Every participant receives one unseen track per round",
            feature="specs/features/scheduling.feature",
            precondition=z3.And(world.owner[0] == 0, world.owner[1] == 1, world.owner[2] == 2),
            action=z3.And(
                [
                    _turn(world, 0, 0, 1, 0),
                    _turn(world, 1, 1, 2, 0),
                    _turn(world, 2, 2, 0, 0),
                    _turn(world, 3, 0, 2, 1),
                    _turn(world, 4, 1, 0, 1),
                    _turn(world, 5, 2, 1, 1),
                ]
            ),
            postcondition=z3.And(
                [
                    z3.Not(world.repeat_pair[turn]) for turn in range(TURNS)
                ]
                + [
                    z3.Not(world.own_track_turn[turn]) for turn in range(TURNS)
                ]
            ),
            rules=(
                "R-NO-REPEAT",
                "R-NO-OWN-TRACK",
                "R-ONE-ASSIGNMENT-PER-PERSON-ROUND",
                "R-ONE-ASSIGNMENT-PER-TRACK-ROUND",
            ),
        )
    )
    scenarios.append(
        Scenario(
            id="S-CLASSIC-LENGTH",
            name="A default classic event runs N total rounds",
            feature="specs/features/scheduling.feature",
            precondition=z3.And(
                world.mode == Mode.CLASSIC,
                z3.Not(world.round_open[0]),
                world.round_open[1],
                world.host_opened_round[1],
                z3.Not(world.round_open[2]),
            ),
            action=world.host_opened_round[1],
            postcondition=world.round_open[1],
            rules=("R-ROUNDS-HOST-OPENED",),
        )
    )
    scenarios.append(
        Scenario(
            id="S-REVEAL-HOST",
            name="The host triggers the reveal",
            feature="specs/features/reveal.feature",
            precondition=z3.And(
                [z3.Not(world.turn_active[turn]) for turn in range(TURNS)]
                + [world.host_reveal_action]
            ),
            action=world.host_reveal_action,
            postcondition=z3.And(world.reveal_published, world.no_active_assignments),
            rules=("R-REVEAL-AFTER-HOST", "R-REVEAL-NO-ACTIVE"),
        )
    )
    scenarios.append(
        Scenario(
            id="S-REVEAL-PENDING",
            name="The reveal is refused while work is pending",
            feature="specs/features/reveal.feature",
            precondition=z3.And(
                world.host_reveal_action,
                world.owner[1] == 1,
                _turn(world, 0, 0, 1, 1),
            ),
            action=z3.Not(world.reveal_published),
            postcondition=z3.And(
                z3.Not(world.reveal_published),
                world.active_assignments_count == 1,
            ),
            rules=("R-REVEAL-NO-ACTIVE",),
        )
    )
    scenarios.append(
        Scenario(
            id="S-SOFT-DEADLINE",
            name="A soft deadline does not move a track",
            feature="specs/features/deadlines_and_extensions.feature",
            precondition=z3.And(
                world.round_open[1],
                world.host_opened_round[1],
                world.deadline_passed[1],
                z3.Not(world.auto_advanced[1]),
            ),
            action=world.deadline_passed[1],
            postcondition=z3.Not(world.auto_advanced[1]),
            rules=("R-NO-AUTO-ADVANCE", "R-ROUNDS-HOST-OPENED"),
        )
    )
    scenarios.append(
        Scenario(
            id="S-ONE-OPEN-ROUND",
            name="The host opens at most one round at a time",
            feature="specs/features/deadlines_and_extensions.feature",
            precondition=z3.And(
                world.round_open[0],
                world.host_opened_round[0],
                z3.Not(world.round_open[1]),
            ),
            action=world.round_open[0],
            postcondition=world.open_round_count == 1,
            rules=("R-ONE-OPEN-ROUND",),
        )
    )
    scenarios.append(
        Scenario(
            id="S-LEAVE-HOLDING",
            name="A participant leaves while holding a track",
            feature="specs/features/join_and_seed.feature",
            precondition=z3.And(
                world.leave[0],
                world.leave_after_claim[0],
                world.leave_held_track[0] == 1,
                world.seen[0][1],
            ),
            action=world.leave[0],
            postcondition=z3.And(
                world.held_track_seen[0],
                z3.Not(world.has_future_assignment[0]),
            ),
            rules=("R-LEAVE-SEEN", "R-LEAVE-NO-FUTURE"),
        )
    )
    scenarios.append(
        Scenario(
            id="S-LEAVER-RETURN",
            name="A leaver can return as a fill-in",
            feature="specs/features/join_and_seed.feature",
            precondition=z3.And(
                world.restored[0],
                world.invitation_action[0],
                world.invitation_sent_h[0] == 100,
                world.invitation_action_h[0] == 124,
                z3.Not(world.invitation_expired[0]),
            ),
            action=world.invitation_action[0],
            postcondition=z3.And(
                world.role[0] == Role.FILL_IN,
                z3.Not(world.has_personal_track[0]),
                world.invitation_valid_h == RESTORE_INVITATION_HOURS,
            ),
            rules=(
                "R-RESTORE-FILLIN-ONLY",
                "R-RESTORE-EXPIRY",
                "R-RESTORE-EXPIRY-SETTING",
            ),
        )
    )
    scenarios.append(
        Scenario(
            id="S-RESTORE-EXPIRY",
            name="A restore invitation expires after 7 days",
            feature="specs/features/join_and_seed.feature",
            precondition=z3.And(
                world.invitation_sent_h[0] == 100,
                world.invitation_action_h[0] == 300,
                world.invitation_expired[0],
                z3.Not(world.invitation_action[0]),
            ),
            action=z3.Not(world.invitation_action[0]),
            postcondition=z3.And(
                world.invitation_expired[0],
                world.invitation_valid_h == RESTORE_INVITATION_HOURS,
            ),
            rules=(
                "R-RESTORE-EXPIRY",
                "R-RESTORE-EXPIRY-SETTING",
                "R-RESTORE-EXPIRED-BLOCKS",
            ),
        )
    )
    scenarios.append(
        Scenario(
            id="S-FILL-DUE-CLAIM",
            name="A claimed fill-in offer starts the 24-hour edit time",
            feature="specs/features/fill_ins.feature",
            precondition=z3.And(
                world.fill_in_flow == Flow.AUTO_CHAIN,
                world.turn_kind[0] == TurnKind.FILL_IN,
                world.claim_time_h[0] == 100,
            ),
            action=world.fill_in_due_h[0] == 124,
            postcondition=z3.And(
                world.edit_window_h == EDIT_WINDOW_H,
                world.fill_in_due_h[0] == world.claim_time_h[0] + EDIT_WINDOW_H,
            ),
            rules=("R-FILL-DUE-CLAIM", "R-EDIT-WINDOW"),
        )
    )
    scenarios.append(
        Scenario(
            id="S-FILL-DUE-APPOINT",
            name="A host-appointed fill-in gets the later due time",
            feature="specs/features/fill_ins.feature",
            precondition=z3.And(
                world.fill_in_flow == Flow.HOST_APPOINTS,
                world.turn_kind[0] == TurnKind.FILL_IN,
                world.turn_round[0] == 1,
                world.round_deadline_h[1] == 150,
                world.appoint_time_h[0] == 100,
            ),
            action=world.fill_in_due_h[0] == 150,
            postcondition=z3.And(
                world.edit_window_h == EDIT_WINDOW_H,
                world.fill_in_due_h[0]
                == _max_int(
                    world.round_deadline_h[1],
                    world.appoint_time_h[0] + EDIT_WINDOW_H,
                ),
            ),
            rules=("R-FILL-DUE-APPOINT", "R-EDIT-WINDOW"),
        )
    )
    scenarios.append(
        Scenario(
            id="S-CLAIM-REMINDER",
            name="The bot reminds the participant to claim",
            feature="specs/features/async_queue.feature",
            precondition=world.claim_reminder_h == 12,
            action=world.claim_reminder_h < world.claim_window_h[OfferKind.REGULAR],
            postcondition=z3.And(
                world.claim_reminder_h == 12,
                world.claim_reminder_h < world.claim_window_h[OfferKind.REGULAR],
            ),
            rules=("R-CLAIM-REMINDER",),
        )
    )
    scenarios.append(
        Scenario(
            id="S-CLAIM-24",
            name="A claim delivers the file and starts the deadline",
            feature="specs/features/async_queue.feature",
            precondition=z3.And(
                world.claim_window_h[OfferKind.REGULAR] == REGULAR_CLAIM_WINDOW_H,
                world.owner[0] == 0,
                _turn(world, 0, 1, 0, 1),
            ),
            action=world.turn_claimed[0],
            postcondition=z3.And(
                world.turn_seen[0],
                world.claim_window_h[OfferKind.REGULAR] == REGULAR_CLAIM_WINDOW_H,
            ),
            rules=("R-CLAIM-WINDOW-REGULAR", "R-EDIT-WINDOW"),
        )
    )
    return scenarios
