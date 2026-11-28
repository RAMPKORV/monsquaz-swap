---
name: design-review
description: Run the Monsquaz Swap UI review waves (visual design, UX and copy, accessibility) with fresh subagents, record the findings, and loop until no must-fix remains. Use when a change touches the host UI under ui/ or the Discord copy.
---

# Design review

The review contract is `docs/UI_REVIEW.md`. The copy and layout rules are in
`ai-rules/UX.md`. The deterministic gate is `scripts/design-check.sh`
(`ai-rules/DESIGN.md`). This skill tells you how to run the waves. It does
not add rules.

## Before the waves

1. Run `scripts/design-check.sh`. Fix every finding first. The waves do not
   replace the gate.
2. Capture the screenshot matrix in `docs/UI_REVIEW.md` for every affected
   view. Use real sample data from the engine fixtures.
3. Put the curated screenshots in `docs/ui/screens/`. Name each file by view,
   state, theme, and width.

## Run one wave

1. Start one fresh subagent for each wave: visual design, UX and copy, and
   accessibility.
2. Give the reviewer these items only:
   - the screenshots for the affected views;
   - the user task, for example "a first-time host opens round 2";
   - the wave checklist from `docs/UI_REVIEW.md`;
   - `ai-rules/UX.md` and the event rules that the view shows.
3. Do not give the reviewer your conclusions, your self-review, or the
   history of the work.
4. Ask for findings in the format of `docs/UI_REVIEW.md`:
   `view | state | severity | problem | suggested fix | screenshot`.
   The severity is `must-fix`, `should-fix`, or `nit`.

You can run the three waves in parallel. They are independent.

## Loop

1. Fix every `must-fix`. Fix a `should-fix` or record why you keep it.
2. Run `scripts/design-check.sh` again.
3. Capture the affected screenshots again. Do not present a screenshot from an
   older build as evidence.
4. Start new fresh reviewers for the waves that had a `must-fix`.
5. Stop when no `must-fix` remains.
6. After three rounds with a `must-fix` still open, stop and ask the human.

## Record

- Write the review to `docs/ui/reviews/MSW-###.md`: the screenshots, each
  wave, each finding, and the fix.
- Add the review record to the tracker issue.
- Link the review file from the PR body.
