# Event settings

This file lists every event setting, its default, its options, and its lock
behaviour. The host can change a setting only when the lock column allows it.
All instants are UTC in the database and local in the UI. The async release
time is a wall-clock time in the host IANA zone (D-042).

A row marked **decided, building in MSW-###** is an accepted decision that the
code does not implement yet. The named issue builds it.

## 1. Event identity

| Setting | Default | Options | Lock at commit |
|---|---|---|---|
| Event name | `Monsquaz Swap <n>` | Free text | No |
| Guild / server | The host's selected guild | One active event per guild | Yes |
| Host | The app owner | One primary host | Yes |
| Co-hosts | None | Add Discord accounts; full mapping access; audited | No |
| Language | English | English in v1; strings are i18n-ready | No |

## 2. Mode, length, completeness

| Setting | Default | Options | Lock at commit |
|---|---|---|---|
| Mode | Classic for 12 or fewer participants; async for more | Classic, Async | Yes |
| Classic length | `N` total rounds: R1 seed and R2..RN exchanges | Any number from 2 to `N` | Yes |
| Overcrowding warning | On above 12 participants | On, off | No |
| Seed deadline | 7 days | 1–30 days | Yes |
| Async completeness | Exposure: every participant receives every planned track | Exposure; capped rounds with coverage report; no completeness promise | Yes |
| Async length | `N-1` for 12 or fewer; 6–8 rounds above 12 | Host can choose full exposure; the planner shows the projected duration | Yes |
| Async handoff deadline | 24 hours after claim | 12–168 hours | Yes |
| Async expected songs | Seeded track count | 1–1000; must be reached before start | Yes |
| Async claims per participant | Tracks minus one | 1–40; must be below the track count | Yes |
| Async release | 18:00 in the host IANA zone, two waves two hours apart | Any HH:MM in the host IANA zone; the second wave is two hours after the first | Yes |
| Async late seeds | Off | Off; host review with a feasibility check. Decided, building in MSW-091 | Yes |
| Classic round deadline | 24 hours; final exchange 48 hours | 12–336 hours; final can differ | Yes |
| Participation | Strict full exposure | Strict; Flexible earliest rounds with gaps | Yes |
| Schedule visibility | Own assignments only | Own assignments only; Open schedule (assignments only, D-045) | Yes |
| Away ranges | None | Host-recorded UTC ranges; planner skips overlapping rounds | No |
| Round/phase dates | Host-set | Calendar windows; host can shift | No |

Rules:

- Classic `N` total rounds means every participant can touch every other track:
  R1 seed and N-1 exchanges.
- Strict participation requires every active handoff cap to cover every
  exchange round. Flexible participation gives each person the earliest rounds
  up to the cap and leaves coverage gaps. The host fills the gaps.
- An away range removes the overlapping projected rounds. Strict participation
  refuses the plan and asks for flexible participation.
- Open schedule shows which participant edits which track code in each round.
  It never shows seed owners, titles, or comments before the reveal (D-045).
  The join card discloses this rule.
- Above 12 participants, each track can have many contributors. The planner
  must show the contributor count and the event duration.
- The host can cap the length. A capped event does not touch every track with
  every participant.
- Async "exposure" means a participant claims a track at least once. The
  planned exposure set depends on the chosen length. Full N-1 exposure is the
  special case; capped events report coverage and gaps. Exposure does not mean
  the participant submits an edit. A claim-then-skip or a claimed deadline miss
  counts as seen. The app must report exposure gaps and must not loop forever.
- Full async exposure takes about 2–3 months at 40 participants. The planner
  must show this estimate before the host chooses it.

## 3. Classic groups

Async mode has no formal groups. It uses one global queue and the
social-diversity objective. Optional genre/mood content tags can come later.

