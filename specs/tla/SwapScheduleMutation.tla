------------------------- MODULE SwapScheduleMutation -------------------------
\* Mutation bound: 6 participants, 6 tracks, 2 groups of 3, 4 exchange rounds.
\* The bound exercises repeats, own tracks, group containment, early seal, and
\* the type of every variable.
EXTENDS Naturals, TLC
CONSTANTS Participants, Tracks, NoOne, GroupCount, RoundCount, Cadence, FallbackPhases
VARIABLES plan, nextRound, sealed
INSTANCE SwapSchedule WITH
    Owner <-
        [t \in Tracks |->
            CASE t = "t1" -> "p1"
              [] t = "t2" -> "p2"
              [] t = "t3" -> "p3"
              [] t = "t4" -> "p4"
              [] t = "t5" -> "p5"
              [] OTHER -> "p6"],
    GroupOf <-
        [p \in Participants |->
            IF p \in {"p1", "p2", "p3"} THEN 1 ELSE 2]
=============================================================================
