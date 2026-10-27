Feature: As a desk user, I want to control a Windows computer at its login screen, elevation prompts and administrator applications
  Portable Mouser on Windows returns the pointer when an application running as administrator comes to the front, and can control such applications once restarted as administrator. These are the scenarios core observes over VirtualPlatform.

  Scenario: Story 5 scenario 4: installed Mouser delivers input to an administrator application
    Given Mouser is installed on a Windows computer and an application running as administrator is in front
    When the desk user crosses to it and types, clicks, scrolls and moves the pointer
    Then the application receives that input

  Scenario: Story 5 scenario 5: edge, panic key and hotkeys with an administrator application in front
    Given Mouser is installed on a Windows computer the desk user is physically at and an application running as administrator is in front
    When they move the pointer past an edge, press the panic key or press a Mouser hotkey
    Then it behaves as with any other application in front

  Scenario: Story 5 scenario 6: portable Mouser returns the pointer and explains
    Given portable Mouser runs on a Windows computer that has the pointer from another desk computer
    When an application running as administrator comes to the front there
    Then the pointer and keyboard return to the computer they came from, and that computer shows a message naming the application and the Windows computer, saying portable Mouser cannot control applications running as administrator, and offering "Restart Mouser as administrator" as one action, with installing Mouser named as the way to also control the login screen, elevation prompts and Ctrl-Alt-Del

  Scenario: Story 5 scenario 7: portable Mouser restarted as administrator
    Given the desk owner chose "Restart Mouser as administrator" and approved Windows' prompt
    When the desk user crosses to that Windows computer with an application running as administrator in front
    Then the application receives their input, and moving past an edge, the panic key and Mouser hotkeys work there

  Scenario: Story 5 scenario 8: portable Mouser limitations in Health
    Given portable Mouser runs on a Windows computer
    When the desk owner opens its Health entry
    Then "What doesn't work here" states that portable mode cannot control the login screen, permission prompts or Ctrl-Alt-Del, and, unless Mouser was restarted as administrator, applications running as administrator
