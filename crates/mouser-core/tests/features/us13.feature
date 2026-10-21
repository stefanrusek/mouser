Feature: As a desk owner, I want to run Mouser without installing it
  The owner can run Mouser from a folder or removable drive on a computer where they cannot or do not want to install software, and use it as a normal desk member.

  Scenario: Story 13 scenario 1: run from a removable drive
    Given the portable build is on a removable drive
    When the owner runs it on a computer
    Then it joins a desk and behaves as a desk member without anything being installed on that computer

  Scenario: Story 13 scenario 2: settings stay on the drive
    Given the portable build has been run
    When the owner removes the drive
    Then the computer keeps no Mouser settings, and the settings are on the drive the next time it runs

  Scenario: Accepted limitation: Windows portable mode
    Given portable mode on Windows cannot control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator
    When the owner looks at the app running in portable mode on Windows
    Then the app states that portable mode cannot control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator
