@v1 @ops
Feature: Host operations and recovery
  A non-technical host can run an event.
  The bot and the local data survive normal failures.

  @pending
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

  @pending
  Scenario: Closing the window keeps the bot online
    Given an app with an online bot
    When the host closes the window
    Then the app stays in the tray
    And the bot stays online
    When the host chooses Quit
    Then the app stops the bot
    And the app closes the database

  @pending
  Scenario: A second instance is refused
    Given a running app instance
    When the host starts a second instance
    Then the second instance exits
    And the first instance stays in control
    And no second bot session starts

  @pending
  Scenario: A crash after a send repeats that send
    Given an assignment that was committed and sent to Discord
    And the app stops before it records the send
    When the app restarts
    Then the app retries the send
    And the recipient can receive the same message again
    And the delivery row records the recovery attempt
    And the app does not repeat the state transition

  Scenario: A crash before a send does not lose the effect
    Given an assignment that was committed but not sent
    When the app crashes and restarts
    Then the app retries the send once
    And the recipient receives the message
    And the delivery row records the recovery

  @pending
  Scenario: The host restarts during an active event
    Given an active event with committed assignments
    When the host closes the app and starts it again
    Then the event state and the seen ledger are unchanged
    And the outbox contains every unacknowledged effect
    And the app retries each unacknowledged effect
    And the app never loses a committed effect

  @pending
  Scenario: A failed command does not change state
    Given a command that changes an assignment and enqueues an effect
    When the command fails before it commits
    Then the assignment keeps its previous state
    And the outbox contains no effect for the failed command

  @pending
  Scenario: A backup can be restored
    Given an automatic backup from before a transition
    When the host restores the backup
    Then the app checks the backup integrity
    And the app refuses a backup that is older than the current ledger
    And the host confirms the restore
    And the app records the restore

  Scenario: The app makes a backup before an event state transition
    Given a backup folder that the host selected
    And an event in the signup state
    When the host locks the roster
    Then the app writes a backup of the signup state before the event changes state
    And the backup contains a database snapshot and a manifest
    And the backup contains every file that the database refers to

  Scenario: A failed backup stops the transition
    Given a backup folder that the app cannot write to
    And an event in the signup state
    When the host locks the roster
    Then the app refuses the transition
    And the event stays in the signup state
    And the app tells the host that the backup failed

  Scenario: The app makes a daily backup
    Given a backup folder that the host selected
    And no daily backup from the last 24 hours
    When the daily backup timer runs
    Then the app writes a daily backup

  Scenario: The host makes a backup on request
    Given a backup folder that the host selected
    When the host selects back up now
    Then the app writes a manual backup

  Scenario: The host selects a backup folder
    Given an installation with no backup folder
    When the host selects a folder outside the app data folder
    Then the app stores the folder
    And the app shows the folder name
    And the app refuses a folder inside the app data folder

  Scenario: The app shows the backup history
    Given a backup folder with three complete backups
    When the host views the backup status
    Then the app shows each backup time in the event timezone
    And the app shows the backup kind and the event state

  Scenario: A backup copies each file one time
    Given a backup folder that contains a file from an earlier backup
    When the app writes a new backup
    Then the app does not copy that file again
    And the new manifest refers to that file

  Scenario: A partial backup never looks complete
    Given a backup that stops before the app writes the manifest
    When the host opens the restore wizard
    Then the app does not show the partial backup
    And the next backup removes the partial backup

  Scenario: Retention removes old backups
    Given 21 transition backups and 15 daily backups
    When the app writes a new backup
    Then the app keeps the newest 20 transition backups
    And the app keeps the newest 14 daily backups
    And the app removes each file that no kept backup refers to

  Scenario: Retention keeps the newest complete backup
    Given a backup folder where the newest backup is the only complete backup
    When retention runs
    Then the app keeps the newest complete backup

  Scenario: The restore wizard checks the backup
    Given a backup folder
    When the host selects a backup to restore
    Then the app checks the manifest format
    And the app checks the database hash
    And the app runs the database integrity check
    And the app checks that every file exists and matches its hash
    And the app shows the events, the event states, and the backup time

  Scenario: The restore wizard refuses a damaged backup
    Given a backup with a file that does not match its hash
    When the host selects the backup
    Then the app refuses the restore
    And the app does not change the current data

  Scenario: The restore wizard warns about an old backup
    Given a current database with audit entries after the backup time
    When the host selects the backup
    Then the app warns that the backup is older than the current data

  Scenario: The host must type a confirmation to restore
    Given a checked backup
    When the host types a word other than RESTORE
    Then the app refuses the restore
    And the app does not change the current data

  Scenario: A restore keeps the replaced database
    Given a current database and a checked backup
    When the host confirms the restore
    Then the app moves the current database to a pre-restore folder
    And the app installs the backup database and its files
    And the app records the restore in the audit log

  Scenario: The host enters the bot token after a restore
    Given a completed restore
    When the restore wizard finishes
    Then the app asks the host for the bot token
    And the app stores the token in the operating system keychain

  Scenario: A backup never contains the bot token
    Given a bot token in the keychain
    When the app writes a backup
    Then no file in the backup folder contains the bot token

  Scenario: A second machine refuses to start the bot
    Given a writer lock from another machine with a heartbeat less than 5 minutes old
    When the app tries to start the bot
    Then the app refuses to start the bot
    And the app tells the host that another machine runs the event

  Scenario: A stale writer lock does not block the bot
    Given a writer lock from another machine with a heartbeat 5 minutes old or older
    When the app tries to start the bot
    Then the app starts the bot
    And the app records its own machine in the writer lock

  Scenario: The host takes over the event
    Given a writer lock from another machine with a recent heartbeat
    When the host types TAKE OVER
    Then the app records this machine in the writer lock
    And the app records the takeover in the audit log
    And the app can start the bot

  Scenario: A machine that lost the writer lock stops the bot
    Given a running bot on this machine
    And a newer writer lock from another machine in the backup folder
    When the heartbeat timer runs
    Then the app stops the bot
    And the app tells the host that another machine took over

  Scenario: The host purges the backups
    Given a backup folder with backups
    When the host types PURGE
    Then the app removes every backup and every backup file from the folder

  @pending
  Scenario: The audit log records an override
    Given an active event
    When the host reassigns a track with a reason
    Then the audit log records the actor, time, before state, after state, and reason
    And the log entry cannot be edited
    And the host can export the log

  @pending
  Scenario: A bot outage is visible
    Given an online bot
    When the bot loses its gateway connection
    Then the app shows the bot as offline
    And the app shows the last successful connection time
    And the app warns that commands and offers are paused
    And the app reconnects with backoff

  @pending
  Scenario: A DM sent during an outage is recovered
    Given a bot outage
    And a participant who sends a file to the bot during the outage
    When the bot reconnects
    Then the bot reads the DM channel after the last seen message
    And the app stores the file once
    And the app sends a receipt

  @pending
  Scenario: The host runs a dry run
    Given a draft event
    When the host starts a dry run
    Then the app uses fake participants and a fake clock
    And the app does not contact Discord
    And the app injects one late submission and one skip
    And the app reports the result

  @pending
  Scenario: The event library keeps a past event
    Given a completed event
    When the host opens the event library
    Then the host can open the archive
    And the host can clone the settings into a new event
    And the host can export the event bundle
