@v1 @privacy
Feature: Privacy and data control
  The app collects only what the event needs.
  The host is the data controller for the event.

  @pending
  Scenario: Joining records consent
    Given a guild member who joins an event
    When the member submits the join form
    Then the member accepts the event rules and the privacy note
    And the app records the consent time
    And the app records only the data needed for the event

  Scenario: A participant exports their data
    Given a participant with assignments and files
    When the participant requests an export
    Then the app produces an export with the participant's records
    And the export includes the files the participant submitted
    And the export does not include another participant's private data

  Scenario: A participant deletes their data
    Given a participant who asks for deletion
    When the host processes the deletion
    Then the app removes direct identifiers from the active event
    And the app keeps a hash-only audit record where needed
    And the app reports what it removed

  @pending
  Scenario: Telemetry is off by default
    Given a new installation
    When the app starts
    Then the app sends no telemetry
    And the app sends no crash report
    And the app records only local logs

  @pending
  Scenario: Diagnostics are redacted
    Given a host who exports diagnostics
    When the app builds the diagnostics bundle
    Then the bundle contains logs and system information
    And the bundle contains no bot token
    And the bundle contains no author mapping
    And the bundle contains no absolute participant paths

  @pending
  Scenario: Backups expire
    Given an automatic backup schedule
    When a backup is older than the retention limit
    Then the app deletes the old backup
    And the app keeps the current event data
    And the host can see the backup age

  @pending
  Scenario: The album license is stated
    Given a completed event
    When the host exports the album
    Then the package states the album license
    And the package credits the participants
    And the package states that participants keep authorship

  @pending
  Scenario: A display name change does not change identity
    Given a participant who changes their Discord display name
    When the app updates the credit name
    Then the app keeps the Discord user ID as the identity
    And the app asks the host to confirm the new credit name
    And the audit log records the change
