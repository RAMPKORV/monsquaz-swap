@v1 @scheduler
Feature: Classic anonymous scheduling
  The scheduler must protect two hard invariants.
  A participant must never receive their own track.
  A participant must never receive a track twice.

  Scenario: Every participant receives one unseen track per round
    Given a locked roster of 8 participants with one seed track each
    And a classic schedule with 7 swap rounds
    When the host generates the schedule
    And the host commits the schedule
    Then each round assigns one track to each active participant
    And each round assigns each track to at most one participant
    And no assignment gives a participant their own track
    And no participant receives the same track twice

  Scenario: A default classic event runs N-1 rounds
    Given a locked roster of 6 participants
    When the host creates a default classic event
    Then the schedule has 5 swap rounds
    And the planner reports that each track can reach every eligible participant

  Scenario: The planner warns about overcrowding
    Given a locked roster of 20 participants
    When the host opens the schedule planner
    Then the planner shows that each track can receive up to 20 contributors
    And the planner shows the projected event duration
    And the host can cap the number of rounds

  Scenario: An infeasible schedule is refused
    Given a round where every eligible participant has already seen the only available track
    When the host tries to start the round
    Then the planner refuses the round
    And the planner lists the conflicted tracks and participants
    And the bot sends no offer

  Scenario: The host previews and edits the schedule
    Given a generated classic schedule
    When the host changes one assignment
    Then the planner validates the change
    And the planner blocks a change that creates a repeat or an own-track assignment
    And the change is recorded in the audit log

  Scenario: A manual edit swaps the two people in the round
    Given a generated classic schedule with fewer rounds than the full exposure
    When the host changes one assignment
    Then the two people swap tracks in that round
    And each person keeps one assignment in the round
    And the plan version and commitment change

  Scenario: A complete schedule has no valid single-cell swap
    Given a generated classic schedule with full exposure
    When the host opens one assignment
    Then the planner reports that no valid swap exists
    And the host can replace the draft or shorten the plan

  Scenario: Locking the schedule publishes a commitment
    Given a validated classic schedule
    When the host locks the schedule
    Then the bot posts a commitment hash to the event channel
    And the commitment does not include any file hash
    And the schedule cannot change without a new plan version

  Scenario: A replan creates a new commitment
    Given a locked schedule
    When the host replans the remaining rounds with a reason
    Then the app shows a diff of the changed assignments
    And the app creates a new plan version
    And the bot posts a new commitment
    And the app records the replan in the audit log

  Scenario: A third replan is refused
    Given an event with two completed replans
    When the host tries a third replan
    Then the app refuses the replan
    And the host must start a new event
