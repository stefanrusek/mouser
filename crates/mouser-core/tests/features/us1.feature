Feature: As a desk owner, I want to put my computers on a desk
  The owner installs Mouser on each computer, grants the operating-system permissions it needs, and puts the computers on a named desk. Mouser finds desks on the local network by itself; joining one or creating a new one is a single choice. Several independent desks can share one network.

  Scenario: Story 1 scenario 1: first-run permissions walkthrough
    Given Mouser is launched for the first time
    When the first-run flow starts
    Then it walks the owner through each operating-system permission Mouser needs, showing which are granted and opening the right system screen for each one still missing

  Scenario: Story 1 scenario 2: join a desk with one-click approval
    Given another computer on the network belongs to the desk "Home office"
    When the owner reaches the desk step on a new computer, chooses "Home office", and clicks Allow on the prompt shown on a computer already on that desk
    Then the new computer is a member, with no code, password, or address typed

  Scenario: Story 1 scenario 3 (first): declined join request
    Given a join request is pending
    When the owner declines it on a desk computer
    Then the requesting computer is told it was not admitted and is not a member

  Scenario: Story 1 scenario 3 (second): create a desk
    Given no desk is found, or the owner prefers a new one
    When they choose to create a desk and give it a name
    Then the desk exists with this computer as its only member and is offered to other computers on the network

  Scenario: Story 1 scenario 4: two desks on one network are isolated
    Given two desks, "Home office" and "Lab bench", on one network
    When a computer on "Lab bench" is in use
    Then computers on "Home office" receive none of its pointer movement, keystrokes, clipboard contents, or files, and neither desk lists the other's computers as members

  Scenario: Story 1 scenario 5: move or remove a computer
    Given a computer is on a desk
    When the owner moves it to another desk or removes it from its desk
    Then it appears only in the new desk's member list (or none) on every computer

  Scenario: Story 1 scenario 6: add a computer by network address
    Given a computer cannot be found automatically
    When the owner adds it by network address
    Then it can join the desk like a discovered computer

  Scenario: Story 1 scenario 7: allowlist refuses unlisted address
    Given the owner has turned on the allowlist
    When a computer whose address is not on the list tries to join
    Then it is not admitted and the owner sees which computer was refused

  Scenario: Story 1 scenario 8: incompatible versions
    Given two computers run Mouser versions that cannot work together
    When they find each other
    Then each shows a plain-language message naming the other computer, both versions, and which one to update

  Scenario: Story 1 scenario 9: starts at login
    Given setup is complete
    When the computer restarts and the owner logs in
    Then Mouser is running and the computer is back on its desk without any action

  Scenario: Story 1 scenario 10: tray icon status
    Given Mouser is running
    When the owner looks at the menu bar or system tray
    Then an icon matching the system's light or dark appearance shows whether Mouser is active or paused and how many computers and displays are on the desk

  Scenario: Edge case: two desks with the same name
    Given two computers on the network
    When they try to create desks with the same name
    Then both desks are offered, distinguished by their member computers' names
