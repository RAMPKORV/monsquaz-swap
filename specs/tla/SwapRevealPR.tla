---------------------------- MODULE SwapRevealPR -----------------------------
\* PR bound: 2 participants and 2 tracks.
EXTENDS Naturals, TLC
CONSTANTS Participants, Tracks, NoOne
VARIABLES eventState, hostAction, active, streamSafe, confirmed,
          projection, presenter
INSTANCE SwapReveal WITH
    Owner <-
        [t \in Tracks |->
            CASE t = "t1" -> "p1"
              [] OTHER -> "p2"]
=============================================================================
