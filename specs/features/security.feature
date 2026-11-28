@v1 @security
Feature: Security boundaries
  The bot token is a secret.
  A submitted file is untrusted.

  @pending
  Scenario: The token never reaches the UI or a log
    Given a configured bot token
    When the app runs a normal event
    Then the token is not in the UI
    And the token is not in a default log line
    And the token is not in SQLite
    And the token is not in a git file

  Scenario: The token is stored in the OS keychain
    Given a host who saves a token
    When the app stores the token
    Then the token is in the OS keychain
    And the app stores no plaintext copy

  Scenario: Linux without a secret service
    Given a Linux host without a secret service
    When the host saves a token
    Then the app refuses to store plaintext without consent
    And the app offers a re-entry option or an explicit less-secure fallback

  @pending
  Scenario: File bytes never cross the IPC boundary
    Given a stored file
    When the UI requests a preview or a file action
    Then the app gives the UI an opaque file ID
    And the app does not give the UI the file bytes
    And the app does not give the UI the absolute path

  Scenario: Path traversal is blocked
    Given a submission with a file name that contains parent directory segments
    When the app stores the file
    Then the app uses a generated path
    And the stored path stays inside the event library
    And the app rejects the unsafe name

  @pending
  Scenario: An executable is blocked
    Given a file whose magic bytes show an executable
    When a participant submits the file with a module extension
    Then the app quarantines the file
    And the app does not distribute the file
    And the host sees the quarantine reason

  @pending
  Scenario: An archive bomb is refused
    Given an archive with an unsafe expansion ratio or depth
    When a participant submits the archive
    Then the app refuses to extract it
    And the app reports the limit
    And the app stays responsive

  @pending
  Scenario: A parser crash does not crash the app
    Given a crafted module that stalls the parser
    When the app parses the module in the isolated process
    Then the parser times out
    And the app reports an error
    And the app does not exit
    And the host can quarantine the file

  @pending
  Scenario: The bot uses no privileged intents or self-bot
    Given a configured bot
    When the app connects to Discord
    Then the bot uses only the guilds and direct messages intents
    And the bot does not read message content in public channels
    And the app does not use a user token or a self-bot

  @pending
  Scenario: A co-host mapping view is audited
    Given a co-host with mapping access
    When the co-host opens the author mapping
    Then the app records the access time and actor
    And the host can review the access log
