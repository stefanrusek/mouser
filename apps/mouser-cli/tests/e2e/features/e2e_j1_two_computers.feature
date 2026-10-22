Feature: J1 two real computers on a LAN
  A Mac (computer A, the controller) and a Windows 11 VM (computer B) get Mouser installed from the release build, form a desk with approval, cross both ways and type, hit the panic key, sleep and wake B, and cross again. Assembled from spec.md Story 1 scenarios 2-4 and 9, Story 2 scenarios 2, 3 and 8, and Story 3 scenarios 1 and 2.

  Scenario: Install, form a desk, cross, panic, sleep and wake, cross again
    Given Mouser is installed on both computers
    And Mouser is launched on both computers

    # Story 1 scenario 3 (second): create a desk
    When the owner chooses Create desk "Home office" on computer A
    Then computer A is the only member of "Home office"

    # Story 1 scenario 4: two desks stay apart (a third computer is not available, so B holds the second desk)
    When the owner chooses Create desk "Lab bench" on computer B
    Then computer B is the only member of "Lab bench"
    And neither desk lists the other's computer as a member
    When the owner removes computer B from its desk

    # Story 1 scenario 3 (first): declined join
    And the owner chooses "Home office" on computer B
    And the owner declines the request on computer A
    Then computer B is told it was not admitted to "Home office"
    And computer B is not a member of any desk

    # Story 1 scenario 2: join with one-click approval
    When the owner chooses "Home office" on computer B
    And the owner clicks Allow on computer A
    Then computer B is told it joined "Home office"
    And both computers list each other as connected members of "Home office"

    # Story 1 scenario 9: back on the desk after Mouser restarts
    When Mouser is restarted on computer B
    Then computer B is back on "Home office" with no action in Mouser

    # Story 2 scenarios 2 and 3: cross both ways and type
    Given a plain text file is open in Notepad on computer B
    When the owner moves the pointer off the right edge of computer A's display
    Then the pointer is on computer B
    When the owner types "hello from computer a 123" on computer A
    And the owner saves with Command-S on computer A
    Then the text file on computer B contains exactly "hello from computer a 123"
    When the owner moves the pointer off the left edge of computer B's display
    Then the pointer is back on computer A

    # Story 2 scenario 8: panic key
    When the owner moves the pointer off the right edge of computer A's display
    Then the pointer is on computer B
    When the owner presses Escape twice on computer A
    Then the pointer is back on computer A

    # Story 3 scenario 1: continuous typing
    Given a plain text file is open in Notepad on computer B
    When the owner moves the pointer off the right edge of computer A's display
    Then the pointer is on computer B
    When the owner types a continuous passage for 60 seconds on computer A
    And the owner saves with Command-S on computer A
    Then the text file on computer B contains the continuous passage exactly
    When the owner moves the pointer off the left edge of computer B's display
    Then the pointer is back on computer A

    # Story 3 scenario 2: sleep and wake one computer, cross again
    When computer B is put to sleep
    And computer B wakes from sleep
    And 5 seconds pass
    Given a plain text file is open in Notepad on computer B
    When the owner moves the pointer off the right edge of computer A's display
    Then the pointer is on computer B
    When the owner types "after wake 456" on computer A
    And the owner saves with Command-S on computer A
    Then the text file on computer B contains exactly "after wake 456"
    When the owner moves the pointer off the left edge of computer B's display
    Then the pointer is back on computer A
