-------------------------- MODULE SwapRevealNightly --------------------------
\* Nightly bound: 3 participants and 3 tracks.
EXTENDS Naturals, TLC
CONSTANTS Participants, Tracks, NoOne, OpenSchedule
VARIABLES eventState, hostAction, hostCommand, active, streamSafe, confirmed,
          planComplete, commitmentVerified, projection, presenter
INSTANCE SwapReveal WITH
    Owner <-
        [t \in Tracks |->
            CASE t = "t1" -> "p1"
              [] t = "t2" -> "p2"
              [] OTHER -> "p3"]
=============================================================================
