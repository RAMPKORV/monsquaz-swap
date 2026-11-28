------------------------- MODULE SwapScheduleGroupPR -------------------------
\* Grouped PR bound: 4 participants, 4 tracks, 2 groups of 2, 2 exchange
\* rounds. It makes GroupContainment non-vacuous in the PR gate.
EXTENDS Naturals, TLC
CONSTANTS Participants, Tracks, NoOne, GroupCount, RoundCount, Cadence, FallbackPhases
VARIABLES plan, nextRound, sealed
INSTANCE SwapSchedule WITH
    Owner <-
        [t \in Tracks |->
            CASE t = "t1" -> "p1"
              [] t = "t2" -> "p2"
              [] t = "t3" -> "p3"
              [] OTHER -> "p4"],
    GroupOf <-
        [p \in Participants |->
            IF p \in {"p1", "p2"} THEN 1 ELSE 2]
=============================================================================
