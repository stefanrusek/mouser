Feature: As a troubleshooting agent, I want the diagnostic log to show what Mouser did and why
  Diagnostic logging stays opt-in, but when it is on, the log must be enough to reconstruct an incident, and the agent can gather every desk computer's log in one step.

  Scenario: Story 2 scenario 1: first record of every log file
    Given logging is on
    When the agent opens any log file, including one started when the previous file was rotated
    Then its first record states the Mouser version, the operating system and its version, the computer's id and name, and the desk

  Scenario: Story 2 scenario 2: control moves between computers
    Given logging is on
    When control moves between computers (a crossing, the panic key, QuickJump, a pause, or a lost link)
    Then a record names the computer control left and the computer it reached, how it moved, and the application in front on the computer that received control, including whether that application runs as administrator

  Scenario: Story 2 scenario 3: pointer hidden or shown
    Given logging is on
    When Mouser hides or shows the pointer on this computer
    Then a record says which, and why

  Scenario: Story 2 scenario 4: refused input
    Given logging is on
    When the operating system refuses an input Mouser passes on
    Then a record states the kind of input (key, mouse button, pointer move, scroll), the application in front and whether it runs as administrator

  Scenario: Story 2 scenario 5: desk link lost
    Given logging is on
    When a desk link is lost
    Then a record names the other computer and the reason (the other computer left or quit, became unreachable, runs an incompatible version, a fault on this computer, or this computer quit)

  Scenario: Story 2 scenario 6: fault inside Mouser
    Given logging is on
    When a fault occurs inside Mouser
    Then a record describes the fault and where it happened, and a later record shows Mouser carrying on

  Scenario: Story 2 scenario 7: display or arrangement change
    Given logging is on
    When a display is added, removed or rearranged on a computer, or the desk owner moves a computer or display in the arranger
    Then a record states which computer and display changed and how

  Scenario: Story 2 scenario 8: setting change
    Given logging is on
    When a setting changes
    Then a record names the setting, its old and new values, and where it was changed (app window, command-line tool, configuration file, or another desk computer)

  Scenario: Story 2 scenario 9: clipboard content moves
    Given logging is on
    When clipboard content moves between computers
    Then a record names the two computers and each representation's type and size

  Scenario: Story 2 scenario 10: collect the desk's logs
    Given a desk with several computers
    When the agent asks the command-line tool to collect the desk's logs
    Then it produces one bundle file on this computer whose contents follow FR-007

  Scenario: Edge case: the diagnostic log rotates in the middle of an incident
    Given the diagnostic log rotates in the middle of an incident
    When the agent opens the new file and collects the desk's logs
    Then the new file starts with the record of Story 2 scenario 1, and the bundle includes both files

  Scenario: Edge case: a desk computer is offline, or has logging off, when logs are collected
    Given a desk computer is offline, or has logging off, when logs are collected
    When the agent asks the command-line tool to collect the desk's logs
    Then the bundle lists that computer with that reason
