---------------------------- MODULE SwapClaimPR ----------------------------
EXTENDS Naturals, TLC
CONSTANTS Participants, Tracks, NoOne, ClaimWindow, MaxTime
VARIABLES phase, seen, banned, holder, due, now
INSTANCE SwapClaim WITH Owner <- [t \in Tracks |-> IF t = "t1" THEN "p1" ELSE "p2"]
=============================================================================
