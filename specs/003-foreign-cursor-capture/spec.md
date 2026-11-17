# Feature Specification: Coexist With Apps That Capture the Mouse

**Feature Branch**: `003-foreign-cursor-capture` (no branch created; the spec directory name is independent of any branch)

**Created**: 2026-11-17

**Status**: Draft

**Input**: User description: "the fix" — the fix for the bug diagnosed earlier in this session: when another app on the Mac (a video game) captures the mouse, Mouser and that app fight over the cursor. The owner sees the cursor on both computers' screens, input going to the other computer, and confusing behaviour when moving the pointer onto the Mac while the game holds the mouse.

## Background

Games and some creative tools "capture" the mouse: they hide the system cursor, stop it moving on screen, and read only raw movement. Mouser does the same thing to the Mac's cursor while the pointer is on another computer, so that the Mac cursor stays out of the way and input goes only to the computer that has the pointer.

When both want control at once, today's behaviour is that Mouser sets its state once and never looks again. The other app can undo it, so the Mac's cursor reappears and moves while the keyboard and mouse still go to the other computer. In the opposite order, Mouser taking the pointer back to the Mac undoes the game's capture partway through.

Root cause is a diagnosis from reading the code, not an observation of real events. Before planning, `/speckit-plan` MUST confirm it, and it MUST NOT assume it:

- The diagnosis rests on a recollection of how the game's windowing library captures the mouse; that recollection has not been checked.
- The diagnosis found no recent commit that changed this behaviour, so the "about a week" timing may come from the game's side.
- Which computer is the controller in the owner's setup is assumed, not verified.
- The owner's recovery key was reported as `Cmd+Shift+J`; the owner has since confirmed they press the default QuickJump (`Ctrl+Alt+Shift+J`), so no custom hotkey is involved.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - As a Desk user, I want the Mac to stay quiet while my pointer is on another computer, even if a game grabs the Mac's mouse (Priority: P1)

The situation: the Desk user is working on the other computer through Mouser, and a game on the Mac starts, restarts or re-captures the mouse. The Mac's screen should stay quiet (no visible cursor, nothing jumping to the game window) and keyboard and mouse should keep going only to the computer that has the pointer.

**Why this priority**: This is the bug the owner hits daily, and it breaks the core promise that input goes to exactly one computer. It also has the worst outcome: clicks and keystrokes land somewhere the user cannot see.

**Independent Test**: With the pointer on the other computer, make another app capture and then release the Mac's mouse, and check that the Mac cursor is never visible and that clicks and keys still reach only the other computer.

**Acceptance Scenarios**:

1. **Given** the pointer is on the other computer and the Mac cursor is hidden, **When** another Mac app captures the mouse, **Then** the Mac cursor stays hidden and does not appear on the Mac screen.
2. **Given** the pointer is on the other computer, **When** another Mac app captures and later releases the mouse, **Then** after the release the Mac cursor is still hidden and still not moving, and the other computer still receives all input.
3. **Given** the pointer is on the other computer, **When** another Mac app moves the Mac cursor to the middle of its window, **Then** that move does not move the pointer on the other computer and does not change which computer has the pointer.
4. **Given** any of the above happened, **When** the user later brings the pointer back to the Mac, **Then** it returns normally and the cursor appears at the expected landing point.

---

### User Story 2 - As a Desk user, I want my pointer to be fully mine when I move it onto the Mac, even if a game was holding the mouse (Priority: P2)

The situation: a game on the Mac is capturing the mouse and the Desk user moves the pointer from the other computer onto the Mac. Mouser takes the cursor back at once, so the cursor is visible and under the user's control and the game's capture is broken, reliably and completely, with no tug of war. If the game keeps re-capturing, Mouser holds the cursor for a 1 s settle window, after which the game wins.

**Why this priority**: This is the confusing case the owner reported. It is less harmful than Story 1 because the user can see what is going on, but it is the case that makes them reach for recovery keys.

**Independent Test**: Start a mouse-capturing app on the Mac, move the pointer onto the Mac from the other computer, and check that the cursor is visible and under the user's control and the game's capture is broken.

