---------------------------- MODULE SwapClaim ----------------------------
\* Design model for the classic claim, offer, custody, and expiry rules.
\* It checks the core safety invariants for a small finite roster. It does
\* not model files, Discord, SQLite, the UI, or real time.
EXTENDS Naturals, Integers, FiniteSets, TLC

CONSTANTS Participants, Tracks, Owner, NoOne, ClaimWindow, MaxTime

ASSUME Participants # {}
ASSUME Tracks # {}
ASSUME NoOne \notin Participants
ASSUME Owner \in [Tracks -> Participants]
ASSUME ClaimWindow \in Nat
ASSUME MaxTime \in Nat

VARIABLES phase, seen, banned, holder, due, now

vars == <<phase, seen, banned, holder, due, now>>

Init ==
    /\ phase = [t \in Tracks |-> [p \in Participants |-> "idle"]]
    /\ seen = {}
    /\ banned = {}
    /\ holder = [t \in Tracks |-> NoOne]
    /\ due = [t \in Tracks |-> [p \in Participants |-> 0]]
    /\ now = 0

Offer(t, p) ==
    /\ phase[t][p] = "idle"
    /\ holder[t] = NoOne
    /\ p /= Owner[t]
    /\ \A other \in Participants : phase[t][other] /= "offered"
    /\ \A other \in Tracks : phase[other][p] /= "offered"
    /\ <<t, p>> \notin seen
    /\ <<t, p>> \notin banned
    /\ phase' = [phase EXCEPT ![t] = [phase[t] EXCEPT ![p] = "offered"]]
    /\ due' = [due EXCEPT ![t] = [due[t] EXCEPT ![p] = now + ClaimWindow]]
    /\ UNCHANGED <<seen, banned, holder, now>>

Claim(t, p) ==
    /\ phase[t][p] = "offered"
    /\ holder[t] = NoOne
    /\ now <= due[t][p]
    /\ \A other \in Tracks : holder[other] /= p
    /\ phase' = [phase EXCEPT ![t] = [phase[t] EXCEPT ![p] = "claimed"]]
    /\ seen' = seen \cup {<<t, p>>}
    /\ holder' = [holder EXCEPT ![t] = p]
    /\ UNCHANGED <<banned, due, now>>

Expire(t, p) ==
    /\ phase[t][p] = "offered"
    /\ now >= due[t][p]
    /\ phase' = [phase EXCEPT ![t] = [phase[t] EXCEPT ![p] = "idle"]]
    /\ banned' = banned \cup {<<t, p>>}
    /\ UNCHANGED <<seen, holder, due, now>>

SkipBefore(t, p) ==
    /\ phase[t][p] = "offered"
    /\ phase' = [phase EXCEPT ![t] = [phase[t] EXCEPT ![p] = "idle"]]
    /\ UNCHANGED <<seen, banned, holder, due, now>>

Submit(t, p) ==
    /\ phase[t][p] = "claimed"
    /\ phase' = [phase EXCEPT ![t] = [phase[t] EXCEPT ![p] = "done"]]
    /\ holder' = [holder EXCEPT ![t] = NoOne]
    /\ UNCHANGED <<seen, banned, due, now>>

SkipAfter(t, p) ==
    /\ phase[t][p] = "claimed"
    /\ phase' = [phase EXCEPT ![t] = [phase[t] EXCEPT ![p] = "done"]]
    /\ holder' = [holder EXCEPT ![t] = NoOne]
    /\ UNCHANGED <<seen, banned, due, now>>

Tick ==
    /\ now < MaxTime
    /\ now' = now + 1
    /\ UNCHANGED <<phase, seen, banned, holder, due>>

Next ==
    \/ \E t \in Tracks, p \in Participants : Offer(t, p)
    \/ \E t \in Tracks, p \in Participants : Claim(t, p)
    \/ \E t \in Tracks, p \in Participants : Expire(t, p)
    \/ \E t \in Tracks, p \in Participants : SkipBefore(t, p)
    \/ \E t \in Tracks, p \in Participants : Submit(t, p)
    \/ \E t \in Tracks, p \in Participants : SkipAfter(t, p)
    \/ Tick

Spec == Init /\ [][Next]_vars /\ WF_vars(Tick)

TypeOK ==
    /\ phase \in [Tracks -> [Participants -> {"idle", "offered", "claimed", "done"}]]
    /\ seen \subseteq Tracks \X Participants
    /\ banned \subseteq Tracks \X Participants
    /\ holder \in [Tracks -> Participants \cup {NoOne}]
    /\ due \in [Tracks -> [Participants -> Nat]]
    /\ now \in Nat

NoRepeat ==
    \A t \in Tracks, p \in Participants :
        <<t, p>> \in seen => phase[t][p] \in {"claimed", "done"}

NoOwnTrack ==
    \A t \in Tracks : holder[t] /= NoOne => holder[t] /= Owner[t]

OneTrackPerPerson ==
    \A p \in Participants :
        Cardinality({t \in Tracks : holder[t] = p}) <= 1

NoExpiredReoffer ==
    \A t \in Tracks, p \in Participants :
        <<t, p>> \in banned => phase[t][p] /= "offered"

NoOwnOffer ==
    \A t \in Tracks, p \in Participants :
        phase[t][p] = "offered" => p /= Owner[t]

AtMostOneOfferPerTrack ==
    \A t \in Tracks :
        Cardinality({p \in Participants : phase[t][p] = "offered"}) <= 1

AtMostOneOfferPerPerson ==
    \A p \in Participants :
        Cardinality({t \in Tracks : phase[t][p] = "offered"}) <= 1

Inv ==
    /\ TypeOK
    /\ NoRepeat
    /\ NoOwnTrack
    /\ OneTrackPerPerson
    /\ NoExpiredReoffer
    /\ NoOwnOffer
    /\ AtMostOneOfferPerTrack
    /\ AtMostOneOfferPerPerson
=============================================================================