| Setting | Default | Options | Lock at commit |
|---|---|---|---|
| Group suggestion | Simulation lookup | Lookup, single group, manual | No |
| Group count | Lookup: 1 for ≤9; 2 for 10–13; 3 for 14–18 and 23–32; 4 for 19–22 and 33–40 | 1–4, or manual | Yes |
| Group size | Balanced, 5–12 | Soft bounds; warn outside | Yes |
| Group visibility | Hidden | Hidden, host-only names | Yes |
| Rotation default | Participants | Participants; track pools; merge/re-split. Rotate both is rejected (D-012). Track pools and merge/re-split: decided, building in MSW-090 | Yes |
| Scheduler strategy | Balanced | Balanced, random matching, cyclic | No |
| Swap cadence | `K ≈ T/4` (3–5 phases) | K rounds, calendar, host-triggered, exhausted | No |
| Exhausted policy | Allowed with a warning | Warn when `group_size > T+1` | No |
| Calendar early swap | Auto-early by default | Auto-early, strict calendar | No |
| Merge/re-split | Repair only | Repair, scheduled, host-only, never | No |
| Unequal groups | Warn and suggest | Warn, hard equal, allow any, auto-rebalance | No |
| Infeasible phase | Global pool fallback | Fallback, shorten phase, stall, retire | No |

The scheduler setting is available through the event settings API. The event
wizard selector is a follow-up. `ScheduleOutcome.fallback` reports when a
strategy used random matching. The plan record and UI do not persist that flag
yet.

The group lookup runs when the planner generates the plan, because the active
roster is known then. The plan stores the effective group count and size. The
manual count and size controls are enabled only for the Manual group mode.
Soft-bound and cadence warnings do not block the plan. Participants rotate one
group forward at each phase. A group phase that cannot fill uses the global pool
for that phase and reports a host warning. Groups return in the next phase.
Before the event starts, the host can repair an underfilled or unequal draft.
The repair writes a new plan version with the reason `group repair` and an audit
entry. Active-phase replans are not in v1.

Rules:

- The planner must show the fun estimate and the guardrails. The fun score is
  host-only.
- The planner must explain an infeasible phase with the exact people and
  tracks.
- Group membership is hidden from participants.
- A group swap must not break the seen ledger.
- Async mode has no groups and no group migration (D-024).

## 4. Visibility and anonymity

| Setting | Default | Options | Lock at commit |
|---|---|---|---|
| Schedule visibility | Hidden | Hidden; visible assignments only | Yes |
| Visible-schedule surface | None | `/swap schedule` + pinned post | No |
| Owner visibility | Hidden | Hidden; host-only | Yes |
| Track title visibility | Hidden | Hidden until reveal; visible | Yes |
| Group membership | Hidden | Hidden; visible to members; full | Yes |
| Public progress | Status + countdown | Status + countdown; exact counts; coarse buckets; nothing | No |
| Exact counts after deadline | Allowed | Allowed, suppressed | No |
| Participant progress | Personal season view | Personal season; current offer only; global dashboard | No |
| Skip visibility | Host-only | Host-only; anonymous aggregate; public named | Yes |
| Comments/questionnaire visibility | Host + reveal | Host + reveal; host only; public at reveal | Yes |
| Host participation | Allowed with disclosure | Allowed, no, co-host plays | Yes |
| Stream-safe mode | On when the presenter opens | On, off | No |

Rules:

- If the schedule is visible, participants see past and future assignments.
  They do not see owners or titles.
- Public progress must not allow a participant to identify who is late. Use
  status and countdown before the deadline.
- The host can see the mapping. The rules must say this.
- The app does not enforce a minimum participant count and does not show a
  small-event anonymity warning. The rules state the limits; the host decides.

## 5. Host pack and samples

| Setting | Default | Options | Lock at commit |
|---|---|---|---|
| Pack type | Module template + samples | Module template + samples; samples only; reference track; none. Decided, building in MSW-089 | Yes |
| Pack requirement | Required with warnings | Required with warnings; recommended; no enforcement | Yes |
| Pack check | Hash report + host review | Hash report; name report; manual | No |
| New samples | Disallowed | Disallowed; host-approved; allowed | Yes |
| Altered pack samples | Allowed | Allowed; disallowed | Yes |
| Added instruments | Allowed | Allowed; disallowed | Yes |
| Pack delivery | DM at join/start | DM; pinned post; both | No |

Rules:

- The pack check must not reject a seed for a suspected new sample. It must
  flag the seed for host review.
- The host decides whether to accept a flagged seed.
- The app must store the pack hash and the seed report.

## 6. Files and formats