**Acceptance Scenarios**:

1. **Given** another Mac app is capturing the mouse and the pointer is on the other computer, **When** the user moves the pointer onto the Mac screen, **Then** the cursor is visible and under the user's control at once (the game's capture is broken), and it does not flicker or get re-hidden by the game during the 1 s settle window.
2. **Given** the pointer arrived on the Mac while a game was capturing, **When** the game releases the mouse, **Then** the cursor behaves normally with no further action from the user.
3. **Given** the pointer arrived on the Mac while a game was capturing, **When** the user presses the recovery key, **Then** the pointer ends up in a clear, usable state.

---

### User Story 3 - As a Desk user, I want the panic and QuickJump keys to always recover the cursor, and as a Troubleshooting agent I want the log to say what happened (Priority: P3)

The situation: the Mac cursor ever disagrees with where the Desk user expects it. The existing keys recover it, and the diagnostic log shows what happened, so a stuck state is a one-line answer rather than a code dive.

**Why this priority**: It is a safety net and an aid to diagnosis, not the fix itself. It also lets the owner confirm the diagnosis before and after the fix.

**Independent Test**: Cause the disagreement on purpose, then check that the recovery key restores a sane state and that the log records the foreign change.

**Acceptance Scenarios**:

1. **Given** another app has changed the Mac cursor's visibility or position, **When** it happens, **Then** the diagnostic log records it, including whether Mouser corrected it.
2. **Given** a mismatch between Mouser's state and the Mac cursor, **When** the user presses the panic or QuickJump key, **Then** the pointer returns to a well-defined computer and the cursor is visible only there.

---

### Edge Cases

- A game captures the mouse many times in a row (pause and resume), including while the pointer moves between computers.
- A game quits or crashes while holding the capture, with the pointer on either computer.
- Two apps both capture the mouse, or one captures while another releases.
- The Mac is the computer being controlled (the other computer has the pointer and the Mac is the guest), instead of the controller.
- The Mac sleeps, wakes, switches Spaces, or its display layout changes while the game holds the capture. Covered by T032 (re-establish the expected state on the next guard tick), except sleep/wake and display-layout change, which are out of scope: the existing capture code already rebuilds on those and a game-induced variant adds no new mechanism.
- Mouser's own cursor moves (its own warps and hides) must not be mistaken for another app's changes.
- Full-screen games on a secondary display. Out of scope: the guard acts on the global cursor, not per display, so it is covered by the same tests; confirmed in the owner batch only.
- Secure Input held by another app at the same time. Out of scope: it affects key capture, not cursor state, and has no change here.

Task coverage: repeated captures T015/T018; quit or crash while holding T015/T020; two capturing apps T033; Mac as controlled computer T034; Mouser's own moves T013/T015.

## Clarifications

### Session 2026-11-17

