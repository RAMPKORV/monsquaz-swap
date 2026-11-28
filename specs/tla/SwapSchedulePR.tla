---------------------------- MODULE SwapSchedulePR ----------------------------
\* PR bound: 4 participants, 4 tracks, 3 exchange rounds, one group.
\* This is the default full-exposure classic event for a 4-person roster.
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
    GroupOf <- [p \in Participants |-> 1]
=============================================================================
