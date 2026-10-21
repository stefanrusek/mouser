Feature: As a desk user, I want my desk to keep working through sleep, network changes, and VPNs
  The connection between computers heals itself. Waking from sleep, switching between Wi-Fi and Ethernet, an address change, or connecting a work VPN or virtual machine must not leave the desk broken or require restarting Mouser.

  Scenario: Story 3 scenario 1: continuous typing
    Given the desk user is typing continuously on another computer
    When they keep typing
    Then every keystroke arrives in order with none dropped and the session does not end

  Scenario: Story 3 scenario 2: wake from sleep
    Given a computer on the desk wakes from sleep
    When the desk user moves the pointer toward it
    Then the pointer crosses onto it without the user restarting, re-joining, or opening Mouser

  Scenario: Story 3 scenario 3: Wi-Fi/Ethernet switch or new address
    Given a computer switches between Wi-Fi and Ethernet or receives a new network address
    When the desk user crosses to it afterwards
    Then crossing works without any action in Mouser

  Scenario: Story 3 scenario 4: VPN, virtual machine, or container network
    Given a VPN connection, virtual machine, or container network is started or stopped on any desk computer
    When the desk user crosses to it
    Then crossing works without any action or setting in Mouser

  Scenario: Story 3 scenario 5: computer becomes unreachable with the pointer
    Given a desk computer becomes unreachable while it has the pointer
    When the connection is lost
    Then the pointer and keyboard return to the computer the desk user is physically at

  Scenario: Edge case: unreachable computer when the pointer approaches
    Given a desk computer is unreachable
    When the pointer approaches its display
    Then the pointer stays on the current display and the health panel shows that computer as offline

  Scenario: Edge case: key held when the connection drops
    Given a mouse button or modifier key is held down
    When the connection drops
    Then no key or button stays stuck down on any computer

  Scenario: SC-008: crossing within 5 seconds of waking or a network change
    Given a desk computer wakes from sleep or completes a network change
    When 5 seconds have passed
    Then the desk user can cross to it
