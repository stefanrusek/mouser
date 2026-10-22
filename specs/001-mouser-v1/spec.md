# Feature Specification: Mouser v1

**Feature Branch**: `001-mouser-v1`

**Created**: 2026-10-21

**Status**: Draft

**Input**: User description: "Mouser v1: the entire approved feature set in docs/product-requirements.md (all Must and Nice-to-have items, with the product principles, Desks model, and target setup described there; Out of scope items excluded). Personas are in .specify/memory/personas.md."

Inputs (read for intent, not copied):

- `docs/product-requirements.md` (commit 84a6fad): approved Must / Nice / Out-of-scope lists, product principles, Desks model, target setup.
- `docs/technical-decisions.md` (commit 84a6fad): stack decisions; consumed only by the Plan-Phase Handoff section.
- Requirements picker artifact, https://claude.ai/artifact/TvNfFXVgVq675VQv8xXFSv: the owner's per-feature Must/Nice/Skip choices and notes, transcribed into `docs/product-requirements.md`.

## Clarifications

### Session 2026-10-21

- Q: Where do updates come from? → A: One desk computer checks the published release location, downloads new releases, and spreads them to the rest over the LAN; this is the only internet access (Story 10, FR-003; constitution Principle III amended).
- Q: Does joining a desk need approval? → A: Yes, a one-click "Allow <computer> to join <desk>?" prompt on a computer already on the desk (Story 1, FR-013).
- Q: Timing targets? → A: Crossing shows no visible pause; recovery within 5 seconds of wake or network change (SC-007, SC-008).
- Q (plan D3): Which platform limitations are stated in the app for v1? → A: All five accepted — see Assumptions, "Accepted platform limitations".

## User Scenarios & Testing *(mandatory)*

Stories are ordered by dependency (constitution Principle 2.5): each story's criteria can pass once the stories above it are built.

### User Story 1 - As a desk owner, I want to put my computers on a desk (Priority: P1)

The owner installs Mouser on each computer, grants the operating-system permissions it needs, and puts the computers on a named desk. Mouser finds desks on the local network by itself; joining one or creating a new one is a single choice. Several independent desks can share one network.

**Why this priority**: Nothing else works until computers are on the same desk. ShareMouse's password-based grouping is the setup pain this replaces.

**Independent Test**: Install on two computers on the same network, create a desk on one, join it from the other, and confirm both list each other as desk members.

**Acceptance Scenarios**:

1. **Given** Mouser is launched for the first time, **When** the first-run flow starts, **Then** it walks the owner through each operating-system permission Mouser needs, showing which are granted and opening the right system screen for each one still missing.
2. **Given** another computer on the network belongs to the desk "Home office", **When** the owner reaches the desk step on a new computer, chooses "Home office", and clicks Allow on the prompt shown on a computer already on that desk, **Then** the new computer is a member, with no code, password, or address typed.
3. **Given** a join request is pending, **When** the owner declines it on a desk computer, **Then** the requesting computer is told it was not admitted and is not a member.
3. **Given** no desk is found, or the owner prefers a new one, **When** they choose to create a desk and give it a name, **Then** the desk exists with this computer as its only member and is offered to other computers on the network.
4. **Given** two desks, "Home office" and "Lab bench", on one network, **When** a computer on "Lab bench" is in use, **Then** computers on "Home office" receive none of its pointer movement, keystrokes, clipboard contents, or files, and neither desk lists the other's computers as members.
5. **Given** a computer is on a desk, **When** the owner moves it to another desk or removes it from its desk, **Then** it appears only in the new desk's member list (or none) on every computer.
6. **Given** a computer cannot be found automatically, **When** the owner adds it by network address, **Then** it can join the desk like a discovered computer.
7. **Given** the owner has turned on the allowlist, **When** a computer whose address is not on the list tries to join, **Then** it is not admitted and the owner sees which computer was refused.
8. **Given** two computers run Mouser versions that cannot work together, **When** they find each other, **Then** each shows a plain-language message naming the other computer, both versions, and which one to update.
9. **Given** setup is complete, **When** the computer restarts and the owner logs in, **Then** Mouser is running and the computer is back on its desk without any action.
10. **Given** Mouser is running, **When** the owner looks at the menu bar or system tray, **Then** an icon matching the system's light or dark appearance shows whether Mouser is active or paused and how many computers and displays are on the desk.

