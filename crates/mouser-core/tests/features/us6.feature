Feature: As a desk user, I want copy and paste to work between computers
  Copying on one computer and pasting on another just works, for plain text, formatted text, images, and files.

  Scenario: Story 6 scenario 1: text and images
    Given the desk user copied plain text, formatted text, or an image on one computer
    When they cross to another computer and paste
    Then the same content is pasted, with formatting kept wherever the receiving application supports it

  Scenario: Story 6 scenario 2: files and folders
    Given the desk user copied one or more files or folders
    When they paste them on another computer
    Then copies of those files and folders appear there

  Scenario: Story 6 scenario 3: targeted paste
    Given the desk user wants clipboard content on just one computer
    When they use the targeted paste hotkey on that computer
    Then it pastes the content copied on the other computer and no other computer's clipboard changes

  Scenario: Edge case: unrepresentable clipboard format
    Given the clipboard contains a format the receiving computer cannot represent
    When the desk user pastes on the receiving computer
    Then the best available representation is pasted (for example plain text for formatted text)

  Scenario: Story 1 scenario 4: clipboard isolation between desks
    Given two desks, "Home office" and "Lab bench", on one network
    When a computer on "Lab bench" is in use
    Then computers on "Home office" receive none of its pointer movement, keystrokes, clipboard contents, or files, and neither desk lists the other's computers as members
