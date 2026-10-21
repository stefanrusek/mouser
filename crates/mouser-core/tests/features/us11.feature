Feature: As a desk user, I want to jump the pointer straight to any display
  One hotkey shows a large letter on every display across every computer on the desk; pressing a letter moves the pointer to the centre of that display. No setup is needed.

  Scenario: Story 11 scenario 1: letters appear on every display
    Given QuickJump has never been configured
    When the desk user presses the QuickJump hotkey
    Then a distinct letter appears on every display on the desk

  Scenario: Story 11 scenario 2: pressing a letter jumps
    Given the letters are showing
    When the desk user presses one
    Then the pointer moves to the centre of that display, that computer becomes the active computer, and the letters disappear

  Scenario: Story 11 scenario 3: Esc or another key cancels
    Given the letters are showing
    When the desk user presses Esc or any key that is not a shown letter
    Then the letters disappear and the pointer stays where it was