---

### User Story 2 - As a desk user, I want one mouse and keyboard to drive every computer on my desk (Priority: P1)

Moving the pointer off the edge of one display brings it onto the adjacent display, whichever computer that display belongs to. Typing goes to the computer that has the pointer.

**Why this priority**: This is the product. Combined with Story 1 it is a usable replacement for ShareMouse.

**Independent Test**: With two computers on a desk, move the pointer across the shared edge in both directions and type into a text field on each side.

**Acceptance Scenarios**:

1. **Given** two computers are on a desk and a new computer joins, **When** the join completes, **Then** its displays are placed next to the existing layout automatically and the desk owner is offered a review of the placement.
2. **Given** the pointer is on a display whose edge adjoins another computer's display, **When** the desk user moves it past that edge, **Then** it appears at the corresponding position on the adjoining display, and keystrokes, mouse buttons, and scrolling now act on that computer.
3. **Given** any computer on the desk has the pointer, **When** the desk user moves it to any other computer's display, **Then** the crossing works the same way regardless of which computer the mouse and keyboard are physically plugged into.
4. **Given** computers run different operating systems (macOS, Windows, Linux), **When** the desk user presses a shortcut on the active computer, **Then** it has the effect that shortcut is expected to have on that computer's operating system (for example ⌘C on a Mac keyboard copies on Windows).
5. **Given** displays with different pixel densities or scaling, **When** the pointer crosses between them, **Then** it moves at a consistent physical speed and lands at the matching position along the shared edge.
6. **Given** a computer has several displays, **When** the pointer moves among them and to other computers, **Then** every display on the desk is reachable through the layout.
7. **Given** an application on the active computer ignores ordinary simulated input (such as some games and remote-desktop clients), **When** the desk user types and clicks in it, **Then** the input reaches the application.
8. **Given** the pointer is on another computer, **When** the desk user presses the panic key, **Then** the pointer and keyboard return to the computer they are physically at.
9. **Given** the pointer crosses to another computer, **When** it arrives, **Then** a brief highlight shows where it landed.
10. **Given** a macOS computer has an application holding Secure Input, **When** that blocks Mouser on that computer, **Then** the desk user sees a notification naming the application.
11. **Given** the desk user has assigned mouse buttons 4 and 5 to pass through unchanged or to act as Back and Forward, **When** they press those buttons on another computer, **Then** the chosen behavior happens on that computer.
12. **Given** Mouser is active, **When** the desk user presses the enable/disable hotkey, **Then** crossing stops (or resumes) on every computer and the tray icon shows the paused (or active) state.

---

### User Story 3 - As a desk user, I want my desk to keep working through sleep, network changes, and VPNs (Priority: P1)

The connection between computers heals itself. Waking from sleep, switching between Wi-Fi and Ethernet, an address change, or connecting a work VPN or virtual machine must not leave the desk broken or require restarting Mouser.

**Why this priority**: ShareMouse failing to connect, dropping as soon as typing starts, and breaking when a VPN is active are the reasons this project exists (constitution Principle I).

**Independent Test**: With crossing working, put one computer to sleep and wake it, change its network connection, and connect and disconnect a VPN on it; after each event, cross to it and type without touching Mouser.

**Acceptance Scenarios**:

1. **Given** the desk user is typing continuously on another computer, **When** they keep typing, **Then** every keystroke arrives in order with none dropped and the session does not end.
2. **Given** a computer on the desk wakes from sleep, **When** the desk user moves the pointer toward it, **Then** the pointer crosses onto it without the user restarting, re-joining, or opening Mouser.
3. **Given** a computer switches between Wi-Fi and Ethernet or receives a new network address, **When** the desk user crosses to it afterwards, **Then** crossing works without any action in Mouser.
4. **Given** a VPN connection, virtual machine, or container network is started or stopped on any desk computer, **When** the desk user crosses to it, **Then** crossing works without any action or setting in Mouser.
5. **Given** a desk computer becomes unreachable while it has the pointer, **When** the connection is lost, **Then** the pointer and keyboard return to the computer the desk user is physically at.

