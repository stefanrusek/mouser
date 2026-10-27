# Feature Specification: Mouser v2

**Feature Branch**: `002-mouser-v2`

**Created**: 2026-10-26

**Status**: Draft

**Input**: User description: "Mouser v2 (feature 002): builds on the shipped v1 (spec 001, v0.2.8). One feature covering: A. Reliability (refused input injection logged and skipped, never a panic; the engine recovers from internal panics and logs them; the cursor is never left hidden when hooks go blind, the link drops, or the app exits; tray Quit runs full cleanup; version-mismatched desk-mates shown clearly; Windows hook-health probes swallowed by third-party hooks such as ASUS AsHotplugCtrl don't count as misses; trigger case: tester Steve's cursor vanishes on the PC running VMware Workstation, Esc×2 doesn't recover, link drops 66 ms after an elevated foreground and never reconnects). B. Windows elevation deferred from v1 (login screen, UAC prompts, user switching, Send Ctrl-Alt-Del, applications running as administrator via a Windows service; portable mode returns the pointer with a clear message; signed installer and service; headless on Windows and Linux). C. Useful opt-in diagnostic logging and a collect-logs-from-the-whole-desk action. D. One app window with sidebar navigation in the style of Fluid Voice, actionable non-duplicated correctly attributed notices, onboarding and permissions in it, configurable panic and QuickJump hotkeys, friendly computer names, readable arranger labels and an explained Review placement, left-click on the menu-bar icon toggles pause, landing-highlight toggle, Cmd↔Ctrl mapping setting. E. Unfinished v1 real-hardware validation." Owner addition (2026-10-26, part D): the menu-bar/tray icon also shows connection state (for example a green dot when connected, a red dot when unexpectedly disconnected), not relying on colour alone.

Note: the input's "signed installer and service" was superseded by the 2026-10-26 clarification below: feature 002 ships unsigned (FR-004).

Inputs (read for intent, not copied):

- The owner's typed request above, given in the session of 2026-10-26, with the owner context that the owner tests on two Macs and a Parallels Windows 11 virtual machine, a tester (Steve) has two real Windows PCs, and a Linux user wants to try Mouser; one combined spec was the owner's decision.
- `specs/001-mouser-v1/spec.md` (baseline, shipped as v0.2.8): stories, FR and SC numbers referred to below as "001 Story n", "001 FR-nnn", "001 SC-nnn".
- `specs/001-mouser-v1/evidence/quickstart.md`: the PENDING-OWNER real-hardware checks that Story 7 completes.
- `specs/001-mouser-v1/tasks.md` T367 and T415–T426: the Windows work deferred from v1 (owner decision 2026-10-22) that Story 5 brings back in scope.

Reference artifacts (copied into this feature; see FR-011):

