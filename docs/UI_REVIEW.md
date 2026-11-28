# UI review plan

The host UI must be beautiful, calm, and easy to understand. It must not look
like a generic generated admin panel. This file is the review contract. It is
the single owner of the screenshot matrix, the review waves, the finding format,
and the reject list. `ai-rules/UX.md` owns the voice, copy, and layout rules.

## Screenshot inventory

Capture each view in the required states. Use realistic sample data.

| View | Empty | Loading | Populated | Error | Long/dense | 1024 px |
|---|---|---|---|---|---|---|
| Setup wizard | yes | yes | yes | yes | n/a | yes |
| Mission control | yes | yes | yes | yes | yes | yes |
| Event wizard | yes | yes | yes | yes | yes | yes |
| Participant manager | yes | yes | yes | yes | yes | yes |
| Schedule planner (matrix) | yes | yes | yes | yes | yes | yes |
| Schedule planner (graph) | yes | yes | yes | yes | yes | yes |
| Round board | yes | yes | yes | yes | yes | yes |
| File browser | yes | yes | yes | yes | yes | yes |
| Audit log | yes | yes | yes | yes | yes | yes |
| Reveal presenter | yes | yes | yes | yes | yes | yes |
| Export/reveal package | yes | yes | yes | yes | n/a | yes |
| Settings | yes | n/a | yes | yes | yes | yes |
| Logs | n/a | yes | yes | yes | yes | yes |
| Dry run | yes | yes | yes | yes | yes | yes |

Capture every state at 1280×800 and at the 1024×700 minimum. Use a
deterministic seed and sample data. Store curated screenshots under
`docs/ui/screens/` and name them by view and state.

Also capture:

- dark and light theme for every populated view;
- 200% zoom for the mission control, planner, participant table, and reveal;
- the reveal presenter in stream-safe mode;
- a destructive confirmation;
- a blocked/infeasible schedule;
- a bot-offline state.

## Deterministic gate

Run `scripts/design-check.sh` before the review waves. It renders the page
matrix at 1280×800, 1024×700, and 640×400, checks overflow and clipping with
Refract, checks text contrast and keyboard focus with Playwright, and checks
the CSS policy in `design/policy.json`. Existing accepted debt lives in
`design/baseline.json`. The rules and the change process are in
`ai-rules/DESIGN.md`.

The review waves do not replace the gate. The gate does not replace the review
waves. A screenshot can show a beautiful layout with a clipped control; the
gate can pass a layout with a weak hierarchy.

## Review waves

Each wave uses a fresh reviewer with no memory of building the UI. Give the
reviewer the screenshots, the user task, and the relevant event rules. Run at
least the first three waves for every UI issue. The procedure is in
`ai-rules/skills/design-review/SKILL.md`.

### Wave 1 — Visual design

Check:

- alignment on a shared grid;
- consistent spacing scale;
- one clear hierarchy per screen;
- density: not crowded, not wasteful;
- typography and line length;
- color, contrast, and status coding;
- overflow, truncation, and wrapping;
- responsive behavior at 1024 px;
- consistency of cards, tables, buttons, tabs, and modals;
- no nested scrollbars or card-inside-card-inside-card.

### Wave 2 — UX and copy

Check:

- the next action is obvious;
- the host can recover from an error;
- empty states explain what to do;
- destructive actions confirm consequences;
- jargon is defined;
- a first-time host can complete the main journey;
- no public shaming or pressure;
- the reveal feels generous and clear.

### Wave 3 — Accessibility

Check:

- contrast, focus order, keyboard path, and visible focus;
- screen-reader labels;
- no color-only status;
- 200% zoom without loss;
- reduced motion;
- touch targets;
- localization-ready strings.

### Wave 4 — Final polish (M8)

Repeat the matrix after the full event loop exists. Check density at real data
volume (40 participants, 20 tracks, 10 rounds), the reveal presenter on a
projector, and the stream-safe mode.

## Human in the loop

The human reviews the UI at every checkpoint. The agent does not continue
without the human.

1. At every UI checkpoint, the agent captures fresh screenshots of the
   affected views.
2. The agent publishes the screenshots to the shared review gallery for the
   human.
3. The agent continues only after the agent publishes the screenshots.
4. When a visual choice is open, the agent renders 2 or 3 variants (A, B, C)
   as screenshots.
5. The agent publishes the variants and asks the human to choose one. The
   agent must not choose silently.
6. Before the next iteration, the agent reads the human's comments in the
   gallery.
7. The agent fixes or answers every comment in the next iteration.

A UI checkpoint is the end of each review wave, a visual choice, and the end
of the UI issue.

The review gallery is a private page. The repository is public, so the docs
must not contain the gallery link.

- The orchestrating agent publishes the checkpoint screenshots to the private
  review gallery.
- The orchestrating agent puts the gallery link in the PR body.
- `docs/ui/screens/` keeps the curated screenshots that the change commits.

## Finding format

```
view | state | severity (must-fix / should-fix / nit) | problem | suggested fix | screenshot
```

Record findings in the tracker under the UI issue. Fix every must-fix. Re-run
the review. Do not close the issue until the latest wave has no must-fix.

## Reject list

- Misaligned labels or inputs.
- Inconsistent gaps between sections.
- Crowded tables with no detail view.
- More than one primary action.
- Placeholder data in a review screenshot.
- Modal inside modal.
- Scrollbar inside scrollbar.
- A graph or chart that does not explain itself.
- Playful styling that hides the next action.
- A screen that only works at one width.
- A screenshot from an older build presented as evidence.
