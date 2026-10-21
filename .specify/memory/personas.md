# Mouser Personas

Personas and shared domain terms for Mouser, per constitution Principle 2.2. Every user story names one of these personas; acceptance criteria must be confirmable from that persona's vantage.

## Personas

### Desk user

- **Role**: Works across two or more computers (macOS, Windows, Linux) at one physical desk using a single mouse and keyboard. Cares that the pointer crosses to the right screen when moved there, that typing goes where the pointer is, that copy/paste and dragged files arrive on the other computer, and that none of this drops out mid-task.
- **Vantage**: The pointer and its position on each physical display; what typed keys do in the focused app; pasted and dropped content; the menu bar / system tray icon and its menu; on-screen notifications and overlays (pointer locator, QuickJump letters, transfer progress); the panic key's effect.
- **Horizon**: How computers find or talk to each other, network adapters and ports, encryption, protocols, configuration files, logs, process structure, and OS input APIs.

### Desk owner

- **Role**: Sets up and maintains Mouser on their computers: installs it, grants OS permissions, creates or joins a desk, arranges displays to match the physical desk, adjusts behavior, and fixes it when something stops working. Cares that setup is quick, that settings are findable where they apply, and that when something is wrong the app says what and offers the fix. Usually the same human as the desk user, at a different moment.
- **Vantage**: Everything the desk user sees, plus: the first-run flow, the OS permission prompts and Mouser's guided permission steps, the display arranger, desk membership screens (desks found nearby, join/create/leave), settings screens, the status and health panel, the diagnostic logging checkbox, update prompts, and install/uninstall.
- **Horizon**: Wire protocols, discovery and transport mechanics, key exchange, internal state, source code, and which component produced a given behavior.

### Troubleshooting agent

- **Role**: An AI coding agent (e.g. Claude Code) that the desk owner asks to work out why Mouser is misbehaving, or to change its configuration. Cares that it can read what happened and what is configured without a GUI, and can change settings safely.
- **Vantage**: The `mouser` command-line tool and its output; the plain-text configuration file; diagnostic log files (JSON Lines) once logging is enabled; documentation shipped with Mouser.
- **Horizon**: The GUI, what is on the desk user's screens, Mouser's source code and internal data structures, and anything not written to the CLI output, config file, or logs.

### Mouser developer

- **Role**: Builds, tests, and releases Mouser for all three operating systems. Cares that builds and tests pass on each platform, that reliability regressions are caught before release, and that signed, notarized installers come out the other end.
- **Vantage**: A terminal running the build, test, lint, and packaging commands and their output; CI check statuses; produced installers and their signature/notarization results; test reports, including simulated-network test results.
- **Horizon**: End users' machines and networks, the desk user's live experience, and anything not reported by a build, test, or packaging command.

## Shared terms

- **Desk**: A named group of computers that share one mouse and keyboard. A computer belongs to at most one desk at a time; computers on other desks on the same network are not controlled by and do not receive clipboard or files from this one.
- **Computer**: A machine running Mouser, shown by its name (the OS computer name unless renamed in Mouser).
- **Display**: One physical screen attached to a computer, as shown in the display arranger.
- **Layout**: The arrangement of every display on a desk, as shown and edited in the display arranger; it decides which edge leads to which display.
- **Crossing**: The pointer leaving one display's edge and appearing on the adjacent display, possibly on another computer.
- **Active computer**: The computer that currently has the pointer, and therefore receives keyboard input.
- **Blocked crossing**: A crossing that Mouser deliberately prevented (for example in a display corner); the desk user can override it by bumping the edge twice.
- **Panic key**: The key press (Esc twice by default) that returns the pointer and keyboard to the computer the desk user is physically at.
- **QuickJump**: A hotkey that shows a letter on every display; pressing a letter moves the pointer to that display.
- **Health panel**: The screen listing each computer on the desk with its connection status, responsiveness, missing permissions, and the most recent problem in plain language.
