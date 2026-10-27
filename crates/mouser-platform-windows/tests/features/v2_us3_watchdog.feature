Feature: As a desk owner, I want one Mouser window where I can find everything
  Health reports only problems the owner can act on; a third-party hook that swallows Mouser's probes is not one while real input keeps arriving.

  Scenario: Story 3 scenario 16: probes swallowed by another hook
    Given software on a Windows computer intercepts Mouser's own checks of its hold on the mouse and keyboard
    When the desk user's mouse and keyboard keep working on that computer
    Then Health shows no problem about that computer's hold on the mouse and keyboard
