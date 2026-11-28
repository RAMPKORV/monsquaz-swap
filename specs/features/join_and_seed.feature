@v1 @join
Feature: Joining and seeding
  A participant joins by submitting a seed track.
  A signup without a seed does not make a participant.

  @pending
  Scenario: A guild member joins by submitting a seed
    Given an open event in the join phase
    When a guild member submits an allowed seed file and accepts the rules
    Then the app records the member as seeded
    And the bot sends a receipt with the file hash and size
    And the seed is not visible to other participants

  @pending
  Scenario: A signup without a seed is provisional
    Given a guild member who accepts the rules but submits no seed
    When the seed deadline passes
    Then the member does not become a participant
    And the app marks the member as a fill-in by default

  @pending
  Scenario: The DM handshake gates joining
    Given a guild member who starts the join flow
    When the bot tests a direct message
    Then the member must receive the test message
    And the app stores the DM channel ID

  @pending
  Scenario: Blocked DMs use a private thread
    Given a guild member who cannot receive direct messages
    When the join flow tests direct messages
    Then the bot offers a host-approved private thread
    And the host sees the member on the delivery-risk list
    And the member cannot join until the fallback is approved

  @pending
  Scenario: Signup collects scheduling inputs
    Given a guild member who joins
    Then the app records the member's timezone
    And the app records the maximum handoff budget
    And the app records optional away dates
    And the app records notification preferences

  @pending
  Scenario: The host opens Round 1 with the pack
    Given an event draft with a required host pack
    When the host opens Round 1 and sets the seed deadline
    Then the app enters the seed round
    And the bot sends the pack to every pre-signup
    And the bot posts the seed instructions in the channel
    And a seed sent before the deadline becomes one track for that participant

  @pending
  Scenario: The host locks the roster
    Given an event with 8 seeded participants and 2 unseeded signups
    When the host closes Round 1 and locks the roster
    Then the 8 seeded participants become active
    And the 2 unseeded signups become fill-ins
    And Round 1 is closed
    And no new seeds are accepted until the host reopens Round 1

  Scenario: The host cannot lock fewer than two seeds
    Given an event with one seeded participant
    When the host closes Round 1
    Then the app refuses the close
    And Round 1 stays open
    And the host can invite another seed or cancel the event

  @pending
  Scenario: A late seed waits for host approval
    Given an event whose seed deadline has passed
    And Round 1 is still open
    When a new guild member submits a seed
    Then the app holds the seed
    And the bot tells the member that the host must approve it
    And the host can accept or reject the seed before the host closes Round 1

  @pending
  Scenario: A second seed replaces the first before the deadline
    Given a seeded participant
    And an open Round 1 before the seed deadline
    When the participant submits another seed
    Then the app keeps the same track code and owner
    And the app replaces version 1
    And the app keeps the old file in the host vault
    And the app records the replacement in the audit log

  @pending
  Scenario: A participant replaces a seed after the deadline
    Given a seeded participant
    And an open Round 1 after the seed deadline
    When the participant submits another seed
    Then the app refuses the replacement
    And the bot explains that the seed deadline has passed

  @pending
  Scenario: A participant leaves while holding a track
    Given a participant who claimed a track
    When the participant leaves
    Then the participant must submit or skip the current track first
    And the app preserves past credits
    And the app removes future assignments
    And the app alerts the host if the track is left unsubmitted

  @pending
  Scenario: A leaver can return as a fill-in
    Given a participant who left the event
    When the participant asks to return
    Then the app allows a fill-in return by default
    And the app does not assign a new personal track

  @pending
  Scenario: A rejoin with replan needs the event option
    Given a host who enabled rejoin-with-replan
    When a leaver asks to return
    Then the app creates a new plan version
    And the app re-seals the schedule
    And the app shows the change in the audit log

  @pending
  Scenario: The host pack is required with a review
    Given an event with a host pack
    When a participant submits a seed that adds an unknown sample
    Then the app flags the seed in the pack report
    And the app does not reject the seed automatically
    And the host can accept or reject the seed
