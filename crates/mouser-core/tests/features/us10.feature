Feature: As a desk owner, I want every computer on my desk to stay up to date without visiting each one
  When one computer on the desk is updated, the rest follow automatically, and an update never costs the owner their granted permissions.

  Scenario: Story 10 scenario 1: others follow a newer version
    Given one computer on the desk has a newer version than the others
    When the others notice it
    Then they update to that version without the owner visiting them, and the desk works afterwards

  Scenario: Story 10 scenario 2: permissions survive an update
    Given a computer has been updated
    When Mouser starts again
    Then the operating-system permissions granted before the update are still granted

  Scenario: Story 10 scenario 3: one computer downloads, the rest update over the LAN
    Given a newer release has been published
    When a computer on the desk next checks for releases
    Then it downloads the release and the other desk computers update from it over the local network without downloading it themselves

  Scenario: Story 10 scenario 4: tray menu shows versions
    Given an update is available
    When the owner looks at the tray menu
    Then it shows the current and available versions
