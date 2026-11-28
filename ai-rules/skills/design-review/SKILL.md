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
   view. Use the demo data in `ui/src/data/demo.ts`. Engine-generated fixtures
   are MSW-101.
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

## Human in the loop

Follow "Human in the loop" in `docs/UI_REVIEW.md` at every checkpoint:

1. Publish fresh screenshots to the shared review gallery before you continue.
2. For an open visual choice, publish 2 or 3 variants (A, B, C) and ask the
   human to choose. Do not choose silently.
3. Read the human's gallery comments before the next iteration.

## Record

- Write the review to `docs/ui/reviews/`. Use one file for each issue, or one
  batch file named by the issue keys.
- Add one review record to the tracker for each issue.
- Link the review file from the PR body.