- Q: When the pointer moves onto the Mac while a game is holding the mouse captured, who should end up controlling the cursor? -> A: Mouser takes the cursor back at once, re-associating and showing it, which breaks the game's capture, and it must do so reliably and completely (it does not fully today). Updated FR-005, Story 2, SC-004, Assumptions.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: While the pointer is on another computer, Mouser MUST keep the Mac cursor hidden and still, and MUST restore that state within 100 ms of any other app changing it, so the cursor is never visible on the Mac while the pointer is elsewhere.
- **FR-002**: While the pointer is on another computer, anything another Mac app does to the Mac cursor (including moving it to a window centre) MUST NOT move the pointer on the other computer, MUST NOT send input to it, and MUST NOT change which computer has the pointer.
- **FR-003**: When the pointer is on another computer, keyboard and mouse input MUST reach only that computer, regardless of any other Mac app's mouse capture.
- **FR-004**: Mouser's own cursor moves and hides MUST NOT be mistaken for another app's, so the cursor never visibly flickers or jitters from Mouser correcting itself.
- **FR-005**: When the pointer moves onto the Mac while another app is capturing the mouse, Mouser MUST take the cursor back at once so it is visible and under the user's control, which breaks the game's capture. It MUST do so completely and reliably every time (no partial state, no flicker). If the game tries to re-capture, Mouser holds the cursor for a bounded 1 s settle window (plan D3, approved by the owner); after that window the other app wins.
- **FR-006**: When another app releases a capture while Mouser still holds the pointer elsewhere, the Mac cursor MUST stay hidden. When the pointer is on the Mac, the cursor MUST behave as the releasing app left it.
- **FR-007**: Mouser MUST record in its JSON Lines diagnostic log each time another app changes whether the Mac cursor is visible or where it is (whether it follows the mouse is inferred, not readable), and whether Mouser corrected it.
- **FR-008**: The existing recovery keys (panic and QuickJump) MUST keep working while another app has the mouse captured, and MUST leave the cursor visible only on the computer that has the pointer.
- **FR-009**: The behaviour in FR-001 to FR-008 MUST hold on every supported OS that has the same kind of capture, or a limitation MUST be documented in docs/ (the owner chose documentation only, no in-app note; constitution principle V).
- **FR-010**: The change MUST NOT add latency to normal pointer and keyboard forwarding and MUST NOT interrupt an active session (constitution principle I).
- **FR-011**: The fix MUST ship with a regression test that fails before the fix and passes after it (constitution principle VI), including the capture-while-remote case and the move-onto-the-Mac case.

### Key Entities

- **Pointer owner**: which computer currently has the pointer; the thing the user trusts.
- **Mac cursor state**: whether the cursor is visible, whether it follows the mouse (inferred), and where it is on the Mac screen. Another app can change it at any time.
- **Foreign capture**: a period during which another app holds the mouse; it has a start and an end, and either can happen while Mouser holds the pointer elsewhere.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In 100 consecutive start, pause and quit cycles of a mouse-capturing game while the pointer is on the other computer, the Mac cursor is never visible and no input reaches the Mac.
- **SC-002**: After any foreign cursor change, the Mac cursor is back in Mouser's expected state in under 100 ms.
- **SC-003**: The owner can run the game and the other computer side by side for a full working session without pressing a recovery key.
- **SC-004**: In 100% of moves onto the Mac while a game holds the mouse, the cursor is visible and under user control (the game's capture broken) within 100 ms, with no flicker; if the game re-captures, Mouser holds it for at most the 1 s settle window, then yields.
- **SC-005**: The diagnostic log answers "did another app change the cursor, and did Mouser correct it?" for every such event, without needing the code.
- **SC-006**: Normal forwarding latency and the existing test suite are unchanged; no existing test needs loosening to pass.

## Assumptions

- **Rule for FR-005 (decided by the owner in clarification, 2026-11-17):** when the pointer arrives on the Mac while a game holds the mouse, Mouser takes the cursor back at once, re-associating and showing it, which breaks the game's capture. Today it does this only partly, so the requirement is that it does so reliably and completely. This reverses the earlier default (game keeps capture). The owner then approved the plan's bounded hold: Mouser holds the cursor for a 1 s settle window against a re-capturing game, after which the foreign app wins (plan D3).
- The owner proceeds with the diagnosis only partly confirmed: real-game probe S5 showed the cursor becoming visible with Mouser unaware (position did not move); probes S2 and S4 are unrun.
- The diagnosis in the Background section is treated as a hypothesis. Planning starts by confirming it using the log check and the idle-Mac check described in the earlier diagnosis, neither of which needs a remote crossing or touches the VM.
- Scope is macOS first, because that is where the bug was seen. Windows and Linux get the same behaviour or a documented limitation (FR-009).
- This is a bug fix inside the frozen v2 scope; it adds no new user-facing settings and no protocol change.
- The controller case (the Mac holds the keyboard and mouse and sent the pointer away) is the one the owner hit; the controlled-Mac case is treated as an edge case to cover, not the main story.
- Windows VM testing, if any, follows the existing VM rules: small tasks, nothing that can crash the VM, and the owner's confirmations batched after an unattended run.
