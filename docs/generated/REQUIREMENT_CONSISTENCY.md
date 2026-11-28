# Requirement consistency

> <!-- GENERATED FILE - run `scripts/requirements-check.sh`; do not edit by hand. -->

Status: **PASS**

This report is the output of the finite requirement model in
`specs/requirements/`. The model formalizes the normative fill-in,
leave, handoff, and scheduling rules. The natural-language source wins.
A stale model is a defect.

## Model

- Solver: `z3 5.3.2`
- Rules: **36** must rules.
- Formalized scenarios: **18**.
- Rule pairs checked: **630**.
- Participants: **3**, tracks: **3**, rounds: **3**, turns: **6**, invitations: **2**.

## Rules

| ID | Source | Rule (ASD-STE100) | Scenarios |
|---|---|---|---|
| `R-CLAIM-WINDOW-REGULAR` | [D-025](../../ai-rules/DECISIONS.md), [REQ-DELIVERY-002](../../docs/PRD.md), [Claim window](../../docs/EVENT_SETTINGS.md), [A claim delivers the file and starts the deadline](../../specs/features/async_queue.feature) | A regular offer must expire after the 24-hour claim window. | `S-CLAIM-24` |
| `R-CLAIM-WINDOW-FILLIN` | [D-069](../../ai-rules/DECISIONS.md), [REQ-FILL-003](../../docs/PRD.md), [Fill-in offer claim window](../../docs/EVENT_SETTINGS.md), [A fill-in offer uses the standard sequence](../../specs/features/fill_ins.feature) | A fill-in offer must expire after the 6-hour claim window. | `S-FILL-SEQUENCE` |
| `R-CLAIM-REMINDER` | [D-041](../../ai-rules/DECISIONS.md), [Claim reminder](../../docs/EVENT_SETTINGS.md), [The bot reminds the participant to claim](../../specs/features/async_queue.feature) | The claim reminder must occur before the claim window ends. | `S-CLAIM-REMINDER` |
| `R-EDIT-WINDOW` | [D-033](../../ai-rules/DECISIONS.md), [D-069](../../ai-rules/DECISIONS.md), [Async handoff deadline](../../docs/EVENT_SETTINGS.md), [A claimed fill-in offer starts the 24-hour edit time](../../specs/features/fill_ins.feature) | The default edit window must be 24 hours. | `S-FILL-SEQUENCE`, `S-FILL-DUE-CLAIM`, `S-FILL-DUE-APPOINT`, `S-CLAIM-24` |
| `R-FILL-AUTO-CHAIN` | [D-014](../../ai-rules/DECISIONS.md), [D-064](../../ai-rules/DECISIONS.md), [D-069](../../ai-rules/DECISIONS.md), [REQ-FILL-003](../../docs/PRD.md), [Fill-in flow](../../docs/EVENT_SETTINGS.md), [A fill-in offer uses the standard sequence](../../specs/features/fill_ins.feature) | An auto-chain must make at most three offers before the host decides. | `S-FILL-SEQUENCE` |
| `R-FILL-HOST-LIST` | [D-070](../../ai-rules/DECISIONS.md), [REQ-FILL-003](../../docs/PRD.md), [Fill-in flow](../../docs/EVENT_SETTINGS.md), [A fill-in offer uses the standard sequence](../../specs/features/fill_ins.feature) | A host-ordered fill-in list must have no size limit. | `S-FILL-SEQUENCE` |
| `R-FILL-SEQUENTIAL` | [D-014](../../ai-rules/DECISIONS.md), [REQ-FILL-003](../../docs/PRD.md), [A fill-in offer uses the standard sequence](../../specs/features/fill_ins.feature) | The bot must offer a fill-in slot to one person at a time. | `S-FILL-SEQUENCE` |
| `R-FILL-FIRST-ACCEPT` | [D-014](../../ai-rules/DECISIONS.md), [REQ-FILL-003](../../docs/PRD.md), [A fill-in offer uses the standard sequence](../../specs/features/fill_ins.feature) | The first accept must win the fill-in offer. | `S-FILL-SEQUENCE` |
| `R-FILL-DUE-APPOINT` | [D-040](../../ai-rules/DECISIONS.md), [D-069](../../ai-rules/DECISIONS.md), [Fill-in due time](../../docs/EVENT_SETTINGS.md), [A host-appointed fill-in gets the later due time](../../specs/features/fill_ins.feature) | A host-appointed fill-in must have a due time at the later of two times. The two times are the round deadline and 24 hours after the appointment. | `S-FILL-DUE-APPOINT` |
| `R-FILL-DUE-CLAIM` | [D-069](../../ai-rules/DECISIONS.md), [REQ-FILL-003](../../docs/PRD.md), [Fill-in offer claim window](../../docs/EVENT_SETTINGS.md), [A claimed fill-in offer starts the 24-hour edit time](../../specs/features/fill_ins.feature) | A claimed fill-in offer must start the 24-hour edit time at the claim. | `S-FILL-SEQUENCE`, `S-FILL-DUE-CLAIM` |
| `R-FILL-ELIGIBLE-NEVER-SEEN` | [D-007](../../ai-rules/DECISIONS.md), [D-050](../../ai-rules/DECISIONS.md), [REQ-FILL-002](../../docs/PRD.md), [A fill-in has never seen the track](../../specs/features/fill_ins.feature) | The app must not offer a fill-in slot to a person who has seen the track. | `S-FILL-NEVER-SEEN` |
| `R-FILL-ELIGIBLE-NOT-OWNER` | [D-007](../../ai-rules/DECISIONS.md), [D-050](../../ai-rules/DECISIONS.md), [REQ-FILL-002](../../docs/PRD.md), [A fill-in has never seen the track](../../specs/features/fill_ins.feature) | The app must not offer a fill-in slot to the track owner. | `S-FILL-NEVER-SEEN` |
| `R-TIER1-ELIGIBLE` | [D-050](../../ai-rules/DECISIONS.md), [REQ-FILL-002](../../docs/PRD.md), [Fill-in flow](../../docs/EVENT_SETTINGS.md), [The fill-in pool has three sources](../../specs/features/fill_ins.feature) | A tier-1 fill-in must have the fill-in role, be active, and be ready. A tier-1 fill-in must not have seen the track or own the track. | `S-FILL-NEVER-SEEN`, `S-FILL-POOL` |
| `R-TIER2-ELIGIBLE` | [D-050](../../ai-rules/DECISIONS.md), [REQ-FILL-002](../../docs/PRD.md), [The fill-in pool has three sources](../../specs/features/fill_ins.feature) | A tier-2 fill-in must be an active seeded participant who submitted their own track in the round. The plan must leave a gap for that track. | `S-FILL-POOL` |
| `R-TIER1-BEFORE-TIER2` | [D-050](../../ai-rules/DECISIONS.md), [REQ-FILL-002](../../docs/PRD.md), [The fill-in pool has three sources](../../specs/features/fill_ins.feature) | A tier-2 offer must not go to a person who is eligible as tier 1. | `S-FILL-POOL` |
| `R-TIER2-ONLY-NO-TIER1` | [D-050](../../ai-rules/DECISIONS.md), [REQ-FILL-002](../../docs/PRD.md), [The fill-in pool has three sources](../../specs/features/fill_ins.feature) | A tier-2 offer must not occur while a tier-1 person is eligible for the track. | `S-FILL-POOL` |
| `R-TIER2-ONE-PER-ROUND` | [D-063](../../ai-rules/DECISIONS.md), [REQ-FILL-003](../../docs/PRD.md), [A tier-2 fill-in rescue is an extra turn](../../specs/features/fill_ins.feature) | A person must take at most one tier-2 fill-in turn in one round. | `S-TIER2-EXTRA-TURN` |
| `R-TIER2-NO-EVENT-CAP` | [D-063](../../ai-rules/DECISIONS.md), [A tier-2 fill-in rescue is an extra turn](../../specs/features/fill_ins.feature) | The event must not cap the total number of tier-2 fill-in turns. | `S-TIER2-EXTRA-TURN` |
| `R-BUDGET-REGULAR` | [D-014](../../ai-rules/DECISIONS.md), [D-034](../../ai-rules/DECISIONS.md), [REQ-FILL-004](../../docs/PRD.md), [A tier-2 fill-in rescue is an extra turn](../../specs/features/fill_ins.feature) | A planned edit must count toward the shared handoff budget. | `S-TIER2-EXTRA-TURN` |
| `R-BUDGET-TIER2-EXTRA` | [D-050](../../ai-rules/DECISIONS.md), [REQ-FILL-004](../../docs/PRD.md), [A tier-2 fill-in rescue is an extra turn](../../specs/features/fill_ins.feature) | A tier-2 rescue must be an extra turn outside the shared handoff budget. | `S-TIER2-EXTRA-TURN` |
| `R-NO-REPEAT` | [D-007](../../ai-rules/DECISIONS.md), [REQ-SCHED-001](../../docs/PRD.md), [Every participant receives one unseen track per round](../../specs/features/scheduling.feature) | A participant must never work on the same track twice. | `S-SCHEDULE-ROUND` |
| `R-NO-OWN-TRACK` | [D-007](../../ai-rules/DECISIONS.md), [REQ-SCHED-001](../../docs/PRD.md), [Every participant receives one unseen track per round](../../specs/features/scheduling.feature) | The app must never assign a participant their own track. | `S-SCHEDULE-ROUND` |
| `R-ONE-ASSIGNMENT-PER-PERSON-ROUND` | [D-008](../../ai-rules/DECISIONS.md), [REQ-SCHED-003](../../docs/PRD.md), [Every participant receives one unseen track per round](../../specs/features/scheduling.feature) | A round must assign at most one track to each participant. | `S-SCHEDULE-ROUND` |
| `R-ONE-ASSIGNMENT-PER-TRACK-ROUND` | [D-008](../../ai-rules/DECISIONS.md), [REQ-SCHED-003](../../docs/PRD.md), [Every participant receives one unseen track per round](../../specs/features/scheduling.feature) | A round must assign a track to at most one participant. | `S-SCHEDULE-ROUND` |
| `R-ROUNDS-HOST-OPENED` | [D-031](../../ai-rules/DECISIONS.md), [D-010](../../ai-rules/DECISIONS.md), [REQ-DEADLINE-001](../../docs/PRD.md), [A default classic event runs N total rounds](../../specs/features/scheduling.feature) | The host must open every round. A round must not open automatically. | `S-CLASSIC-LENGTH`, `S-SOFT-DEADLINE` |
| `R-NO-AUTO-ADVANCE` | [D-010](../../ai-rules/DECISIONS.md), [D-031](../../ai-rules/DECISIONS.md), [REQ-ASYNC-008](../../docs/PRD.md), [A soft deadline does not move a track](../../specs/features/deadlines_and_extensions.feature) | A passed deadline must not advance a round without a host action. | `S-SOFT-DEADLINE` |
| `R-ONE-OPEN-ROUND` | [D-031](../../ai-rules/DECISIONS.md), [The host opens at most one round at a time](../../specs/features/deadlines_and_extensions.feature) | The host must open at most one round at a time. | `S-ONE-OPEN-ROUND` |
| `R-REVEAL-AFTER-HOST` | [D-017](../../ai-rules/DECISIONS.md), [REQ-REVEAL-001](../../docs/PRD.md), [Reveal trigger](../../docs/EVENT_SETTINGS.md), [The host triggers the reveal](../../specs/features/reveal.feature) | The host must trigger the reveal. | `S-REVEAL-HOST` |
| `R-REVEAL-NO-ACTIVE` | [D-017](../../ai-rules/DECISIONS.md), [D-031](../../ai-rules/DECISIONS.md), [REQ-REVEAL-001](../../docs/PRD.md), [The reveal is refused while work is pending](../../specs/features/reveal.feature) | The app must not publish the reveal while an assignment is active. | `S-REVEAL-HOST`, `S-REVEAL-PENDING` |
| `R-LEAVE-SEEN` | [D-051](../../ai-rules/DECISIONS.md), [REQ-JOIN-005](../../docs/PRD.md), [A participant leaves while holding a track](../../specs/features/join_and_seed.feature) | A leave after a claim must mark the held track as seen. | `S-LEAVE-HOLDING` |
| `R-LEAVE-NO-FUTURE` | [D-051](../../ai-rules/DECISIONS.md), [REQ-JOIN-005](../../docs/PRD.md), [A participant leaves while holding a track](../../specs/features/join_and_seed.feature) | A leave must remove the future assignments of the person. | `S-LEAVE-HOLDING` |
| `R-RESTORE-FILLIN-ONLY` | [D-051](../../ai-rules/DECISIONS.md), [REQ-JOIN-006](../../docs/PRD.md), [A leaver can return as a fill-in](../../specs/features/join_and_seed.feature) | A leaver must return as a fill-in only. | `S-LEAVER-RETURN` |
| `R-RESTORE-EXPIRY` | [D-068](../../ai-rules/DECISIONS.md), [A restore invitation expires after 7 days](../../specs/features/join_and_seed.feature) | A restore invitation must stop working 7 days after the bot sends it. | `S-LEAVER-RETURN`, `S-RESTORE-EXPIRY` |
| `R-RESTORE-EXPIRY-SETTING` | [D-068](../../ai-rules/DECISIONS.md), [A restore invitation expires after 7 days](../../specs/features/join_and_seed.feature) | The restore invitation must expire after 168 hours. | `S-LEAVER-RETURN`, `S-RESTORE-EXPIRY` |
| `R-RESTORE-EXPIRED-BLOCKS` | [D-068](../../ai-rules/DECISIONS.md), [A restore invitation expires after 7 days](../../specs/features/join_and_seed.feature) | An expired restore invitation must not accept a Confirm or Decline action. | `S-RESTORE-EXPIRY` |
| `R-FILL-PASS-UNCHANGED` | [D-052](../../ai-rules/DECISIONS.md), [REQ-FILL-005](../../docs/PRD.md), [No eligible fill-in exists](../../specs/features/fill_ins.feature) | When no eligible fill-in exists, classic must pass the track unchanged. Async must return the track to the pool. | `S-FILL-NO-ELIGIBLE` |

