# ADR 0008 — Participation, availability, and schedule visibility

**Status:** accepted

## Context

The join flow collects `max_handoffs`, but the classic scheduler ignored it. A
full plan assigned every active person every exchange round. The host could not
honor a low cap. Away dates were collected in the product text but not stored.
A visible schedule either showed nothing or implied full transparency. The host
needs an explicit choice.

## Decisions

1. **The host chooses the participation policy.** Strict full exposure requires
   every active participant to have a handoff cap that covers every exchange
   round. The seed is not an edit. If a cap is lower, the planner refuses the
   plan and names the person. Flexible participation gives each person their
   earliest exchange rounds up to the cap and leaves the later cells empty.
   The host fills the gaps with a fill-in or a manual edit.

2. **The default handoff cap is 20.** The join command still accepts 1 to 20.
   The old default of 4 made a strict plan impossible for most events. A cap of
   0 means no cap for legacy rows.

3. **Away ranges are stored in UTC.** The host records a start, an end, and an
   optional note. The planner projects each exchange round from the event open
   time. A range that overlaps the projected window removes that participant
   from the round. Strict participation refuses the plan. Flexible
   participation reports the gap. The host can remove a range before the
   schedule locks.

4. **The default schedule is own assignments only.** Participants do not see
   the matrix, the seed band, group names, owners, titles, or comments.

5. **Open schedule is the explicit zero-anonymity mode.** The host can choose
   it in setup. The review step and the join card show a warning. Open schedule
   exposes the R2..RN matrix, seed owners, editor names, titles, and comments.
   The R1 seed band is part of that disclosure.

6. **Gaps are visible, not silent.** The planner shows the empty-cell count and
   the reason. The host must review the gaps before the schedule locks. A
   flexible plan is a draft until the host accepts it.

## Consequences

- `validate_plan_assignments` accepts gaps only when the policy is Flexible.
  Strict validation still requires every active person and track in each round.
- The scheduler still builds a full matching first. Flexible participation
  removes cells after the match. This keeps the hard invariants and the
  deterministic seed.
- Availability uses projected windows. The actual round deadline can move
  later. The host must review the gaps again after a deadline move.
- Open schedule is a host setting, not a participant preference. It locks at
  commit.
