Feature: As a desk user, I want one mouse and keyboard to drive every computer on my desk
  Moving the pointer off the edge of one display brings it onto the adjacent display, whichever computer that display belongs to. Typing goes to the computer that has the pointer.

  Scenario: Story 2 scenario 1: new computer placed automatically
    Given two computers are on a desk and a new computer joins
    When the join completes
    Then its displays are placed next to the existing layout automatically and the desk owner is offered a review of the placement

  Scenario: Story 2 scenario 2: crossing an adjoining edge
    Given the pointer is on a display whose edge adjoins another computer's display
    When the desk user moves it past that edge
    Then it appears at the corresponding position on the adjoining display, and keystrokes, mouse buttons, and scrolling now act on that computer

  Scenario: Story 2 scenario 3: crossing is independent of the physical hardware owner
    Given any computer on the desk has the pointer
    When the desk user moves it to any other computer's display
    Then the crossing works the same way regardless of which computer the mouse and keyboard are physically plugged into

  Scenario: Story 2 scenario 4: shortcuts map across operating systems
    Given computers run different operating systems (macOS, Windows, Linux)
    When the desk user presses a shortcut on the active computer
    Then it has the effect that shortcut is expected to have on that computer's operating system (for example ⌘C on a Mac keyboard copies on Windows)

  Scenario: Story 2 scenario 5: different pixel densities
    Given displays with different pixel densities or scaling
    When the pointer crosses between them
    Then it moves at a consistent physical speed and lands at the matching position along the shared edge

  Scenario: Story 2 scenario 6: every display reachable
    Given a computer has several displays
    When the pointer moves among them and to other computers
    Then every display on the desk is reachable through the layout

  Scenario: Story 2 scenario 7: applications ignoring simulated input
    Given an application on the active computer ignores ordinary simulated input (such as some games and remote-desktop clients)
    When the desk user types and clicks in it
    Then the input reaches the application

  Scenario: Story 2 scenario 8: panic key
    Given the pointer is on another computer
    When the desk user presses the panic key
    Then the pointer and keyboard return to the computer they are physically at

  Scenario: Story 2 scenario 9: arrival highlight
    Given the pointer crosses to another computer
    When it arrives
    Then a brief highlight shows where it landed

  Scenario: Story 2 scenario 10: macOS Secure Input
    Given a macOS computer has an application holding Secure Input
    When that blocks Mouser on that computer
    Then the desk user sees a notification naming the application

  Scenario: Story 2 scenario 11: mouse buttons 4 and 5
    Given the desk user has assigned mouse buttons 4 and 5 to pass through unchanged or to act as Back and Forward
    When they press those buttons on another computer
    Then the chosen behavior happens on that computer

  Scenario: Story 2 scenario 12: enable/disable hotkey
    Given Mouser is active
    When the desk user presses the enable/disable hotkey
    Then crossing stops (or resumes) on every computer and the tray icon shows the paused (or active) state

  Scenario: Story 2 scenario 12 (isolation): nobody can cross onto a paused computer
    Given two up-to-date computers are on a desk and Beta is paused
    When the desk user pushes the pointer across the edge toward Beta
    Then the pointer stays on Alpha and nothing reaches Beta

  Scenario: Story 2 scenario 12 (isolation): the pointer of a paused computer never leaves it
    Given two up-to-date computers are on a desk and Alpha is paused
    When the desk user pushes the pointer across the edge of Alpha
    Then the pointer stays on Alpha and nothing reaches Beta

  Scenario: Story 2 scenario 12 (isolation): pausing the controlled computer returns control
    Given two up-to-date computers are on a desk and the pointer and a held key are on Beta
    When Beta is paused
    Then control returns to Alpha and Beta's held key is released

  Scenario: Story 2 scenario 12 (isolation): resuming restores crossing
    Given two up-to-date computers are on a desk and Beta was paused and resumed
    When the desk user pushes the pointer across the edge toward Beta
    Then the pointer arrives on Beta

  Scenario: Edge case: button or modifier held at the crossing
    Given a mouse button or modifier key is held down
    When the pointer crosses or the connection drops
    Then no key or button stays stuck down on any computer

  Scenario: Edge case: local mouse on the active computer
    Given a mouse or keyboard is attached directly to the active computer
    When it is used
    Then its input acts on that computer, the same as the shared mouse and keyboard

  Scenario: Edge case: docked or undocked display change
    Given a computer joins with a different screen arrangement than last time (for example a laptop docked or undocked)
    When it joins
    Then its current displays replace its previous ones in the layout and the desk owner is offered a review

  Scenario: Accepted limitation: applications ignoring simulated input are not reached
    Given applications that ignore simulated input are not reached on macOS or Windows
    When the desk user looks at the app on the affected computer
    Then the app states that applications ignoring simulated input are not reached
