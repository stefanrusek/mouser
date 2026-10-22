Feature: As a desk user, I want to drag files from one computer to another
  Dragging a file or folder across the screen edge and dropping it on another computer copies it there, with visible progress.

  Scenario: Story 7 scenario 1: drag and drop files
    Given the desk user is dragging files or folders
    When they cross to another computer and drop them on a folder or the desktop
    Then copies appear at the drop location

  Scenario: Story 7 scenario 2: transfer progress
    Given a transfer is under way
    When the desk user looks at the screen
    Then it shows the file being copied, how much is done, the speed, and the time remaining

  Scenario: Story 7 scenario 3: name collision
    Given a dropped file has the same name as one already at the destination
    When the transfer reaches it
    Then the desk user is asked whether to overwrite it

  Scenario: Story 7 scenario 4: not enough space
    Given the destination does not have room for the transfer
    When the drop happens
    Then the desk user is told there is not enough space and nothing is copied

  Scenario: Story 7 scenario 5: cancelled drag
    Given the desk user has dragged files to another computer
    When they cancel the drag there or drop it on something that is not a folder
    Then nothing is copied, the progress display closes without an error, the computer the files came from is told the drag ended, and no problem is raised

  Scenario: Accepted limitation: file drag unavailable on Linux
    Given file drag between computers is unavailable on Linux
    When the desk user looks at the app on a Linux computer
    Then the app states that file drag between computers is unavailable and that copy and paste (Story 6) is the alternative
