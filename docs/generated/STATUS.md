# Status — Monsquaz Swap

> <!-- GENERATED FILE — edit project.db via tools/project/cli.py; do not edit by hand. -->

**PRD:** draft v1  
**Blocked issues:** 12 · **Ready leaves:** 4  
**Open PRD questions:** 0 (0 high) · **Open reviews:** 0 · **Human queue:** 0  
**Stale scores:** 23

## Issues by status

| Status | Count |
|---|---|
| Triage | 10 |
| Research | 0 |
| Specification | 0 |
| Ready for implementation | 11 |
| Implementation | 1 |
| Reviewing | 1 |
| Testing | 0 |
| Merging | 0 |
| Done | 8 |
| Cancelled | 0 |
| Duplicate | 0 |

## Next actionable

| Rank | Key | Title | Status | Bucket |
|---|---|---|---|---|
| 2 | `MSW-030` | Twilight gateway runtime | ready | 3 |
| 14 | `MSW-020` | Event wizard, participant manager, and schedule planner | ready | 2 |
| 19 | `MSW-026` | Simulation harness and adversarial review | ready | 2 |
| 20 | `MSW-027` | Privacy export/delete and accessibility pass | ready | 2 |

## Last session

- Agent: agent
- Started: 2026-11-24 22:15:05 · Ended: 2026-11-24 22:15:05
- Summary: Spec-first scaffold: Tauri shell, Rust core invariant, hand-written PRD/settings/research/architecture, 12 Gherkin features, tracker, CI, and GitHub Release pipeline.
- Handoff: Start with MSW-002 (cucumber-rs harness) and MSW-012 (state machines/seen ledger). Read AGENTS.md, ai-rules/STE.md, ai-rules/DOMAIN.md, and docs/EVENT_SETTINGS.md first. Async is fully decided: no groups, global queue + diversity, 24h claim, two waves, exposure completeness, anti-loop protection.