## Scenario results

| ID | Scenario | Feature | Result | Rules |
|---|---|---|---|---|
| `S-FILL-SEQUENCE` | A fill-in offer uses the standard sequence | [`specs/features/fill_ins.feature`](../../specs/features/fill_ins.feature) | PASS | `R-CLAIM-WINDOW-FILLIN`, `R-EDIT-WINDOW`, `R-FILL-AUTO-CHAIN`, `R-FILL-HOST-LIST`, `R-FILL-SEQUENTIAL`, `R-FILL-FIRST-ACCEPT`, `R-FILL-DUE-CLAIM` |
| `S-TIER2-EXTRA-TURN` | A tier-2 fill-in rescue is an extra turn | [`specs/features/fill_ins.feature`](../../specs/features/fill_ins.feature) | PASS | `R-TIER2-ONE-PER-ROUND`, `R-TIER2-NO-EVENT-CAP`, `R-BUDGET-REGULAR`, `R-BUDGET-TIER2-EXTRA` |
| `S-FILL-NEVER-SEEN` | A fill-in has never seen the track | [`specs/features/fill_ins.feature`](../../specs/features/fill_ins.feature) | PASS | `R-FILL-ELIGIBLE-NEVER-SEEN`, `R-FILL-ELIGIBLE-NOT-OWNER`, `R-TIER1-ELIGIBLE` |
| `S-FILL-POOL` | The fill-in pool has three sources | [`specs/features/fill_ins.feature`](../../specs/features/fill_ins.feature) | PASS | `R-TIER1-ELIGIBLE`, `R-TIER2-ELIGIBLE`, `R-TIER1-BEFORE-TIER2`, `R-TIER2-ONLY-NO-TIER1` |
| `S-FILL-NO-ELIGIBLE` | No eligible fill-in exists | [`specs/features/fill_ins.feature`](../../specs/features/fill_ins.feature) | PASS | `R-FILL-PASS-UNCHANGED` |
| `S-SCHEDULE-ROUND` | Every participant receives one unseen track per round | [`specs/features/scheduling.feature`](../../specs/features/scheduling.feature) | PASS | `R-NO-REPEAT`, `R-NO-OWN-TRACK`, `R-ONE-ASSIGNMENT-PER-PERSON-ROUND`, `R-ONE-ASSIGNMENT-PER-TRACK-ROUND` |
| `S-CLASSIC-LENGTH` | A default classic event runs N total rounds | [`specs/features/scheduling.feature`](../../specs/features/scheduling.feature) | PASS | `R-ROUNDS-HOST-OPENED` |
| `S-REVEAL-HOST` | The host triggers the reveal | [`specs/features/reveal.feature`](../../specs/features/reveal.feature) | PASS | `R-REVEAL-AFTER-HOST`, `R-REVEAL-NO-ACTIVE` |
| `S-REVEAL-PENDING` | The reveal is refused while work is pending | [`specs/features/reveal.feature`](../../specs/features/reveal.feature) | PASS | `R-REVEAL-NO-ACTIVE` |
| `S-SOFT-DEADLINE` | A soft deadline does not move a track | [`specs/features/deadlines_and_extensions.feature`](../../specs/features/deadlines_and_extensions.feature) | PASS | `R-NO-AUTO-ADVANCE`, `R-ROUNDS-HOST-OPENED` |
| `S-ONE-OPEN-ROUND` | The host opens at most one round at a time | [`specs/features/deadlines_and_extensions.feature`](../../specs/features/deadlines_and_extensions.feature) | PASS | `R-ONE-OPEN-ROUND` |
| `S-LEAVE-HOLDING` | A participant leaves while holding a track | [`specs/features/join_and_seed.feature`](../../specs/features/join_and_seed.feature) | PASS | `R-LEAVE-SEEN`, `R-LEAVE-NO-FUTURE` |
| `S-LEAVER-RETURN` | A leaver can return as a fill-in | [`specs/features/join_and_seed.feature`](../../specs/features/join_and_seed.feature) | PASS | `R-RESTORE-FILLIN-ONLY`, `R-RESTORE-EXPIRY`, `R-RESTORE-EXPIRY-SETTING` |
| `S-RESTORE-EXPIRY` | A restore invitation expires after 7 days | [`specs/features/join_and_seed.feature`](../../specs/features/join_and_seed.feature) | PASS | `R-RESTORE-EXPIRY`, `R-RESTORE-EXPIRY-SETTING`, `R-RESTORE-EXPIRED-BLOCKS` |
| `S-FILL-DUE-CLAIM` | A claimed fill-in offer starts the 24-hour edit time | [`specs/features/fill_ins.feature`](../../specs/features/fill_ins.feature) | PASS | `R-FILL-DUE-CLAIM`, `R-EDIT-WINDOW` |
| `S-FILL-DUE-APPOINT` | A host-appointed fill-in gets the later due time | [`specs/features/fill_ins.feature`](../../specs/features/fill_ins.feature) | PASS | `R-FILL-DUE-APPOINT`, `R-EDIT-WINDOW` |
| `S-CLAIM-REMINDER` | The bot reminds the participant to claim | [`specs/features/async_queue.feature`](../../specs/features/async_queue.feature) | PASS | `R-CLAIM-REMINDER` |
| `S-CLAIM-24` | A claim delivers the file and starts the deadline | [`specs/features/async_queue.feature`](../../specs/features/async_queue.feature) | PASS | `R-CLAIM-WINDOW-REGULAR`, `R-EDIT-WINDOW` |

