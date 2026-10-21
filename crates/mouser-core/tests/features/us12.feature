Feature: As a desk user, I want to control a computer that is at its login or system security screen
  The shared mouse and keyboard keep working when a computer is at its login window, after a cold boot, at a permission elevation prompt, or when switching users, and the user can send the secure attention sequence to a Windows computer.

  Scenario: Story 12 scenario 1: Mac login window
    Given a Mac on the desk is at its login window
    When the desk user crosses to it
    Then they can type their password and log in

  Scenario: Story 12 scenario 2: Windows login after start
    Given a Windows computer has just started and shows its login screen
    When the desk user crosses to it
    Then they can log in with the shared mouse and keyboard

  Scenario: Story 12 scenario 3: elevation prompt or user switching
    Given a Windows computer shows a permission elevation prompt or a user-switching screen
    When the desk user crosses to it
    Then they can operate that screen

  Scenario: Story 12 scenario 4: Send Ctrl-Alt-Del
    Given the active computer runs Windows
    When the desk user chooses Send Ctrl-Alt-Del from the tray menu
    Then that computer responds as if Ctrl-Alt-Del had been pressed on its own keyboard
