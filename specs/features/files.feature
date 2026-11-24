@v1 @files
Feature: Files, packs, and metadata
  The app must accept only allowed files.
  The app must not trust a file name or its metadata.

  Scenario: The allowlist blocks a forbidden type
    Given an event whose allowlist contains modules, audio, and zip
    When a participant submits an executable file
    Then the app rejects the file
    And the bot explains the allowed types
    And the app does not store the file as a track version

  Scenario: A format change is refused
    Given a track whose current format is IT
    When a participant submits an XM file
    Then the app rejects the format change
    And the bot explains that the original format must stay
    And the host can change the event rule for future events

  Scenario: The pack check reports an unknown sample
    Given a host pack with known sample hashes
    When a seed contains an unknown sample
    Then the pack report flags the sample
    And the app does not reject the seed automatically
    And the host can accept the seed
    And the host can reject the seed with a reason

  Scenario: Added instruments and altered pack samples are allowed
    Given a seed that adds an instrument
    And a seed that slightly alters a pack sample
    When the app runs the pack check
    Then the report marks both seeds for review
    And the app does not reject either seed for that reason

  Scenario: An oversized file uses the fallback
    Given a file larger than the 20 MiB cap
    When a participant submits the file
    Then the bot refuses the direct upload
    And the bot offers a link download or a host import
    And the app records which fallback the host used

  Scenario: A corrupt file is rejected
    Given a truncated module file
    When a participant submits the file
    Then the app rejects the file with a clear reason
    And the app lets the participant replace the file before the deadline
    And the app stays responsive

  Scenario: Metadata is scrubbed and checked
    Given a module with a song message and sample names
    When the app prepares the file for delivery
    Then the app removes the message and the names where safe
    And the app renders the original and the scrubbed file
    And the app accepts the scrub only when the audio matches
    And the app keeps the untouched original in the host vault

  Scenario: The bot downloads an external link
    Given a participant who provides an allowed external link
    When the bot can download the file
    Then the app stores the bytes and the hash
    And the app uses the stored bytes for delivery
    When the bot cannot download the file
    Then the host can import the file manually
    And the app records the source

  Scenario: A stored file has a stable identity
    Given a stored file
    Then the app records its hash, size, extension, and storage path
    And the app gives the UI an opaque file ID
    And the UI never receives the absolute path

  Scenario: The host previews a module
    Given a stored module file
    When the host opens the preview
    Then the app renders a short preview
    And the preview does not change the stored file
    And the preview is not sent to participants
