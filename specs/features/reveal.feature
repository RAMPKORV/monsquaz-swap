@v1 @reveal
Feature: Reveal and album
  The host triggers the reveal after the release stream.
  The whole server can see the reveal.

  Scenario: The host triggers the reveal
    Given an event whose assignments are all resolved
    When the host finishes the release stream
    And the host presses Reveal
    Then the bot publishes the album and credits to the reveal channel
    And the publish is atomic
    And the bot does not publish a partial reveal

  Scenario: The reveal is refused while work is pending
    Given an assignment that is still in progress
    When the host presses Reveal
    Then the app refuses the reveal
    And the app lists the blocking assignments
    And the host can resolve or explicitly mark each gap

  Scenario: Credits show the full history
    Given a track with a seed and three edits
    When the reveal publishes the credits
    Then the credits show the seed owner
    And the credits show each editor in order
    And the credits mark each fill-in
    And the credits show the round for each edit

  Scenario: Comments are published with attribution
    Given comments and questionnaire answers for a track
    When the reveal publishes the track
    Then the comments appear with the contributor names
    And the host can redact an item before publish
    And a redacted item does not appear

  Scenario: Skips show as anonymous gaps by default
    Given a track with a skipped turn
    When the reveal publishes the track
    Then the lineage shows a "no edit" gap
    And the gap does not name the person who skipped
    And the host can choose a different skip display for the event

  Scenario: The album order is secret-shuffled
    Given tracks with different submission times
    When the app builds the album order
    Then the order uses HMAC(track code, event secret)
    And the order does not use submission time

  Scenario: The presenter shows the lineage and the audio
    Given a completed track
    When the host opens the presenter
    Then the presenter shows the version timeline
    And the presenter can play the seed version
    And the presenter can play the final version
    And the presenter shows the credits
    And the presenter hides the mapping in stream-safe mode

  Scenario: The presenter uses opaque audio URLs
    Given a track with a module seed and a final version
    When the host opens the presenter
    Then the audio control receives an opaque preview URL
    And the UI does not receive file bytes or absolute paths
    And the app caches the rendered preview

  Scenario: The host records a comment before the reveal
    Given a track with no comments
    When the host adds a comment for a participant
    Then the comment appears with the participant name
    And the host can redact the comment
    And a redacted comment does not appear in the presenter

  Scenario: Stream-safe mode hides the mapping
    Given the presenter shows a completed track
    When the host turns on stream-safe mode
    Then the presenter shows role labels instead of names
    And the presenter hides the host tools
    And the album, credits, comments, and audio stay visible

  Scenario: The app exports the album package
    Given a completed event
    When the host exports the album
    Then the app writes a ZIP with the final files
    And the ZIP contains the credits, manifest, cover, and README
    And the app writes a Markdown and HTML runbook
    And the app writes a personal keepsake for each participant

  Scenario: A large export uses the host choice
    Given an album ZIP larger than 20 MiB
    When the host exports the album
    Then the app writes the ZIP locally
    And the host chooses a neutral link or split DMs
    And the bot does not attempt a direct ZIP upload above the cap

  Scenario: The schedule is disclosed at the reveal
    Given a hidden schedule
    When the host publishes the Discord reveal
    Then the app publishes the full assignment matrix
    And the app publishes the comments
    And the host can choose to delay the schedule disclosure

  Scenario: The reveal verifies the commitment
    Given a published commitment and a revealed schedule
    When the host runs the verify action
    Then the app recomputes the commitment
    And the app reports a match or a mismatch
    And the app does not publish file hashes

  Scenario: The last holder names the final track
    Given the final holder of a track
    When the final submission is accepted
    Then the bot asks for the final title
    And the app stores the title for the reveal

  Scenario: The previous submitter names the final track when the final holder does not
    Given a final track without a title
    When the final holder does not provide a title
    Then the bot asks the previous submitter for the title
    And the previous submitter can use /swap title with the track code
    And the app records the fallback in the audit log
    And the host can set the title before the reveal
    And the app keeps the track code until a title is set