| Setting | Default | Options | Lock at commit |
|---|---|---|---|
| Allowlist | Modules, audio, zip | Any list of extensions | Yes |
| Module extensions | `.it`, `.mptm`, `.xm`, `.mod`, `.s3m`, `.mtm`, `.669` | Per-event | Yes |
| Audio extensions | `.wav`, `.flac`, `.mp3`, `.ogg` | Per-event | Yes |
| Archive extensions | `.zip` | Per-event | Yes |
| Format change | Forbidden | Forbidden; allowed with warning | Yes |
| Maximum file size | 20 MiB | 1–20 MiB | Yes |
| Oversized fallback | Bot downloads link; else host import | Bot download; host import; reject | No |
| Metadata scrub | Module-aware | Module-aware; filename-only; off | Yes |
| Corrupt file | Reject + allow replace | Reject; quarantine; accept | No |
| Resubmission | Latest wins, keep all | Latest wins; one submission; host approval | No |

Rules:

- The bot must download an attachment before the Discord CDN URL expires.
- The app must never use the participant filename as a storage path.
- The bot must remove the original holder and version from the outbound name.
- The app must keep an untouched original in a host-only vault.

## 7. Delivery and claim

| Setting | Default | Options | Lock at commit |
|---|---|---|---|
| Claim first | On | On; push files | Yes |
| Claim window | 24 hours | 1–48 hours | Yes |
| Claim reminder | 12 hours after offer | 6h; 12h; none; custom. The reminder must be before the end of the claim window. Decided, building in MSW-086 | No |
| Ready gate | On | On; off | Yes |
| Expiry protection | Strike + cooldown: the first expiry starts a 24-hour cooldown, the second expiry pauses the participant | Strike + cooldown; cooldown only; host review; none. Decided, building in MSW-084 | No |
| Same-track re-offer after expiry | Never | Never; after cooldown; last resort; host. Decided, building in MSW-085 | No |
| Release windows | One per day, two waves | One wave; two waves; four; fixed interval; continuous | No |
| Release time | 18:00 in the host IANA zone | Any HH:MM in the host IANA zone; changeable | No |
| Release change | Future offers shift; claimed assignments keep their due date | Future shifts; shift all; fixed after start | No |
| DM fallback | Private thread | Private thread; host mediation; block join | Yes |
| Submission channel | DM attachment | DM; slash command | Yes |
| Receipt | On | On; off | No |

Rules:

- The bot sends the file only after the participant claims.
- A claim starts the async deadline.
- An unclaimed offer does not mark the pair seen.
- A claim-then-skip marks the pair seen.
- Strikes reset when the participant resumes. An auto-pause alerts the host.

## 8. Deadlines, reminders, extensions

| Setting | Default | Options | Lock at commit |
|---|---|---|---|
| Deadline type | Soft | Soft; hard with grace; hard auto-skip | Yes |
| Reminder ladder | 50%, 80%, 95%, deadline | Percentage; fixed offsets; daily; host-set | No |
| Quiet hours | 22:00–08:00 local | Any window; off when start equals end; per-participant | No |
| At-deadline prompt | On | On; off | No |
| Extensions | +1h, +6h, +12h | Host-defined presets | No |
| First extension | Auto-approve up to 6h | Auto; host approval | No |
| Maximum extensions | 2 per assignment | 0–3 | No |
| No-response fallback | Grace, then host | Grace + host; auto-skip; wait | Yes |
| Host deadline prompt | DM + decision card; 6h target | 6h target; no target; host-set | No |
| Event pause | Host action | On | No |

## 9. Join, leave, rejoin

| Setting | Default | Options | Lock at commit |
|---|---|---|---|
| Join mode | Self-serve + join by submission | Self-serve; role-gated; manual | Yes |
| Join by submission after R1 | Held for host approval | Held until the host closes Round 1; refuse | No |
| Late seed cutoff | Seed deadline | Seed deadline; host close | No |
| Late seed integration | Host choice; if allowed in a capped event, extra quota + minimum editor quota | Extra quota + minimum; no late seeds; fill-in only | Yes |
| Unseeded signup | Auto fill-in | Fill-in; waitlist; remove; spectator (not in v1) | Yes |
| Leave | Allowed | Allowed; before lock only; host approval | Yes |
| Rejoin | Fill-in only | Fill-in; rejoin with replan; next event; host | Yes |
| Participant cap | None | None; cap; waitlist | No |

## 10. Comments, naming, reveal

