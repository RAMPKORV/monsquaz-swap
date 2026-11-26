# UI review plan

The host UI must be beautiful, calm, and easy to understand. It must not look
like a generic generated admin panel. This file is the review contract.

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

Also capture:

- light theme for every populated view;
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
reviewer the screenshots, the user task, and the relevant event rules.

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

## Finding format

```
view | state | severity (must-fix / should-fix / nit) | problem | suggested fix | screenshot
```

Record findings in the tracker under the UI issue. Fix every must-fix. Re-run
the review. Do not close the issue until the latest wave has no must-fix.

## Reject list

- Misaligned labels or inputs.
- Inconsistent gaps.
- Crowded tables with no detail view.
- More than one primary action.
- Placeholder data in a review screenshot.
- Modal inside modal.
- Scrollbar inside scrollbar.
- A graph or chart that does not explain itself.
- Playful styling that hides the next action.
- A screen that only works at one width.
- A screenshot from an older build presented as evidence.