## Contradiction pairs

None. Every rule pair has a satisfiable conjunction.

## Base conjunction

| Check | Result | Unsat core |
|---|---|---|
| All must rules together | PASS | none |

## Unsat cores

None. Every scenario is satisfiable under all must rules.

## Coverage gaps

None. Every rule has a scenario and every scenario has a rule.

## Known-bad corpus

The corpus reproduces the contradictions from the 2026-11-28 reviews.
The gate must detect each case and name both sources.

| Case | Expected sources | Actual sources | Result | Unsat core |
|---|---|---|---|---|
| `claim-window-12-vs-6` | [D-014](../../ai-rules/DECISIONS.md), [D-069](../../ai-rules/DECISIONS.md) | [D-014](../../ai-rules/DECISIONS.md), [D-069](../../ai-rules/DECISIONS.md) | DETECTED | KB-CLAIM-12, KB-CLAIM-6 |
| `max-3-vs-no-cap` | [D-014](../../ai-rules/DECISIONS.md), [D-070](../../ai-rules/DECISIONS.md) | [D-014](../../ai-rules/DECISIONS.md), [D-070](../../ai-rules/DECISIONS.md) | DETECTED | KB-HOST-LIST-UNLIMITED, KB-GLOBAL-CAP-3 |
| `shared-budget-vs-extra-turn` | [D-014](../../ai-rules/DECISIONS.md), [D-050](../../ai-rules/DECISIONS.md) | [D-014](../../ai-rules/DECISIONS.md), [D-050](../../ai-rules/DECISIONS.md) | DETECTED | domain, KB-SHARED-BUDGET, KB-EXTRA-TURN |

