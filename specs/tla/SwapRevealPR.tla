---------------------------- MODULE SwapRevealPR -----------------------------
\* PR bound: 2 participants and 2 tracks.
EXTENDS Naturals, TLC
CONSTANTS Participants, Tracks, NoOne, OpenSchedule
VARIABLES eventState, hostAction, hostCommand, active, streamSafe, confirmed,
          planComplete, commitmentVerified, projection, presenter
INSTANCE SwapReveal WITH
    Owner <-
        [t \in Tracks |->
            CASE t = "t1" -> "p1"
              [] OTHER -> "p2"]
=============================================================================
