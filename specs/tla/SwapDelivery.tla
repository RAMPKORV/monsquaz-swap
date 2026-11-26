---------------------------- MODULE SwapDelivery ----------------------------
\* Design model for the durable delivery outbox.
\* It checks at-least-once safety: an enqueued effect is never lost, every
\* acknowledgement refers to a sent effect, and dedupe keys are unique.
\* Real time, Discord, SQLite, and the file store are outside the model.
EXTENDS Naturals, Integers, FiniteSets, TLC

CONSTANTS Effects, Keys, Key, MaxAttempts

ASSUME Effects # {}
ASSUME Keys # {}
ASSUME Key \in [Effects -> Keys]
ASSUME MaxAttempts \in Nat
ASSUME MaxAttempts > 0

VARIABLES enqueued, pending, sent, acked, attempts

vars == <<enqueued, pending, sent, acked, attempts>>

Init ==
    /\ enqueued = {}
    /\ pending = {}
    /\ sent = {}
    /\ acked = {}
    /\ attempts = [e \in Effects |-> 0]

Enqueue(e) ==
    /\ e \in Effects
    /\ e \notin enqueued
    /\ \A other \in enqueued : Key[other] /= Key[e]
    /\ enqueued' = enqueued \cup {e}
    /\ pending' = pending \cup {e}
    /\ UNCHANGED <<sent, acked, attempts>>

\* One send attempt. A duplicate attempt is allowed; the transport must dedupe
\* or the effect is acknowledged once.
Attempt(e) ==
    /\ e \in pending
    /\ attempts[e] < MaxAttempts
    /\ sent' = sent \cup {e}
    /\ attempts' = [attempts EXCEPT ![e] = attempts[e] + 1]
    /\ UNCHANGED <<enqueued, pending, acked>>

\* The recipient or transport confirms the effect. Ack is idempotent because
\* acked is a set.
Ack(e) ==
    /\ e \in sent
    /\ pending' = pending \ {e}
    /\ acked' = acked \cup {e}
    /\ UNCHANGED <<enqueued, sent, attempts>>

Next ==
    \/ \E e \in Effects : Enqueue(e)
    \/ \E e \in Effects : Attempt(e)
    \/ \E e \in Effects : Ack(e)

Spec == Init /\ [][Next]_vars /\ WF_vars(\E e \in Effects : Attempt(e)) /\ WF_vars(\E e \in Effects : Ack(e))

TypeOK ==
    /\ enqueued \subseteq Effects
    /\ pending \subseteq Effects
    /\ sent \subseteq Effects
    /\ acked \subseteq Effects
    /\ attempts \in [Effects -> Nat]

SentWereEnqueued == sent \subseteq enqueued

AckedWereSent == acked \subseteq sent

AckedWereEnqueued == acked \subseteq enqueued

NoLostEffect == enqueued = pending \cup acked

UniqueKeys ==
    \A left, right \in enqueued :
        Key[left] = Key[right] => left = right

AttemptsBounded ==
    \A e \in Effects : attempts[e] <= MaxAttempts

Inv ==
    /\ TypeOK
    /\ SentWereEnqueued
    /\ AckedWereSent
    /\ AckedWereEnqueued
    /\ NoLostEffect
    /\ UniqueKeys
    /\ AttemptsBounded
=============================================================================
