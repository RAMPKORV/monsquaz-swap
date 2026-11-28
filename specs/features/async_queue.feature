@v1 @async
Feature: Async per-person queue
  Async mode uses one global queue.
  It does not use participant groups.

  @pending
  Scenario: An offer does not deliver a file
    Given a ready participant with no current track
    And an eligible track in the pool
    When the bot offers the track
    Then the participant receives the track code and the claim deadline
    And the participant does not receive the file
    And the seen ledger does not change

  @pending
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
    And with the default re-offer setting the app never offers that track to that participant again in this event

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

  @pending
  Scenario: The host starts and stops the queue explicitly
    Given a locked async roster that reaches the expected track count
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
    Given a daily release window at 18:00 in the host IANA zone
    When the first wave runs
    Then the bot sends the ready offers
    When the second wave runs two hours later
    Then the bot sends offers for tracks that became ready after the first wave

  Scenario: The release time keeps its wall-clock time across a DST change
    Given a daily release window at 18:00 in the host IANA zone
    When the host zone changes from summer time to standard time
    Then the first wave still runs at 18:00 in the host IANA zone

  Scenario: The release uses the stored host zone
    Given an event with the host IANA zone Europe/Stockholm
    When the host sets the release time to 18:00
    Then the first wave runs at 17:00 UTC in winter
    And the first wave runs at 16:00 UTC in summer

  Scenario: A release time in the spring-forward gap moves forward
    Given an event with the host IANA zone Europe/Stockholm
    And a release time of 02:30 on the spring-forward day
    When the app computes the first wave
    Then the wave runs at 03:30 in the host IANA zone
    And the wave runs at 01:30 UTC

  Scenario: A release time in the fall-back overlap uses the earlier offset
    Given an event with the host IANA zone Europe/Stockholm
    And a release time of 02:30 on the fall-back day
    When the app computes the first wave
    Then the wave uses the earlier offset
    And the wave runs at 00:30 UTC

  Scenario: A changed host zone moves the next release
    Given an event with the host IANA zone Europe/Stockholm
    When the host creates a new event with the zone America/New_York
    Then the next release uses the new zone

  Scenario: The second expiry pauses the participant and alerts the host
    Given an event with the strike and cooldown expiry preset
    And a participant with one expired offer
    When a second offer to the participant expires
    Then the app pauses the participant
    And the app alerts the host

  Scenario: A resume resets the expiry strikes
    Given a participant with one expired offer
    When the participant resumes
    Then the participant has zero expiry strikes
    And the participant has no cooldown

  Scenario: The queue skips a participant in cooldown and reports why
    Given a participant with an active expiry cooldown
    When the release wave runs
    Then the queue sends no offer to the participant
    And the queue reports the cooldown as the reason

  Scenario: The cooldown-only preset never pauses a participant
    Given an event with the cooldown-only expiry preset
    And a participant with one expired offer
    When a second offer to the participant expires
    Then the participant stays ready
    And the participant has a 24-hour cooldown

  Scenario: The host-review preset opens a decision without an automatic pause
    Given an event with the host-review expiry preset
    When an offer expires
    Then the app opens a host decision
    And the participant stays ready
    And the app sets no automatic cooldown

  Scenario: The none preset has no automatic effect
    Given an event with the none expiry preset
    When an offer expires
    Then the participant stays ready
    And the app sets no cooldown
    And the app pauses nobody

  Scenario: An expired pair can return after the cooldown
    Given an event with the after-cooldown re-offer setting
    And an expired pair
    When the cooldown ends and the release wave runs
    Then the app can offer the same track to the same participant
    And the queue reports the re-offer rule

  Scenario: The last-resort rule uses the pair only when nobody else can take it
    Given an event with the last-resort re-offer setting
    And an expired pair
    When another ready person can take the track
    Then the app does not offer the pair
    When no other person can take the track
    Then the app can offer the pair

  Scenario: The host decides a same-track re-offer
    Given an event with the host-decides re-offer setting
    When an offer expires
    Then the app opens a host decision
    When the host chooses to re-offer
    Then the app offers the same track to the same participant

  Scenario: A host re-offer is refused under the never rule
    Given an event with the never re-offer setting
    When the host tries to re-offer an expired pair
    Then the app refuses the re-offer

  Scenario: The default re-offer rule never returns an expired pair
    Given an event with the default re-offer setting
    And an expired pair
    When the release wave runs again
    Then the app does not offer the pair
    And the app keeps the expired pair for the record

  Scenario: A claim cancels the claim reminder
    Given an offer with a claim reminder
    When the participant claims
    Then the bot sends no claim reminder for that offer
    And the other offers keep their reminder

  Scenario: A disabled claim reminder sends nothing
    Given an event with no claim reminder
    When the claim window nearly ends
    Then the bot sends no claim reminder

  Scenario: The claim reminder must be before the claim window
    Given a claim window of 6 hours
    When the host sets a 6-hour claim reminder
    Then the app refuses the setting
    When the host sets a 5-hour claim reminder
    Then the app accepts the setting

  @pending
  Scenario: Exposure completeness is reported, not forced
    Given an async event with a 6-round cap and 20 participants
    When the event ends
    Then the app reports the exposed pairs and the unexposed pairs
    And the app lists each unexposed pair with a reason
    And the app does not re-offer a track forever to an unresponsive participant

  @pending
  Scenario: A small async event uses N-1 rounds
    Given an async event with 10 participants
    When the host creates the event with defaults
    Then the event uses N-1 rounds

  @pending
  Scenario: A large async event uses a capped length
    Given an async event with 20 participants
    When the host creates the event with defaults
    Then the event uses a 6 to 8 round cap
    And the planner shows the projected duration and coverage

  @pending
  Scenario: An async claimed deadline miss counts as seen
    Given a participant who claimed a track
    When the participant misses the edit deadline
    Then the app records the pair as seen
    And the track returns to the pool
    And the app reports the missed edit in the gap report

  @pending
  Scenario: The host deadline prompt does not auto-advance
    Given a claimed track whose deadline has passed
    When the deadline passes
    Then the bot sends the host a decision card
    And the app does not move the track
    And the host decides to extend, skip, or reassign

  @pending
  Scenario: A late seed can get a minimum editor quota
    Given a capped async event that allows late seeds
    When a late participant seeds a track
    Then the app gives the late participant an extra handoff quota
    And the app gives the late track a minimum editor quota
    And the planner warns if it cannot meet the quota