---

### User Story 4 - As a desk owner, I want to arrange displays to match my physical desk (Priority: P2)

A visual arranger shows every display on the desk, from every computer. The owner drags displays to match where they really sit; display-specific options are on the display itself.

**Why this priority**: Automatic placement (Story 2) gets crossing working. Correcting it to match the real desk is the next most common setup task.

**Independent Test**: Open the arranger, drag one computer's display to the other side, and confirm the pointer now crosses at the new edge.

**Acceptance Scenarios**:

1. **Given** the arranger is open, **When** the owner drags a display to a new position, **Then** the pointer crosses at the edges that position implies, on every computer on the desk.
2. **Given** the arranger is open, **When** the owner points at a display, **Then** it shows the display's resolution, scale, owning computer, and whether that computer is online, or when it was last seen if offline.
3. **Given** a display should not be part of the desk, **When** the owner removes it from the layout, **Then** the pointer never crosses onto it, and the owner can undo the removal.
4. **Given** adjoining displays differ in physical size, **When** the owner turns on equalized dimensions, **Then** crossing maps the full length of one edge onto the full length of the other.

---

### User Story 5 - As a desk user, I want the pointer to stay put when I don't mean to cross (Priority: P2)

Aiming at a screen corner (menus, hot corners, window controls) or working in a full-screen application must not throw the pointer onto another computer. A deliberate double bump of the edge still crosses.

**Why this priority**: Accidental crossings are the most frequent annoyance once basic crossing works.

**Independent Test**: With corner blocking on, push the pointer into a corner adjoining another computer and confirm it stays; bump the edge twice and confirm it crosses.

**Acceptance Scenarios**:

1. **Given** corner blocking is on for an edge, **When** the desk user pushes the pointer into a corner of that edge, **Then** it stays on the current display.
2. **Given** a full-screen application is in front and full-screen blocking is on, **When** the desk user moves the pointer past an edge, **Then** it stays on the current display.
3. **Given** a crossing is blocked, **When** the desk user bumps the same edge twice in quick succession, **Then** the pointer crosses.
4. **Given** a crossing is blocked, **When** it is the first blocked crossing since the desk user last saw this explanation, **Then** a notification says what blocked it and how to cross anyway or change the setting.
5. **Given** the desk owner opens an edge in the display arranger, **When** they change its corner blocking, **Then** the change applies to that edge.

---

### User Story 6 - As a desk user, I want copy and paste to work between computers (Priority: P2)

Copying on one computer and pasting on another just works, for plain text, formatted text, images, and files.

**Why this priority**: The most common data exchange at a multi-computer desk, and a ShareMouse feature the owner relies on.

**Independent Test**: Copy formatted text, an image, and a file on one computer; cross to another; paste each into an application that accepts it.

**Acceptance Scenarios**:

1. **Given** the desk user copied plain text, formatted text, or an image on one computer, **When** they cross to another computer and paste, **Then** the same content is pasted, with formatting kept wherever the receiving application supports it.
2. **Given** the desk user copied one or more files or folders, **When** they paste them on another computer, **Then** copies of those files and folders appear there.
3. **Given** the desk user wants clipboard content on just one computer, **When** they use the targeted paste hotkey on that computer, **Then** it pastes the content copied on the other computer and no other computer's clipboard changes.

---

### User Story 7 - As a desk user, I want to drag files from one computer to another (Priority: P2)

Dragging a file or folder across the screen edge and dropping it on another computer copies it there, with visible progress.

**Why this priority**: Moving files between machines without network shares is a core ShareMouse feature the owner chose as Must.

**Independent Test**: Drag a folder of several files from one computer and drop it into a folder on another; confirm the copy and the progress display.

**Acceptance Scenarios**:

