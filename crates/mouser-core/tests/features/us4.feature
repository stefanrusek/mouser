Feature: As a desk owner, I want to arrange displays to match my physical desk
  A visual arranger shows every display on the desk, from every computer. The owner drags displays to match where they really sit; display-specific options are on the display itself.

  Scenario: Story 4 scenario 1: drag a display
    Given the arranger is open
    When the owner drags a display to a new position
    Then the pointer crosses at the edges that position implies, on every computer on the desk

  Scenario: Story 4 scenario 2: display details
    Given the arranger is open
    When the owner points at a display
    Then it shows the display's resolution, scale, owning computer, and whether that computer is online, or when it was last seen if offline

  Scenario: Story 4 scenario 3: remove a display from the layout
    Given a display should not be part of the desk
    When the owner removes it from the layout
    Then the pointer never crosses onto it, and the owner can undo the removal

  Scenario: Story 4 scenario 4: equalized dimensions
    Given adjoining displays differ in physical size
    When the owner turns on equalized dimensions
    Then crossing maps the full length of one edge onto the full length of the other

  Scenario: Edge case: all displays of a computer removed
    Given the desk owner removes every display of a computer from the layout
    When the desk is in use
    Then that computer stays a desk member, receives clipboard content, and the pointer never crosses onto it

  Scenario: Edge case: docked or undocked review offer
    Given a computer joins with a different screen arrangement than last time (for example a laptop docked or undocked)
    When it joins
    Then its current displays replace its previous ones in the layout and the desk owner is offered a review
