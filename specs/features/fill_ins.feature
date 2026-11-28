@v1 @fillins
Feature: Skips and fill-ins
  A fill-in covers a skipped or stranded turn.
  A fill-in must never receive a track twice.

  Scenario: The fill-in pool has two tiers in classic mode
    Given a classic event with dedicated fill-in volunteers
    And signed-up non-seeders who became fill-ins at roster lock
    When a track needs a fill-in
    Then the app lists the dedicated fill-ins first
    And the app lists a seeded participant only when no dedicated fill-in is eligible

  Scenario: A seeded participant can take one extra turn
    Given a classic event with groups
    And no dedicated fill-in is eligible for track S7F3A
    And a seeded participant submitted their own turn in the current round
    And the plan never gives that participant track S7F3A
    When S7F3A needs a fill-in
    Then the app lists that participant as an extra turn
    And the extra turn does not count toward the handoff cap of that participant
    And the reveal marks the extra turn as a fill-in

  Scenario: A participant cannot take a second extra turn in the same round
    Given a seeded participant took one extra turn in the current round
    When another track needs a fill-in in the same round
    Then the app does not list that participant
    And the app refuses an appointment for that participant

  Scenario: A participant planned for the track is refused
    Given a seeded participant whom the plan gives track S7F3A in a later round
    And the participant submitted their own turn in the current round
    When the host tries to appoint that participant as the fill-in for S7F3A
    Then the app refuses the appointment
    And the app does not list that participant as a candidate

  Scenario: A participant who skipped is not eligible
    Given a seeded participant who skipped their own turn in the current round
    When a track needs a fill-in
    Then the app does not list that participant

  Scenario: A participant who receives every track is never a candidate
    Given a classic event where the plan gives each participant every other track
    When a track needs a fill-in
    Then the app lists no seeded participant

  Scenario: The host appoints a fill-in by default
    Given the fill-in flow is host_appoints
    And the host appoints an eligible fill-in for track S7F3A
    When the fill-in skips the offer before the claim
    Then the app opens a host decision for S7F3A
    And the decision shows the next eligible people

  Scenario: The automatic chain offers the slot to one person at a time
    Given the fill-in flow is auto_chain
    And a stranded track and four eligible fill-ins
    When the slot needs a fill-in
    Then the bot offers the track to the fill-in with the fewest edits
    And each offer gets the fill-in claim window to claim
    And the bot offers the track to the next fill-in after a skip or an expiry
    And the first claim wins
    And the bot makes at most 3 offers before the host decides

  Scenario: The automatic chain moves from tier 1 to tier 2
    Given the fill-in flow is auto_chain
    And two dedicated fill-ins and one eligible extra turn
    When both dedicated fill-ins skip the offer
    Then the bot offers the track to the eligible extra turn
    When the extra turn skips the offer
    Then the app opens a host decision

  Scenario: A chain offer has a separate claim and edit window
    Given the fill-in flow is auto_chain
    And the fill-in claim window is 6 hours
    And the fill-in edit window is 24 hours
    When the bot offers the track to a fill-in
    Then the offer expires 6 hours after the offer
    When the fill-in claims the offer 2 hours after the offer
    Then the edit is due 26 hours after the offer

  Scenario: The host orders a fill-in list
    Given the fill-in flow is host_list
    When a slot needs a fill-in
    Then the app opens a host decision
    When the host orders a list of many people
    Then the app accepts the list without a size limit
    And the bot offers the track down the list, one person at a time
    And each offer gets the fill-in claim window
    And the bot passes over a person who is not eligible now
    And the bot opens a host decision after the last person on the list

  Scenario: A fill-in chain respects quiet hours
    Given the next fill-in in a chain is inside their quiet hours
    When the chain reaches that person
    Then the bot waits until the quiet hours end
    And the bot sends the offer after the quiet hours

  Scenario: A fill-in chain survives a restart
    Given a fill-in chain with one open offer
    When the host app restarts
    And the offer expires
    Then the bot offers the track to the next person in the chain

  Scenario: A fill-in chain never moves a round
    Given a fill-in chain that waits for quiet hours
    When the host opens the next round
    Then the chain stops
    And the track passes unchanged

  Scenario: The host sets the fill-in due time
    Given the round deadline passed
    When the host appoints a fill-in without a due time
    Then the due time is 24 hours from the appointment
    When the host sets a due time in the past
    Then the app refuses the appointment
    When the host sets a due time before the round deadline
    And the fill-in claims the offer
    Then the due time stays as the host set it
    And the next round waits for the fill-in

  Scenario: A fill-in is refused for a cell that is not stranded
    Given a track that already has a submitted turn in the round
    When the host appoints a fill-in for that track
    Then the app refuses the appointment
    And the app does not offer the track

  Scenario: A fill-in has never seen the track
    Given a fill-in who has already worked on track S7F3A
    When S7F3A needs a fill-in
    Then the app does not offer S7F3A to that fill-in
    And the app offers it to another eligible fill-in

  Scenario: A dedicated fill-in edit counts toward the shared budget
    Given a dedicated fill-in with a maximum handoff budget of 4
    And the fill-in has completed 2 edits
    When the fill-in accepts a third edit
    Then the app records 3 completed handoffs
    And the app does not offer a fifth handoff

  Scenario: No eligible fill-in exists
    Given a skipped slot with no eligible fill-in
    When the fill-in offers end
    Then the classic mode passes the track unchanged to the next holder
    And the async mode returns the track to the pool
    And the app does not repeat a track to rescue the slot

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

  @pending
  Scenario: A fill-in wins over a late original holder
    Given a fill-in who claimed a skipped slot
    When the original holder submits late
    Then the fill-in version remains current by default
    And the host can override the choice
    And the app records the override in the audit log
