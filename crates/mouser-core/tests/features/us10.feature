Feature: As a desk owner, I want every computer on my desk to stay up to date without visiting each one
  When one computer on the desk is updated, the rest follow automatically, and an update never costs the owner their granted permissions.

  Scenario: Story 10 scenario 1: others follow a newer version
    Given one computer on the desk has a newer version than the others
    When the others notice it
    Then they update to that version without the owner visiting them, and the desk works afterwards

  Scenario: Story 10 scenario 1: the elected computer restarting does not cut the others off
    Given a desk where every computer updates automatically
    When the elected computer finds the release and restarts as the new version
    Then every computer installed it, the elected one restarting only after the others fetched it

  Scenario: Story 10 scenario 1: an update that failed is retried when the elected computer is back
    Given a desk where every computer updates automatically and one other computer's first install fails
    When the elected computer finds the release and restarts as the new version
    Then the computer whose install failed updates without the owner visiting it

  Scenario: Story 10 scenario 1: a computer updated over the LAN offers the release to older ones
    Given a desk where every computer has updated over the LAN
    When one that was not the elected computer restarts as the new version and an older one rejoins
    Then the older one updates from it

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
