"""Typed schema for the requirement consistency model (D-076).

The model is finite and explicit. It is a reviewable formalization of the
normative rules, not a product dependency.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import IntEnum
from typing import Literal


class Mode(IntEnum):
    """Event mode."""

    CLASSIC = 0
    ASYNC = 1


class Role(IntEnum):
    """Participant role. The role is independent of membership."""

    PARTICIPANT = 0
    FILL_IN = 1


class Membership(IntEnum):
    """Membership state from the domain model."""

    SIGNED_UP = 0
    SEEDED = 1
    ACTIVE = 2
    LEFT = 3
    REMOVED = 4


class Tier(IntEnum):
    """Fill-in eligibility tier from D-050."""

    TIER1 = 0
    TIER2 = 1


class Flow(IntEnum):
    """Fill-in flow setting from D-040 and D-064."""

    HOST_APPOINTS = 0
    AUTO_CHAIN = 1
    HOST_LIST = 2


class OfferKind(IntEnum):
    """Offer kind. The claim window depends on the kind."""

    REGULAR = 0
    FILL_IN = 1


class TurnKind(IntEnum):
    """Assignment kind."""

    PLANNED = 0
    FILL_IN = 1


class FillInSource(IntEnum):
    """A tier-1 fill-in source from D-014 and D-050."""

    VOLUNTEER = 0
    CONVERTED_SIGNUP = 1
    HOST_APPOINTED = 2
    NONE = 3


class Participation(IntEnum):
    """Participation mode from REQ-SCHED-012."""

    STRICT = 0
    FLEXIBLE = 1


class TrackOutcome(IntEnum):
    """The outcome when no eligible fill-in exists."""

    PASSED = 0
    RETURNED = 1
    HELD = 2


# The finite domain. The numbers are small on purpose: a reviewer can read
# every pair, and the solver stays under the one-minute gate budget.
PARTICIPANTS = 3
TRACKS = 3
ROUNDS = 3
TURNS = 6
INVITATIONS = 2

HOURS_PER_DAY = 24
RESTORE_INVITATION_HOURS = 7 * HOURS_PER_DAY
DEFAULT_REGULAR_CLAIM_WINDOW_H = 24
DEFAULT_FILL_IN_CLAIM_WINDOW_H = 6
DEFAULT_CLAIM_REMINDER_H = 12
DEFAULT_EDIT_WINDOW_H = 24
AUTO_CHAIN_MAX_OFFERS = 3

CLAIM_WINDOW_MIN_H = 1
CLAIM_WINDOW_MAX_H = 48

SourceKind = Literal["decision", "prd", "setting", "scenario"]


@dataclass(frozen=True)
class Source:
    """One normative source. The report renders the path as a link."""

    kind: SourceKind
    ref: str
    path: str

    def label(self) -> str:
        return f"{self.ref} ({self.path})"


@dataclass(frozen=True)
class Rule:
    """One must rule with a stable id, sources, formula, and STE paraphrase."""

    id: str
    sources: tuple[Source, ...]
    paraphrase: str
    formula: object  # z3.BoolRef; typed as object to keep z3 out of the schema.
    severity: str = "must"
    kind: str = "behaviour"
    tags: tuple[str, ...] = field(default_factory=tuple)

    def source_refs(self) -> tuple[str, ...]:
        return tuple(source.ref for source in self.sources)


@dataclass(frozen=True)
class Scenario:
    """One formalized Gherkin scenario.

    The formula is the conjunction of the precondition, the action, and the
    postcondition. The rule links define coverage. `pending` mirrors the
    source scenario's @pending tag: PASS is model satisfiability, not
    implementation.
    """

    id: str
    name: str
    feature: str
    precondition: object
    action: object
    postcondition: object
    rules: tuple[str, ...]
    pending: bool = False

    @property
    def formula(self):
        import z3

        return z3.And(self.precondition, self.action, self.postcondition)

    @property
    def body(self):
        import z3

        return z3.And(self.precondition, self.action)


@dataclass(frozen=True)
class Premise:
    """A reachable mode that the rules must stay consistent under."""

    id: str
    description: str
    formula: object
