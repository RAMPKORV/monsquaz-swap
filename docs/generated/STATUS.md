# Status — Monsquaz Swap

> <!-- GENERATED FILE — edit project.db via tools/project/cli.py; do not edit by hand. -->

**PRD:** draft v1  
**Blocked issues:** 5 · **Ready leaves:** 13  
**Open PRD questions:** 0 (0 high) · **Open reviews:** 1 · **Human queue:** 0  
**Stale scores:** 69

## Issues by status

| Status | Count |
|---|---|
| Triage | 48 |
| Research | 0 |
| Specification | 0 |
| Ready for implementation | 15 |
| Implementation | 6 |
| Reviewing | 0 |
| Testing | 0 |
| Merging | 0 |
| Done | 68 |
| Cancelled | 0 |
| Duplicate | 0 |

## Next actionable

| Rank | Key | Title | Status | Bucket |
|---|---|---|---|---|
| 1 | `MSW-029` | Release pipeline v0.1.0 draft | ready | 6 |
| 2 | `MSW-041` | Formal: reveal gate model | ready | 5 |
| 5 | `MSW-038` | Formal: async queue model | ready | 3 |
| 17 | `MSW-044` | Delivery transport dedupe | ready | 1 |
| 20 | `MSW-081` | DB-enforced scheduling invariants | ready | 1 |
| 21 | `MSW-082` | Host sets the fill-in due time | ready | 1 |
| 22 | `MSW-083` | Fill-in flow event setting | ready | 1 |
| 33 | `MSW-100` | New design-gate rules | ready | 1 |
| 34 | `MSW-101` | Engine-generated demo fixtures | ready | 1 |
| 35 | `MSW-102` | App shell: lifecycle nav, status chip, wordmark | ready | 1 |

## Open reviews

| ID | Scope | Subject | Round | Score |
|---|---|---|---|---|
| 55 | test | E2E foundation and simulator | 1 | 8/7 |

## Last session

- Agent: agent
- Started: 2026-11-24 22:15:05 · Ended: 2026-11-24 22:15:05
- Summary: Spec-first scaffold: Tauri shell, Rust core invariant, hand-written PRD/settings/research/architecture, 12 Gherkin features, tracker, CI, and GitHub Release pipeline.
- Handoff: Start with MSW-002 (cucumber-rs harness) and MSW-012 (state machines/seen ledger). Read AGENTS.md, ai-rules/STE.md, ai-rules/DOMAIN.md, and docs/EVENT_SETTINGS.md first. Async is fully decided: no groups, global queue + diversity, 24h claim, two waves, exposure completeness, anti-loop protection.