1. **Given** the desk user is dragging files or folders, **When** they cross to another computer and drop them on a folder or the desktop, **Then** copies appear at the drop location.
2. **Given** a transfer is under way, **When** the desk user looks at the screen, **Then** it shows the file being copied, how much is done, the speed, and the time remaining.
3. **Given** a dropped file has the same name as one already at the destination, **When** the transfer reaches it, **Then** the desk user is asked whether to overwrite it.
4. **Given** the destination does not have room for the transfer, **When** the drop happens, **Then** the desk user is told there is not enough space and nothing is copied.

---

### User Story 8 - As a desk owner, I want Mouser to tell me what is wrong and how to fix it (Priority: P2)

When something does not work, the owner should not have to guess. A health panel shows each computer's state in plain language, and notifications explain problems with a one-click fix where one exists.

**Why this priority**: ShareMouse's silent failures cost the owner weeks. Explaining failures is the second half of reliability.

**Independent Test**: Revoke a required permission on one computer and confirm the health panel names the computer and permission and opens the right system screen when clicked.

**Acceptance Scenarios**:

1. **Given** the desk owner opens the health panel, **When** it shows, **Then** it lists every computer on the desk with whether it is connected, how responsive it is, any missing permissions, and the most recent problem in plain language.
2. **Given** a computer is missing a required permission, **When** the owner views it in the health panel, **Then** a single action opens the system screen where that permission is granted.
3. **Given** a problem occurs that Mouser could not fix by itself (for example a refused computer, an incompatible version, or a missing permission), **When** it occurs, **Then** a notification states the problem and, where a fix exists, offers it as one action.
4. **Given** the owner turns on diagnostic logging with one checkbox, **When** Mouser runs afterwards, **Then** diagnostic logs are written to a file whose location the owner can open from the same place.

---

### User Story 9 - As a troubleshooting agent, I want to inspect and change Mouser from the command line (Priority: P3)

An AI agent asked to diagnose or configure Mouser works through the command-line tool, the plain-text configuration file, and the diagnostic logs, with no GUI.

**Why this priority**: The owner fixes problems by asking an agent. This only matters once the features it inspects exist.

**Independent Test**: Using only the command line, list the desk's computers and their state, change a setting, turn on diagnostic logging, and read the resulting log entries.

**Acceptance Scenarios**:

1. **Given** Mouser is running, **When** the agent asks the command-line tool for status, **Then** it reports the desk, its computers, their connection state, missing permissions, and recent problems, in both human-readable and machine-readable form.
2. **Given** any setting available in the GUI, **When** the agent changes it in the configuration file or through the command-line tool, **Then** Mouser behaves as if it had been changed in the GUI, and the GUI shows the new value.
3. **Given** diagnostic logging is on, **When** the agent reads the log file, **Then** each line is one self-contained JSON record with a timestamp, the computer, and the event.

---

### User Story 10 - As a desk owner, I want every computer on my desk to stay up to date without visiting each one (Priority: P3)

When one computer on the desk is updated, the rest follow automatically, and an update never costs the owner their granted permissions.

**Why this priority**: Keeps the version check in Story 1 from becoming a chore. It depends on everything above already shipping.

**Independent Test**: Install an older version on two computers, update one, and confirm the other updates by itself and keeps working without re-granting permissions.

**Acceptance Scenarios**:

1. **Given** one computer on the desk has a newer version than the others, **When** the others notice it, **Then** they update to that version without the owner visiting them, and the desk works afterwards.
2. **Given** a computer has been updated, **When** Mouser starts again, **Then** the operating-system permissions granted before the update are still granted.
3. **Given** a newer release has been published, **When** a computer on the desk next checks for releases, **Then** it downloads the release and the other desk computers update from it over the local network without downloading it themselves.
4. **Given** an update is available, **When** the owner looks at the tray menu, **Then** it shows the current and available versions.

---

### User Story 11 - As a desk user, I want to jump the pointer straight to any display (Priority: P3)

One hotkey shows a large letter on every display across every computer on the desk; pressing a letter moves the pointer to the centre of that display. No setup is needed.

**Why this priority**: A convenience on top of crossing. Nice-to-have in the approved requirements.

**Independent Test**: Press the QuickJump hotkey, confirm letters appear on all displays, press one, and confirm the pointer and keyboard are now on that display's computer.

**Acceptance Scenarios**:

