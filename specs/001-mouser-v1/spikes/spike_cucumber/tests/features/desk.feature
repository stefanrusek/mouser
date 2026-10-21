Feature: Put my computers on a desk (Story 1 probe)

  Scenario: A second computer joins after one-click approval
    Given a desk "Home" created on "mac-mini"
    When "win-laptop" asks to join and "mac-mini" approves
    Then the desk "Home" has 2 members
