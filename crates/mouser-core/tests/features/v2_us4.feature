Feature: As a desk owner, I want to tailor hotkeys, names and pointer behavior from the Mouser window
  The panic key, QuickJump and other hotkeys, computer names, the landing highlight and the Command/Control mapping can be changed without restarting Mouser.

  Scenario: Story 4 scenario 1: new panic key
    Given Settings is shown
    When the desk owner records a new key and number of presses for the panic key
    Then pressing that key that many times returns the pointer and keyboard to the computer the desk user is at, and the previous panic key no longer does

  Scenario: Story 4 scenario 2: new QuickJump combination
    Given Settings is shown
    When the desk owner records a new key combination for QuickJump by pressing it
    Then that combination performs the action on this computer and the previous one no longer does

  Scenario: Story 4 scenario 2: new targeted paste combination
    Given Settings is shown
    When the desk owner records a new key combination for targeted paste by pressing it
    Then that combination performs the action on this computer and the previous one no longer does

  Scenario: Story 4 scenario 3: friendly computer name
    Given a desk computer
    When the desk owner gives it a friendly name from any desk computer's window
    Then every desk computer shows that name wherever it names that computer, including the arranger, Health, notifications and the command-line tool's status

  Scenario: Story 4 scenario 4: landing highlight off
    Given the landing highlight is turned off
    When the pointer crosses to another computer
    Then no highlight is shown where it lands

  Scenario: Story 4 scenario 5: Command as Control on
    Given "Use Command as Control on Windows and Linux" is on
    When the desk user at a Mac presses Command with another key on a Windows or Linux computer
    Then it acts as Control with that key

  Scenario: Story 4 scenario 6: Command as Control off
    Given "Use Command as Control on Windows and Linux" is off
    When the desk user at a Mac presses Command or Control with another key on a Windows or Linux computer
    Then Command acts as the Windows key and Control acts as Control

  Scenario: Edge case: a computer is renamed while another desk computer is offline
    Given a computer is renamed while another desk computer is offline
    When that desk computer reconnects
    Then that computer shows the new name once it reconnects