## Trusted gap

The model is a finite formalization. A wrong or incomplete model can
miss a contradiction in the natural-language source. The source links
and the review of the model are mandatory. The pair check covers two
rules at a time; the base conjunction check covers all must rules
together. The model does not prove
the product code, the SQLite schema, Discord delivery, or wall-clock
time. The model uses a finite domain and integer hours.

## Commands

```bash
scripts/requirements-check.sh           # generate the report
scripts/requirements-check.sh --check   # fail when the report is stale
python3 -m unittest discover -s specs/requirements/tests -t . -p 'test_*.py'
```

## Formula appendix

### `R-CLAIM-WINDOW-REGULAR`

```text
(= claim_window_h_0 24)
```

### `R-CLAIM-WINDOW-FILLIN`

```text
(= claim_window_h_1 6)
```

### `R-CLAIM-REMINDER`

```text
(and (= claim_reminder_h 12) (< claim_reminder_h claim_window_h_0))
```

### `R-EDIT-WINDOW`

```text
(= edit_window_h 24)
```

### `R-FILL-AUTO-CHAIN`

```text
(and fill_in_offer_cap_1 (= max_auto_chain_offers 3))
```

### `R-FILL-HOST-LIST`

```text
(not fill_in_offer_cap_2)
```

### `R-FILL-SEQUENTIAL`

```text
(<= open_fill_in_offers 1)
```

### `R-FILL-FIRST-ACCEPT`

```text
first_accept_wins
```

### `R-FILL-DUE-APPOINT`

