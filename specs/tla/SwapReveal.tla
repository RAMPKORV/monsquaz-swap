---------------------------- MODULE SwapReveal ----------------------------
\* Design model for the reveal gate.
\* The host command is one-shot. The app publishes only when no assignment is
\* active, the plan is complete, and the commitment is verified.
\* Before the gate, the participant projection never carries a seed owner,
\* title, or comment. An open schedule can show the editor assignment.
\* The presenter starts in stream-safe mode. The host must confirm before names
\* appear, and stream-safe can turn off before or after the publish.
\* SQLite, Discord, files, and wall-clock time are outside the model.
EXTENDS Naturals, FiniteSets, TLC

CONSTANTS Participants, Tracks, Owner, NoOne, OpenSchedule

ASSUME Participants # {}
ASSUME Tracks # {}
ASSUME NoOne \notin Participants
ASSUME Owner \in [Tracks -> Participants]
ASSUME OpenSchedule \in BOOLEAN

EventStates == {"active", "paused", "revealed"}

OwnerFacts ==
    {[kind |-> "owner", track |-> t, who |-> Owner[t]] : t \in Tracks}

TitleFacts ==
    {[kind |-> "title", track |-> t, who |-> NoOne] : t \in Tracks}

CommentFacts ==
    {[kind |-> "comments", track |-> t, who |-> p] :
        t \in Tracks, p \in Participants}

\* The editor assignment is allowed in an open schedule before the reveal.
AssignmentFacts ==
    {[kind |-> "assignment", track |-> t, who |-> p] :
        t \in Tracks, p \in Participants}

\* These facts are hidden from the participant projection in every mode.
OwnerTitleCommentFacts == OwnerFacts \cup TitleFacts \cup CommentFacts

AllFacts == OwnerTitleCommentFacts \cup AssignmentFacts

SafeProjection == IF OpenSchedule THEN AssignmentFacts ELSE {}

FullProjection == AllFacts

VARIABLES eventState, hostAction, hostCommand, active, streamSafe, confirmed,
          planComplete, commitmentVerified, projection, presenter

vars == <<eventState, hostAction, hostCommand, active, streamSafe, confirmed,
          planComplete, commitmentVerified, projection, presenter>>

Init ==
    /\ eventState = "active"
    /\ hostAction = FALSE
    /\ hostCommand = FALSE
    /\ active = {}
    /\ streamSafe = TRUE
    /\ confirmed = FALSE
    /\ planComplete = FALSE
    /\ commitmentVerified = FALSE
    /\ projection = SafeProjection
    /\ presenter = {}

Pause ==
    /\ eventState = "active"
    /\ eventState' = "paused"
    /\ UNCHANGED <<hostAction, hostCommand, active, streamSafe, confirmed,
                   planComplete, commitmentVerified, projection, presenter>>

Resume ==
    /\ eventState = "paused"
    /\ eventState' = "active"
    /\ UNCHANGED <<hostAction, hostCommand, active, streamSafe, confirmed,
                   planComplete, commitmentVerified, projection, presenter>>

Claim(t, p) ==
    /\ eventState = "active"
    /\ <<t, p>> \notin active
    /\ active' = active \cup {<<t, p>>}
    /\ UNCHANGED <<eventState, hostAction, hostCommand, streamSafe, confirmed,
                   planComplete, commitmentVerified, projection, presenter>>

Resolve(t, p) ==
    /\ eventState \in {"active", "paused"}
    /\ <<t, p>> \in active
    /\ active' = active \ {<<t, p>>}
    /\ UNCHANGED <<eventState, hostAction, hostCommand, streamSafe, confirmed,
                   planComplete, commitmentVerified, projection, presenter>>

\* The plan must have every round before the reveal.
CompletePlan ==
    /\ eventState \in {"active", "paused"}
    /\ ~planComplete
    /\ planComplete' = TRUE
    /\ UNCHANGED <<eventState, hostAction, hostCommand, active, streamSafe,
                   confirmed, commitmentVerified, projection, presenter>>

\* The reveal verifies the locked plan commitment.
VerifyCommitment ==
    /\ eventState \in {"active", "paused"}
    /\ ~commitmentVerified
    /\ commitmentVerified' = TRUE
    /\ UNCHANGED <<eventState, hostAction, hostCommand, active, streamSafe,
                   confirmed, planComplete, projection, presenter>>

