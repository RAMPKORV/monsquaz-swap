@v1 @join
Feature: Joining and seeding
  A participant joins by submitting a seed track.
  A signup without a seed does not make a participant.

  Scenario: A guild member joins by submitting a seed
    Given an open event in the join phase
    When a guild member submits an allowed seed file and accepts the rules
    Then the app records the member as seeded
    And the bot sends a receipt with the file hash and size
    And the seed is not visible to other participants

  Scenario: A signup without a seed is provisional
    Given a guild member who accepts the rules but submits no seed
    When the seed deadline passes
    Then the member does not become a participant
    And the app marks the member as a fill-in by default

  Scenario: The DM handshake gates joining
    Given a guild member who starts the join flow
    When the bot tests a direct message
    Then the member must receive the test message
    And the app stores the DM channel ID

  Scenario: Blocked DMs use a private thread
    Given a guild member who cannot receive direct messages
    When the join flow tests direct messages
    Then the bot offers a host-approved private thread
    And the host sees the member on the delivery-risk list
    And the member cannot join until the fallback is approved

  Scenario: Signup collects scheduling inputs
    Given a guild member who joins
    Then the app records the member's timezone
    And the app records the maximum handoff budget
    And the app records optional away dates
    And the app records notification preferences

  Scenario: The host locks the roster
    Given an event with 8 seeded participants and 2 unseeded signups
    When the host locks the roster
    Then the 8 seeded participants become active
    And the 2 unseeded signups become fill-ins
    And no new seeds are accepted

  Scenario: Late seeds are closed after Round 1 by default
    Given an event whose Round 1 has started
    When a new guild member tries to join by submitting a seed
    Then the bot refuses the seed
    And the bot offers the fill-in path
    And the host can open a late-seed window

  Scenario: A participant leaves while holding a track
    Given a participant who claimed a track
    When the participant leaves
    Then the participant must submit or skip the current track first
    And the app preserves past credits
    And the app removes future assignments
    And the app alerts the host if the track is left unsubmitted

  Scenario: A leaver can return as a fill-in
    Given a participant who left the event
    When the participant asks to return
    Then the app allows a fill-in return by default
    And the app does not assign a new personal track

  Scenario: A rejoin with replan needs the event option
    Given a host who enabled rejoin-with-replan
    When a leaver asks to return
    Then the app creates a new plan version
    And the app re-seals the schedule
    And the app shows the change in the audit log

  Scenario: The host pack is required with a review
    Given an event with a host pack
    When a participant submits a seed that adds an unknown sample
    Then the app flags the seed in the pack report
    And the app does not reject the seed automatically
    And the host can accept or reject the seed
