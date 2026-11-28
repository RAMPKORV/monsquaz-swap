@v1 @deadlines
Feature: Deadlines, reminders, and extensions
  A deadline is a prompt for the host.
  A deadline is not a silent auto-advance.

  @pending
  Scenario: One classic round has one shared deadline
    Given the host opens an exchange round for 24 hours
    When the round opens
    Then every offer in the round expires at the same deadline
    And a claim at any time before the deadline uses the same edit deadline
    And the bot states the exact local time and the UTC time

  @pending
  Scenario: The final exchange round uses the final duration
    Given the host opens the last exchange round
    Then the round deadline is 48 hours after the open
    And every offer in that round uses that deadline

  @pending
  Scenario: The host opens at most one round at a time
    Given an open round with an active assignment
    When the host tries to open the next round
    Then the app refuses the open
    And the host must resolve the active assignment first

  @pending
  Scenario: The host moves a whole round deadline
    Given an open round with offered, claimed, and overdue assignments
    And one assignment has a later individual extension
    When the host moves the round deadline forward
    Then every offered, claimed, and overdue assignment follows the new deadline
    And overdue returns to claimed
    And the individual extension is preserved
    And the app re-arms every reminder
    And every holder receives one message
    And the app writes one audit entry

  @pending
  Scenario: A moved deadline does not reopen submitted work
    Given a submitted assignment in the open round
    When the host moves the round deadline
    Then the submitted assignment stays final
    And the host can reopen that assignment explicitly

  Scenario: A soft deadline does not move a track
    Given a claimed track whose deadline has passed
    When the deadline passes
    Then the app marks the assignment overdue
    And the bot notifies the host
    And the bot does not move the track
    And the host decides the next action

  @pending
  Scenario: Reminders follow the percentage ladder
    Given an assignment window of 24 hours
    When the window reaches 50 percent, 80 percent, and 95 percent
    Then the bot sends one reminder at each step
    And the bot sends a final prompt at the deadline
    And the bot sends each reminder as a separate message

  Scenario: Quiet hours delay a non-urgent reminder
    Given a participant in a timezone where it is 23:00
    And a reminder that is not the deadline prompt
    When the reminder time arrives
    Then the bot waits until 08:00 local time
    And the deadline does not change

  Scenario: The first extension auto-approves up to 6 hours
    Given a participant with 2 hours left on an assignment
    When the participant requests a 6-hour extension
    Then the app approves the extension
    And the deadline moves by 6 hours
    And the app records the extension

  @pending
  Scenario: A second extension needs the host
    Given a participant who already used one extension
    When the participant requests another extension
    Then the app sends the request to the host
    And the deadline does not move until the host approves

  @pending
  Scenario: A third extension is refused
    Given a participant who already used two extensions on one assignment
    When the participant requests another extension
    Then the app refuses the request
    And the app offers Skip or a host decision

  @pending
  Scenario: A classic claimed deadline miss counts as seen
    Given a participant in a classic event who claimed a track and did not submit
    When the deadline passes and the host marks the turn missed
    Then the app records the pair as seen
    And the track passes unchanged into the next round or a fill-in covers the turn
    And the app reports the missed edit in the gap report

  @pending
  Scenario: The deadline decision card offers clear actions
    Given a deadline decision card for a participant
    Then the host can extend the assignment by 1, 6, or 12 hours
    And the host can mark the turn missed
    And the host can keep waiting
    And every action records an audit entry
    And marking the turn missed records the pair as seen

  @pending
  Scenario: The extension decision card offers clear actions
    Given an extension request decision card for a participant
    Then the host can approve the request
    And the host can refuse the request
    And an approval moves the deadline
    And every action records an audit entry
