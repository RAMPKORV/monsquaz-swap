@v1 @groups
Feature: Classic group swaps
  Groups apply to classic mode only.
  Async mode uses one global queue.

  Scenario: The host chooses a schedule strategy
    Given a classic event with a seeded track for every participant
    When the host selects the balanced, random matching, or cyclic strategy
    Then the plan contains no repeat and no own track
    And the plan is the same for the same input and seed
    And the balanced strategy does not increase repeated handoffs

  Scenario: The planner suggests groups
    Given a classic event with 20 expected participants
    When the host opens the event wizard
    Then the planner suggests 4 groups of 5 participants
    And the planner shows the projected fun score and guardrails
    And the host can accept or override the suggestion

  Scenario: Group sizes stay in the soft bounds
    Given a plan with a group of 3 participants
    When the host previews the plan
    Then the planner warns that the group is smaller than 5
    And the host can override the warning
    And the planner does not block the plan unless another hard invariant fails

  Scenario: Participants rotate between groups
    Given a classic event with 2 groups and a 4-round phase
    When the phase ends
    Then the planner rotates participants to the other group
    And the tracks stay in their original group
    And no participant receives a track they have seen

  Scenario: The swap cadence targets 3 to 5 phases
    Given a classic event with 8 rounds
    When the host uses the default groups
    Then the planner sets K to 2
    And the plan has 4 phases

  Scenario: Exhausted swap warns when it cannot fire
    Given a classic event with 8 rounds and a group of 10 participants
    When the host selects "swap when exhausted"
    Then the planner warns that the group cannot exhaust before the event ends
    And the planner suggests a K-round swap

  Scenario: An infeasible phase falls back to the global pool
    Given a group that cannot fill the next round
    When the planner validates the phase
    Then the planner uses the global pool for that phase
    And the planner preserves the seen ledger
    And the planner notifies the host
    And the next phase restores groups

  Scenario: Unequal groups get a warning
    Given a plan with group sizes 12 and 5
    When the host previews the plan
    Then the planner warns about the size ratio
    And the planner suggests a rebalance
    And the host can override

  Scenario: Hidden group membership
    Given a classic event with groups
    When a participant opens the season view
    Then the participant sees the current assignment
    And the participant does not see a group name or other groups

  Scenario: Merge is a repair, not a routine phase
    Given a group phase that becomes infeasible after two participants leave
    When the planner repairs the phase
    Then the planner can merge or re-split groups
    And the planner records the repair
    And the planner does not merge groups in a routine phase

  Scenario: The host sees the fun score
    Given a generated classic plan
    When the host opens the planner
    Then the host sees the projected fun score and guardrails

  Scenario: The participant does not see the fun score
    Given a generated classic plan
    When a participant opens the season view
    Then the participant does not see the fun score
