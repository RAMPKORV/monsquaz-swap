@v1 @delivery
Feature: Claim and submit delivery
  The bot is transport.
  The local store is the system of record.

  @pending
  Scenario: A claimed offer delivers one file
    Given a participant with an offer for track S7F3A
    When the participant claims the offer
    Then the bot sends one file named S7F3A with the current extension
    And the file contains no holder name and no version number
    And the app records the delivery time

  Scenario: A submission is stored before the CDN URL expires
    Given a participant who sends a file to the bot
    When the bot receives the attachment
    Then the app downloads the bytes immediately
    And the app computes the hash and size
    And the app stores the file in the event library
    And the bot sends a receipt

  Scenario: A resubmission keeps every version
    Given a participant who already submitted a valid file
    When the participant submits a new file before the deadline
    Then the new file becomes the current version
    And the app keeps the earlier version in the track history
    And the bot confirms which version is current

  @pending
  Scenario: A blocked DM uses a private thread
    Given a participant with a claimed track
    When the bot cannot send a direct message
    Then the bot creates or uses a host-approved private thread
    And the bot sends the file there
    And the host sees the delivery failure and the fallback

  @pending
  Scenario: A participant skips after claiming
    Given a participant who claimed a track
    When the participant taps Skip
    Then the track returns unchanged
    And the app records the pair as seen
    And the participant can leave the event
    And the host sees the skip

  @pending
  Scenario: An expired offer does not mark the pair seen
    Given a participant who does not claim an offer
    When the offer expires
    Then the track returns to the pool
    And the app does not record the pair as seen
    And with the default re-offer setting the app never offers that track to that participant again in this event
    And the app offers the track to another eligible participant

  @pending
  Scenario: A file above the cap uses the fallback
    Given a participant with a file larger than 20 MiB
    When the participant submits the file
    Then the bot rejects the attachment with an explanation
    And the participant can provide a link for the bot to download
    And the host can import the file if the bot cannot download it

  Scenario: A participant with no assignment cannot submit
    Given a participant with no active offer or claimed track
    When the participant sends a file to the bot
    Then the bot does not store the file as an assignment
    And the bot explains that no active track exists

  @pending
  Scenario: A receipt names the next action
    Given a valid submission
    When the app stores the file
    Then the bot sends a receipt
    And the receipt states the current version and the next expected action
    And the receipt does not name another participant

  @pending
  Scenario: The host opens the next round
    Given a classic event with every assignment resolved
    When the host opens the next round
    Then the app creates one offer for each planned participant
    And the app does not open another round
    And the app records the round in the audit log