1. **Given** QuickJump has never been configured, **When** the desk user presses the QuickJump hotkey, **Then** a distinct letter appears on every display on the desk.
2. **Given** the letters are showing, **When** the desk user presses one, **Then** the pointer moves to the centre of that display, that computer becomes the active computer, and the letters disappear.
3. **Given** the letters are showing, **When** the desk user presses Esc or any key that is not a shown letter, **Then** the letters disappear and the pointer stays where it was.

---

### User Story 12 - As a desk user, I want to control a computer that is at its login or system security screen (Priority: P3)

The shared mouse and keyboard keep working when a computer is at its login window, after a cold boot, at a permission elevation prompt, or when switching users, and the user can send the secure attention sequence to a Windows computer. (Deferred to a future feature, owner decision 2026-10-22: every Windows part of this story, that is the login screen, permission elevation prompts, user switching and Send Ctrl-Alt-Del. The macOS login window is in scope.)

**Why this priority**: Nice-to-have in the approved requirements; needed less often than everyday control.

**Independent Test**: (Windows deferred, 2026-10-22: restart a Windows computer and log in using the shared keyboard;) lock a Mac and unlock it from the shared keyboard.

**Acceptance Scenarios**:

1. **Given** a Mac on the desk is at its login window, **When** the desk user crosses to it, **Then** they can type their password and log in.
2. *(Deferred to a future feature, 2026-10-22.)* **Given** a Windows computer has just started and shows its login screen, **When** the desk user crosses to it, **Then** they can log in with the shared mouse and keyboard.
3. *(Deferred to a future feature, 2026-10-22.)* **Given** a Windows computer shows a permission elevation prompt or a user-switching screen, **When** the desk user crosses to it, **Then** they can operate that screen.
4. *(Deferred to a future feature, 2026-10-22.)* **Given** the active computer runs Windows, **When** the desk user chooses Send Ctrl-Alt-Del from the tray menu, **Then** that computer responds as if Ctrl-Alt-Del had been pressed on its own keyboard.

---

### User Story 13 - As a desk owner, I want to run Mouser without installing it (Priority: P3)

The owner can run Mouser from a folder or removable drive on a computer where they cannot or do not want to install software, and use it as a normal desk member.

**Why this priority**: Nice-to-have for borrowed or locked-down machines.

**Independent Test**: Copy the portable build to a USB drive, run it on a computer without installing, join an existing desk, and cross to it.

**Acceptance Scenarios**:

1. **Given** the portable build is on a removable drive, **When** the owner runs it on a computer, **Then** it joins a desk and behaves as a desk member without anything being installed on that computer.
2. **Given** the portable build has been run, **When** the owner removes the drive, **Then** the computer keeps no Mouser settings, and the settings are on the drive the next time it runs.

---

### Edge Cases

