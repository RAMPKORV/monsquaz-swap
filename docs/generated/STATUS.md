# Status — Monsquaz Swap

> <!-- GENERATED FILE — edit project.db via tools/project/cli.py; do not edit by hand. -->

**PRD:** draft v1  
**Blocked issues:** 11 · **Ready leaves:** 3  
**Open PRD questions:** 0 (0 high) · **Open reviews:** 0 · **Human queue:** 0  
**Stale scores:** 22

## Issues by status

| Status | Count |
|---|---|
| Triage | 12 |
| Research | 0 |
| Specification | 0 |
| Ready for implementation | 10 |
| Implementation | 0 |
| Reviewing | 0 |
| Testing | 0 |
| Merging | 0 |
| Done | 51 |
| Cancelled | 0 |
| Duplicate | 0 |

## Next actionable

| Rank | Key | Title | Status | Bucket |
|---|---|---|---|---|
| 1 | `MSW-029` | Release pipeline v0.1.0 draft | ready | 6 |
| 2 | `MSW-037` | Formal: classic schedule model | ready | 3 |
| 20 | `MSW-044` | Delivery transport dedupe | ready | 1 |

## Last session

- Agent: agent
- Started: 2026-11-24 22:15:05 · Ended: 2026-11-24 22:15:05
- Summary: Spec-first scaffold: Tauri shell, Rust core invariant, hand-written PRD/settings/research/architecture, 12 Gherkin features, tracker, CI, and GitHub Release pipeline.
- Handoff: Start with MSW-002 (cucumber-rs harness) and MSW-012 (state machines/seen ledger). Read AGENTS.md, ai-rules/STE.md, ai-rules/DOMAIN.md, and docs/EVENT_SETTINGS.md first. Async is fully decided: no groups, global queue + diversity, 24h claim, two waves, exposure completeness, anti-loop protection.
