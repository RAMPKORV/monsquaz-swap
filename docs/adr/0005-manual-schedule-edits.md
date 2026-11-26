# ADR 0005 — Manual schedule edits before lock

Date: 2026-11-26

## Status

Accepted for MSW-051.

## Context

The host must fix one bad assignment without replacing the whole draft. A
classic round assigns each active participant one track and each track one
holder. A single cell change that only changes the holder would give that person
two tracks in the round. The planner also must not create a repeat or an own
track.

## Decisions

1. **The edit swaps two people in one round.** The host selects a track cell
   and a new holder. The new holder takes the track, and the previous holder
   takes the new holder's track in the same round. Each person keeps exactly one
   assignment in the round.
2. **The whole plan is validated.** The engine checks own tracks, repeated
   pairs, duplicate people in a round, duplicate tracks in a round, full
   per-round coverage of every active ready person and track, and active
   membership. A failed check writes nothing. The candidate list uses the same
   validation, so the UI offers only valid swaps. A paused or removed
   participant invalidates the plan until the host fixes the roster or drafts
   again.
3. **The edit creates a new plan version.** The old version stays. The new
   version has the same seed and rounds, a new commitment, and the reason
   `manual edit`. The audit entry records the round, track, previous holder,
   new holder, and commitment. The host sends the expected plan version. A
   stale edit is refused with `PlanChanged`.
4. **A complete plan can have no valid single-cell swap.** A classic plan with
   full exposure already uses every legal person-and-track pairing. The UI
   reports that no valid swap exists and offers Replace draft or a shorter plan.
   The engine refuses the swap with a repeat or own-track error.
5. **The edit is available only in schedule review.** After the event starts,
   the host uses the replan flow. This issue does not change a locked plan.

## Consequences

- The host can repair a capped or partial draft without losing the rest.
- Every edit is a reviewed plan version, not an in-place mutation.
- The commitment changes on every edit. The host must lock the final version.
- The UI can show an empty candidate list for a complete plan. That is a
  correctness result, not a bug.

## Rollback

No schema change. The previous plan version remains in the database. Reverting
means selecting the previous version or generating a new draft. The feature can
be removed without a data migration.
