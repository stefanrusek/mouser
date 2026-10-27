Feature: As a desk user, I want my pointer and keyboard to keep working whatever goes wrong inside Mouser
  The Windows backend returns the pointer even when Mouser's process is ended from outside.

  Scenario: Story 1 scenario 6: Mouser ends unexpectedly while another computer has the pointer
    Given the pointer is on another desk computer
    When Mouser on the computer the desk user is at ends unexpectedly
    Then the pointer is visible on that computer
