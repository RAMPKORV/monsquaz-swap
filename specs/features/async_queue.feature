@v1 @async
Feature: Async per-person queue
  Async mode uses one global queue.
  It does not use participant groups.

  Scenario: An offer does not deliver a file
    Given a ready participant with no current track
    And an eligible track in the pool
    When the bot offers the track
    Then the participant receives the track code and the claim deadline
    And the participant does not receive the file
    And the seen ledger does not change

  Scenario: A claim delivers the file and starts the deadline
    Given an offer with the default 24-hour claim window
    When the participant claims the offer
    Then the bot sends the file
    And the app records the pair as seen
    And the edit deadline starts at the claim time

  Scenario: An expiry returns the track without a seen mark
    Given an offer that the participant does not claim within 24 hours
    When the claim window ends
    Then the track returns to the pool
    And the app does not mark the pair as seen
    And the app starts a 24-hour cooldown
    And the app never offers that track to that participant again in this event

  Scenario: The bot reminds the participant to claim
    Given an offer with 12 hours left in the claim window
    When the reminder time arrives
    Then the bot sends one claim reminder
    And the bot does not send another claim reminder

  Scenario: The Ready gate prevents a loop
    Given a participant with a first expired offer
    Then the queue waits for the cooldown
    When a second offer expires
    Then the queue pauses the participant
    And the participant must use /swap resume to return
    And the queue sends no offer while the participant is paused

  Scenario: The host starts and stops the queue explicitly
    Given a locked async roster that reaches the expected song count
    When the host starts the queue
    Then the app freezes the claim and edit windows
    And the app opens the first release wave
    And no timer changes the roster or the policy

  Scenario: The global queue prefers social diversity
    Given an idle participant and two eligible tracks
    And one track has an owner the participant has worked with before
    And one track has an owner the participant has never worked with
    When the scheduler assigns the next track
    Then the scheduler prefers the unfamiliar track
    And the scheduler does not use participant group labels

  Scenario: A daily window runs two dispatch waves
    Given a daily release window at 18:00 local time
    When the first wave runs
    Then the bot sends the ready offers
    When the second wave runs two hours later
    Then the bot sends offers for tracks that became ready after the first wave

  Scenario: Exposure completeness is reported, not forced
    Given an async event with a 6-round cap and 20 participants
    When the event ends
    Then the app reports the exposed pairs and the unexposed pairs
    And the app lists each unexposed pair with a reason
    And the app does not re-offer a track forever to an unresponsive participant

  Scenario: A small async event uses N-1 rounds
    Given an async event with 10 participants
    When the host creates the event with defaults
    Then the event uses N-1 rounds

  Scenario: A large async event uses a capped length
    Given an async event with 20 participants
    When the host creates the event with defaults
    Then the event uses a 6 to 8 round cap
    And the planner shows the projected duration and coverage

  Scenario: A claimed deadline miss counts as seen
    Given a participant who claimed a track
    When the participant misses the edit deadline
    Then the app records the pair as seen
    And the track returns to the pool
    And the app reports the missed edit in the gap report

  Scenario: The host deadline prompt does not auto-advance
    Given a claimed track whose deadline has passed
    When the deadline passes
    Then the bot sends the host a decision card
    And the app does not move the track
    And the host decides to extend, skip, or reassign

  Scenario: A late seed can get a minimum editor quota
    Given a capped async event that allows late seeds
    When a late participant seeds a track
    Then the app gives the late participant an extra handoff quota
    And the app gives the late track a minimum editor quota
    And the planner warns if it cannot meet the quota