- A desk computer is unreachable when the pointer approaches its display: the pointer stays on the current display and the health panel shows that computer as offline. (Story 3, Story 8)
- Two computers try to create desks with the same name: both desks are offered, distinguished by their member computers' names. (Story 1)
- A mouse button or modifier key is held down at the moment the pointer crosses or the connection drops: no key or button stays stuck down on any computer. (Story 2, Story 3)
- A mouse or keyboard attached directly to the active computer is used: its input acts on that computer, the same as the shared mouse and keyboard. (Story 2)
- The clipboard contains a format the receiving computer cannot represent: the best available representation is pasted (for example plain text for formatted text). (Story 6)
- A computer joins with a different screen arrangement than last time (for example a laptop docked or undocked): its current displays replace its previous ones in the layout and the desk owner is offered a review. (Story 2, Story 4)
- The desk owner removes every display of a computer from the layout: that computer stays a desk member, receives clipboard content, and the pointer never crosses onto it. (Story 4)
- An agent edits the configuration file while Mouser is running: the change takes effect without restarting Mouser. (Story 9)

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001** (Stories 1–13): All behavior in this specification works on macOS, Windows, and Linux (X11, GNOME Wayland, KDE Wayland). Where an operating system makes a behavior impossible, the desk owner sees that limitation stated in the app on that computer; a limitation not shown in the app is a defect.
- **FR-002** (Stories 1, 6, 7): Pointer movement, keystrokes, clipboard contents, and file transfers between desk computers are encrypted, with no encryption setup shown to the desk owner.
- **FR-003** (Stories 1, 6, 7, 10): All communication is between computers on the local network, with one exception: checking for and downloading new Mouser releases from the published release location (Story 10). That exchange carries no pointer, keyboard, clipboard, file, or desk information.
- **FR-004** (Stories 1–13): Every behavior described here works with default settings; no setting has to be changed for any story's primary flow.
- **FR-005** (Stories 1–13): Every setting available in the GUI is also available in the plain-text configuration file and through the command-line tool (Story 9).
- **FR-006** (Stories 4, 5): Settings that apply to a display or an edge are changed on that display or edge in the display arranger.
- **FR-007** (Stories 1–13): Text in the main GUI uses plain language. Network ports, network adapters, and other technical options appear only under an "Advanced" section and in the configuration file.
- **FR-008** (Story 1): The desk owner can change the network ports Mouser uses under Advanced settings.
- **FR-009** (Story 1): There is no limit on the number of computers on a desk or displays in a layout.
- **FR-010** (Stories 8, 9): Diagnostic logs are written only to a file, one JSON record per line, and never to the operating system's console or system log. Diagnostic logging is turned on or off by one switch in the GUI, the command-line tool, or the configuration file, with no special key held at launch.
- **FR-011** (Story 1): Mouser starts automatically when the owner logs in, on every supported operating system.
- **FR-012** (Stories 1–13): Mouser does not include licensing, registration, accounts, or telemetry.
- **FR-013** (Story 1): A computer joins a desk without entering a code or password, and only after a desk owner approves it with one click on a computer already on the desk ("Allow <computer> to join <desk>?"). Creating a desk needs no approval.

### Key Entities

- **Desk**: A named group of computers sharing one mouse and keyboard. Has a name and a set of member computers; optionally an allowlist of network addresses.
- **Computer**: A machine running Mouser. Has a name, operating system, Mouser version, desk membership, connection state, granted and missing permissions, and last-seen time.
- **Display**: A physical screen attached to a computer. Has resolution, scale, physical size where known, owning computer, position in the layout, and whether it is included.
- **Layout**: The arrangement of every included display on a desk, defining which edge leads to which display, plus per-edge options (corner blocking) and desk options (equalized dimensions, full-screen blocking).
- **Clipboard content**: What was last copied on a desk computer, in every representation it was copied with (plain text, formatted text, image, files).
- **Transfer**: A copy of files or folders from one computer to another, with its items, progress, and outcome.
- **Problem**: Something that stopped part of the desk working, with the affected computer, a plain-language description, and an available fix if any; shown in the health panel and notifications.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Over a working session that includes typing on every desk computer, at least three sleep/wake cycles, a Wi-Fi/Ethernet switch, and connecting and disconnecting a VPN, the desk user never has to restart Mouser, re-join a desk, or change a setting to keep crossing. (Stories 2, 3)
- **SC-002**: In that same session, zero keystrokes typed on a remote computer are lost or arrive out of order. (Story 3)
- **SC-003**: A desk owner sets up two computers on a new desk, from first launch to crossing in both directions, without consulting documentation and without typing anything other than the desk name. (Story 1)
- **SC-004**: Two desks on one network operate side by side for a full working session with zero input, clipboard, or file content reaching the other desk. (Story 1)
- **SC-005**: Every problem in Story 8's list that a desk owner can fix (missing permission, incompatible version, refused computer, Secure Input, blocked crossing) is shown with its cause in plain language, and each one that has a fix is resolved by the owner from Mouser's own message without outside help. (Story 8)
- **SC-006**: A troubleshooting agent with no GUI access can determine, from the command-line tool and logs alone, which computer on the desk has a problem and what it is, for every problem type in SC-005. (Story 9)
- **SC-007**: When the pointer crosses to another computer, it appears on the other display with no pause visible to the desk user. (Story 2)
- **SC-008**: Within 5 seconds of a desk computer waking from sleep or completing a network change, the desk user can cross to it. (Story 3)

