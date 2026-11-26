---------------------------- MODULE SwapDelivery ----------------------------
\* Design model for the durable delivery outbox.
\* It checks terminal safety: an enqueued effect is pending, acknowledged, or
\* failed with a host alert. Every acknowledgement refers to a sent effect, and
\* dedupe keys are unique.
\* Real time, Discord, SQLite, and the file store are outside the model.
EXTENDS Naturals, Integers, FiniteSets, TLC

CONSTANTS Effects, Keys, Key, MaxAttempts

ASSUME Effects # {}
ASSUME Keys # {}
ASSUME Key \in [Effects -> Keys]
ASSUME MaxAttempts \in Nat
ASSUME MaxAttempts > 0

VARIABLES enqueued, pending, sent, acked, failed, alerted, attempts

vars == <<enqueued, pending, sent, acked, failed, alerted, attempts>>

Init ==
    /\ enqueued = {}
    /\ pending = {}
    /\ sent = {}
    /\ acked = {}
    /\ failed = {}
    /\ alerted = {}
    /\ attempts = [e \in Effects |-> 0]

Enqueue(e) ==
    /\ e \in Effects
    /\ e \notin enqueued
    /\ \A other \in enqueued : Key[other] /= Key[e]
    /\ enqueued' = enqueued \cup {e}
    /\ pending' = pending \cup {e}
    /\ UNCHANGED <<sent, acked, failed, alerted, attempts>>

\* One send attempt. A duplicate attempt is allowed; the transport must dedupe
\* or the effect is acknowledged once.
Attempt(e) ==
    /\ e \in pending
    /\ attempts[e] < MaxAttempts
    /\ sent' = sent \cup {e}
    /\ attempts' = [attempts EXCEPT ![e] = attempts[e] + 1]
    /\ UNCHANGED <<enqueued, pending, acked, failed, alerted>>

\* The recipient or transport confirms the effect. Ack is idempotent because
\* acked is a set.
Ack(e) ==
    /\ e \in sent
    /\ e \in pending
    /\ pending' = pending \ {e}
    /\ acked' = acked \cup {e}
    /\ UNCHANGED <<enqueued, sent, failed, alerted, attempts>>

\* The retry budget is spent. The implementation marks the row failed and
\* queues a host alert. No effect disappears without a terminal record.
Fail(e) ==
    /\ e \in pending
    /\ attempts[e] = MaxAttempts
    /\ pending' = pending \ {e}
    /\ failed' = failed \cup {e}
    /\ alerted' = alerted \cup {e}
    /\ UNCHANGED <<enqueued, sent, acked, attempts>>

Next ==
    \/ \E e \in Effects : Enqueue(e)
    \/ \E e \in Effects : Attempt(e)
    \/ \E e \in Effects : Ack(e)
    \/ \E e \in Effects : Fail(e)

Spec == Init /\ [][Next]_vars /\ WF_vars(\E e \in Effects : Attempt(e)) /\ WF_vars(\E e \in Effects : Ack(e))

TypeOK ==
    /\ enqueued \subseteq Effects
    /\ pending \subseteq Effects
    /\ sent \subseteq Effects
    /\ acked \subseteq Effects
    /\ failed \subseteq Effects
    /\ alerted \subseteq Effects
    /\ attempts \in [Effects -> Nat]

SentWereEnqueued == sent \subseteq enqueued

AckedWereSent == acked \subseteq sent

AckedWereEnqueued == acked \subseteq enqueued

NoLostEffect == enqueued = pending \cup acked \cup failed

FailedWereEnqueued == failed \subseteq enqueued

FailedWereAlerted == failed \subseteq alerted

TerminalExclusive == acked \cap failed = {}

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
    /\ FailedWereEnqueued
    /\ FailedWereAlerted
    /\ TerminalExclusive
    /\ UniqueKeys
    /\ AttemptsBounded
=============================================================================
