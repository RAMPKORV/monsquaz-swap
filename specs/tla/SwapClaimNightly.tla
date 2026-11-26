---------------------------- MODULE SwapClaimNightly -----------------------
EXTENDS Naturals, TLC
CONSTANTS Participants, Tracks, NoOne, ClaimWindow, MaxTime
VARIABLES phase, seen, banned, holder, due, now
INSTANCE SwapClaim WITH Owner <-
    [t \in Tracks |->
        CASE t = "t1" -> "p1"
          [] t = "t2" -> "p2"
          [] OTHER -> "p3"]
=============================================================================
