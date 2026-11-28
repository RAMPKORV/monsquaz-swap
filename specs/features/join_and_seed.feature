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

  # D-051: a leave is immediate after one explicit confirmation. Only the
  # host can start a return, and only the person's confirm completes it.

  @leave
  Scenario: The leave command shows a warning first
    Given an active participant
    When the participant uses the leave command
    Then the bot answers with an ephemeral warning and a Confirm button
    And the app does not change the membership

  @leave
  Scenario: A DM with a file or a prompt carries a Leave button
    Given an active participant
    When the bot sends an offer, a track file, a reminder, or a deadline prompt
    Then the DM carries a Leave button

  @leave
  Scenario: The Leave button shows the same warning
    Given an active participant
    When the participant presses the Leave button on a DM
    Then the bot answers with an ephemeral warning and a Confirm button
    And the bot does not send a DM

  @leave
  Scenario: The warning states every consequence
    When the bot shows the leave warning
    Then the warning states that a held track is skipped now and returns unchanged
    And the warning states what happens to a seed that the participant owns
    And the warning states that past credits keep the name
    And the warning states that the bot sends no DMs unless the host asks the participant to return
    And the warning states that only the host can undo the leave

  @leave
  Scenario: A participant leaves while holding a track
    Given a participant who claimed a track
    When the participant confirms the leave within 15 minutes
    Then the app skips the track as seen
    And the track returns to the pool unchanged
    And the app records the participant as left

  @leave
  Scenario: A leave before a claim does not count as seen
    Given a participant with an offer that is not claimed
    When the participant confirms the leave
    Then the app skips the offer as not seen

  @leave
  Scenario: A leave starts the fill-in flow
    Given a participant who holds a track in an open round
    When the participant confirms the leave
    Then the app starts the fill-in flow of the event for the stranded slot
    And the app alerts the host

  @leave
  Scenario: An expired confirmation does nothing
    Given a leave warning that the bot showed more than 15 minutes ago
    When the participant presses Confirm
    Then the app does not change the membership
    And the bot answers that the confirmation expired

  @leave
  Scenario: A leaver gets no more event DMs
    Given a participant who left the event
    When the app sends offers, reminders, or prompts
    Then the app cancels every unsent DM to the leaver with an audit entry
    And the app sends no event DM to the leaver
    But the host can send one restore invitation

  @leave
  Scenario: A leaver gets an ephemeral answer to a command
    Given a participant who left the event
    When the participant uses a swap command
    Then the bot answers with an ephemeral message
    And the bot does not send a DM

  @leave
  Scenario: A leaver who sends a file gets one direct answer
    Given a participant who left the event
    When the participant sends a file to the bot
    Then the bot sends one answer to that message
    And the app does not store the file

  @leave
  Scenario: Future planned turns of a leaver become gaps
    Given a participant who left the event
    When the host opens the next round
    Then the app sends no offer for the planned turns of the leaver
    And the app does not make a new plan

  @leave
  Scenario: A leaver keeps past credits
    Given a participant who submitted an edit and then left the event
    When the host triggers the reveal
    Then the credits show the name of the leaver

  @leave
  Scenario: A seed stays when the owner leaves before the roster lock
    Given a seeded participant
    When the participant leaves before the roster lock
    And the host locks the roster and makes the plan
    Then the plan keeps the seed of the leaver
    And each active participant gets one track in each round

  @leave
  Scenario: The host marks a participant as left
    Given an active participant
    When the host marks the participant as left and confirms two times
    Then the app applies the same leave as the participant leave
    And the audit log names the host as the actor

  @leave
  Scenario: The host restore needs the consent of the person
    Given a participant who left the event
    When the host restores the participant and confirms two times
    Then the bot sends one DM with "Confirm" and "Decline" buttons
    And the participant stays left until the participant presses Confirm

  @leave
  Scenario: A confirmed restore starts at the next round
    Given a participant who pressed Confirm on the restore DM
    When the host opens the next round
    Then the participant gets the planned turns of that round
    And a turn that the leave skipped stays skipped

  @leave
  Scenario: A declined restore keeps the participant out
    Given a participant who pressed "Decline" on the restore DM
    Then the participant stays left
    And the bot sends no DMs unless the host asks again

  @leave
  Scenario: A restore invitation expires after 7 days
    Given a participant who left the event
    And the host asked the participant to return 7 days ago
    When the participant presses Confirm
    Then the participant stays left
    And the app records the expired restore
    And the host can ask again

  @leave
  Scenario: A restore in schedule review voids the draft plan
    Given an event in schedule review with a draft plan
    And a participant left before the roster locked
    When the host restores the participant and the participant confirms
    Then the draft plan is void
    And the host must generate the schedule again
    And the audit log records the change

  @pending
  Scenario: A rejoin with replan needs the event option
    Given a host who enabled rejoin-with-replan
    When a leaver asks to return
    Then the app creates a new plan version
    And the app re-seals the schedule
    And the app shows the change in the audit log


  @pending
  Scenario: A restore invitation expires after 7 days
    Given a person who left the event
    And a restore invitation that the bot sent 8 days ago
    When the person presses Confirm
    Then the app refuses the restore
    And the app shows Restore expired
    And the host can send a new invitation

  @pending
  Scenario: The host pack is required with a review
    Given an event with a host pack
    When a participant submits a seed that adds an unknown sample
    Then the app flags the seed in the pack report
    And the app does not reject the seed automatically
    And the host can accept or reject the seed
