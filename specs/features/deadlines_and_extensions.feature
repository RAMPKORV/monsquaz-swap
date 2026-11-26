@v1 @deadlines
Feature: Deadlines, reminders, and extensions
  A deadline is a prompt for the host.
  A deadline is not a silent auto-advance.

  Scenario: A soft deadline does not move a track
    Given a claimed track whose deadline has passed
    When the deadline passes
    Then the app marks the assignment overdue
    And the bot notifies the host
    And the bot does not move the track
    And the host decides the next action

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

  Scenario: A second extension needs the host
    Given a participant who already used one extension
    When the participant requests another extension
    Then the app sends the request to the host
    And the deadline does not move until the host approves

  Scenario: A third extension is refused
    Given a participant who already used two extensions on one assignment
    When the participant requests another extension
    Then the app refuses the request
    And the app offers Skip or a host decision

  Scenario: A claimed deadline miss counts as seen
    Given a participant who claimed a track and did not submit
    When the deadline passes and the host marks the turn missed
    Then the app records the pair as seen
    And the track returns to the pool or passes to the next holder
    And the app reports the missed edit in the gap report

  Scenario: The deadline decision card offers clear actions
    Given a deadline decision card for a participant
    Then the host can extend the assignment by 1, 6, or 12 hours
    And the host can mark the turn missed
    And the host can keep waiting
    And every action records an audit entry
    And marking the turn missed records the pair as seen

  Scenario: The extension decision card offers clear actions
    Given an extension request decision card for a participant
    Then the host can approve the request
    And the host can refuse the request
    And an approval moves the deadline
    And every action records an audit entry
