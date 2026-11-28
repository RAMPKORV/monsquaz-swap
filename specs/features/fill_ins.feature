@v1 @fillins
Feature: Skips and fill-ins
  A fill-in covers a skipped or stranded turn.
  A fill-in must never receive a track twice.

  Scenario: The fill-in pool has three sources
    Given an event with dedicated fill-in volunteers
    And signed-up non-seeders at roster lock
    And a host-appointed eligible participant
    Then the app can offer a slot to any of them
    And the app records the source of the fill-in

  # Decided, building in MSW-083.
  Scenario: A fill-in offer uses the standard sequence
    Given a stranded track and eligible fill-ins
    When the bot offers the track
    Then the bot offers it to one fill-in at a time
    And the first accept wins
    And the bot waits up to 6 hours for a claim
    And an auto-chain makes at most 3 offers before the host decides
    And a host-ordered list has no size limit

  Scenario: A fill-in has never seen the track
    Given a fill-in who has already worked on track S7F3A
    When S7F3A needs a fill-in
    Then the app does not offer S7F3A to that fill-in
    And the app offers it to another eligible fill-in

  # Decided, building in MSW-080..MSW-083.
  Scenario: A tier-2 fill-in rescue is an extra turn
    Given a seeded participant with a maximum handoff budget of 4
    And the participant has completed 2 edits
    When the participant rescues a slot as a tier-2 fill-in
    Then the app records the rescue as an extra turn
    And the app does not count the rescue toward the budget
    And the app can still offer the next planned handoff

  Scenario: No eligible fill-in exists
    Given a skipped slot with no eligible fill-in
    When the fill-in offers end
    Then the classic mode passes the track unchanged to the next holder
    And the async mode returns the track to the pool
    And the app does not repeat a track to rescue the slot

  # Decided, building in MSW-080.
  Scenario: A fill-in rescue is credited
    Given a seeded participant who rescued a slot as a fill-in
    When the reveal runs
    Then the credits mark that turn as a fill-in rescue
    And the credits mark the participant's planned turns as plain credits
    And the participant is credited in the round where they worked

  Scenario: A final fill-in can name the track
    Given a fill-in who is the last editor of a track
    When the track reaches the final submission
    Then the bot asks the fill-in for the final title
    And the app stores the title for the reveal

  Scenario: A fill-in wins over a late original holder
    Given a fill-in who claimed a skipped slot
    When the original holder submits late
    Then the fill-in version remains current by default
    And the host can override the choice
    And the app records the override in the audit log
