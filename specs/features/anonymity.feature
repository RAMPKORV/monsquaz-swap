@v1 @anonymity
Feature: Anonymity until reveal
  Participants must not learn who made what before the reveal.
  The host can see the mapping.

  Scenario: A participant view hides the owner
    Given a track with a known owner and contributors
    When a participant views the track
    Then the view shows the opaque track code and the assignment
    And the view does not show the owner
    And the view does not show earlier contributors
    And the view does not show the original title
    And the view does not show comments

  Scenario: The outbound file name hides the lineage
    Given a track version with a holder and a version number
    When the bot sends the file to the next participant
    Then the file name is the opaque track code plus the extension
    And the file name has no holder name
    And the file name has no version number
    And the file name has no original file name

  Scenario: The host store keeps a readable name
    Given a stored track version
    Then the host file name shows the version, the track code, and the holder
    And the host file name is not sent to participants

  Scenario: Public progress does not expose a person
    Given an active round
    When the bot posts public progress before the deadline
    Then the post shows the round state and the next release time
    And the post does not show a per-person submission count
    And the post does not name a late participant

  Scenario: A visible schedule shows assignments only
    Given an event with schedule visibility enabled
    When a participant opens the schedule
    Then the participant sees past and future assignments
    And the participant does not see owners
    And the participant does not see titles
    And the participant does not see comments

  Scenario: Comments are host and reveal only
    Given a submission with a comment
    When the next participant receives the track
    Then the next participant does not receive the comment
    And the host can read the comment
    And the comment can be published at the reveal

  Scenario: Skips stay private
    Given a participant who skipped a turn
    When another participant opens the event view
    Then the other participant does not see the skip
    And the host sees the skip
    When the reveal runs
    Then the skip appears as an anonymous gap by default

  Scenario: Stream-safe mode hides the mapping
    Given the host opens the reveal presenter
    When stream-safe mode is on
    Then the presenter hides the owner and contributor mapping
    And the presenter redacts Discord IDs
    And the host can switch back only behind a confirmation

  Scenario: The host participation disclosure is visible
    Given an event where the host also participates
    When a participant reads the rules
    Then the rules state that the host can see the mapping
    And the rules do not claim that the host is anonymous

  Scenario: A commitment does not publish a file hash
    Given a locked schedule
    When the bot publishes the commitment
    Then the commitment covers the schedule and the policy
    And the commitment does not contain a hash of any submitted file

  Scenario: The reveal order does not follow submission time
    Given submissions at different times
    When the reveal orders the tracks
    Then the order is HMAC-sorted by track code and event secret
    And the order is not the submission order