\* The host action is a one-shot command. It is a precondition of the publish.
HostPress ==
    /\ eventState /= "revealed"
    /\ ~hostAction
    /\ hostAction' = TRUE
    /\ hostCommand' = TRUE
    /\ UNCHANGED <<eventState, active, streamSafe, confirmed, planComplete,
                   commitmentVerified, projection, presenter>>

\* One atomic publish. It checks the host command, the active assignments, the
\* plan, and the commitment. The command token is consumed.
Publish ==
    /\ eventState \in {"active", "paused"}
    /\ hostCommand
    /\ active = {}
    /\ planComplete
    /\ commitmentVerified
    /\ eventState' = "revealed"
    /\ hostCommand' = FALSE
    /\ projection' = FullProjection
    /\ UNCHANGED <<hostAction, active, streamSafe, confirmed, planComplete,
                   commitmentVerified, presenter>>

\* Stream-safe can turn off before or after the publish, after a confirmation.
ConfirmStreamSafeOff ==
    /\ streamSafe
    /\ ~confirmed
    /\ confirmed' = TRUE
    /\ UNCHANGED <<eventState, hostAction, hostCommand, active, streamSafe,
                   planComplete, commitmentVerified, projection, presenter>>

DisableStreamSafe ==
    /\ streamSafe
    /\ confirmed
    /\ streamSafe' = FALSE
    /\ presenter' = FullProjection
    /\ UNCHANGED <<eventState, hostAction, hostCommand, active, confirmed,
                   planComplete, commitmentVerified, projection>>

EnableStreamSafe ==
    /\ ~streamSafe
    /\ streamSafe' = TRUE
    /\ confirmed' = FALSE
    /\ presenter' = {}
    /\ UNCHANGED <<eventState, hostAction, hostCommand, active,
                   planComplete, commitmentVerified, projection>>

Next ==
    \/ Pause
    \/ Resume
    \/ \E t \in Tracks, p \in Participants : Claim(t, p)
    \/ \E t \in Tracks, p \in Participants : Resolve(t, p)
    \/ CompletePlan
    \/ VerifyCommitment
    \/ HostPress
    \/ Publish
    \/ ConfirmStreamSafeOff
    \/ DisableStreamSafe
    \/ EnableStreamSafe

Spec == Init /\ [][Next]_vars

NoRevealBeforeHostAction ==
    (eventState = "revealed") => hostAction

NoRevealWithActiveAssignments ==
    (eventState = "revealed") => active = {}

\* D-045: the open schedule can show editor assignments. It never shows seed
\* owners, titles, or comments before the reveal. The hidden schedule shows no
\* assignment facts before the reveal.
NoIdentityBeforeReveal ==
    (eventState /= "revealed") =>
        /\ {f \in projection : f.kind = "owner"} = {}
        /\ {f \in projection : f.kind = "title"} = {}
        /\ {f \in projection : f.kind = "comments"} = {}
        /\ (~OpenSchedule =>
              {f \in projection : f.kind = "assignment"} = {})

\* Stream-safe is on at Init. It hides every identity and assignment fact while
\* it is on. It can turn off only after the host confirmation.
StreamSafeDefault ==
    /\ (streamSafe => presenter \cap AllFacts = {})
    /\ (~streamSafe => confirmed)

PublishedImpliesPlanComplete ==
    (eventState = "revealed") => planComplete

PublishedImpliesCommitmentVerified ==
    (eventState = "revealed") => commitmentVerified

TypeOK ==
    /\ eventState \in EventStates
    /\ hostAction \in BOOLEAN
    /\ hostCommand \in BOOLEAN
    /\ active \subseteq Tracks \X Participants
    /\ streamSafe \in BOOLEAN
    /\ confirmed \in BOOLEAN
    /\ planComplete \in BOOLEAN
    /\ commitmentVerified \in BOOLEAN
    /\ projection \subseteq AllFacts
    /\ presenter \subseteq AllFacts

Inv ==
    /\ TypeOK
    /\ NoRevealBeforeHostAction
    /\ NoRevealWithActiveAssignments
    /\ NoIdentityBeforeReveal
    /\ StreamSafeDefault
    /\ PublishedImpliesPlanComplete
    /\ PublishedImpliesCommitmentVerified
=============================================================================
