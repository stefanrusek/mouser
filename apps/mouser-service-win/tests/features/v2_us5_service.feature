Feature: As a desk user, I want to control a Windows computer at its login screen, elevation prompts and administrator applications
  With Mouser installed, the service seam starts the session engine where the desk user needs it and keeps working across user switches.

  Scenario: Story 5 scenario 1: sign in at a started or locked computer
    Given Mouser is installed on a Windows computer that has just started or is locked
    When the desk user crosses to it
    Then they can sign in with the shared mouse and keyboard

  Scenario: Story 5 scenario 2: elevation prompt and user switching
    Given Mouser is installed on a Windows computer that shows a permission elevation prompt or a user-switching screen
    When the desk user crosses to it
    Then they can operate that screen, and after a switch the newly signed-in user's session is controllable

  Scenario: Story 5 scenario 3: Send Ctrl-Alt-Del
    Given Mouser is installed and the active computer runs Windows
    When the desk user chooses Send Ctrl-Alt-Del from the tray menu
    Then that computer responds as if Ctrl-Alt-Del had been pressed on its own keyboard; where that computer's policy prevents it, its Health entry states so under "What doesn't work here"

  Scenario: FR-013: uninstalling leaves nothing running
    Given Mouser is installed on a Windows computer
    When the desk owner uninstalls it
    Then no part of Mouser is left running on that computer
