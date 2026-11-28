---------------------------- MODULE SwapReveal ----------------------------
\* Design model for the reveal gate.
\* The host presses reveal, then the app publishes only when no assignment is
\* active. The publish is one atomic step. Before the gate, the participant
\* projection has no identity fact. The presenter starts in stream-safe mode,
\* and the host must confirm before names appear.
\* SQLite, Discord, files, and wall-clock time are outside the model.
EXTENDS Naturals, FiniteSets, TLC

CONSTANTS Participants, Tracks, Owner, NoOne

ASSUME Participants # {}
ASSUME Tracks # {}
ASSUME NoOne \notin Participants
ASSUME Owner \in [Tracks -> Participants]

EventStates == {"active", "paused", "revealed"}

OwnerFacts ==
    {[kind |-> "owner", track |-> t, who |-> Owner[t]] : t \in Tracks}

ContributorFacts ==
    {[kind |-> "contributor", track |-> t, who |-> p] :
        t \in Tracks, p \in Participants}

TitleFacts ==
    {[kind |-> "title", track |-> t, who |-> NoOne] : t \in Tracks}

CommentFacts ==
    {[kind |-> "comments", track |-> t, who |-> p] :
        t \in Tracks, p \in Participants}

MappingFacts ==
    {[kind |-> "mapping", track |-> t, who |-> p] :
        t \in Tracks, p \in Participants}

IdentityFacts ==
    OwnerFacts \cup ContributorFacts \cup TitleFacts
        \cup CommentFacts \cup MappingFacts

SafeProjection == {}
FullProjection == IdentityFacts

VARIABLES eventState, hostAction, active, streamSafe, confirmed,
          projection, presenter

vars == <<eventState, hostAction, active, streamSafe, confirmed,
          projection, presenter>>

Init ==
    /\ eventState = "active"
    /\ hostAction = FALSE
    /\ active = {}
    /\ streamSafe = TRUE
    /\ confirmed = FALSE
    /\ projection = SafeProjection
    /\ presenter = SafeProjection

Pause ==
    /\ eventState = "active"
    /\ eventState' = "paused"
    /\ UNCHANGED <<hostAction, active, streamSafe, confirmed,
                   projection, presenter>>

Resume ==
    /\ eventState = "paused"
    /\ eventState' = "active"
    /\ UNCHANGED <<hostAction, active, streamSafe, confirmed,
                   projection, presenter>>

Claim(t, p) ==
    /\ eventState = "active"
    /\ <<t, p>> \notin active
    /\ active' = active \cup {<<t, p>>}
    /\ UNCHANGED <<eventState, hostAction, streamSafe, confirmed,
                   projection, presenter>>

Resolve(t, p) ==
    /\ eventState \in {"active", "paused"}
    /\ <<t, p>> \in active
    /\ active' = active \ {<<t, p>>}
    /\ UNCHANGED <<eventState, hostAction, streamSafe, confirmed,
                   projection, presenter>>

\* The host action is a precondition of the publish. It is not the publish.
HostPress ==
    /\ eventState /= "revealed"
    /\ ~hostAction
    /\ hostAction' = TRUE
    /\ UNCHANGED <<eventState, active, streamSafe, confirmed,
                   projection, presenter>>

\* One atomic publish. It checks the host action and the active assignments.
Publish ==
    /\ eventState \in {"active", "paused"}
    /\ hostAction
    /\ active = {}
    /\ eventState' = "revealed"
    /\ projection' = FullProjection
    /\ presenter' = SafeProjection
    /\ UNCHANGED <<hostAction, active, streamSafe, confirmed>>

ConfirmStreamSafeOff ==
    /\ eventState = "revealed"
    /\ streamSafe
    /\ ~confirmed
    /\ confirmed' = TRUE
    /\ UNCHANGED <<eventState, hostAction, active, streamSafe,
                   projection, presenter>>

DisableStreamSafe ==
    /\ eventState = "revealed"
    /\ streamSafe
    /\ confirmed
    /\ streamSafe' = FALSE
    /\ presenter' = FullProjection
    /\ UNCHANGED <<eventState, hostAction, active, confirmed, projection>>

EnableStreamSafe ==
    /\ eventState = "revealed"
    /\ ~streamSafe
    /\ streamSafe' = TRUE
    /\ confirmed' = FALSE
    /\ presenter' = SafeProjection
    /\ UNCHANGED <<eventState, hostAction, active, projection>>

Next ==
    \/ Pause
    \/ Resume
    \/ \E t \in Tracks, p \in Participants : Claim(t, p)
    \/ \E t \in Tracks, p \in Participants : Resolve(t, p)
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

NoIdentityBeforeReveal ==
    (eventState /= "revealed") =>
        /\ {f \in projection : f.kind = "owner"} = {}
        /\ {f \in projection : f.kind = "contributor"} = {}
        /\ {f \in projection : f.kind = "title"} = {}
        /\ {f \in projection : f.kind = "comments"} = {}
        /\ {f \in projection : f.kind = "mapping"} = {}

StreamSafeDefault ==
    /\ (eventState /= "revealed" => streamSafe)
    /\ (streamSafe => presenter \cap IdentityFacts = {})
    /\ (~streamSafe => confirmed)

TypeOK ==
    /\ eventState \in EventStates
    /\ hostAction \in BOOLEAN
    /\ active \subseteq Tracks \X Participants
    /\ streamSafe \in BOOLEAN
    /\ confirmed \in BOOLEAN
    /\ projection \subseteq IdentityFacts
    /\ presenter \subseteq IdentityFacts

Inv ==
    /\ TypeOK
    /\ NoRevealBeforeHostAction
    /\ NoRevealWithActiveAssignments
    /\ NoIdentityBeforeReveal
    /\ StreamSafeDefault
=============================================================================