```text
(let ((a!1 (ite (= turn_round_0 0)
                round_deadline_h_0
                (ite (= turn_round_0 1) round_deadline_h_1 round_deadline_h_2)))
      (a!3 (ite (= turn_round_1 0)
                round_deadline_h_0
                (ite (= turn_round_1 1) round_deadline_h_1 round_deadline_h_2)))
      (a!5 (ite (= turn_round_2 0)
                round_deadline_h_0
                (ite (= turn_round_2 1) round_deadline_h_1 round_deadline_h_2)))
      (a!7 (ite (= turn_round_3 0)
                round_deadline_h_0
                (ite (= turn_round_3 1) round_deadline_h_1 round_deadline_h_2)))
      (a!9 (ite (= turn_round_4 0)
                round_deadline_h_0
                (ite (= turn_round_4 1) round_deadline_h_1 round_deadline_h_2)))
      (a!11 (ite (= turn_round_5 0)
                 round_deadline_h_0
                 (ite (= turn_round_5 1) round_deadline_h_1 round_deadline_h_2))))
(let ((a!2 (= fill_in_due_h_0
              (ite (>= a!1 (+ appoint_time_h_0 24)) a!1 (+ appoint_time_h_0 24))))
      (a!4 (= fill_in_due_h_1
              (ite (>= a!3 (+ appoint_time_h_1 24)) a!3 (+ appoint_time_h_1 24))))
      (a!6 (= fill_in_due_h_2
              (ite (>= a!5 (+ appoint_time_h_2 24)) a!5 (+ appoint_time_h_2 24))))
      (a!8 (= fill_in_due_h_3
              (ite (>= a!7 (+ appoint_time_h_3 24)) a!7 (+ appoint_time_h_3 24))))
      (a!10 (= fill_in_due_h_4
               (ite (>= a!9 (+ appoint_time_h_4 24))
                    a!9
                    (+ appoint_time_h_4 24))))
      (a!12 (= fill_in_due_h_5
               (ite (>= a!11 (+ appoint_time_h_5 24))
                    a!11
                    (+ appoint_time_h_5 24)))))
  (and (=> (and is_fill_in_turn_0 (= fill_in_flow 0)) a!2)
       (=> (and is_fill_in_turn_1 (= fill_in_flow 0)) a!4)
       (=> (and is_fill_in_turn_2 (= fill_in_flow 0)) a!6)
       (=> (and is_fill_in_turn_3 (= fill_in_flow 0)) a!8)
       (=> (and is_fill_in_turn_4 (= fill_in_flow 0)) a!10)
       (=> (and is_fill_in_turn_5 (= fill_in_flow 0)) a!12))))
```

### `R-FILL-DUE-CLAIM`

```text
(let ((a!1 (=> (and is_fill_in_turn_0
                    (or (= fill_in_flow 1) (= fill_in_flow 2)))
               (= fill_in_due_h_0 (+ claim_time_h_0 edit_window_h))))
      (a!2 (=> (and is_fill_in_turn_1
                    (or (= fill_in_flow 1) (= fill_in_flow 2)))
               (= fill_in_due_h_1 (+ claim_time_h_1 edit_window_h))))
      (a!3 (=> (and is_fill_in_turn_2
                    (or (= fill_in_flow 1) (= fill_in_flow 2)))
               (= fill_in_due_h_2 (+ claim_time_h_2 edit_window_h))))
      (a!4 (=> (and is_fill_in_turn_3
                    (or (= fill_in_flow 1) (= fill_in_flow 2)))
               (= fill_in_due_h_3 (+ claim_time_h_3 edit_window_h))))
      (a!5 (=> (and is_fill_in_turn_4
                    (or (= fill_in_flow 1) (= fill_in_flow 2)))
               (= fill_in_due_h_4 (+ claim_time_h_4 edit_window_h))))
      (a!6 (=> (and is_fill_in_turn_5
                    (or (= fill_in_flow 1) (= fill_in_flow 2)))
               (= fill_in_due_h_5 (+ claim_time_h_5 edit_window_h)))))
  (and a!1 a!2 a!3 a!4 a!5 a!6))
```

### `R-FILL-ELIGIBLE-NEVER-SEEN`

```text
(and (=> fill_in_offer_0_0 (not seen_0_0))
     (=> fill_in_offer_0_1 (not seen_0_1))
     (=> fill_in_offer_0_2 (not seen_0_2))
     (=> fill_in_offer_1_0 (not seen_1_0))
     (=> fill_in_offer_1_1 (not seen_1_1))
     (=> fill_in_offer_1_2 (not seen_1_2))
     (=> fill_in_offer_2_0 (not seen_2_0))
     (=> fill_in_offer_2_1 (not seen_2_1))
     (=> fill_in_offer_2_2 (not seen_2_2)))
```

### `R-FILL-ELIGIBLE-NOT-OWNER`

```text
(and (=> fill_in_offer_0_0 (distinct owner_0 0))
     (=> fill_in_offer_0_1 (distinct owner_1 0))
     (=> fill_in_offer_0_2 (distinct owner_2 0))
     (=> fill_in_offer_1_0 (distinct owner_0 1))
     (=> fill_in_offer_1_1 (distinct owner_1 1))
     (=> fill_in_offer_1_2 (distinct owner_2 1))
     (=> fill_in_offer_2_0 (distinct owner_0 2))
     (=> fill_in_offer_2_1 (distinct owner_1 2))
     (=> fill_in_offer_2_2 (distinct owner_2 2)))
```

### `R-TIER1-ELIGIBLE`

