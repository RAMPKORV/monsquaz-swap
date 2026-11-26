# Host UI screenshots

The curated matrix for the host shell and the event tools (mission control,
event setup, participants, schedule planner, settings, audit log). The images
come from a production UI build with deterministic preview data
(`VITE_DEMO=1`). They are not mockups.

Regenerate:

```bash
docs/ui/capture.sh
ONLY=plan docs/ui/capture.sh   # one view or state
```

Requirements: `pnpm`, `chromium`, and `curl`. The script serves the built UI on
port 4173, captures each state, and writes to this directory.

## Matrix

| View | First run / empty | Loading | Populated | Error | Long | Dense | 1024 px | Light |
|---|---|---|---|---|---|---|---|---|
| Mission control | `mission-first-run-dark-1280` | `mission-loading-dark-1280` | `mission-populated-dark-1280` | `mission-error-dark-1280` | `mission-long-dark-1280` | n/a | `mission-populated-dark-1024` | `mission-populated-light-1280` |
| Event setup | `setup-first-run-dark-1280` | `setup-loading-dark-1280` | `setup-populated-dark-1280` | `setup-error-dark-1280` | `setup-long-dark-1280` | n/a | `setup-populated-dark-1024` | `setup-populated-light-1280` |
| Participants | `people-empty-dark-1280` | `people-loading-dark-1280` | `people-populated-dark-1280` | `people-error-dark-1280` | `people-long-dark-1280` | `people-dense-dark-1280` | `people-populated-dark-1024` | `people-populated-light-1280` |
| Schedule planner | `plan-empty-dark-1280` | `plan-loading-dark-1280` | `plan-populated-dark-1280` | `plan-error-dark-1280` | `plan-long-dark-1280` | `plan-dense-dark-1280` | `plan-populated-dark-1024` | `plan-populated-light-1280` |
| Settings | `settings-first-run-dark-1280` | n/a | `settings-populated-dark-1280` | `settings-error-dark-1280` | `settings-long-dark-1280` | n/a | `settings-populated-dark-1024` | `settings-populated-light-1280` |
| Audit log | `audit-empty-dark-1280` | `audit-loading-dark-1280` | `audit-populated-dark-1280` | `audit-error-dark-1280` | `audit-long-dark-1280` | `audit-dense-dark-1280` | `audit-populated-dark-1024` | `audit-populated-light-1280` |

Additional states:

- `mission-bot-offline-dark-1280` — gateway offline.
- `settings-delete-dialog-dark-1280` — destructive confirmation.
- `settings-dirty-dark-1280` and `settings-dirty-dark-1024` — unsaved save bar.
- `plan-infeasible-dark-1280` — refused round and recovery actions.
- `plan-participant-view-dark-1280` — host preview of the participant view.
- `plan-full-dark-1280x1600` — the complete planner page.
- `plan-graph-dark-1280x1200` — the round graph below the matrix.
- `plan-populated-dark-zoom200` — 640×400, a true 200% zoom of the
  planner. `people-populated-dark-zoom200` — 640 px wide, equivalent to 200%
  zoom.
- `shell-command-palette-dark-1280` — the command palette with its focus ring.
- `mission-populated-light-1024` — light theme at the minimum window width.
- `mission-populated-dark-zoom200` — 640×400 CSS pixels, equivalent to 200%
  zoom at 1280×800.
- `audit-populated-dark-zoom200` — 640×1000, the audit card layout at 200%
  zoom with extra vertical evidence.
- `settings-full-dark-1280x2000` and `settings-full-light-1280x2000` — the
  complete settings page, including notifications, appearance, and the danger
  zone.

## Review record

The three review waves and the fixed findings are recorded in
`docs/ui/reviews/MSW-020.md`. Do not close a UI issue until the latest wave has
no must-fix item.
