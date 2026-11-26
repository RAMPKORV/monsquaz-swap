# ADR 0003 — Soft deadlines, reminders, and extensions

**Status:** accepted

## Context

The classic loop opens a round and sends offers. A claim moves the track into
custody and sets an edit deadline. Nothing marks a deadline overdue, reminds a
participant, extends a deadline, or gives the host a decision card. The data
model has `due_at` only.

The product rule is fixed: a deadline is a prompt for the host. A deadline does
not move a track.

## Decisions

1. **The deadline tick is a pure engine operation.** `tick_deadlines(db, now)`
   reads active events and claimed assignments. It runs on the dispatcher tick
   and never runs from a participant command. It returns a report for tests.

2. **Soft deadlines only change the assignment state.** When `now` reaches
   `due_at`, a claimed assignment becomes `Overdue`. The track stays in custody.
   The tick creates one open host decision and one durable host alert. The host
   chooses extend, mark missed, or keep waiting.

3. **Reminders use the 50/80/95 percent ladder.** The window starts at
   `claimed_at` and ends at `due_at`. Each step sends one direct message. A
   reminder row makes the step idempotent. The deadline prompt is a
   separate step at 100 percent.

4. **Quiet hours delay non-urgent reminders.** A reminder at 22:00–08:00 in the
   participant timezone waits. The next tick after 08:00 sends it. Quiet hours
   never move `due_at`. The deadline prompt is exempt.

5. **The first small extension is automatic.** A participant can request 1, 6,
   or 12 hours. The first request of 6 hours or less moves `due_at` and records
   an audit entry. Every other request creates an open host decision. A third
   request is refused.

6. **Host decisions are durable.** Migration 007 adds a `host_decisions` table.
   Open decisions appear in mission control. The host can approve or refuse an
   extension and can extend, mark missed, or keep waiting on an overdue
   assignment. Every decision writes an audit entry and a direct message.

7. **Host alerts use the delivery outbox.** A new `HostAlert` effect writes a
   private host thread through the delivery worker. It contains no participant
   identifier and no track code before the reveal. A crash cannot lose the
   alert after the state change commits.

8. **Marking a turn missed records seen.** The assignment becomes
   `SkippedAfterClaim`, the track returns to the pool, and the pair is recorded
   as seen. This is the same rule as a participant skip after claim.

9. **No silent auto-advance.** The tick never opens a round, never moves a
   track out of custody, and never marks a turn missed. The host decides.

## Data

Migration 007:

- `assignment_deadlines(assignment_id, extension_count, reminded_50,
  reminded_80, reminded_95, deadline_reminded)` with one row per assignment.
- `host_decisions(id, event_id, assignment_id, kind, status, requested_hours,
  created_at, decided_at, reason)` with an index on `(event_id, status)`.

The migration is additive. An older build refuses the newer schema and needs a
backup restore.

## Verification

- Engine tests use a fixed clock for the ladder, quiet hours, overdue state,
  extension policy, idempotence, and seen semantics.
- A restart test proves reminder flags and decisions survive a reopen.
- `scripts/check.sh` passes.
- UI review covers the decision card and extension dialog at 1280×800 and
  1024×700.
