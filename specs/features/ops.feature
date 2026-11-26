@v1 @ops
Feature: Host operations and recovery
  A non-technical host can run an event.
  The bot and the local data survive normal failures.

  Scenario: The host completes the setup wizard
    Given a new installation
    When the host completes the setup wizard
    Then the app validates the bot token
    And the app shows the bot identity
    And the host invites the bot to the server
    And the host maps the channels and roles
    And the bot sends a test direct message
    And the app creates a sample event

  Scenario: The bot token is never shown again
    Given a saved bot token
    When the host opens Settings
    Then the app shows only that a token is configured
    And the app does not show the token
    And the app does not show the last characters of the token

  Scenario: The host maps the announcement and reveal channels
    Given a valid bot token and a guild
    When the host selects the announcement channel and the reveal channel
    Then the app stores both channel ids
    And the app refuses the same channel twice
    And the app refuses a channel that the bot cannot see

  Scenario: The bot registers the guild command
    Given a valid bot token and a guild
    When the bot connects to Discord
    Then the app registers the swap command in that guild
    And the command contains the participant subcommands
    And the command contains the host subcommands
    And the app reports the bot as online after Discord sends ready

  Scenario: Closing the window keeps the bot online
    Given an app with an online bot
    When the host closes the window
    Then the app stays in the tray
    And the bot stays online
    When the host chooses Quit
    Then the app stops the bot
    And the app closes the database

  Scenario: A second instance is refused
    Given a running app instance
    When the host starts a second instance
    Then the second instance exits
    And the first instance stays in control
    And no second bot session starts

  Scenario: A crash does not duplicate a send
    Given an assignment that was committed but not sent
    When the app crashes and restarts
    Then the app retries the send once
    And the app does not send a duplicate message
    And the app records the recovery

  Scenario: The host restarts during an active event
    Given an active event with committed assignments
    When the host closes the app and starts it again
    Then the event state and the seen ledger are unchanged
    And the outbox contains every unacknowledged effect
    And the app retries each unacknowledged effect
    And the app never loses a committed effect

  Scenario: A failed command does not change state
    Given a command that changes an assignment and enqueues an effect
    When the command fails before it commits
    Then the assignment keeps its previous state
    And the outbox contains no effect for the failed command

  Scenario: A backup can be restored
    Given an automatic backup from before a transition
    When the host restores the backup
    Then the app checks the backup integrity
    And the app refuses a backup that is older than the current ledger
    And the host confirms the restore
    And the app records the restore

  Scenario: The audit log records an override
    Given an active event
    When the host reassigns a track with a reason
    Then the audit log records the actor, time, before state, after state, and reason
    And the log entry cannot be edited
    And the host can export the log

  Scenario: A bot outage is visible
    Given an online bot
    When the bot loses its gateway connection
    Then the app shows the bot as offline
    And the app shows the last successful connection time
    And the app warns that commands and offers are paused
    And the app reconnects with backoff

  Scenario: A DM sent during an outage is recovered
    Given a bot outage
    And a participant who sends a file to the bot during the outage
    When the bot reconnects
    Then the bot reads the DM channel after the last seen message
    And the app stores the file once
    And the app sends a receipt

  Scenario: The host runs a dry run
    Given a draft event
    When the host starts a dry run
    Then the app uses fake participants and a fake clock
    And the app does not contact Discord
    And the app injects one late submission and one skip
    And the app reports the result

  Scenario: The event library keeps a past event
    Given a completed event
    When the host opens the event library
    Then the host can open the archive
    And the host can clone the settings into a new event
    And the host can export the event bundle