- `reference/mouser-v0.2.8-windows.png`: the owner's screenshot of v0.2.8's three separate windows (desk window, "Mouser health", "Arrange displays"), showing the repeated notices, notices attributed to the wrong computer, the raw value "8.707351000000001 ms", truncated arranger labels and the unexplained "Review placement" button.
- `reference/fluid-voice-style.png`: the owner's screenshot of Fluid Voice, the style the owner wants: one window, a sidebar with search and sections grouped under headings, and a home overview.
- `reference/vmware-cursor-investigation.md`: a read-only investigation of the trigger case (Steve's log, v0.2.8, two Windows PCs).

## Clarifications

### Session 2026-10-26

- Q: Which Windows code-signing certificate? → A: None. Feature 002 ships unsigned ("I don't have the cash"). Windows' unknown-publisher (SmartScreen) warning on install is an accepted limitation, and the install instructions and release notes tell the desk owner how to get past it; no Story 5 behavior may depend on a code signature (FR-004, SC-010, Assumptions).
- Q: Should portable Mouser on Windows offer one-click elevation? → A: Yes (option B). Portable Mouser returns the pointer, and its message offers "Restart Mouser as administrator"; after one approval, applications running as administrator become controllable in portable mode. The login screen, elevation prompts and Ctrl-Alt-Del still need installed Mouser (Story 5 scenarios 6–8, FR-005).
- Q: Does Linux headless mode include the Linux login screen? → A: No (option A). Linux headless runs in a signed-in session only, with no window or tray; the Linux login screen stays a listed limitation (Story 6 scenario 2, FR-006).
- Q: Must the trigger case be validated with VMware on a real Windows PC? → A: No. The gating check runs on the Parallels Windows 11 VM with any application running as administrator (e.g. elevated cmd.exe or Task Manager) standing in for VMware; VMware Workstation on a real PC (Steve) is optional extra confirmation (Story 7 scenario 1, FR-011, SC-001).

## Specification approach

This specification is differential. Everything specified in 001 continues to hold unless a story or requirement here says it amends or supersedes a named part of 001. Where this specification is silent, 001 governs.

## User Scenarios & Testing *(mandatory)*

Stories are ordered by dependency (constitution Principle 2.5): each story's criteria can pass once the stories above it are built.

### User Story 1 - As a desk user, I want my pointer and keyboard to keep working whatever goes wrong inside Mouser (Priority: P1)

In v0.2.8, on Steve's Windows PC running VMware Workstation, the pointer vanished, pressing Esc twice did nothing, and the desk never reconnected until Mouser was restarted. Whatever an application, the operating system or a fault inside Mouser does, the desk user is never left with an invisible pointer or a keyboard that goes nowhere, and the desk heals itself. This story amends 001 Story 2 scenario 8 (panic key) and 001 Story 3 scenario 5 (connection lost while another computer has the pointer) so that they hold under faults, and extends 001 Story 3's self-healing to links lost through faults.

**Why this priority**: A vanished pointer that only a restart fixes is exactly the failure Mouser exists to eliminate (constitution Principle I), and it is happening to a real tester now.

**Independent Test**: On a Windows computer that has the pointer from another desk computer, click an application running as administrator so it comes to the front; then cause each fault in the scenarios below; after each, confirm the pointer is visible and usable on the computer the desk user is at, the panic key works, and crossing works again without restarting Mouser.

**Acceptance Scenarios**:

1. **Given** this computer has the pointer from another desk computer, **When** its operating system refuses a click, key, scroll or pointer move that Mouser passes on (for example because an application running as administrator is in front), **Then** that input is skipped, every later input the operating system accepts still arrives, and the desk stays connected.
2. **Given** a fault occurs inside Mouser on a desk computer, **When** the desk user keeps working, **Then** crossing to and from that computer, typing on it, the panic key and pausing keep working without Mouser being restarted.
3. **Given** the pointer has left this computer for another desk computer, **When** this computer's mouse and keyboard stop reaching Mouser (for example because an application running as administrator came to the front, or another program intercepted the mouse and keyboard), **Then** the pointer is visible again on this computer's display and the mouse and keyboard act on this computer.
4. **Given** the pointer is on another desk computer, **When** the connection to it is lost for any reason, including a fault on either computer, **Then** the pointer is visible on the computer the desk user is at and the mouse and keyboard act there.
5. **Given** the pointer is on another desk computer, **When** Mouser on the computer the desk user is at quits (Quit in the menu-bar or tray menu, logging out, shutting down, or restarting for an update), **Then** the pointer is visible on that computer and no key or mouse button stays held down on any desk computer.
6. **Given** the pointer is on another desk computer, **When** Mouser on the computer the desk user is at ends unexpectedly, **Then** the pointer is visible on that computer.
7. **Given** a desk link was lost for any reason, including a fault on either computer, **When** the two computers can reach each other again, **Then** the desk user can cross between them without restarting Mouser on either.
8. **Given** the pointer is on another desk computer that has refused input or had a fault, **When** the desk user presses the panic key or the enable/disable hotkey, or chooses Pause from the menu-bar or tray menu, on the computer they are at, **Then** the pointer and keyboard are back on that computer.

---

### User Story 2 - As a troubleshooting agent, I want the diagnostic log to show what Mouser did and why (Priority: P1)

v0.2.8's log showed a disconnect followed by silence: no crossings, no pointer hiding or showing, no faults, no reason for the drop, and only one computer's side. Diagnostic logging stays opt-in (001 FR-010), but when it is on, the log must be enough to reconstruct an incident, and the agent can gather every desk computer's log in one step. This story amends 001 Story 9 scenario 3 and 001 Story 8 scenario 4.

**Why this priority**: Every other fix in this feature, and every future bug report from testers, depends on being able to read what happened. It is needed to confirm Story 1 on real hardware.

**Independent Test**: Turn logging on for two desk computers, cross back and forth, copy text, change a setting, move a display, pull one computer's network, then collect the desk's logs with the command-line tool and reconstruct the sequence from the bundle alone.

**Acceptance Scenarios**:

1. **Given** logging is on, **When** the agent opens any log file, including one started when the previous file was rotated, **Then** its first record states the Mouser version, the operating system and its version, the computer's id and name, and the desk.
2. **Given** logging is on, **When** control moves between computers (a crossing, the panic key, QuickJump, a pause, or a lost link), **Then** a record names the computer control left and the computer it reached, how it moved, and the application in front on the computer that received control, including whether that application runs as administrator.
3. **Given** logging is on, **When** Mouser hides or shows the pointer on this computer, **Then** a record says which, and why.
4. **Given** logging is on, **When** the operating system refuses an input Mouser passes on, **Then** a record states the kind of input (key, mouse button, pointer move, scroll), the application in front and whether it runs as administrator.
5. **Given** logging is on, **When** a desk link is lost, **Then** a record names the other computer and the reason (the other computer left or quit, became unreachable, runs an incompatible version, a fault on this computer, or this computer quit).
6. **Given** logging is on, **When** a fault occurs inside Mouser, **Then** a record describes the fault and where it happened, and a later record shows Mouser carrying on.
7. **Given** logging is on, **When** a display is added, removed or rearranged on a computer, or the desk owner moves a computer or display in the arranger, **Then** a record states which computer and display changed and how.
8. **Given** logging is on, **When** a setting changes, **Then** a record names the setting, its old and new values, and where it was changed (app window, command-line tool, configuration file, or another desk computer).
9. **Given** logging is on, **When** clipboard content moves between computers, **Then** a record names the two computers and each representation's type and size.
10. **Given** a desk with several computers, **When** the agent asks the command-line tool to collect the desk's logs, **Then** it produces one bundle file on this computer whose contents follow FR-007.

---

### User Story 3 - As a desk owner, I want one Mouser window where I can find everything (Priority: P2)

v0.2.8 has three separate windows (desk, "Mouser health", "Arrange displays") that cannot reach each other, fill up with the same reminders repeated, attribute one Mac's limitations to every computer, and show raw values such as "8.707351000000001 ms" (`reference/mouser-v0.2.8-windows.png`). v2 replaces them with one window in the style of `reference/fluid-voice-style.png`, where problems appear only when the owner can act on them, once, under the computer they concern. This story supersedes the separate desk window, health panel window and arranger window that 001 Stories 1, 4 and 8 were delivered in; what those stories show and do is unchanged unless amended here.

**Why this priority**: The current windows hide real problems among repeated non-problems, so the owner cannot tell what is wrong (constitution Principle II). It does not stop the desk working, so it follows Stories 1 and 2.

**Independent Test**: On a desk with two Macs and a Windows computer, open Mouser from the tray, visit every section from the sidebar and by search, and confirm each notice appears exactly once under the right computer.

**Acceptance Scenarios**:

1. **Given** Mouser is running, **When** the desk owner opens Mouser, **Then** one window opens with a search field and a sidebar whose sections are grouped under headings: "Your desk" with Desk, Displays and Health; "Configure" with Settings; "Support" with Diagnostics and Help. Choosing a section shows it in the same window, and the window opens on Desk.
2. **Given** the window is open, **When** the desk owner chooses a tray-menu item that leads to a part of Mouser (for example arranging displays), **Then** the same window comes to the front at that section, and no second Mouser window opens.
3. **Given** the window is open, **When** the desk owner types in the search field, **Then** matching sections and individual settings are listed, and choosing one opens it.
4. **Given** the Desk section is shown, **When** the desk owner looks at it, **Then** it shows the desk's name and, for each computer, its name, operating system, Mouser version, whether it is connected, which computer has the pointer, and how many problems need the owner's action, each count leading to that computer's entry in Health.
5. **Given** a problem the desk owner can act on is raised for a computer, **When** the owner opens Health, **Then** the problem appears once, in that computer's entry and no other, with its fix as one action where a fix exists.
6. **Given** a computer has platform limitations (001 FR-001), **When** the desk owner opens Health, **Then** they are listed once, under "What doesn't work here" in that computer's entry, and in no other computer's entry and not as a problem notice.
7. **Given** Health shows how responsive a computer is, **When** the desk owner reads it, **Then** the time is in whole milliseconds (for example "Responds quickly (9 ms)").
8. **Given** a desk computer runs a different Mouser version from this one, **When** the desk owner views Desk or Health, **Then** both versions are shown next to that computer; if the versions cannot work together, it says which computer to update and offers the update as one action where this computer can perform it (amends how 001 Story 1 scenario 8's message is shown).
9. **Given** Mouser is launched for the first time, **When** the window opens, **Then** the first-run steps of 001 Story 1 scenario 1 and the desk step run inside the window, and afterwards Health shows this computer's permissions, each with whether it is granted and one action that opens the system screen to grant it.
10. **Given** the Displays section is shown, **When** the desk owner looks at the arranger, **Then** every display's computer name and display name can be read in full, on the display or beside it when the display is drawn too small.
11. **Given** a computer's displays were placed automatically (001 Story 2 scenario 1), **When** the desk owner views Displays, **Then** a message names the computer, says its displays were placed automatically and should be checked against the real desk, and offers to keep the placement; the message goes away when the owner keeps or moves the placement.
12. **Given** the Diagnostics section is shown, **When** the desk owner uses it, **Then** it offers the diagnostic logging switch, opening the log's location, and "Collect logs from the desk", which produces the bundle of FR-007 and shows it in the file manager.
13. **Given** Mouser is running, **When** the desk owner left-clicks the menu-bar or tray icon, **Then** Mouser pauses (or resumes) as in 001 Story 2 scenario 12, and the icon shows the new state; right-clicking the icon opens the menu.
14. **Given** this computer is on a desk with other computers, **When** the desk owner looks at the menu-bar or tray icon, **Then** it shows whether this computer is connected to its desk-mates or unexpectedly disconnected from them (for example a green dot when connected and a red dot when unexpectedly disconnected), using a difference in shape or symbol as well as colour, alongside the active or paused state of 001 Story 1 scenario 10.
15. **Given** this computer is the only computer on its desk, or is on no desk, or the desk owner removed the other computers or left the desk, **When** the desk owner looks at the menu-bar or tray icon, **Then** it does not show the unexpectedly-disconnected state.
16. **Given** software on a Windows computer intercepts Mouser's own checks of its hold on the mouse and keyboard (for example ASUS AsHotplugCtrl), **When** the desk user's mouse and keyboard keep working on that computer, **Then** Health shows no problem about that computer's hold on the mouse and keyboard.
17. **Given** the Help section is shown, **When** the desk owner opens it, **Then** it shows this computer's Mouser version and the troubleshooting guide shipped with Mouser.

---

### User Story 4 - As a desk owner, I want to tailor hotkeys, names and pointer behavior from the Mouser window (Priority: P2)

The panic key and QuickJump hotkeys exist in the configuration file today but cannot be changed in the app; computers can only be told apart by their operating-system names; the landing highlight cannot be turned off; and how Mac Command and Control map onto Windows and Linux cannot be chosen. All of this moves into Settings. It amends 001 Story 2 scenarios 4 and 9, and 001 Story 11.

**Why this priority**: Defaults work (001 FR-004), so these are adjustments, and they need the window from Story 3.

**Independent Test**: In Settings, change the panic key, the QuickJump hotkey, a computer's name, the landing highlight and the Command/Control mapping, and confirm each takes effect without restarting Mouser.

**Acceptance Scenarios**:

1. **Given** Settings is shown, **When** the desk owner records a new key and number of presses for the panic key, **Then** pressing that key that many times returns the pointer and keyboard to the computer the desk user is at, and the previous panic key no longer does.
2. **Given** Settings is shown, **When** the desk owner records a new key combination for QuickJump, enable/disable, or targeted paste by pressing it, **Then** that combination performs the action on this computer and the previous one no longer does.
3. **Given** a desk computer, **When** the desk owner gives it a friendly name from any desk computer's window, **Then** every desk computer shows that name wherever it names that computer, including the arranger, Health, notifications and the command-line tool's status.
4. **Given** the landing highlight is turned off, **When** the pointer crosses to another computer, **Then** no highlight is shown where it lands (amends 001 Story 2 scenario 9, which stays the default).
5. **Given** "Use Command as Control on Windows and Linux" is on (the default, 001 Story 2 scenario 4), **When** the desk user at a Mac presses Command with another key on a Windows or Linux computer, **Then** it acts as Control with that key.
6. **Given** "Use Command as Control on Windows and Linux" is off, **When** the desk user at a Mac presses Command or Control with another key on a Windows or Linux computer, **Then** Command acts as the Windows key and Control acts as Control.

---

### User Story 5 - As a desk user, I want to control a Windows computer at its login screen, elevation prompts and administrator applications (Priority: P2)

With Mouser installed, a Windows computer can be used from the shared mouse and keyboard at its login and lock screen, at permission elevation prompts, while switching users, with Ctrl-Alt-Del, and in applications running as administrator such as VMware Workstation started as administrator. This story brings 001 Story 12 scenarios 2–4 back into scope (superseding the 2026-10-22 deferral) and supersedes 001 accepted platform limitation 4 for installed Mouser. Portable Mouser on Windows can control applications running as administrator once restarted as administrator, and otherwise keeps that limitation, with the defined behavior below.

**Why this priority**: It is the lasting fix for the trigger case with administrator VMware, and the main Windows gap left by v1. Story 1 already keeps the desk usable without it.

**Independent Test**: With Mouser installed on a Windows computer, restart it and log in from another desk computer's keyboard; approve an elevation prompt; switch users; send Ctrl-Alt-Del; start VMware Workstation as administrator and type into it. Then repeat the administrator-application step with portable Mouser.

**Acceptance Scenarios**:

1. **Given** Mouser is installed on a Windows computer that has just started or is locked, **When** the desk user crosses to it, **Then** they can sign in with the shared mouse and keyboard.
2. **Given** Mouser is installed on a Windows computer that shows a permission elevation prompt or a user-switching screen, **When** the desk user crosses to it, **Then** they can operate that screen, and after a switch the newly signed-in user's session is controllable.
3. **Given** Mouser is installed and the active computer runs Windows, **When** the desk user chooses Send Ctrl-Alt-Del from the tray menu, **Then** that computer responds as if Ctrl-Alt-Del had been pressed on its own keyboard; where that computer's policy prevents it, its Health entry states so under "What doesn't work here".
4. **Given** Mouser is installed on a Windows computer and an application running as administrator is in front, **When** the desk user crosses to it and types, clicks, scrolls and moves the pointer, **Then** the application receives that input.
5. **Given** Mouser is installed on a Windows computer the desk user is physically at and an application running as administrator is in front, **When** they move the pointer past an edge, press the panic key or press a Mouser hotkey, **Then** it behaves as with any other application in front.
6. **Given** portable Mouser runs on a Windows computer that has the pointer from another desk computer, **When** an application running as administrator comes to the front there, **Then** the pointer and keyboard return to the computer they came from, and that computer shows a message naming the application and the Windows computer, saying portable Mouser cannot control applications running as administrator, and offering "Restart Mouser as administrator" as one action, with installing Mouser named as the way to also control the login screen, elevation prompts and Ctrl-Alt-Del.
7. **Given** the desk owner chose "Restart Mouser as administrator" and approved Windows' prompt, **When** the desk user crosses to that Windows computer with an application running as administrator in front, **Then** the application receives their input, as in scenario 4, and moving past an edge, the panic key and Mouser hotkeys work there, as in scenario 5.
8. **Given** portable Mouser runs on a Windows computer, **When** the desk owner opens its Health entry, **Then** "What doesn't work here" states that portable mode cannot control the login screen, permission prompts or Ctrl-Alt-Del, and, unless Mouser was restarted as administrator, applications running as administrator.

---

### User Story 6 - As a desk owner, I want Mouser to run without its window or tray on Windows and Linux (Priority: P3)

In v1, headless mode (no window, no tray) works only on macOS and refuses to start on Windows and Linux (001 Story 12 note). v2 allows it on all three. On Windows, installed Mouser's part that runs before anyone signs in (Story 5) is headless. This supersedes the 001 Story 12 note that headless mode refuses to start on Windows and Linux.

**Why this priority**: Needed for computers without a usable tray and for control before sign-in; less common than everyday use.

**Independent Test**: Start Mouser in headless mode on a Windows computer and on a Linux computer, confirm no window or tray appears, and cross to each from another desk computer.

**Acceptance Scenarios**:

1. **Given** a Windows or Linux computer on a desk, **When** the desk owner starts Mouser in headless mode, **Then** no window or tray icon appears, the computer is on its desk, the desk user can cross to it, and other desk computers list it as connected.
2. **Given** Mouser runs on a Linux computer, **When** the desk owner opens its Health entry, **Then** "What doesn't work here" states that Mouser cannot control this computer's login screen (FR-006).

---

### User Story 7 - As a Mouser developer, I want every v1 behavior still unverified on real hardware checked on real hardware (Priority: P3)

001's evidence record (`specs/001-mouser-v1/evidence/quickstart.md`) leaves several checks pending real hardware. v2 closes them, together with the trigger case for Stories 1 and 5, using the hardware available: the owner's two Macs, Parallels Windows 11 virtual machine and Lima Ubuntu virtual machine; Steve's two Windows PCs with VMware Workstation and a Linux user's computer add optional confirmation.

**Why this priority**: Verification of behavior already built; it needs Stories 1–6 in place to verify the v2 parts.

**Independent Test**: Read the evidence record and confirm every check below has a dated real-hardware outcome.

**Acceptance Scenarios**:

1. **Given** a check in this list: Wi-Fi/Ethernet switch and Wi-Fi toggle while controlling a computer; VPN connect and disconnect; unplugging a computer's network while it has the pointer; portable Mouser on Windows from a USB drive (join, eject, nothing left behind); a real Linux computer on X11, GNOME Wayland and KDE Wayland; locking and unlocking a Mac, and the Mac login window, from the shared keyboard; the Windows login screen, elevation prompt, user switching and Ctrl-Alt-Del of Story 5; portable Mouser restarted as administrator controlling an application running as administrator (Story 5 scenario 7); installing Mouser on Windows past the unknown-publisher warning (SC-010); and the trigger case of FR-011 with an application run as administrator and not, with Mouser installed and portable. **When** the check is run on the testing hardware (the Parallels Windows 11 virtual machine counts for Windows checks and the Lima Ubuntu virtual machine for Linux checks), **Then** the evidence record states the outcome, the hardware and operating system, the Mouser version, the date and who ran it, and a failure is recorded as a defect against the story it breaks.

---

### Edge Cases

- A key or mouse button is held when the operating system starts refusing input on the computer with the pointer: no key or button stays held on any computer once control leaves it (001 edge case on held keys still applies). (Story 1, scenario 1)
- A virtual machine application on the active computer captures the pointer itself (for example VMware Workstation after a click into a running virtual machine): Mouser does not leave the pointer hidden; the virtual machine application's own release keys release its capture; whether the virtual machine receives Mouser's input is 001 accepted platform limitation 1, which v2 keeps. (Story 1, Story 7)
- Mouser quits on a computer that has the pointer from another computer: the other computer sees it leave and the pointer and keyboard are back on the computer the desk user is at (Story 1, scenarios 4 and 5).
- The diagnostic log rotates in the middle of an incident: the new file starts with the record of Story 2 scenario 1, and the bundle includes both files (Story 2, FR-008).
- A desk computer is offline, or has logging off, when logs are collected: the bundle lists it with that reason (Story 2, Story 3, FR-007).
- A computer is renamed while another desk computer is offline: that computer shows the new name once it reconnects (Story 4, scenario 3).
- An elevation prompt appears on a Windows computer while the pointer is on another computer: the desk user crosses to it and operates it (Story 5, scenario 2).
- A desk computer is mid-update to a newer version: it shows both versions until the update completes (Story 3, scenario 8).
- A desk-mate goes offline while this computer is paused: the icon shows paused and the unexpectedly-disconnected state together (Story 3, scenarios 13 and 14).
- Mouser is opened again (a second launch) on a Windows computer where portable Mouser was restarted as administrator: the running Mouser's window comes to the front; where Windows does not let the second launch reach the running Mouser, the second launch says that Mouser is already running as administrator on this computer and that its window opens from the menu-bar or tray icon, and then closes; no second Mouser keeps running (Story 5, scenario 7; 001 single-instance behaviour).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001** (Stories 1–7): Every behavior specified in `specs/001-mouser-v1/spec.md` continues to hold except where this specification states that it amends or supersedes a named story, scenario, requirement, or accepted limitation of 001.
- **FR-002** (Stories 1, 5): No refused input, refused operating-system request, or fault inside Mouser ends a desk computer's ability to cross, receive input, reconnect, or respond to the panic key, pause, or the enable/disable hotkey.
- **FR-003** (Stories 2, 3): Diagnostic logs and log bundles never contain typed characters or which keys were pressed, clipboard contents, file contents or file names, or window titles. Applications are identified by name only.
- **FR-004** (Story 5): Feature 002 ships Mouser for Windows without a code signature. Windows' unknown-publisher warning when installing or first running Mouser is an accepted limitation, and Mouser's install instructions and release notes tell the desk owner how to proceed past it. Every behavior of Story 5 works without a code signature.
- **FR-005** (Story 5): Portable Mouser on Windows offers "Restart Mouser as administrator" only from the message of Story 5 scenario 6; restarting it as administrator never needs anything installed, and Mouser stays portable afterwards (001 Story 13).
- **FR-006** (Story 6): Headless Mouser on Linux runs only inside a signed-in session. It does not control the Linux login screen; that is a platform limitation stated on every Linux computer (001 FR-001).
- **FR-007** (Stories 2, 3): A collected log bundle is one file saved on the computer where collection was requested and is not sent anywhere else. It is written by the desk owner's own Mouser, as the desk owner, to their Downloads folder or a folder they choose; on a Windows computer with Mouser installed, the part of Mouser that runs before anyone signs in never writes it to a folder someone chose. It contains, for every desk computer: its name and id, its Mouser version and operating system, its connection status, its current settings, and its diagnostic log files, or, where a computer's log could not be included, the reason (offline, logging off). It never contains desk keys or other secrets.
- **FR-008** (Story 2): The diagnostic log on each computer rotates so that its files together stay within the configured log size limit (001 `diagnostics.max_log_mb`), dropping the oldest records first.
- **FR-009** (Story 3): Numbers shown in the window (Story 3 scenario 7 is one instance) are rounded to the precision a person needs: durations in whole milliseconds or seconds, sizes in whole or one-decimal units.
- **FR-010** (Story 3): Problem notices are shown only for problems the desk owner can act on, and each appears once in the window. This supersedes how 001 presented platform limitations (as notice cards, repeated, under every computer) but not 001 FR-001's requirement that they are stated in the app; Story 3 scenario 6 states where.
- **FR-011** (Stories 1, 3, 5, 7): Reference artifacts are used as follows. The layout and navigation of `reference/fluid-voice-style.png` (one window, search at the top of a sidebar, sections grouped under headings, a home overview as the first section) are normative for Story 3; its colours, fonts, wording and content are not. `reference/mouser-v0.2.8-windows.png` records the defects Story 3 removes and is not a design to keep. The incident recorded in `reference/vmware-cursor-investigation.md` ("The log" section: an application running as administrator comes to the front on a Windows computer receiving control, the link drops and never returns, the pointer stays hidden, the panic key does nothing) is the trigger case Stories 1 and 5 must pass on the testing hardware, where any application running as administrator stands in for VMware Workstation; its analysis and fix plan are non-binding and belong to the Plan-Phase Handoff.
- **FR-012** (Stories 3, 4): Friendly computer names, the landing highlight setting, and the Command/Control mapping setting are shared by the whole desk; hotkeys and the panic key are set per computer.
- **FR-013** (Story 5): Installing Mouser on Windows needs one administrator approval and gives every behavior of Story 5 with no further step; uninstalling it leaves no part of Mouser running on that computer.

### Key Entities

- **Log record**: One line of the diagnostic log (001 Story 9). v2 adds records for control changes, pointer hide and show, refused input, link loss with reason, faults, display and layout changes, setting changes, and clipboard transfers; every file starts with an identifying record.
- **Log bundle**: One file gathering the whole desk's diagnostics at one moment (FR-007).
- **Problem notice**: Something the desk owner can act on, attributed to exactly one computer, with a fix where one exists. Distinct from a platform limitation.
- **Platform limitation**: Something that does not work on a given computer because of its operating system or mode (001 FR-001), listed under that computer only.
- **Computer**: Gains a desk-wide friendly name, and its Mouser version is shown to the desk owner.
- **Connection state**: Whether this computer is connected to its desk-mates, intentionally alone (no desk-mates, or left the desk), or unexpectedly disconnected; shown on the menu-bar or tray icon.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: In 20 consecutive repetitions of the trigger case (FR-011) on a Windows computer (the owner's Parallels Windows 11 virtual machine suffices; Clarifications), with Mouser installed and with it portable, the desk user has a visible, working pointer and keyboard on a computer they can use in 20 of 20, and can cross between the two computers afterwards without restarting Mouser in 20 of 20, within 001 SC-008's 5 seconds of the computers being able to reach each other. (Stories 1, 5)
- **SC-002**: For each situation in Story 1 (refused input, fault inside Mouser, input intercepted, link lost, quit, unexpected end), zero runs end with the pointer hidden on the computer the desk user is at. (Story 1)
- **SC-003**: For every Story 1 situation reproduced with logging on, a troubleshooting agent given only the collected bundle names the computer involved, the time, the cause, and the application in front, in 100% of reproductions. (Story 2)
- **SC-004**: After a session in which a known marker string is typed and copied on every desk computer with logging on, searching every log and the bundle for the marker finds zero matches. (Story 2, FR-003)
- **SC-005**: From the open window, the desk owner reaches every section and every setting in at most two clicks, or by typing its name in search. (Stories 3, 4)
- **SC-006**: On a desk of two Macs, a Windows computer and a Linux computer, the window shows every notice and limitation exactly once, under the computer it concerns: zero duplicates, zero misattributions, zero unrounded numbers. (Story 3)
- **SC-007**: A desk owner new to v2 renames a computer, changes the panic key, turns off the landing highlight, and collects the desk's logs without documentation. (Stories 3, 4)
- **SC-008**: Over a session in which a desk-mate is disconnected unexpectedly, reconnected, and then removed from the desk, the menu-bar or tray icon shows the unexpectedly-disconnected state exactly while the desk-mate is unexpectedly disconnected, and the state can be told apart in a greyscale screenshot. (Story 3)
- **SC-009**: On a Windows computer with Mouser installed, the desk user signs in after a cold boot, approves an elevation prompt, switches users, sends Ctrl-Alt-Del, and types into an application running as administrator, using only the shared mouse and keyboard from another desk computer. (Story 5)
- **SC-010**: A desk owner new to Mouser installs it on a Windows computer past Windows' unknown-publisher warning using only Mouser's install instructions or release notes. (Story 5, FR-004)
- **SC-011**: Before v2 is released, every check in Story 7 has a recorded real-hardware outcome. (Story 7)

## Assumptions

- Baseline: 001 as shipped in v0.2.8. 001's accepted platform limitations 1, 2, 3 and 5 are unchanged; limitation 4 is superseded for installed Mouser on Windows and kept for portable Mouser (Story 5). 001's out-of-scope list and its Story 7 future item (a scratch folder for cancelled drags) remain out of scope.
- No new timing targets were given by the owner (constitution Principle 2.7). Reconnection after a lost link uses 001 SC-008's existing 5-second target; restoring the pointer and returning control in Story 1 has no time target beyond happening without action by the desk user.
- The 66 ms between the administrator application coming to the front and the link dropping in the trigger case is evidence of the incident, not a requirement.
- "Unexpectedly disconnected" (Story 3 scenarios 14–15) means at least one desk-mate is not connected for a reason other than the desk owner's own action (removing it, leaving the desk) or pausing; a desk-mate showing as reconnecting or offline per 001 counts. Decided here from the owner's description.
- Story 3 scenario 6 applies to limitations about something that cannot be done on that computer; a statement that is true of every non-Windows computer and is not actionable (for example "Ctrl-Alt-Del can only be sent to Windows computers" on a Mac) is a limitation of that Mac only and appears only in its entry.
- The left click on the menu-bar or tray icon pausing (Story 3 scenario 13) follows 001 FR-001: where a Linux desktop's tray cannot tell left from right clicks, that is stated as a limitation on that computer.
- Diagnostic logging remains per computer and off by default; the bundle includes a computer's log only if logging was on there.
- Desk computers are on one trusted local network (constitution Principle III); the log bundle travels between desk computers with the same encryption as everything else (001 FR-002) and leaves the desk only when the owner sends it.
- No new persona or shared term is needed. The shared term "Health panel" in `.specify/memory/personas.md` now refers to the Health section of the Mouser window; its meaning is unchanged.
- `reference/fluid-voice-style.png` has the owner's name in its greeting and the text of its "Recent dictations" entries blacked out (2026-10-26); only its layout is used (FR-011).
- Added accepted platform limitations for v2 (owner decisions 2026-10-26), shown in the app per 001 FR-001: Windows shows an unknown-publisher warning when installing or first running Mouser (FR-004); Mouser cannot control a Linux computer's login screen (FR-006); on a Linux computer using Wayland, Mouser cannot tell which application is in front, so its diagnostic log does not name it (Story 2 scenarios 2 and 4). Code signing for Windows is future work outside 002.
- Testing hardware: the owner's two Macs, Parallels Windows 11 virtual machine and Lima Ubuntu virtual machine with a desktop session (owner decision 2026-10-27; it counts as "the Linux computer" for every gating check); Steve's two real Windows PCs with VMware Workstation and a Linux user's own computer are optional extra confirmation and gate nothing.

## Plan-Phase Handoff

Non-binding hypotheses for the plan phase to confirm, refute, or supersede (constitution Principle 2.6). Source: `reference/vmware-cursor-investigation.md` unless noted.

- Trigger-case root cause (hypothesis H1): the receiving Windows PC's injected input was refused by UIPI after a click brought administrator VMware to the front; injection failures are `.expect` panics while the `crossing` mutex is held; the panic poisons `crossing`, then `core` during `on_link_closed`, so `drop_lost`/`return_local` never run, the input thread and `reconcile_loop` die, and the PC never reconnects. Alternative H2: the other PC dropped first; the other PC's log decides. There is no panic hook, so panics are never logged.
- Windows cursor hiding uses `SetSystemCursor` (global, outlives the process); restore is `SPI_SETCURSORS`, and `recover_at_startup` runs only at the next launch. An unexpected end (Story 1 scenario 6) therefore likely needs something outside the process, or a restore that does not depend on the process surviving.
- Tray Quit calls `app.exit(0)` without `Engine::shutdown`; the engine lives in Tauri managed state, so `WinCapture::drop` probably does not run.
- The watchdog skips its probe when the foreground is elevated (so blindness is never detected); `Platform::foreground_elevated()` is never called outside tests; the W3 "administrator application" problem is not implemented.
- Hooks do not check `LLMHF_INJECTED`; injected motion uses `SetCursorPos`, which raw-input applications such as VMware do not see; injected keys carry `LLKHF_INJECTED`.
- Third-party low-level hooks (ASUS AsHotplugCtrl) may swallow the watchdog's tagged probe events; a probe miss should be judged against real input still arriving, not the probe alone.
- Story 5's design is in 001 tasks T367 (hardware probe 2: winlogon-token agent at cold boot, UAC and fast user switching; `SendSAS` with `SoftwareSASGeneration` 1 vs 3) and T415–T426 (a LocalSystem service `MouserService` running a headless engine with machine-scope desk secrets, a per-session agent started with a duplicated `winlogon.exe` token that follows the input desktop, a named pipe restricted to SYSTEM and the agent's session, `SendSAS(FALSE)`). Controlling elevated windows needs a high-integrity agent. Because 002 ships unsigned (FR-004), `uiAccess=true` manifest approaches are ruled out (they need a signed binary in a secure location); the installed SYSTEM service and its agent are the route for installed Mouser, and a portable Mouser restarted as administrator (high integrity, FR-005) is the route for portable mode. `apps/mouser-service-win` is a stub.
- Future, out of scope for 002: SignPath Foundation offers free code signing for open-source projects (the repository is public, MIT licensed); worth revisiting to remove the FR-004 limitation.
- Existing configuration keys (001 `contracts/config.md`) already hold `hotkeys.panic`, `hotkeys.quickjump`, `hotkeys.enable_disable`, `hotkeys.targeted_paste` and `general.computer_name` (per computer); a desk-shared friendly name (FR-012) likely moves naming into desk state. Today's log rotation keeps one previous file (`mouser.log.1`).
- The macOS headless mode is `mouser-app --headless` reading a machine folder copy of config and secrets (docs/configuration.md); Windows headless is expected to be the service from Story 5.
- `docs/troubleshooting.md` still says the Windows backend is not implemented and that crossings are not logged; the Polish phase should correct it.