```text
(and (= tier1_eligible_0_0
        (and (= role_0 1)
             (= membership_0 2)
             ready_0
             (not seen_0_0)
             (distinct owner_0 0)))
     (= tier1_eligible_0_1
        (and (= role_0 1)
             (= membership_0 2)
             ready_0
             (not seen_0_1)
             (distinct owner_1 0)))
     (= tier1_eligible_0_2
        (and (= role_0 1)
             (= membership_0 2)
             ready_0
             (not seen_0_2)
             (distinct owner_2 0)))
     (= tier1_eligible_1_0
        (and (= role_1 1)
             (= membership_1 2)
             ready_1
             (not seen_1_0)
             (distinct owner_0 1)))
     (= tier1_eligible_1_1
        (and (= role_1 1)
             (= membership_1 2)
             ready_1
             (not seen_1_1)
             (distinct owner_1 1)))
     (= tier1_eligible_1_2
        (and (= role_1 1)
             (= membership_1 2)
             ready_1
             (not seen_1_2)
             (distinct owner_2 1)))
     (= tier1_eligible_2_0
        (and (= role_2 1)
             (= membership_2 2)
             ready_2
             (not seen_2_0)
             (distinct owner_0 2)))
     (= tier1_eligible_2_1
        (and (= role_2 1)
             (= membership_2 2)
             ready_2
             (not seen_2_1)
             (distinct owner_1 2)))
     (= tier1_eligible_2_2
        (and (= role_2 1)
             (= membership_2 2)
             ready_2
             (not seen_2_2)
             (distinct owner_2 2))))
```

### `R-TIER2-ELIGIBLE`

```text
(and (= tier2_eligible_0_0
        (and (= role_0 0)
             (= membership_0 2)
             ready_0
             submitted_own_0
             (not seen_0_0)
             (distinct owner_0 0)
             plan_leaves_gap_0_0))
     (= tier2_eligible_0_1
        (and (= role_0 0)
             (= membership_0 2)
             ready_0
             submitted_own_0
             (not seen_0_1)
             (distinct owner_1 0)
             plan_leaves_gap_0_1))
     (= tier2_eligible_0_2
        (and (= role_0 0)
             (= membership_0 2)
             ready_0
             submitted_own_0
             (not seen_0_2)
             (distinct owner_2 0)
             plan_leaves_gap_0_2))
     (= tier2_eligible_1_0
        (and (= role_1 0)
             (= membership_1 2)
             ready_1
             submitted_own_1
             (not seen_1_0)
             (distinct owner_0 1)
             plan_leaves_gap_1_0))
     (= tier2_eligible_1_1
        (and (= role_1 0)
             (= membership_1 2)
             ready_1
             submitted_own_1
             (not seen_1_1)
             (distinct owner_1 1)
             plan_leaves_gap_1_1))
     (= tier2_eligible_1_2
        (and (= role_1 0)
             (= membership_1 2)
             ready_1
             submitted_own_1
             (not seen_1_2)
             (distinct owner_2 1)
             plan_leaves_gap_1_2))
     (= tier2_eligible_2_0
        (and (= role_2 0)
             (= membership_2 2)
             ready_2
             submitted_own_2
             (not seen_2_0)
             (distinct owner_0 2)
             plan_leaves_gap_2_0))
     (= tier2_eligible_2_1
        (and (= role_2 0)
             (= membership_2 2)
             ready_2
             submitted_own_2
             (not seen_2_1)
             (distinct owner_1 2)
             plan_leaves_gap_2_1))
     (= tier2_eligible_2_2
        (and (= role_2 0)
             (= membership_2 2)
             ready_2
             submitted_own_2
             (not seen_2_2)
             (distinct owner_2 2)
             plan_leaves_gap_2_2)))
```

### `R-TIER1-BEFORE-TIER2`

```text
(and (=> tier2_offer_0_0 (not tier1_eligible_0_0))
     (=> tier2_offer_0_1 (not tier1_eligible_0_1))
     (=> tier2_offer_0_2 (not tier1_eligible_0_2))
     (=> tier2_offer_1_0 (not tier1_eligible_1_0))
     (=> tier2_offer_1_1 (not tier1_eligible_1_1))
     (=> tier2_offer_1_2 (not tier1_eligible_1_2))
     (=> tier2_offer_2_0 (not tier1_eligible_2_0))
     (=> tier2_offer_2_1 (not tier1_eligible_2_1))
     (=> tier2_offer_2_2 (not tier1_eligible_2_2)))
```

### `R-TIER2-ONLY-NO-TIER1`

```text
(and (=> tier2_offer_0_0
         (not (or tier1_eligible_0_0 tier1_eligible_1_0 tier1_eligible_2_0)))
     (=> tier2_offer_0_1
         (not (or tier1_eligible_0_1 tier1_eligible_1_1 tier1_eligible_2_1)))
     (=> tier2_offer_0_2
         (not (or tier1_eligible_0_2 tier1_eligible_1_2 tier1_eligible_2_2)))
     (=> tier2_offer_1_0
         (not (or tier1_eligible_0_0 tier1_eligible_1_0 tier1_eligible_2_0)))
     (=> tier2_offer_1_1
         (not (or tier1_eligible_0_1 tier1_eligible_1_1 tier1_eligible_2_1)))
     (=> tier2_offer_1_2
         (not (or tier1_eligible_0_2 tier1_eligible_1_2 tier1_eligible_2_2)))
     (=> tier2_offer_2_0
         (not (or tier1_eligible_0_0 tier1_eligible_1_0 tier1_eligible_2_0)))
     (=> tier2_offer_2_1
         (not (or tier1_eligible_0_1 tier1_eligible_1_1 tier1_eligible_2_1)))
     (=> tier2_offer_2_2
         (not (or tier1_eligible_0_2 tier1_eligible_1_2 tier1_eligible_2_2))))
```

### `R-TIER2-ONE-PER-ROUND`

```text
(and (<= tier2_turns_0_0 1)
     (<= tier2_turns_0_1 1)
     (<= tier2_turns_0_2 1)
     (<= tier2_turns_1_0 1)
     (<= tier2_turns_1_1 1)
     (<= tier2_turns_1_2 1)
     (<= tier2_turns_2_0 1)
     (<= tier2_turns_2_1 1)
     (<= tier2_turns_2_2 1))
```

### `R-TIER2-NO-EVENT-CAP`

```text
(not event_wide_tier2_cap)
```

### `R-BUDGET-REGULAR`

```text
(and (= budget_used_0 planned_edits_0)
     (= budget_used_1 planned_edits_1)
     (= budget_used_2 planned_edits_2))
```