## Assumptions

- The desk owner and desk user are usually the same person; all desk computers are on one local network the owner trusts (constitution Principle III).
- Typical desks have 2–3 computers and up to 5 displays; FR-009 removes any hard limit, but testing targets this size.
- Linux support targets X11 and the GNOME and KDE Wayland sessions; other Wayland compositors may not support input capture and will show that limitation (FR-001).
- macOS releases are signed and notarized by the owner's Apple Developer ID, which is what keeps permissions across updates (Story 10, scenario 2).
- Portable mode (Story 13) is required on Windows; on macOS and Linux, where operating-system permissions are tied to an installed application, it is provided where the operating system allows and the limitation is shown otherwise (FR-001).
- Accepted platform limitations (operator decision 2026-10-21, plan D3), shown in the app on the affected computer per FR-001:
  1. Story 2 sc. 7: applications that ignore simulated input are not reached on macOS or Windows.
  2. Wayland: GNOME before 51 and Plasma before 6.7 ask for permission at each login and cannot share the clipboard from the computer sharing its mouse; desktops without input-capture support (Sway, Hyprland, COSMIC) are unsupported; full-screen blocking (Story 5 sc. 2) is unavailable.
  3. Story 7: file drag between computers is unavailable on Linux; copy and paste (Story 6) is the alternative.
  4. Story 13 on Windows: portable mode cannot control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator.
  5. GNOME without the AppIndicator extension shows no tray icon; tray-menu actions are available from the app window instead.
- The installed ShareMouse 7.0.29 was inventoried for its feature set (preference screens, messages, defaults) while writing the requirements; it is not a reference artifact and nothing in it is binding.
- Out of scope for v1, per docs/product-requirements.md: remote mouse/scroll speed tuning, multimedia keys, lock-key sync, keyboard language sync, named or location-based layout profiles, shared displays and monitor input switching, toggle-monitors hotkey, wrap-around, require-a-key-to-switch, blocking in maximized windows or chosen applications, per-display jump hotkeys, display dimming, power/lock/screensaver sync, remote Mac login with stored credentials, reset-to-defaults, settings UI lock, licensing, Wake-on-LAN, password grouping, and pairing codes.

## Plan-Phase Handoff

Non-binding hypotheses for the plan phase to confirm, refute, or supersede (constitution Principle 2.6).

- Stack per docs/technical-decisions.md: Rust core, Tauri UI, QUIC transport, per-OS input crates (macOS CGEvent tap + IOHID injection; Windows Raw Input + SendInput, with a Windows service for login/UAC/Ctrl-Alt-Del, deferred to a future feature (owner decision 2026-10-22); Linux libei / InputCapture portal on Wayland and X11).
- QUIC connection migration is the expected mechanism for surviving address and adapter changes (Story 3). The VPN/VM breakage in ShareMouse is suspected to come from binding to one adapter or advertising adapter-specific addresses; Mouser should listen on all suitable interfaces, prefer LAN routes, and ignore virtual/VPN adapters for discovery.
- The "disconnects as soon as I start typing" symptom in ShareMouse may come from keep-alive design, VPN route changes, or macOS Secure Input being treated as a failure; Story 3 scenario 1 and Story 2 scenario 10 should be tested against each.
- Discovery is expected to use LAN service advertisement (e.g. mDNS/DNS-SD) carrying the desk name, with per-desk keys exchanged at join; ShareMouse defaults were TCP 6555 / UDP 1046, which Mouser need not reuse.
- macOS Secure Input can be detected and attributed to the owning process; ShareMouse does this and names the app.
- The diagnostic log is expected at a per-OS application-support path, rotated by size; the CLI is expected to read the same state the tray app shows.
- ShareMouse's preference keys (e.g. `CanSwitchCorners`, `CanSwitchFullScreen`, `DoubleForceSwitch`, `EasySwitch`, `ClbHotkey`, `PanicKey`/`PanicCount`, `ForceBackForward`) map to Stories 2, 4, 5, 6 and may help name defaults: panic key Esc pressed twice; targeted paste ⌘/Ctrl+V variant.