| Setting | Default | Options | Lock at commit |
|---|---|---|---|
| Comments | Note + questionnaire | Note + questionnaire; note only; questionnaire only; none | Yes |
| Comment visibility | Host + reveal | Host + reveal; host only; next holder (not v1) | Yes |
| Comment requirement | Optional | Optional; host can require questions | No |
| Final naming | Last holder at final submission | Last holder; previous submitter; host; code | Yes |
| Titles at reveal | Final only | Final; final + original; original; host | Yes |
| Reveal trigger | Host after release stream | Host after stream; scheduled; simultaneous | No |
| Reveal audience | Whole server | Whole server; participants; role | No |
| Reveal layout | Host-configurable | Per-track + pinned album; album + threads; single post | No |
| Comments at reveal | All, attributed | All attributed; all anonymous; host selects; private | No |
| Skips at reveal | Anonymous gaps | Anonymous gaps; named; hidden; host | No |
| Fun stats | On | On; minimal; off | No |
| Runbook export | Markdown + HTML | Markdown + HTML; PDF; JSON; none | No |
| Personal keepsake | On | On; final album only; host | No |
| Large export delivery | Host chooses; default host-mediated link | Host-mediated link; split DMs; Discord parts | No |
| Album order | Secret-shuffled | Secret-shuffled; host curated; submission order | No |

## 11. Commit-reveal, audit, privacy

| Setting | Default | Options | Lock at commit |
|---|---|---|---|
| Commit-reveal | On | On; audit only; off | Yes |
| Replans | No cap (D-043) | Each replan creates a new commitment and an audit entry | Yes |
| Verification file | `verify.txt` in the reveal | Participants check the seal with `sha256sum` and `/swap verify` (D-043) | Yes |
| Audit log | On | On | Yes |
| Mapping access | Host + full co-hosts | Host; co-hosts; per-person | No |
| Data location | User-chosen library | Library; app data | No |
| Backups | Auto + restore wizard | Auto; manual; off | No |
| Retention | Until host deletes/archives | Per-event; fixed period | No |
| Telemetry | Off | Off; opt-in diagnostics | No |
| Update | Off | Off; later | No |

## 12. Lock matrix

At schedule commit, these settings lock:

- mode, length, completeness target;
- group count, group sizes, rotation default, group visibility;
- schedule visibility, owner/title visibility, skip visibility;
- pack type, pack requirement, new-sample policy;
- allowlist, format-change policy, metadata scrub;
- claim-first, claim window, Ready gate, DM fallback, submission channel;
- deadline type, no-response fallback;
- join mode, late-seed policy, unseeded-signup policy, leave/rejoin policy;
- comments visibility and requirement, final naming, commit-reveal.

These settings stay changeable with an audit entry:

- release time and future offers;
- reminders and quiet hours;
- extensions;
- co-hosts;
- public progress detail;
- reveal layout and audience;
- backups, retention, diagnostics.

## 13. Presets

| Preset | Mode | Length | Groups | Deadline | Release | Notes |
|---|---|---|---|---|---|---|
| Classic Monsquaz | Classic | N (R1 + N-1) | 1 for ≤9; lookup above | 24h; final 48h | One/day | Hidden schedule, full anonymity |
| Fast Async | Async | N-1 handoffs | None (async has no groups) | 24h from claim | One/day | Per-person queue, claim-first |
| Large Group Swap | Classic | N (R1 + N-1) | Lookup, K≈T/4 | 24h; final 48h | One/day | Participants rotate; cross-pollination |
| Casual | Classic | 8 total | 1–2 | 48h | One/day | Shorter, relaxed |
| Custom | — | — | — | — | — | No preset |

The host can save a custom preset. The wizard must show a reset-to-recommended
action for each section.

## 14. Open settings questions

| # | Question | Default | Trigger |
|---|---|---|---|
| 1 | Exact retention period | Until host deletes; backups 90 days; logs 30 days | Before public release |
| 2 | Windows SmartScreen copy | Document + checksum | Before beta |
| 3 | Pack delivery channel | DM + pinned post | Before pack implementation |
| 4 | Async content tags | Optional later; no participant groups | After v1 |
| 5 | Expiry strike defaults | Closed by D-041: strike + cooldown preset; strikes reset on resume | Closed |