### `R-BUDGET-TIER2-EXTRA`

```text
(and (= budget_after_tier2_0 budget_used_0)
     (= budget_after_tier2_1 budget_used_1)
     (= budget_after_tier2_2 budget_used_2))
```

### `R-NO-REPEAT`

```text
(and (=> turn_assigned_0 (not repeat_pair_0))
     (=> turn_assigned_1 (not repeat_pair_1))
     (=> turn_assigned_2 (not repeat_pair_2))
     (=> turn_assigned_3 (not repeat_pair_3))
     (=> turn_assigned_4 (not repeat_pair_4))
     (=> turn_assigned_5 (not repeat_pair_5)))
```

### `R-NO-OWN-TRACK`

```text
(and (=> turn_assigned_0 (not own_track_turn_0))
     (=> turn_assigned_1 (not own_track_turn_1))
     (=> turn_assigned_2 (not own_track_turn_2))
     (=> turn_assigned_3 (not own_track_turn_3))
     (=> turn_assigned_4 (not own_track_turn_4))
     (=> turn_assigned_5 (not own_track_turn_5))
     (=> turn_offered_0 (not own_track_turn_0))
     (=> turn_offered_1 (not own_track_turn_1))
     (=> turn_offered_2 (not own_track_turn_2))
     (=> turn_offered_3 (not own_track_turn_3))
     (=> turn_offered_4 (not own_track_turn_4))
     (=> turn_offered_5 (not own_track_turn_5)))
```

### `R-ONE-ASSIGNMENT-PER-PERSON-ROUND`

```text
(let ((a!1 (+ (ite (and turn_assigned_0 (= turn_holder_0 0) (= turn_round_0 0))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_holder_1 0) (= turn_round_1 0))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_holder_2 0) (= turn_round_2 0))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_holder_3 0) (= turn_round_3 0))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_holder_4 0) (= turn_round_4 0))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_holder_5 0) (= turn_round_5 0))
                   1
                   0)))
      (a!2 (+ (ite (and turn_assigned_0 (= turn_holder_0 0) (= turn_round_0 1))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_holder_1 0) (= turn_round_1 1))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_holder_2 0) (= turn_round_2 1))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_holder_3 0) (= turn_round_3 1))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_holder_4 0) (= turn_round_4 1))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_holder_5 0) (= turn_round_5 1))
                   1
                   0)))
      (a!3 (+ (ite (and turn_assigned_0 (= turn_holder_0 0) (= turn_round_0 2))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_holder_1 0) (= turn_round_1 2))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_holder_2 0) (= turn_round_2 2))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_holder_3 0) (= turn_round_3 2))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_holder_4 0) (= turn_round_4 2))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_holder_5 0) (= turn_round_5 2))
                   1
                   0)))
      (a!4 (+ (ite (and turn_assigned_0 (= turn_holder_0 1) (= turn_round_0 0))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_holder_1 1) (= turn_round_1 0))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_holder_2 1) (= turn_round_2 0))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_holder_3 1) (= turn_round_3 0))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_holder_4 1) (= turn_round_4 0))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_holder_5 1) (= turn_round_5 0))
                   1
                   0)))
      (a!5 (+ (ite (and turn_assigned_0 (= turn_holder_0 1) (= turn_round_0 1))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_holder_1 1) (= turn_round_1 1))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_holder_2 1) (= turn_round_2 1))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_holder_3 1) (= turn_round_3 1))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_holder_4 1) (= turn_round_4 1))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_holder_5 1) (= turn_round_5 1))
                   1
                   0)))
      (a!6 (+ (ite (and turn_assigned_0 (= turn_holder_0 1) (= turn_round_0 2))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_holder_1 1) (= turn_round_1 2))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_holder_2 1) (= turn_round_2 2))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_holder_3 1) (= turn_round_3 2))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_holder_4 1) (= turn_round_4 2))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_holder_5 1) (= turn_round_5 2))
                   1
                   0)))
      (a!7 (+ (ite (and turn_assigned_0 (= turn_holder_0 2) (= turn_round_0 0))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_holder_1 2) (= turn_round_1 0))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_holder_2 2) (= turn_round_2 0))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_holder_3 2) (= turn_round_3 0))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_holder_4 2) (= turn_round_4 0))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_holder_5 2) (= turn_round_5 0))
                   1
                   0)))
      (a!8 (+ (ite (and turn_assigned_0 (= turn_holder_0 2) (= turn_round_0 1))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_holder_1 2) (= turn_round_1 1))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_holder_2 2) (= turn_round_2 1))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_holder_3 2) (= turn_round_3 1))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_holder_4 2) (= turn_round_4 1))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_holder_5 2) (= turn_round_5 1))
                   1
                   0)))
      (a!9 (+ (ite (and turn_assigned_0 (= turn_holder_0 2) (= turn_round_0 2))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_holder_1 2) (= turn_round_1 2))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_holder_2 2) (= turn_round_2 2))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_holder_3 2) (= turn_round_3 2))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_holder_4 2) (= turn_round_4 2))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_holder_5 2) (= turn_round_5 2))
                   1
                   0))))
  (and (<= a!1 1)
       (<= a!2 1)
       (<= a!3 1)
       (<= a!4 1)
       (<= a!5 1)
       (<= a!6 1)
       (<= a!7 1)
       (<= a!8 1)
       (<= a!9 1)))
```

### `R-ONE-ASSIGNMENT-PER-TRACK-ROUND`

