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
    CLAIM_WINDOW_MAX_H,
    CLAIM_WINDOW_MIN_H,
    DEFAULT_CLAIM_REMINDER_H,
    DEFAULT_EDIT_WINDOW_H,
    DEFAULT_FILL_IN_CLAIM_WINDOW_H,
    DEFAULT_REGULAR_CLAIM_WINDOW_H,
    INVITATIONS,
    PARTICIPANTS,
    RESTORE_INVITATION_HOURS,
    ROUNDS,
    TRACKS,
    TURNS,
    FillInSource,
    Flow,
    Membership,
    Mode,
    OfferKind,
    Participation,
    Premise,
    Role,
    Rule,
    Scenario,
    Source,
    TrackOutcome,
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
        self.participation_mode = z3.Int("participation_mode")
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
        self.submitted_own_this_round = [
            z3.Bool(f"submitted_own_this_round_{person}")
            for person in range(PARTICIPANTS)
        ]
        self.fill_in_source = [
            z3.Int(f"fill_in_source_{person}") for person in range(PARTICIPANTS)
        ]
        self.leave = [z3.Bool(f"leave_{person}") for person in range(PARTICIPANTS)]
        self.leave_after_claim = [
            z3.Bool(f"leave_after_claim_{person}") for person in range(PARTICIPANTS)
        ]
        self.leave_held_track = [
            z3.Int(f"leave_held_track_{person}") for person in range(PARTICIPANTS)
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
        self.plan_never_gives_track = [
            [z3.Bool(f"plan_never_gives_track_{person}_{track}") for track in range(TRACKS)]
            for person in range(PARTICIPANTS)
        ]
        self.tier2_turns = [
            [z3.Int(f"tier2_turns_{person}_{round}") for round in range(ROUNDS)]
            for person in range(PARTICIPANTS)
        ]
        self.tier2_offer_round = [
            [z3.Int(f"tier2_offer_round_{person}_{track}") for track in range(TRACKS)]
            for person in range(PARTICIPANTS)
        ]

        # Settings. The defaults are separate from the current event values.
        self.default_claim_window_h = z3.Int("default_claim_window_h")
        self.default_fill_in_claim_window_h = z3.Int("default_fill_in_claim_window_h")
        self.default_claim_reminder_h = z3.Int("default_claim_reminder_h")
        self.default_edit_window_h = z3.Int("default_edit_window_h")
        self.claim_window_h = [z3.Int(f"claim_window_h_{kind}") for kind in range(2)]
        self.claim_reminder_h = z3.Int("claim_reminder_h")
        self.edit_window_h = z3.Int("edit_window_h")
        self.offer_kind = z3.Int("offer_kind")
        self.offer_claim_window_h = z3.Int("offer_claim_window_h")
        self.fill_in_flow = z3.Int("fill_in_flow")
        self.fill_in_offer_cap = [z3.Bool(f"fill_in_offer_cap_{flow}") for flow in range(3)]
        self.max_auto_chain_offers = z3.Int("max_auto_chain_offers")
        self.open_fill_in_offers = z3.Int("open_fill_in_offers")
        self.first_accept_wins = z3.Bool("first_accept_wins")
        self.event_wide_tier2_cap = z3.Bool("event_wide_tier2_cap")
        self.invitation_valid_h = z3.Int("invitation_valid_h")
        self.replan_allowed = z3.Bool("replan_allowed")

        # Classic length and participation.
        self.participant_count = z3.Int("participant_count")
        self.total_rounds = z3.Int("total_rounds")
        self.exchange_rounds = z3.Int("exchange_rounds")
        self.plan_valid = z3.Bool("plan_valid")

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
        self.turn_edit_due_h = [z3.Int(f"turn_edit_due_h_{turn}") for turn in range(TURNS)]

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
        self.release_stream_finished = z3.Bool("release_stream_finished")
        self.reveal_published = z3.Bool("reveal_published")
        self.active_assignments_count = z3.Int("active_assignments_count")
        self.no_active_assignments = z3.Bool("no_active_assignments")

        # Leave and restore.
        self.current_round = z3.Int("current_round")
        self.invitation_sent_h = [z3.Int(f"invitation_sent_h_{item}") for item in range(INVITATIONS)]
        self.invitation_action_h = [
            z3.Int(f"invitation_action_h_{item}") for item in range(INVITATIONS)
        ]
        self.invitation_action = [z3.Bool(f"invitation_action_{item}") for item in range(INVITATIONS)]
        self.invitation_expired = [
            z3.Bool(f"invitation_expired_{item}") for item in range(INVITATIONS)
        ]

        # Fill-in outcome.
        self.no_eligible_fill_in = z3.Bool("no_eligible_fill_in")
        self.track_outcome = z3.Int("track_outcome")
        self.pass_unchanged = z3.Bool("pass_unchanged")
        self.return_to_pool = z3.Bool("return_to_pool")

        self.core = z3.And(*self._domain_constraints())

    def _domain_constraints(self) -> list:
        constraints: list = [
            self.mode >= Mode.CLASSIC,
            self.mode <= Mode.ASYNC,
            self.participation_mode >= Participation.STRICT,
            self.participation_mode <= Participation.FLEXIBLE,
            self.fill_in_flow >= Flow.HOST_APPOINTS,
            self.fill_in_flow <= Flow.HOST_LIST,
            self.max_auto_chain_offers >= 0,
            self.open_fill_in_offers >= 0,
            self.invitation_valid_h >= 0,
            self.default_edit_window_h > 0,
            self.edit_window_h > 0,
            self.claim_reminder_h >= 0,
            self.participant_count >= 2,
            self.total_rounds >= 1,
            self.exchange_rounds >= 0,
            self.current_round >= 0,
            self.current_round < ROUNDS,
            self.offer_kind >= OfferKind.REGULAR,
            self.offer_kind <= OfferKind.FILL_IN,
            self.track_outcome >= TrackOutcome.PASSED,
            self.track_outcome <= TrackOutcome.HELD,
        ]
        for kind in range(2):
            constraints.extend(
                [
                    self.claim_window_h[kind] >= CLAIM_WINDOW_MIN_H,
                    self.claim_window_h[kind] <= CLAIM_WINDOW_MAX_H,
                ]
            )
        for default in (
            self.default_claim_window_h,
            self.default_fill_in_claim_window_h,
            self.default_claim_reminder_h,
            self.default_edit_window_h,
        ):
            constraints.append(default > 0)

        constraints.extend(
            [
                self.offer_claim_window_h
                == z3.If(
                    self.offer_kind == OfferKind.REGULAR,
                    self.claim_window_h[OfferKind.REGULAR],
                    self.claim_window_h[OfferKind.FILL_IN],
                ),
                self.pass_unchanged == (self.track_outcome == TrackOutcome.PASSED),
                self.return_to_pool == (self.track_outcome == TrackOutcome.RETURNED),
            ]
        )

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
                    self.fill_in_source[person] >= FillInSource.VOLUNTEER,
                    self.fill_in_source[person] <= FillInSource.NONE,
                ]
            )
            constraints.append(
                self.tier2_rescues[person]
                == z3.Sum([self.tier2_turns[person][round] for round in range(ROUNDS)])
            )
            for round in range(ROUNDS):
                constraints.extend(
                    [self.tier2_turns[person][round] >= 0, self.tier2_turns[person][round] <= TURNS]
                )
            for track in range(TRACKS):
                constraints.extend(
                    [
                        self.tier2_offer_round[person][track] >= 0,
                        self.tier2_offer_round[person][track] < ROUNDS,
                    ]
                )
                constraints.append(
                    self.tier1_eligible[person][track]
                    == z3.And(
                        self.role[person] == Role.FILL_IN,
                        self.membership[person] == Membership.ACTIVE,
                        self.ready[person],
                        z3.Not(self.seen[person][track]),
                        self.owner[track] != person,
                        self.fill_in_source[person] != FillInSource.NONE,
                    )
                )
                constraints.append(
                    self.tier2_eligible[person][track]
                    == z3.And(
                        self.role[person] == Role.PARTICIPANT,
                        self.membership[person] == Membership.ACTIVE,
                        self.ready[person],
                        self.submitted_own_this_round[person],
                        z3.Not(self.seen[person][track]),
                        self.owner[track] != person,
                        self.plan_never_gives_track[person][track],
                    )
                )

        constraints.append(
            self.open_fill_in_offers
            == z3.Sum(
                [
                    z3.If(
                        z3.Or(self.tier1_offer[person][track], self.tier2_offer[person][track]),
                        1,
                        0,
                    )
                    for person in range(PARTICIPANTS)
                    for track in range(TRACKS)
                ]
            )
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
                        self.turn_claimed[turn],
                        self.turn_submitted[turn],
                        self.turn_skip_before_claim[turn],
                        self.turn_skip_after_claim[turn],
                        self.turn_expired[turn],
                    ),
                    z3.Implies(self.turn_claimed[turn], self.turn_assigned[turn]),
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

        for person in range(PARTICIPANTS):
            constraints.append(
                self.has_future_assignment[person]
                == z3.Or(
                    [
                        z3.And(
                            self.turn_assigned[turn],
                            self.turn_holder[turn] == person,
                            self.turn_round[turn] > self.current_round,
                        )
                        for turn in range(TURNS)
                    ]
                )
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
            paraphrase="The default regular claim window must be 24 hours.",
            formula=world.default_claim_window_h == DEFAULT_REGULAR_CLAIM_WINDOW_H,
            kind="setting",
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
            paraphrase="The default fill-in claim window must be 6 hours.",
            formula=world.default_fill_in_claim_window_h == DEFAULT_FILL_IN_CLAIM_WINDOW_H,
            kind="setting",
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
            paraphrase=(
                "The default claim reminder must be 12 hours. "
                "The claim reminder must occur before the claim window ends."
            ),
            formula=z3.And(
                world.default_claim_reminder_h == DEFAULT_CLAIM_REMINDER_H,
                world.claim_reminder_h < world.claim_window_h[OfferKind.REGULAR],
            ),
            kind="setting",
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
            formula=world.default_edit_window_h == DEFAULT_EDIT_WINDOW_H,
            kind="setting",
            tags=("deadline", "setting"),
        )
    )
    rules.append(
        Rule(
            id="R-CLAIM-EDIT-DUE",
            sources=(
                _prd("REQ-ASYNC-002"),
                _decision("D-033"),
                _scenario("A claim delivers the file and starts the deadline", "async_queue.feature"),
            ),
            paraphrase="A claimed assignment must start the edit due time at the claim.",
            formula=z3.And(
                [
                    z3.Implies(
                        world.turn_claimed[turn],
                        world.turn_edit_due_h[turn]
                        == world.claim_time_h[turn] + world.edit_window_h,
                    )
                    for turn in range(TURNS)
                ]
            ),
            tags=("claim", "deadline"),
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
            kind="setting",
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
            kind="setting",
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
            paraphrase="The bot must have at most one open fill-in offer at a time.",
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
            kind="setting",
            tags=("fill-in", "setting"),
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
                "The two times are the round deadline and the default edit window."
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
                            world.appoint_time_h[turn] + world.default_edit_window_h,
                        ),
                    )
                    for turn in range(TURNS)
                ]
            ),
            kind="setting",
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
            paraphrase="A claimed fill-in offer must start the edit due time at the claim.",
            formula=z3.And(
                [
                    z3.Implies(
                        z3.And(
                            world.is_fill_in_turn[turn],
                            world.turn_claimed[turn],
                            z3.Or(
                                world.fill_in_flow == Flow.AUTO_CHAIN,
                                world.fill_in_flow == Flow.HOST_LIST,
                            ),
                        ),
                        world.fill_in_due_h[turn]
                        == world.claim_time_h[turn] + world.edit_window_h,
                    )
                    for turn in range(TURNS)
                ]
            ),
            tags=("fill-in", "deadline", "setting"),
        )
    )
    rules.append(
        Rule(
            id="R-TIER1-OFFER-ELIGIBLE",
            sources=(
                _decision("D-050"),
                _prd("REQ-FILL-002"),
                _setting("Fill-in flow"),
                _scenario("The fill-in pool has three sources", "fill_ins.feature"),
            ),
            paraphrase=(
                "A tier-1 offer must go to a person with the fill-in role who is active. "
                "The person must be ready, must have a fill-in source, and must not have seen the track. "
                "The person must not own the track."
            ),
            formula=z3.And(
                [
                    z3.Implies(
                        world.tier1_offer[person][track],
                        z3.And(
                            world.role[person] == Role.FILL_IN,
                            world.membership[person] == Membership.ACTIVE,
                            world.ready[person],
                            z3.Not(world.seen[person][track]),
                            world.owner[track] != person,
                            world.fill_in_source[person] != FillInSource.NONE,
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
            id="R-TIER2-OFFER-ELIGIBLE",
            sources=(
                _decision("D-050"),
                _prd("REQ-FILL-002"),
                _scenario("A tier-2 fill-in rescue is an extra turn", "fill_ins.feature"),
            ),
            paraphrase=(
                "A tier-2 offer must go to an active seeded participant. "
                "The participant must have submitted their own turn in the current round. "
                "The plan must never give the track. "
                "The participant must not have seen the track and must not own it. "
                "The offer round must have at most one tier-2 turn."
            ),
            formula=z3.And(
                [
                    z3.Implies(
                        world.tier2_offer[person][track],
                        z3.And(
                            world.role[person] == Role.PARTICIPANT,
                            world.membership[person] == Membership.ACTIVE,
                            world.ready[person],
                            world.submitted_own_this_round[person],
                            z3.Not(world.seen[person][track]),
                            world.owner[track] != person,
                            world.plan_never_gives_track[person][track],
                            _select_int(
                                world.tier2_offer_round[person][track],
                                world.tier2_turns[person],
                            )
                            <= 1,
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
            id="R-TIER2-ONLY-NO-TIER1",
            sources=(
                _decision("D-050"),
                _prd("REQ-FILL-002"),
                _scenario("A tier-2 fill-in rescue is an extra turn", "fill_ins.feature"),
            ),
            paraphrase=(
                "A tier-2 offer must occur only after tier 1, when no tier-1 person is eligible for the track."
            ),
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
            kind="setting",
            tags=("fill-in", "tier", "setting"),
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
                    world.track_outcome == TrackOutcome.PASSED,
                ),
                z3.Implies(
                    z3.And(world.no_eligible_fill_in, world.mode == Mode.ASYNC),
                    world.track_outcome == TrackOutcome.RETURNED,
                ),
            ),
            tags=("fill-in",),
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
            kind="invariant",
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
            ),
            kind="invariant",
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
            id="R-CLASSIC-LENGTH",
            sources=(
                _decision("D-030"),
                _prd("REQ-SCHED-002"),
                _scenario("A default classic event runs N total rounds", "scheduling.feature"),
            ),
            paraphrase="A full classic event must run N total rounds and N minus one exchange rounds.",
            formula=z3.And(
                world.total_rounds == world.participant_count,
                world.exchange_rounds == world.total_rounds - 1,
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
            paraphrase="An open round must have a host open action.",
            formula=z3.And(
                [
                    z3.Implies(world.round_open[round], world.host_opened_round[round])
                    for round in range(ROUNDS)
                ]
            ),
            kind="invariant",
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
            kind="invariant",
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
            id="R-STRICT-REFUSAL",
            sources=(
                _prd("REQ-SCHED-012"),
                _setting("Participation"),
                _scenario(
                    "Strict participation requires enough handoff budget",
                    "participation.feature",
                ),
            ),
            paraphrase=(
                "A strict event must refuse a plan when a handoff cap is below the exchange count."
            ),
            formula=z3.And(
                [
                    z3.Implies(
                        z3.And(
                            world.participation_mode == Participation.STRICT,
                            world.max_handoffs[person] < world.exchange_rounds,
                        ),
                        z3.Not(world.plan_valid),
                    )
                    for person in range(PARTICIPANTS)
                ]
            ),
            tags=("scheduling", "participation"),
        )
    )
    rules.append(
        Rule(
            id="R-REVEAL-AFTER-HOST",
            sources=(
                _decision("D-017"),
                _prd("REQ-REVEAL-001"),
                _setting("Reveal trigger"),
                _scenario("A reveal without the host action is refused", "reveal.feature"),
            ),
            paraphrase="The app must not publish the reveal before the host acts.",
            formula=z3.Implies(world.reveal_published, world.host_reveal_action),
            kind="invariant",
            tags=("reveal", "invariant"),
        )
    )
    rules.append(
        Rule(
            id="R-REVEAL-HOST-TRIGGER",
            sources=(
                _decision("D-017"),
                _prd("REQ-REVEAL-001"),
                _setting("Reveal trigger"),
                _scenario("The host triggers the reveal", "reveal.feature"),
            ),
            paraphrase=(
                "The app must publish the reveal when the host acts after the release stream. "
                "The event must have no active assignment."
            ),
            formula=z3.Implies(
                z3.And(
                    world.host_reveal_action,
                    world.release_stream_finished,
                    world.no_active_assignments,
                ),
                world.reveal_published,
            ),
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
            kind="invariant",
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
                    z3.Implies(
                        world.leave_after_claim[person],
                        _select_bool(world.leave_held_track[person], world.seen[person]),
                    )
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
            paraphrase=(
                "Without the replan option, a leaver must return as a fill-in only. "
                "The replan option can give a new personal track."
            ),
            formula=z3.Implies(
                z3.Not(world.replan_allowed),
                z3.And(
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
                        < world.invitation_sent_h[item] + RESTORE_INVITATION_HOURS,
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
            kind="setting",
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
    return rules


def build_scenarios(world: World) -> list[Scenario]:
    """Build the formalized scenarios and their rule links."""

    scenarios: list[Scenario] = []

    scenarios.append(
        Scenario(
            id="S-FILL-SEQUENCE",
            name="A fill-in offer uses the standard sequence",
            feature="specs/features/fill_ins.feature",
            precondition=z3.And(
                world.offer_kind == OfferKind.FILL_IN,
                world.claim_window_h[OfferKind.FILL_IN] == world.default_fill_in_claim_window_h,
                world.fill_in_flow == Flow.AUTO_CHAIN,
                world.turn_kind[0] == TurnKind.FILL_IN,
                world.turn_offered[0],
                z3.Or(
                    [
                        z3.Or(world.tier1_offer[person][track], world.tier2_offer[person][track])
                        for person in range(PARTICIPANTS)
                        for track in range(TRACKS)
                    ]
                ),
            ),
            action=world.turn_offered[0],
            postcondition=z3.And(
                world.default_fill_in_claim_window_h == DEFAULT_FILL_IN_CLAIM_WINDOW_H,
                world.offer_claim_window_h == DEFAULT_FILL_IN_CLAIM_WINDOW_H,
                world.max_auto_chain_offers == AUTO_CHAIN_MAX_OFFERS,
                world.fill_in_offer_cap[Flow.AUTO_CHAIN],
                z3.Not(world.fill_in_offer_cap[Flow.HOST_LIST]),
                world.open_fill_in_offers <= 1,
                world.first_accept_wins,
            ),
            rules=(
                "R-CLAIM-WINDOW-FILLIN",
                "R-FILL-AUTO-CHAIN",
                "R-FILL-HOST-LIST",
                "R-FILL-SEQUENTIAL",
                "R-FILL-FIRST-ACCEPT",
            ),
            pending=False,
        )
    )
    scenarios.append(
        Scenario(
            id="S-TIER2-EXTRA-TURN",
            name="A tier-2 fill-in rescue is an extra turn",
            feature="specs/features/fill_ins.feature",
            precondition=z3.And(
                world.owner[0] == 0,
                world.owner[1] == 1,
                world.owner[2] == 2,
                world.tier2_offer[1][0],
                world.tier2_offer_round[1][0] == 1,
                world.planned_edits[0] == 2,
                world.tier2_rescues[0] == 1,
                world.max_handoffs[0] == 4,
            ),
            action=world.tier2_offer[1][0],
            postcondition=z3.And(
                world.role[1] == Role.PARTICIPANT,
                world.membership[1] == Membership.ACTIVE,
                world.ready[1],
                world.submitted_own_this_round[1],
                z3.Not(world.seen[1][0]),
                world.owner[0] != 1,
                world.plan_never_gives_track[1][0],
                z3.Not(
                    z3.Or([world.tier1_eligible[person][0] for person in range(PARTICIPANTS)])
                ),
                world.tier2_turns[1][1] <= 1,
                world.tier2_turns[1][2] <= 1,
                z3.Not(world.event_wide_tier2_cap),
                world.budget_used[0] == world.planned_edits[0],
                world.budget_after_tier2[0] == world.budget_used[0],
            ),
            rules=(
                "R-TIER2-OFFER-ELIGIBLE",
                "R-TIER2-ONLY-NO-TIER1",
                "R-TIER2-ONE-PER-ROUND",
                "R-TIER2-NO-EVENT-CAP",
                "R-BUDGET-REGULAR",
                "R-BUDGET-TIER2-EXTRA",
            ),
            pending=False,
        )
    )
    scenarios.append(
        Scenario(
            id="S-FILL-POOL",
            name="The fill-in pool has three sources",
            feature="specs/features/fill_ins.feature",
            precondition=z3.And(
                world.owner[0] == 0,
                world.owner[1] == 1,
                world.owner[2] == 2,
                world.fill_in_source[0] == FillInSource.HOST_APPOINTED,
                world.fill_in_source[1] == FillInSource.VOLUNTEER,
                world.fill_in_source[2] == FillInSource.CONVERTED_SIGNUP,
                world.tier1_offer[1][0],
                world.tier1_eligible[0][1],
                world.tier1_eligible[2][0],
            ),
            action=world.tier1_offer[1][0],
            postcondition=z3.And(
                [
                    z3.And(
                        world.role[person] == Role.FILL_IN,
                        world.membership[person] == Membership.ACTIVE,
                        world.ready[person],
                        z3.Not(world.seen[person][track]),
                        world.owner[track] != person,
                        world.fill_in_source[person] != FillInSource.NONE,
                    )
                    for person, track in ((0, 1), (1, 0), (2, 0))
                ]
            ),
            rules=("R-TIER1-OFFER-ELIGIBLE",),
            pending=False,
        )
    )
    scenarios.append(
        Scenario(
            id="S-FILL-NO-ELIGIBLE",
            name="No eligible fill-in exists",
            feature="specs/features/fill_ins.feature",
            precondition=z3.And(world.mode == Mode.CLASSIC, world.no_eligible_fill_in),
            action=z3.BoolVal(True),
            postcondition=world.track_outcome == TrackOutcome.PASSED,
            rules=("R-FILL-PASS-UNCHANGED",),
            pending=False,
        )
    )
    scenarios.append(
        Scenario(
            id="S-FILL-PASS-ASYNC",
            name="No eligible fill-in exists",
            feature="specs/features/fill_ins.feature",
            precondition=z3.And(world.mode == Mode.ASYNC, world.no_eligible_fill_in),
            action=z3.BoolVal(True),
            postcondition=world.track_outcome == TrackOutcome.RETURNED,
            rules=("R-FILL-PASS-UNCHANGED",),
            pending=False,
        )
    )
    scenarios.append(
        Scenario(
            id="S-SCHEDULE-ROUND",
            name="Every participant receives one unseen track per round",
            feature="specs/features/scheduling.feature",
            precondition=z3.And(
                world.owner[0] == 0,
                world.owner[1] == 1,
                world.owner[2] == 2,
                *[
                    z3.And(
                        world.turn_assigned[turn],
                        world.turn_claimed[turn],
                        world.turn_active[turn],
                        world.turn_round[turn] == (0 if turn < 3 else 1),
                    )
                    for turn in range(TURNS)
                ],
            ),
            action=world.turn_assigned[0],
            postcondition=z3.And(
                *[
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
                    == 1
                    for person in range(PARTICIPANTS)
                    for round in range(2)
                ],
                *[
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
                    for round in range(2)
                ],
                *[z3.Not(world.own_track_turn[turn]) for turn in range(TURNS)],
                *[z3.Not(world.repeat_pair[turn]) for turn in range(TURNS)],
            ),
            rules=(
                "R-NO-REPEAT",
                "R-NO-OWN-TRACK",
                "R-ONE-ASSIGNMENT-PER-PERSON-ROUND",
                "R-ONE-ASSIGNMENT-PER-TRACK-ROUND",
            ),
            pending=False,
        )
    )
    scenarios.append(
        Scenario(
            id="S-CLASSIC-LENGTH",
            name="A default classic event runs N total rounds",
            feature="specs/features/scheduling.feature",
            precondition=z3.And(
                world.mode == Mode.CLASSIC,
                world.participant_count == 6,
                world.round_open[1],
            ),
            action=world.round_open[1],
            postcondition=z3.And(
                world.total_rounds == 6,
                world.exchange_rounds == 5,
                world.host_opened_round[1],
            ),
            rules=("R-CLASSIC-LENGTH", "R-ROUNDS-HOST-OPENED"),
            pending=True,
        )
    )
    scenarios.append(
        Scenario(
            id="S-REVEAL-HOST",
            name="The host triggers the reveal",
            feature="specs/features/reveal.feature",
            precondition=z3.And(
                [z3.Not(world.turn_active[turn]) for turn in range(TURNS)]
                + [world.host_reveal_action, world.release_stream_finished]
            ),
            action=world.host_reveal_action,
            postcondition=z3.And(world.reveal_published, world.host_reveal_action),
            rules=("R-REVEAL-HOST-TRIGGER",),
            pending=True,
        )
    )
    scenarios.append(
        Scenario(
            id="S-REVEAL-NO-HOST",
            name="A reveal without the host action is refused",
            feature="specs/features/reveal.feature",
            precondition=z3.And(
                [z3.Not(world.turn_active[turn]) for turn in range(TURNS)]
                + [
                    z3.Not(world.host_reveal_action),
                    world.release_stream_finished,
                ]
            ),
            action=world.release_stream_finished,
            postcondition=z3.Not(world.reveal_published),
            rules=("R-REVEAL-AFTER-HOST",),
            pending=True,
        )
    )
    scenarios.append(
        Scenario(
            id="S-REVEAL-PENDING",
            name="The reveal is refused while work is pending",
            feature="specs/features/reveal.feature",
            precondition=z3.And(
                world.host_reveal_action,
                world.release_stream_finished,
                world.turn_active[0],
                world.owner[1] == 1,
                world.turn_holder[0] == 0,
                world.turn_track[0] == 1,
                world.turn_round[0] == 1,
                world.turn_claimed[0],
            ),
            action=world.host_reveal_action,
            postcondition=z3.Not(world.reveal_published),
            rules=("R-REVEAL-NO-ACTIVE",),
            pending=False,
        )
    )
    scenarios.append(
        Scenario(
            id="S-SOFT-DEADLINE",
            name="A soft deadline does not move a track",
            feature="specs/features/deadlines_and_extensions.feature",
            precondition=world.deadline_passed[1],
            action=world.deadline_passed[1],
            postcondition=z3.Not(world.auto_advanced[1]),
            rules=("R-NO-AUTO-ADVANCE",),
            pending=False,
        )
    )
    scenarios.append(
        Scenario(
            id="S-ONE-OPEN-ROUND",
            name="The host opens at most one round at a time",
            feature="specs/features/deadlines_and_extensions.feature",
            precondition=z3.Or(world.round_open[0], world.round_open[1]),
            action=world.round_open[0],
            postcondition=world.open_round_count <= 1,
            rules=("R-ONE-OPEN-ROUND",),
            pending=True,
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
                world.owner[1] == 1,
                world.current_round == 0,
            ),
            action=world.leave[0],
            postcondition=z3.And(
                world.seen[0][1],
                z3.Not(world.has_future_assignment[0]),
            ),
            rules=("R-LEAVE-SEEN", "R-LEAVE-NO-FUTURE"),
            pending=True,
        )
    )
    scenarios.append(
        Scenario(
            id="S-LEAVER-RETURN",
            name="A leaver can return as a fill-in",
            feature="specs/features/join_and_seed.feature",
            precondition=z3.And(
                world.restored[0],
                z3.Not(world.replan_allowed),
                world.invitation_action[0],
                world.invitation_sent_h[0] == 100,
            ),
            action=world.invitation_action[0],
            postcondition=z3.And(
                world.role[0] == Role.FILL_IN,
                z3.Not(world.has_personal_track[0]),
                world.invitation_action_h[0] < world.invitation_sent_h[0] + RESTORE_INVITATION_HOURS,
                world.invitation_valid_h == RESTORE_INVITATION_HOURS,
            ),
            rules=(
                "R-RESTORE-FILLIN-ONLY",
                "R-RESTORE-EXPIRY",
                "R-RESTORE-EXPIRY-SETTING",
            ),
            pending=True,
        )
    )
    scenarios.append(
        Scenario(
            id="S-RESTORE-EXPIRY",
            name="A restore invitation expires after 7 days",
            feature="specs/features/join_and_seed.feature",
            precondition=z3.And(
                world.invitation_sent_h[0] == 100,
                world.invitation_expired[0],
            ),
            action=world.invitation_expired[0],
            postcondition=z3.And(
                z3.Not(world.invitation_action[0]),
                world.invitation_valid_h == RESTORE_INVITATION_HOURS,
            ),
            rules=("R-RESTORE-EXPIRED-BLOCKS", "R-RESTORE-EXPIRY-SETTING"),
            pending=True,
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
                world.turn_claimed[0],
                world.claim_time_h[0] == 100,
                world.edit_window_h == DEFAULT_EDIT_WINDOW_H,
            ),
            action=world.turn_claimed[0],
            postcondition=z3.And(
                world.fill_in_due_h[0] == world.claim_time_h[0] + world.edit_window_h,
                world.turn_edit_due_h[0] == world.claim_time_h[0] + world.edit_window_h,
                world.default_edit_window_h == DEFAULT_EDIT_WINDOW_H,
            ),
            rules=("R-FILL-DUE-CLAIM", "R-CLAIM-EDIT-DUE", "R-EDIT-WINDOW"),
            pending=True,
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
            action=world.is_fill_in_turn[0],
            postcondition=z3.And(
                world.fill_in_due_h[0] == 150,
                world.default_edit_window_h == DEFAULT_EDIT_WINDOW_H,
            ),
            rules=("R-FILL-DUE-APPOINT", "R-EDIT-WINDOW"),
            pending=True,
        )
    )
    scenarios.append(
        Scenario(
            id="S-CLAIM-24",
            name="A claim delivers the file and starts the deadline",
            feature="specs/features/async_queue.feature",
            precondition=z3.And(
                world.offer_kind == OfferKind.REGULAR,
                world.claim_window_h[OfferKind.REGULAR] == world.default_claim_window_h,
                world.owner[0] == 0,
                world.turn_claimed[0],
                world.turn_holder[0] == 1,
                world.turn_track[0] == 0,
                world.turn_round[0] == 1,
                world.claim_time_h[0] == 100,
                world.edit_window_h == DEFAULT_EDIT_WINDOW_H,
            ),
            action=world.turn_claimed[0],
            postcondition=z3.And(
                world.default_claim_window_h == DEFAULT_REGULAR_CLAIM_WINDOW_H,
                world.offer_claim_window_h == DEFAULT_REGULAR_CLAIM_WINDOW_H,
                world.turn_seen[0],
                world.turn_edit_due_h[0] == 124,
                world.default_edit_window_h == DEFAULT_EDIT_WINDOW_H,
            ),
            rules=("R-CLAIM-WINDOW-REGULAR", "R-CLAIM-EDIT-DUE", "R-EDIT-WINDOW"),
            pending=True,
        )
    )
    scenarios.append(
        Scenario(
            id="S-CLAIM-REMINDER",
            name="The bot reminds the participant to claim",
            feature="specs/features/async_queue.feature",
            precondition=z3.And(
                world.offer_kind == OfferKind.REGULAR,
                world.claim_window_h[OfferKind.REGULAR] == world.default_claim_window_h,
                world.claim_reminder_h == DEFAULT_CLAIM_REMINDER_H,
            ),
            action=world.claim_reminder_h == DEFAULT_CLAIM_REMINDER_H,
            postcondition=z3.And(
                world.default_claim_reminder_h == DEFAULT_CLAIM_REMINDER_H,
                world.claim_reminder_h < world.claim_window_h[OfferKind.REGULAR],
            ),
            rules=("R-CLAIM-REMINDER",),
            pending=False,
        )
    )
    scenarios.append(
        Scenario(
            id="S-STRICT-REFUSAL",
            name="Strict participation requires enough handoff budget",
            feature="specs/features/participation.feature",
            precondition=z3.And(
                world.participation_mode == Participation.STRICT,
                world.exchange_rounds == 6,
                world.max_handoffs[0] == 4,
            ),
            action=world.participation_mode == Participation.STRICT,
            postcondition=z3.Not(world.plan_valid),
            rules=("R-STRICT-REFUSAL",),
            pending=True,
        )
    )
    return scenarios


def build_premises(world: World) -> list[Premise]:
    """Reachable modes that every rule set must stay consistent under."""
    return [
        Premise("P-CLASSIC", "Classic mode is reachable.", world.mode == Mode.CLASSIC),
        Premise("P-ASYNC", "Async mode is reachable.", world.mode == Mode.ASYNC),
        Premise(
            "P-REGULAR-OFFER",
            "A regular offer is reachable.",
            world.offer_kind == OfferKind.REGULAR,
        ),
        Premise(
            "P-FILL-IN-OFFER",
            "A fill-in offer is reachable.",
            world.offer_kind == OfferKind.FILL_IN,
        ),
        Premise(
            "P-AUTO-CHAIN",
            "The auto-chain flow is reachable.",
            world.fill_in_flow == Flow.AUTO_CHAIN,
        ),
        Premise(
            "P-HOST-LIST",
            "The host-list flow is reachable.",
            world.fill_in_flow == Flow.HOST_LIST,
        ),
        Premise(
            "P-TIER1-ELIGIBLE",
            "A tier-1 person can be eligible.",
            z3.Or(
                [
                    world.tier1_eligible[person][track]
                    for person in range(PARTICIPANTS)
                    for track in range(TRACKS)
                ]
            ),
        ),
        Premise(
            "P-NO-TIER1",
            "No tier-1 person is eligible.",
            z3.Not(
                z3.Or(
                    [
                        world.tier1_eligible[person][track]
                        for person in range(PARTICIPANTS)
                        for track in range(TRACKS)
                    ]
                )
            ),
        ),
        Premise(
            "P-TIER2-ELIGIBLE",
            "A tier-2 person can be eligible.",
            z3.Or(
                [
                    world.tier2_eligible[person][track]
                    for person in range(PARTICIPANTS)
                    for track in range(TRACKS)
                ]
            ),
        ),
        Premise(
            "P-NO-TIER2",
            "No tier-2 person is eligible.",
            z3.Not(
                z3.Or(
                    [
                        world.tier2_eligible[person][track]
                        for person in range(PARTICIPANTS)
                        for track in range(TRACKS)
                    ]
                )
            ),
        ),
        Premise(
            "P-LEAVE",
            "A participant can leave.",
            z3.Or([world.leave[person] for person in range(PARTICIPANTS)]),
        ),
        Premise(
            "P-RESTORE",
            "A leaver can be restored.",
            z3.Or([world.restored[person] for person in range(PARTICIPANTS)]),
        ),
        Premise(
            "P-STRICT",
            "Strict participation is reachable.",
            world.participation_mode == Participation.STRICT,
        ),
        Premise(
            "P-FLEXIBLE",
            "Flexible participation is reachable.",
            world.participation_mode == Participation.FLEXIBLE,
        ),
    ]
