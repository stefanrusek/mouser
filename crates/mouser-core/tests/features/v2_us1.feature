Feature: As a desk user, I want my pointer and keyboard to keep working whatever goes wrong inside Mouser
  Whatever an application, the operating system or a fault inside Mouser does, the desk user is never left with an invisible pointer or a keyboard that goes nowhere, and the desk heals itself.

  Scenario: Story 1 scenario 1: refused input is skipped
    Given this computer has the pointer from another desk computer
    When its operating system refuses a click, key, scroll or pointer move that Mouser passes on
    Then that input is skipped, every later input the operating system accepts still arrives, and the desk stays connected

  Scenario: Story 1 scenario 2: a fault inside Mouser
    Given a fault occurs inside Mouser on a desk computer
    When the desk user keeps working
    Then crossing to and from that computer, typing on it, the panic key and pausing keep working without Mouser being restarted

  Scenario: Story 1 scenario 3: mouse and keyboard stop reaching Mouser
    Given the pointer has left this computer for another desk computer
    When this computer's mouse and keyboard stop reaching Mouser
    Then the pointer is visible again on this computer's display and the mouse and keyboard act on this computer

  Scenario: Story 1 scenario 4: connection lost for any reason
    Given the pointer is on another desk computer
    When the connection to it is lost for any reason, including a fault on either computer
    Then the pointer is visible on the computer the desk user is at and the mouse and keyboard act there

  Scenario: Story 1 scenario 5: Mouser quits while another computer has the pointer
    Given the pointer is on another desk computer
    When Mouser on the computer the desk user is at quits
    Then the pointer is visible on that computer and no key or mouse button stays held down on any desk computer

  Scenario: Story 1 scenario 7: link lost then reachable again
    Given a desk link was lost for any reason, including a fault on either computer
    When the two computers can reach each other again
    Then the desk user can cross between them without restarting Mouser on either

  Scenario: Story 1 scenario 8: panic key, enable/disable hotkey or Pause after refused input or a fault
    Given the pointer is on another desk computer that has refused input or had a fault
    When the desk user presses the panic key or the enable/disable hotkey, or chooses Pause from the menu-bar or tray menu, on the computer they are at
    Then the pointer and keyboard are back on that computer

  Scenario: Edge case: a key or mouse button is held when the operating system starts refusing input
    Given a key or mouse button is held when the operating system starts refusing input on the computer with the pointer
    When control leaves that computer
    Then no key or button stays held on any computer

  Scenario: Edge case: Mouser quits on a computer that has the pointer from another computer
    Given Mouser quits on a computer that has the pointer from another computer
    When the other computer sees it leave
    Then the pointer and keyboard are back on the computer the desk user is at

  Scenario: Regression: refused input while crossing is held, link closes, desk user crosses again without restarting
    Given the desk user has crossed to another computer and the crossing is held
    And the operating system on that computer refuses an input Mouser passes on while the crossing is held
    And the link between the two computers then closes
    When the two computers can reach each other again
    Then the desk user crosses between them without restarting Mouser on either
