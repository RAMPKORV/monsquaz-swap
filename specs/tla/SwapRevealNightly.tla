-------------------------- MODULE SwapRevealNightly --------------------------
\* Nightly bound: 3 participants and 3 tracks.
EXTENDS Naturals, TLC
CONSTANTS Participants, Tracks, NoOne
VARIABLES eventState, hostAction, active, streamSafe, confirmed,
          projection, presenter
INSTANCE SwapReveal WITH
    Owner <-
        [t \in Tracks |->
            CASE t = "t1" -> "p1"
              [] t = "t2" -> "p2"
              [] OTHER -> "p3"]
=============================================================================
