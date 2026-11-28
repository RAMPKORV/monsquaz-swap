@v1 @scheduler
Feature: Core scheduling invariants
  The core scheduler protects the no-repeat and own-track rules.

  Scenario: A full classic schedule has no repeats or own tracks
    Given a locked roster of 8 participants with one seed track each
    And a classic schedule with 7 swap rounds
    When the host generates the schedule
    Then each round assigns one track to each active participant
    And each round assigns each track to at most one participant
    And no assignment gives a participant their own track
    And no participant receives the same track twice

  Scenario: Too many rounds are refused
    Given a locked roster of 4 participants with one seed track each
    And a classic schedule with 4 swap rounds
    When the host generates the schedule
    Then the scheduler refuses the request
    And the error names the maximum of 3 rounds

  Scenario: Seen touches follow the claim rule
    Given an empty seen ledger
    When a participant claims a track
    And the participant submits the track
    And another participant skips after claiming a track
    And a third participant skips before claiming a track
    And an offer to a fourth participant expires
    Then the claim pair is seen
    And the submit pair is seen
    And the skip-after-claim pair is seen
    And the skip-before-claim pair is not seen
    And the expired-offer pair is not seen

  Scenario: An offer is refused for a seen pair
    Given a participant who claimed track S7F3A
    When any offer path tries to offer S7F3A to that participant again
    Then the app refuses the offer with a typed error
    And the app sends no offer

  Scenario: A second seen record for one pair fails
    Given a seen pair from one assignment
    When a second assignment records the same pair as seen
    Then the store refuses the record
    And a retry of the first assignment changes nothing

  Scenario Outline: The database refuses a scheduling invariant violation
    Given a direct write to the database
    When the write <breaks>
    Then the database refuses the write

    Examples:
      | breaks                                   |
      | gives a participant their own track      |
      | gives a track a second active holder     |
      | gives a participant a second active track |
      | records the same seen pair twice         |
