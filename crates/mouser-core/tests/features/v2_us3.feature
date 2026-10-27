Feature: As a desk owner, I want one Mouser window where I can find everything
  Problems appear only when the owner can act on them, once, under the computer they concern. The scenarios here are the ones core observes through status and the control API; the window-only scenarios are UI tests in the app lane.

  Scenario: Story 3 scenario 4: Desk shows each computer
    Given the Desk section is shown
    When the desk owner looks at it
    Then it shows the desk's name and, for each computer, its name, operating system, Mouser version, whether it is connected, which computer has the pointer, and how many problems need the owner's action, each count leading to that computer's entry in Health

  Scenario: Story 3 scenario 5: an actionable problem appears once
    Given a problem the desk owner can act on is raised for a computer
    When the owner opens Health
    Then the problem appears once, in that computer's entry and no other, with its fix as one action where a fix exists

  Scenario: Story 3 scenario 6: platform limitations listed once
    Given a computer has platform limitations
    When the desk owner opens Health
    Then they are listed once, under "What doesn't work here" in that computer's entry, and in no other computer's entry and not as a problem notice

  Scenario: Story 3 scenario 8: different Mouser versions
    Given a desk computer runs a different Mouser version from this one
    When the desk owner views Desk or Health
    Then both versions are shown next to that computer; if the versions cannot work together, it says which computer to update and offers the update as one action where this computer can perform it

  Scenario: Story 3 scenario 11: displays placed automatically
    Given a computer's displays were placed automatically
    When the desk owner views Displays
    Then a message names the computer, says its displays were placed automatically and should be checked against the real desk, and offers to keep the placement; the message goes away when the owner keeps or moves the placement

  Scenario: Story 3 scenario 14: icon shows connection state
    Given this computer is on a desk with other computers
    When the desk owner looks at the menu-bar or tray icon
    Then it shows whether this computer is connected to its desk-mates or unexpectedly disconnected from them, using a difference in shape or symbol as well as colour, alongside the active or paused state

  Scenario: Story 3 scenario 15: no unexpectedly-disconnected state without desk-mates
    Given this computer is the only computer on its desk, or is on no desk, or the desk owner removed the other computers or left the desk
    When the desk owner looks at the menu-bar or tray icon
    Then it does not show the unexpectedly-disconnected state

  Scenario: Edge case: a desk computer is mid-update to a newer version
    Given a desk computer is mid-update to a newer version
    When the desk owner views Desk or Health
    Then it shows both versions until the update completes

  Scenario: Edge case: a desk-mate goes offline while this computer is paused
    Given this computer is paused
    When a desk-mate goes offline
    Then the icon shows paused and the unexpectedly-disconnected state together
