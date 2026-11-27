# Hierarchical group schedule simulation

**Issue:** MSW-073. **Scope:** 12 to 1000 participants, classic mode.

## Finding

The hard problem is cohort composition, not hierarchy depth. The current single
level rotates whole participant groups, so each person meets the same group for
the whole event. That gives track variety but little social variety.

The model that scales is:

- **Track groups** partition tracks. They stay fixed unless a level moves them.
- **Social cohorts** partition participants each phase. A cohort is assigned to
  one track group.
- **Mixing pool** limits who can meet whom: base group, supergroup, or global.
- **Stickiness** keeps a fraction of each cohort across phases.
- **Cadence K** sets how many rounds a cohort spends with one track group.

For `R` rounds: exposure is `R` tracks; new co-venue people is about
`phases * (b-1)`; group depth is about `K / b`.

## 1000 participants, 10-person groups, 60 rounds

| Mixing | K | Phases | Distinct people | Fun proxy |
|---|---:|---:|---:|---:|
| Atomic, same cohort | 10 | 6 | 9 | 0.52 |
| Fresh global cohorts | 10 | 6 | 52.8 | 0.82 |
| Fresh global cohorts | 5 | 12 | 102.8 | 0.82 |
| 25% sticky global | 10 | 6 | 51.9 | 0.82 |
| 50% sticky global | 10 | 6 | 43.2 | 0.76 |
| 75% sticky global | 10 | 6 | 25.8 | 0.64 |

## Recommendations

- Small events (12–24): flat or atomic groups; full exposure is realistic.
- Large events (100–1000): 8–10 person base groups, global or 100-person local
  pool, 25% stickiness, `K ≈ 5`.
- Default: `b=8`, `K=5`, 25% stickiness, global pool for 100 or more.
- Cosy preset: `K=10`, 50% stickiness, local pool.
- Explorer preset: `K=3`, fresh cohorts, global pool.
- Full exposure stays opt-in for small events. Do not default to 999 rounds.

## Builder

1. Roster and mode. Show `N`, full-exposure cost, and the recommended cap.
2. Length and cadence. `R`, `K`, preset, live phase preview.
3. Groups and pool. Base size, pool, hierarchy depth, balance.
4. Mixing. Fresh, 25/50/75%, persistent; rotation policy.
5. Constraints. Hard invariants, soft bounds, host participation, visibility.
6. Preview. Phase table, participant journey, coverage, social heatmap, warnings.
7. Commit. Freeze the roster, validate, store the plan and commitment.

Host controls: rounds, base size, pool, stickiness, cadence, preset, manual
overrides. Not host-facing: individual pairings or raw group membership.

## Failure modes

- Home-venue exhaustion at `K=b`; start away or use `b-1`.
- Mixed cohorts exhaust a small track pool; lower `K` or use a larger pool.
- Random rotation revisits a seen group; use cyclic, Latin, or optimized.
- Remainder groups need a balanced partition and a flex pool.
- A leave or late seed breaks the group shape; repair at a phase boundary.
- Full exposure at 1000 is 999 rounds; cap it.

## First implementation slice

- Base track groups of size `b`.
- One social pool level: global or a supergroup of `s` base groups.
- Cohorts with a stickiness setting.
- Min-cost cohort-to-track matching and precomputed phase cards.
- Host presets and a phase preview.
- No arbitrary hierarchy depth in the first slice.

## Open decisions

- Social mixing is a goal, or tracks only? The simulation assumes people too.
- Default full exposure cap for small events.
- Default group size (5, 8, 10, or 12).
- Whether tracks move with groups. The first slice keeps tracks fixed.
- Maximum supported `N`. The first slice targets 1000.
