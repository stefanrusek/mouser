Feature: As a desk user, I want the pointer to stay put when I don't mean to cross
  Aiming at a screen corner (menus, hot corners, window controls) or working in a full-screen application must not throw the pointer onto another computer. A deliberate double bump of the edge still crosses.

  Scenario: Story 5 scenario 1: corner blocking
    Given corner blocking is on for an edge
    When the desk user pushes the pointer into a corner of that edge
    Then it stays on the current display

  Scenario: Story 5 scenario 2: full-screen blocking
    Given a full-screen application is in front and full-screen blocking is on
    When the desk user moves the pointer past an edge
    Then it stays on the current display

  Scenario: Story 5 scenario 3: double bump crosses
    Given a crossing is blocked
    When the desk user bumps the same edge twice in quick succession
    Then the pointer crosses

  Scenario: Story 5 scenario 4: first-blocked-crossing explanation
    Given a crossing is blocked
    When it is the first blocked crossing since the desk user last saw this explanation
    Then a notification says what blocked it and how to cross anyway or change the setting

  Scenario: Story 5 scenario 5: per-edge corner blocking setting
    Given the desk owner opens an edge in the display arranger
    When they change its corner blocking
    Then the change applies to that edge

  Scenario: Accepted limitation: full-screen blocking unavailable on Wayland
    Given full-screen blocking (Story 5 sc. 2) is unavailable on Wayland
    When the desk user looks at the app on a Wayland computer
    Then the app states that full-screen blocking is unavailable
