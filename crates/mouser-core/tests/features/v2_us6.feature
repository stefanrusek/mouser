Feature: As a desk owner, I want Mouser to run without its window or tray on Windows and Linux
  Headless mode, with no window and no tray, is allowed on macOS, Windows and Linux.

  Scenario: Story 6 scenario 1: headless on Windows or Linux
    Given a Windows or Linux computer on a desk
    When the desk owner starts Mouser in headless mode
    Then no window or tray icon appears, the computer is on its desk, the desk user can cross to it, and other desk computers list it as connected

  Scenario: Story 6 scenario 2: Linux login screen limitation
    Given Mouser runs on a Linux computer
    When the desk owner opens its Health entry
    Then "What doesn't work here" states that Mouser cannot control this computer's login screen
