Feature: As a troubleshooting agent, I want to inspect and change Mouser from the command line
  An AI agent asked to diagnose or configure Mouser works through the command-line tool, the plain-text configuration file, and the diagnostic logs, with no GUI.

  Scenario: Story 9 scenario 1: status from the command line
    Given Mouser is running
    When the agent asks the command-line tool for status
    Then it reports the desk, its computers, their connection state, missing permissions, and recent problems, in both human-readable and machine-readable form

  Scenario: Story 9 scenario 2: settings via file or command line
    Given any setting available in the GUI
    When the agent changes it in the configuration file or through the command-line tool
    Then Mouser behaves as if it had been changed in the GUI, and the GUI shows the new value

  Scenario: Story 9 scenario 3: JSON log lines
    Given diagnostic logging is on
    When the agent reads the log file
    Then each line is one self-contained JSON record with a timestamp, the computer, and the event

  Scenario: Edge case: configuration file edited while running
    Given an agent edits the configuration file while Mouser is running
    When the edit is saved
    Then the change takes effect without restarting Mouser
