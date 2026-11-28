---------------------------- MODULE SwapSchedule ----------------------------
\* Design model for the classic schedule across rounds.
\* Round 1 is the seed round. Rounds 2..RoundCount are exchange rounds.
\* Each exchange round is a complete matching: every track has one participant
\* and every participant has one track. A pair never repeats and a participant
\* never receives the track they seeded.
\* When groups are enabled, a participant receives a track from the group that
\* is one phase step forward. A fallback phase can use the global pool.
\* The model does not describe SQLite, Discord, files, away ranges, fill-ins,
\* or wall-clock time.
EXTENDS Naturals, FiniteSets, TLC

CONSTANTS Participants, Tracks, Owner, NoOne,
          GroupCount, GroupOf, RoundCount, Cadence, FallbackPhases

ASSUME Participants # {}
ASSUME Tracks # {}
ASSUME NoOne \notin Participants
ASSUME Owner \in [Tracks -> Participants]
ASSUME Cardinality(Tracks) = Cardinality(Participants)
ASSUME \A t1, t2 \in Tracks : Owner[t1] = Owner[t2] => t1 = t2
ASSUME \A p \in Participants : \E t \in Tracks : Owner[t] = p
ASSUME GroupCount \in Nat
ASSUME GroupCount >= 1
ASSUME GroupOf \in [Participants -> (1 .. GroupCount)]
ASSUME \A g \in 1 .. GroupCount : \E p \in Participants : GroupOf[p] = g
ASSUME \A g \in 1 .. GroupCount :
    Cardinality({p \in Participants : GroupOf[p] = g}) =
    Cardinality({t \in Tracks : GroupOf[Owner[t]] = g})
ASSUME \A g, h \in 1 .. GroupCount :
    Cardinality({p \in Participants : GroupOf[p] = g}) =
    Cardinality({p \in Participants : GroupOf[p] = h})
ASSUME RoundCount \in Nat
ASSUME RoundCount >= 2
ASSUME RoundCount <= Cardinality(Participants)
ASSUME Cadence \in Nat
ASSUME Cadence >= 1
ASSUME FallbackPhases \subseteq Nat

ExchangeRounds == 2 .. RoundCount

\* Phase index of an exchange round. Phase 0 is the first exchange round.
PhaseOf(r) == (r - 2) \div Cadence

\* Move a group index forward by a number of phases.
Rotated(group, phase) == ((group - 1 + phase) % GroupCount) + 1

TrackGroup(t) == GroupOf[Owner[t]]

\* The group that a participant visits in a round. Tracks stay in place.
TargetGroup(p, r) == Rotated(GroupOf[p], PhaseOf(r))

VARIABLES plan, nextRound, sealed

vars == <<plan, nextRound, sealed>>

Init ==
    /\ plan = [r \in 1 .. RoundCount |->
         IF r = 1
         THEN [t \in Tracks |-> Owner[t]]
         ELSE [t \in Tracks |-> NoOne]]
    /\ nextRound = 2
    /\ sealed = FALSE

Planned(r) == \A t \in Tracks : plan[r][t] /= NoOne

\* Add one complete exchange round. The matching is a bijection from tracks to
\* participants. The repeat guard looks at every earlier exchange round.
AssignRound(f) ==
    /\ nextRound <= RoundCount
    /\ f \in [Tracks -> Participants]
    /\ \A p \in Participants : Cardinality({t \in Tracks : f[t] = p}) = 1
    /\ \A t \in Tracks : f[t] /= Owner[t]
    /\ \A r \in (2 .. (nextRound - 1)), t \in Tracks : plan[r][t] /= f[t]
    /\ ((\A t \in Tracks : TrackGroup(t) = TargetGroup(f[t], nextRound))
        \/ PhaseOf(nextRound) \in FallbackPhases)
    /\ plan' = [plan EXCEPT ![nextRound] = f]
    /\ nextRound' = nextRound + 1
    /\ UNCHANGED sealed

Seal ==
    /\ nextRound = RoundCount + 1
    /\ ~sealed
    /\ sealed' = TRUE
    /\ UNCHANGED <<plan, nextRound>>

Next ==
    \/ \E f \in [Tracks -> Participants] : AssignRound(f)
    \/ Seal

Spec == Init /\ [][Next]_vars

\* The seed round covers every track, and every planned exchange round covers
\* every participant exactly once. Every track in a planned round has a holder.
CompleteRounds ==
    /\ \A t \in Tracks : plan[1][t] = Owner[t]
    /\ \A r \in ExchangeRounds :
         Planned(r) =>
           \A p \in Participants :
             Cardinality({t \in Tracks : plan[r][t] = p}) = 1

NoRepeat ==
    \A t \in Tracks, p \in Participants :
        Cardinality({r \in ExchangeRounds : plan[r][t] = p}) <= 1

NoOwnTrack ==
    \A r \in ExchangeRounds, t \in Tracks :
        plan[r][t] /= NoOne => plan[r][t] /= Owner[t]

\* Rotation: a participant receives a track from the group one phase step
\* forward. A fallback phase can use the global pool.
GroupContainment ==
    \A r \in ExchangeRounds, t \in Tracks :
        (plan[r][t] /= NoOne /\ PhaseOf(r) \notin FallbackPhases) =>
            TrackGroup(t) = TargetGroup(plan[r][t], r)

\* A locked plan has every exchange round.
SealedPlanComplete ==
    sealed => \A r \in ExchangeRounds : Planned(r)

TypeOK ==
    /\ plan \in [1 .. RoundCount -> [Tracks -> Participants \cup {NoOne}]]
    /\ nextRound \in 2 .. (RoundCount + 1)
    /\ sealed \in BOOLEAN

Inv ==
    /\ TypeOK
    /\ CompleteRounds
    /\ NoRepeat
    /\ NoOwnTrack
    /\ GroupContainment
    /\ SealedPlanComplete
=============================================================================
