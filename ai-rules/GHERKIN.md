# Gherkin style

Feature files live in `specs/features/`. They are the human-readable contract
and will be executable with `cucumber-rs` (step definitions in
`crates/swap-acceptance`). Write them in domain language, not UI language.

Write every feature and scenario in **ASD-STE100 Simplified Technical English**
(see `ai-rules/STE.md`). Use short sentences, the active voice, one action per
step, and the exact terms in `ai-rules/DOMAIN.md`. Do not use `should`; use
`must` or `can`.

## Organization

- One feature per domain area: scheduling, async queue, join/seed, delivery,
  fill-ins, deadlines, anonymity, files, groups, reveal, ops, privacy, security.
- One scenario = one observable behaviour. Cover the happy path plus the error,
  permission, duplicate, boundary, and retry paths that matter.
- Tags: `@pending`, `@v1`, `@scheduler`, `@async`, `@anonymity`, `@delivery`, `@files`,
  `@groups`, `@reveal`, `@ops`, `@privacy`, `@security`, `@simulation`.
- Use `Scenario Outline` only for genuine data tables; avoid brittle examples.

## Form

```gherkin
@v1 @scheduler
Feature: Anonymous scheduling
  A participant never works on the same track twice and never receives their own track.

  Scenario: Every active participant receives exactly one unseen track
    Given a locked roster of 8 participants, each with one seeded track
    And a classic schedule of 7 swap rounds
    When the host generates and commits the schedule
    Then every participant has exactly one assignment in each round
    And no participant is assigned their own track
    And no participant is assigned the same track twice

  Scenario: An infeasible schedule is refused
    Given a locked roster where every eligible holder has already seen a track
    When the host tries to start the next round
    Then the schedule is refused with the conflicted tracks and people
    And no offer or file is sent
```

## Rules

- Given = state, When = action, Then = observable result. Use `And`/`But` for
  continuation; avoid `should`.
- No implementation nouns: no SQL, function names, CSS selectors, or Discord
  API fields unless the behaviour is about Discord.
- Name the actor when it matters: host, participant, fill-in, bot.
- Include the unwanted path: expired claim, DM closed, duplicate upload, corrupt
  file, bot offline, no eligible fill-in, infeasible group, late seed.
- Use absolute time only when testing timezones; otherwise say "before the
  deadline" or "after the claim window".
- Never assert on exact DM prose; assert on the decision/effect. Copy tests are
  separate snapshots.
- A reader must be able to read one feature file in one sitting.

## Executable features

`crates/swap-acceptance/tests/cucumber.rs` runs
`specs/features/core_invariants.feature` with cucumber-rs:

```bash
cargo test -p swap-acceptance --test cucumber
```

Step definitions call app services with a fake `DiscordPort`, a fake clock, and
an in-memory SQLite database. They must not touch the network.

## Traceability (D-038)

Every scenario must have a test that proves it, or the `@pending` tag.

- A test declares each scenario that it proves with a comment directly above
  the test (above its attributes):

  ```rust
  // scenario: An expiry returns the track without a seen mark
  #[test]
  fn an_expiry_returns_the_track_and_blocks_a_late_claim() {
  ```

  ```ts
  // scenario: A complete schedule has no valid single-cell swap
  it("explains when no valid swap exists", () => {
  ```

- The comment text is the exact scenario name. One test can prove more than
  one scenario. One scenario can have more than one test.
- Map a test only when it sets up the `Given` state, does the `When` action,
  and asserts the `Then` results. A test that only touches related code does
  not prove the scenario.
- A feature file that cucumber-rs runs counts as mapped in full.
- Tag a scenario `@pending` when no test proves it. Remove the tag in the PR
  that adds the test.
- Scenario names must be unique across `specs/features/`.

`node scripts/trace-scenarios.mjs` writes
`docs/generated/SCENARIO_COVERAGE.md`. The gate fails when:

- a scenario without `@pending` has no mapping;
- a `@pending` scenario has a mapping;
- a mapping names a scenario that does not exist;
- two scenarios have the same name.

A feature file can be exempt for a limited time through
`scripts/trace-scenarios.baseline.json`. Each entry needs a reason and an issue
or an expiry.

Use stable scenario names and tags. The hand-written PRD links requirements to
feature files; the tracker issue links to the scenario. Do not duplicate the
spec in three places — link.