```text
(let ((a!1 (+ (ite (and turn_assigned_0 (= turn_track_0 0) (= turn_round_0 0))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_track_1 0) (= turn_round_1 0))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_track_2 0) (= turn_round_2 0))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_track_3 0) (= turn_round_3 0))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_track_4 0) (= turn_round_4 0))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_track_5 0) (= turn_round_5 0))
                   1
                   0)))
      (a!2 (+ (ite (and turn_assigned_0 (= turn_track_0 0) (= turn_round_0 1))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_track_1 0) (= turn_round_1 1))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_track_2 0) (= turn_round_2 1))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_track_3 0) (= turn_round_3 1))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_track_4 0) (= turn_round_4 1))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_track_5 0) (= turn_round_5 1))
                   1
                   0)))
      (a!3 (+ (ite (and turn_assigned_0 (= turn_track_0 0) (= turn_round_0 2))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_track_1 0) (= turn_round_1 2))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_track_2 0) (= turn_round_2 2))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_track_3 0) (= turn_round_3 2))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_track_4 0) (= turn_round_4 2))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_track_5 0) (= turn_round_5 2))
                   1
                   0)))
      (a!4 (+ (ite (and turn_assigned_0 (= turn_track_0 1) (= turn_round_0 0))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_track_1 1) (= turn_round_1 0))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_track_2 1) (= turn_round_2 0))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_track_3 1) (= turn_round_3 0))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_track_4 1) (= turn_round_4 0))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_track_5 1) (= turn_round_5 0))
                   1
                   0)))
      (a!5 (+ (ite (and turn_assigned_0 (= turn_track_0 1) (= turn_round_0 1))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_track_1 1) (= turn_round_1 1))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_track_2 1) (= turn_round_2 1))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_track_3 1) (= turn_round_3 1))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_track_4 1) (= turn_round_4 1))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_track_5 1) (= turn_round_5 1))
                   1
                   0)))
      (a!6 (+ (ite (and turn_assigned_0 (= turn_track_0 1) (= turn_round_0 2))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_track_1 1) (= turn_round_1 2))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_track_2 1) (= turn_round_2 2))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_track_3 1) (= turn_round_3 2))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_track_4 1) (= turn_round_4 2))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_track_5 1) (= turn_round_5 2))
                   1
                   0)))
      (a!7 (+ (ite (and turn_assigned_0 (= turn_track_0 2) (= turn_round_0 0))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_track_1 2) (= turn_round_1 0))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_track_2 2) (= turn_round_2 0))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_track_3 2) (= turn_round_3 0))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_track_4 2) (= turn_round_4 0))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_track_5 2) (= turn_round_5 0))
                   1
                   0)))
      (a!8 (+ (ite (and turn_assigned_0 (= turn_track_0 2) (= turn_round_0 1))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_track_1 2) (= turn_round_1 1))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_track_2 2) (= turn_round_2 1))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_track_3 2) (= turn_round_3 1))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_track_4 2) (= turn_round_4 1))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_track_5 2) (= turn_round_5 1))
                   1
                   0)))
      (a!9 (+ (ite (and turn_assigned_0 (= turn_track_0 2) (= turn_round_0 2))
                   1
                   0)
              (ite (and turn_assigned_1 (= turn_track_1 2) (= turn_round_1 2))
                   1
                   0)
              (ite (and turn_assigned_2 (= turn_track_2 2) (= turn_round_2 2))
                   1
                   0)
              (ite (and turn_assigned_3 (= turn_track_3 2) (= turn_round_3 2))
                   1
                   0)
              (ite (and turn_assigned_4 (= turn_track_4 2) (= turn_round_4 2))
                   1
                   0)
              (ite (and turn_assigned_5 (= turn_track_5 2) (= turn_round_5 2))
                   1
                   0))))
  (and (<= a!1 1)
       (<= a!2 1)
       (<= a!3 1)
       (<= a!4 1)
       (<= a!5 1)
       (<= a!6 1)
       (<= a!7 1)
       (<= a!8 1)
       (<= a!9 1)))
```

### `R-ROUNDS-HOST-OPENED`

```text
(and (=> round_open_0 host_opened_round_0)
     (=> round_open_1 host_opened_round_1)
     (=> round_open_2 host_opened_round_2))
```

### `R-NO-AUTO-ADVANCE`

```text
(and (not auto_advanced_0) (not auto_advanced_1) (not auto_advanced_2))
```

### `R-ONE-OPEN-ROUND`

```text
(<= open_round_count 1)
```

### `R-REVEAL-AFTER-HOST`

```text
(=> reveal_published host_reveal_action)
```

### `R-REVEAL-NO-ACTIVE`

```text
(=> reveal_published no_active_assignments)
```

### `R-LEAVE-SEEN`

```text
(and (=> leave_after_claim_0 held_track_seen_0)
     (=> leave_after_claim_1 held_track_seen_1)
     (=> leave_after_claim_2 held_track_seen_2))
```

### `R-LEAVE-NO-FUTURE`

```text
(and (=> leave_0 (not has_future_assignment_0))
     (=> leave_1 (not has_future_assignment_1))
     (=> leave_2 (not has_future_assignment_2)))
```

### `R-RESTORE-FILLIN-ONLY`

```text
(and (=> restored_0 (and (= role_0 1) (not has_personal_track_0)))
     (=> restored_1 (and (= role_1 1) (not has_personal_track_1)))
     (=> restored_2 (and (= role_2 1) (not has_personal_track_2))))
```

### `R-RESTORE-EXPIRY`

```text
(and (=> invitation_action_0
         (<= invitation_action_h_0 (+ invitation_sent_h_0 168)))
     (=> invitation_action_1
         (<= invitation_action_h_1 (+ invitation_sent_h_1 168))))
```

### `R-RESTORE-EXPIRY-SETTING`

```text
(= invitation_valid_h 168)
```

### `R-RESTORE-EXPIRED-BLOCKS`

```text
(and (=> invitation_expired_0 (not invitation_action_0))
     (=> invitation_expired_1 (not invitation_action_1)))
```

### `R-FILL-PASS-UNCHANGED`

```text
(and (=> (and no_eligible_fill_in (= mode 0)) pass_unchanged)
     (=> (and no_eligible_fill_in (= mode 1)) return_to_pool))
```
