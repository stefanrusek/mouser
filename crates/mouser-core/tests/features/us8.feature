Feature: As a desk owner, I want Mouser to tell me what is wrong and how to fix it
  When something does not work, the owner should not have to guess. A health panel shows each computer's state in plain language, and notifications explain problems with a one-click fix where one exists.

  Scenario: Story 8 scenario 1: health panel contents
    Given the desk owner opens the health panel
    When it shows
    Then it lists every computer on the desk with whether it is connected, how responsive it is, any missing permissions, and the most recent problem in plain language

  Scenario: Story 8 scenario 2: one action opens the permission screen
    Given a computer is missing a required permission
    When the owner views it in the health panel
    Then a single action opens the system screen where that permission is granted

  Scenario: Story 8 scenario 3: notification for unfixable problems
    Given a problem occurs that Mouser could not fix by itself (for example a refused computer, an incompatible version, or a missing permission)
    When it occurs
    Then a notification states the problem and, where a fix exists, offers it as one action

  Scenario: Story 8 scenario 4: diagnostic logging checkbox
    Given the owner turns on diagnostic logging with one checkbox
    When Mouser runs afterwards
    Then diagnostic logs are written to a file whose location the owner can open from the same place
