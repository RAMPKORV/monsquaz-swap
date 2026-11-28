# ADR 0007 — Classic round model, seed round, and shared deadlines

**Status:** accepted

## Context

The engine treated seed intake as a phase before round 1. It called the first
exchange round "round 1". The event length was `N-1` exchange rounds. The host
could not open the seed round with a pack, could not set a seed deadline, and
could not see the seed owners in the plan. A claim started a new edit window, so
one round had many edit deadlines. The commitment covered the exchange
assignments only.

The product needs a seed round that counts in the event length. A full classic
event with `N` seeded participants must have `N` total rounds: R1 seed and
R2..RN exchanges. File versions use `v1..vN`. The host must control the pack,
the seed deadline, and the whole-round deadline.

## Decisions

1. **R1 is the seed round.** Each participant makes one track from the host
   pack and submits it before the seed deadline. R1 counts in the event length
   and in the progress display. A full event with `N` seeded participants has
   `N` total rounds and `N-1` exchange rounds.

2. **Native exchange indices do not change.** The database keeps the exchange
   index `0..N-2` in `assignments.round` and `plan_assignments.round`. The
   round table uses the display number: `round_no = exchange index + 2`. One
   helper converts between the two. No surface opens a round with a raw `+1`.

3. **The host opens Round 1.** The event starts in Draft. The host uploads the
   pack, sets the seed deadline, and calls `open_seed_round`. The event then
   enters Signup. If the pack is required, an open without a pack is refused.

4. **Seed intake closes softly.** A seed after the deadline is held in
   `late_seeds` until the host accepts or rejects it. The host closes Round 1
   and locks the roster in one transaction. The close refuses fewer than two
   seeds. Reopen is allowed before the schedule starts; a reopen voids draft
   plans and resets memberships to seeded or signed-up.

5. **A second seed replaces v1 before the deadline.** The track code and the
   owner do not change. The old blob stays in the content store. The audit log
   records the replacement. One participant has one seed.

6. **A classic round has one deadline.** The host sets it when the host opens
   the round. The default is 24 hours for a normal exchange round and 48 hours
   for the final exchange round. The seed default is 7 days. An offer expires
   at the round deadline. A claim uses the same deadline. A later extension is
   an explicit exception and is stored as `extension_secs`.

7. **The host can move a whole round deadline.** The move applies to offered,
   claimed, and overdue assignments. Overdue returns to claimed. Later
   individual extensions are preserved. Reminders are re-armed. The host sends
   one notification batch and writes one audit entry. Submitted work stays
   final. A move is forward only.

8. **Start commits and announces.** `start_event` requires a closed seed round
   and a valid plan. It publishes the commitment and moves the event to Active.
   Opening R2 is a separate host action.

9. **The commitment covers R1.** Commitment v2 covers the R1 owner map, the
   R2..RN assignments, the pack id, the locked policy, the plan version, and a
   random 128-bit nonce. The event secret is a random 128-bit value. The
   commitment is 32 lowercase hex characters. The reveal publishes the recipe
   and the nonce. A verify action recomputes the commitment from stored rows.

10. **The planner shows R1 as a host-only band.** The exchange matrix starts at
    R2. The seed band shows each track and owner. Participants do not see the
    band. The default participant view is the caller's own assignments.

11. **Open schedule is an explicit exception.** The host can enable full
    transparency: the matrix, seed owners, editor names, titles, and comments.
    The join card discloses the loss of anonymity. The R1 band is still
    host-only in the default mode.
    *Superseded in part by D-045 (2026-11-28): an open schedule shows
    assignments only, never owners, titles, or comments before the reveal.*

12. **Versions are not rounds.** A file version is the count of successful
    submissions for that track. A skipped round creates no version. The UI
    shows "R3 edit · file v2" and never uses `R` for a version.

## Consequences

- The engine must keep a round record for the seed round and every exchange
  round. `open_round` writes the round and its deadline in one transaction.
- `tick_deadlines` expires classic offers at the round deadline. Expiry is
  cleanup, not round movement. The host still opens the next round.
- The scheduler still produces `N-1` exchange rounds. The UI and the commitment
  add R1. Sending `N` as the exchange count remains an error.
- Existing events with `round_model = 0` keep the old labels and the v1
  commitment. New events use `round_model = 1` and commitment v2.
- The seed band is host-only. The redaction preview must filter the band and the
  seed owner ids.
- A full-exposure plan is a plan guarantee, not a guarantee that every person
  submits. Expiry, skip, leave, and flexible participation create coverage gaps.
  The gap report must say so.

## Known limits

- Date-driven availability and arbitrary group levels are later issues.
- A pack replacement after R1 opens requires a reopen.
- The reveal runs only after every planned exchange round is opened and
  resolved.
