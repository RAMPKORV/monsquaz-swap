@v1 @participation @visibility
Feature: Participation, availability, and schedule visibility
  The host chooses how much work each person receives.
  The host chooses what participants see.
  The seed is not an edit.

  Scenario: Strict participation requires enough handoff budget
    Given a classic event with 6 exchange rounds
    And a participant with a handoff budget of 4
    When the host generates a strict schedule
    Then the planner refuses the plan
    And the planner names the participant and the required budget
    And the host can raise the budget, bench the person, or choose flexible participation

  Scenario: Flexible participation honors each handoff cap
    Given a classic event with 6 exchange rounds
    And a participant with a handoff budget of 3
    When the host generates a flexible schedule
    Then the participant receives the first 3 exchange rounds
    And the last 3 rounds show empty cells
    And the planner reports 3 coverage gaps
    And the host can fill the gaps with a fill-in or a manual edit

  Scenario: An away range creates a planned gap
    Given a participant with an away range from 3 December 2026 18:00 to 5 December 2026 18:00
    And flexible participation
    When the host generates the schedule
    Then the planner skips the participant in the rounds that overlap the away range
    And the planner reports the gap
    And strict participation refuses the plan and asks for flexible participation

  Scenario: The host records an away range
    Given a participant on the roster
    When the host adds an away range with a start time, an end time, and a note
    Then the app stores the range in UTC
    And the app writes one audit entry
    And the participant row shows the away range

  Scenario: The default schedule shows only the caller's assignments
    Given an event with the default schedule visibility
    When a participant opens the schedule
    Then the participant sees only their own assignments
    And the participant does not see the full matrix
    And the participant does not see group names or the seed band

  Scenario: Open schedule discloses the loss of anonymity
    Given a host who chooses an open schedule
    Then the setup review shows a warning
    And the join instructions say that everyone can see the matrix, owners, names, titles, and comments
    And participants can see the full matrix after the schedule locks

  Scenario: A handoff cap below the plan is not silent
    Given a participant with a handoff cap
    When the planner removes cells to honor the cap
    Then the plan is a draft with warnings
    And the host must confirm the gaps before the schedule locks
