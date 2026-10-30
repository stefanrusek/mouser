# Troubleshooting

For desk owners and for agents helping them. Start with the first section, then look up the message you see.

Status in one paragraph (v2 work, version still 0.2.x until the 0.3.0 release): macOS is tested on real Macs. Windows has a working backend (hooks, injection, clipboard, file drag, portable mode), tested in a Windows 11 virtual machine and on a tester's PCs. Linux is tested in virtual machines and against fake portals, not on real desktops. Releases are GitHub pre-releases.

Not available yet, so do not expect it to work: on a Windows computer with Mouser installed (the `MouserService` service), the Mouser window cannot yet run a notice's fix, answer a "replace this file?" question, show the landing flash, show QuickJump or ask for a review of placement; use the `mouser` command for what it covers. The per-machine Windows installer that sets the service up is not built yet either, and the login screen, permission-prompt (UAC), user-switching and drag-as-SYSTEM behaviour of an installed Windows Mouser has not been verified on real hardware.

## First steps

1. Ask Mouser what is wrong:

   ```
   mouser status
   mouser status --json
   ```

   It lists the desk, each computer's connection (`connected`, `reconnecting`, `offline`, `incompatible`), any missing permission and every open problem with its fix. Each problem is listed once, under the computer it is about. Platform limitations are not problems: they are listed under "What doesn't work here" in that computer's entry in the Mouser window's Health section (and in `mouser status --json`). If it prints `Mouser is not running` (exit code 2), start the app.
2. If the cause is not obvious, turn on the log, reproduce the problem, and read the log:

   ```
   mouser logs enable
   mouser logs tail -n 50
   mouser logs path
   ```

   To gather the logs of every computer on the desk into one file, run `mouser logs collect` (see "Collecting the desk's logs" below).
3. Run `mouser status` on **each** computer on the desk. Each one only reports its own view and its own permissions.

Quick checks that fix many "it won't connect" reports: both computers are on the same network and awake; the UDP port (default 47801, `advanced.port`) is not blocked by a firewall; you are not running ShareMouse at the same time (two programs capturing input fight each other); both computers run compatible Mouser versions.

## Problem kinds

Every notice in the app, and every entry under Problems in `mouser status`, has one of these eight kinds. `fix` is the one-click action in the app; the CLI shows it as `(fix: ...)`. A problem is shown once, under the computer it concerns, and only when you can act on it.

### missing_permission

Mouser cannot do its job without an OS permission. Mouser does not start capturing or injecting input until nothing is Missing, and re-checks every 250 ms, so granting it works without a restart. Fix: `open_system_settings:<permission>`; in the app, click the fix and the right settings pane opens.

- accessibility: Mouser needs Accessibility access to move the pointer and type on this computer. Where: macOS: System Settings > Privacy & Security > Accessibility
- input_monitoring: Mouser needs Input Monitoring access to see the mouse and keyboard on this computer. Where: macOS: Input Monitoring
- screen_recording: Mouser needs Screen Recording access to see where full-screen windows are. Where: macOS: Screen Recording
- full_disk_access: Mouser needs Full Disk Access to copy files to and from this computer. Where: macOS: Full Disk Access
- input_capture_portal: Mouser needs permission to capture input through the desktop's input portal. Where: Linux Wayland
- remote_desktop_portal: Mouser needs permission to control this computer through the desktop's remote-control portal. Where: Linux Wayland

If you rebuilt or reinstalled an unsigned Mouser on macOS, the grants are reset; the released, notarized app keeps them across updates. Allow local network access when macOS asks, or computers cannot find each other.

### incompatible_version

> `<A>` runs Mouser `<X>` and `<B>` runs Mouser `<Y>`, and those versions cannot work together. Update `<the older computer>`.

Cause: the two computers speak different protocol versions. The computer shows as `incompatible` in `mouser status`; a join attempt fails with this text. Fix: update the older one. If it is this computer, the fix is `open_update`: run `mouser update` or use the tray menu. Install the same release on every computer. The Mouser window shows both versions next to the computer and names the one to update; when the computer to update is this one and an update is available, the fix is `update_computer:<id>` (one click). 0.2.x and 0.3.x computers can work together; the 0.3.0 features that need both ends (desk-wide names and settings, collecting logs, "Restart Mouser as administrator" from another computer) are not sent to a 0.2.x computer.

### join_refused

Raised on the computer that asked to join, or on the desk computer that turned someone away. Messages:

- This computer was not admitted to `<desk>`.: Someone clicked Decline (or `mouser decline`). Ask again and approve on a computer already on the desk.
- `<name>` (`<ip>`) was refused because its address is not on the allowlist.: The desk has `advanced.allowlist` set. Add the joiner's network to it on a desk computer (`mouser config set advanced.allowlist 192.168.1.0/24`) or empty it.
- The request to join the desk was not answered.: The desk computer went away while waiting. Check it is on and try again.
- The request to join the desk was interrupted when Mouser stopped.: Mouser on this computer was quit during the request. Join again.
- (the incompatible_version text): The desk computer runs an incompatible version.

A join needs a person to approve: on a desk computer, click Allow, or `mouser requests` then `mouser approve <number>`.

### secure_input (macOS)

> `<app>` is blocking Mouser from typing on `<computer>` (Secure Input). Switch away from `<app>` or close it and typing works again.

Cause: an app (usually a password field, or a terminal or password manager) turned on macOS Secure Input, which hides keystrokes from everyone else. It is a normal state, not a disconnect. Fix: switch away from the named app or close it.

### crossing_blocked

> The pointer is in a screen corner, and corners do not cross. To cross anyway, push the same edge twice in a row. To change this, open the edge in the display arranger.

> A full-screen application is in front, and it blocks crossing. To cross anyway, push the same edge twice in a row. To change this, open the edge in the display arranger.

Cause: corner blocking (corners are 8 logical pixels) or full-screen blocking is on for that edge. Fix: push the same edge twice within 600 ms to cross, or open that edge in the arranger and change its options (`crossing.block_fullscreen` turns full-screen blocking off for the whole desk). The notice is per edge. It clears when you cross, when you apply its fix, or when the edge stops blocking.

### peer_offline

> `<name>` is offline. Make sure it is turned on, awake and on the same network.

Cause: a desk computer has been unreachable for more than 5 seconds (it shows as `reconnecting` before that). Connections heal themselves after sleep, Wi-Fi/Ethernet changes and IP changes, so this clears by itself when the computer comes back. If it does not: check the computer is awake, on the same network and not blocked by a firewall; run `mouser status` there. The `peer.disconnected` records in the log show when the link dropped.

### elevated_app (Windows)

> Portable Mouser on `<computer>` can't control `<app>` because it runs as administrator. Restart Mouser as administrator to control it, or install Mouser to also control the login screen, permission prompts and Ctrl-Alt-Del.

> Mouser on `<computer>` can't control `<app>` because it runs as administrator. Install Mouser to control it.

Cause: an application running as administrator (for example an elevated Command Prompt, Task Manager, or VMware Workstation started as administrator) came to the front on a Windows computer that was receiving your mouse and keyboard. Windows does not let a normal program send input to it, and does not report the refusal. Mouser notices which application is in front, hands the pointer and keyboard back to the computer they came from, and raises this problem on the Windows computer, naming the application. The notice stays until control next enters that computer without such an application in front.

Fix: for a portable Mouser that is not yet elevated, the fix is `restart_elevated:<computer id>` ("Restart Mouser as administrator"). Windows shows its permission prompt on that PC's own screen, so approve it at that PC's keyboard and mouse. If you decline, nothing changes and the notice stays. After the restart Mouser is still portable, the data folder is the same, and administrator applications can be controlled. Where the text has no fix (the second message), the Mouser on that PC cannot do it: the installed Mouser (service) is meant to control administrator applications, but that is not available yet (see the status paragraph at the top). If you start Mouser a second time on a PC where portable Mouser runs as administrator, the second launch says "Mouser is already running as administrator on this computer. Open it from the tray icon." and closes.

### platform_limitation

A thing this operating system does not allow. It is shown on the affected computer and is not a fault. There is no fix action. In v2 limitations are not raised as problems; they are listed under "What doesn't work here" in that computer's Health entry. The kind remains in the status format for older computers. The exact texts are in the next section.

## Platform limitations, with their exact texts

These are the five limitations the owner accepted for v1, the two added for v2 (Windows shows an unknown-publisher warning at install; Mouser cannot control a Linux computer's login screen), and the detail texts the backends produce. Each text is listed only under the computer it is about. v1 limitation 4 (Windows portable cannot control the login screen, permission prompts, Ctrl-Alt-Del or administrator applications) is superseded for an installed Windows Mouser, which is not available yet, and kept for portable Mouser, which can control administrator applications once restarted as administrator.

### macOS

- inject: Some games and remote-desktop apps ignore Mouser's input on this Mac.
- secure attention: Ctrl-Alt-Del can only be sent to Windows computers.
- capture (portable mode only): macOS permissions must be granted again on each computer you run Mouser from this drive on.

Why: macOS only lets an unentitled app post events with `CGEventPost`; apps that read hardware-level input (some games, remote-desktop clients) never see them.

### Linux (all sessions)

- file drag: Dragging files between computers isn't available on Linux; use copy and paste
- secure attention: Ctrl-Alt-Del can only be sent to Windows computers.
- tray (no tray host on the desktop): This desktop shows no tray icon for Mouser. Install the AppIndicator extension to get one; the tray menu actions are also in the Mouser window.
- tray (a tray host is present): Clicking the tray icon doesn't pause Mouser here; use Pause in its menu.
- login screen: Mouser can't control this computer's login screen.

Linux headless mode (`mouser-app --headless`) runs only inside a signed-in session, so the login-screen text always applies on Linux.

### Linux, Wayland (GNOME and KDE)

- This desktop cannot share its mouse and keyboard with other computers. Mouser needs GNOME 46 or later or KDE Plasma 6.1 or later on Wayland. (The input-capture portal is missing. Sway, Hyprland, COSMIC and older desktops are unsupported.)
- This desktop cannot share its keyboard with other computers. / This desktop cannot share its mouse with other computers. (The portal offers only one of the two.)
- This desktop asks you to approve Mouser each time you sign in before it can share this computer's mouse and keyboard. (Expected on GNOME 46/50 and Plasma 6.7.5: the permission cannot be remembered. The approval dialog appears when capture starts and can take several seconds (6 to 40 seconds were seen).)
- This desktop does not let Mouser move the mouse or type here, so other computers cannot control this one. (The remote-desktop portal is missing.)
- Mouser cannot tell when an app is full-screen on Wayland, so the pointer may still cross to another computer during a game or video. (Full-screen blocking is unavailable.)
- QuickJump and hotkey crossings can't start from this computer on Wayland; push the pointer off the screen edge instead. (On Wayland only an edge push starts a crossing.)
- Mouser can't tell which application is in front here, so its diagnostic log doesn't name it. (On Wayland the log's `app` fields (control changes, refused input) are left out. X11 names the application.)
- Mouser cannot share the clipboard from this computer while it shares its mouse. This needs GNOME 51 or later. (GNOME before 51 (or an unknown version).)
- This desktop keeps Mouser's permission after you remove the drive; remove it in the desktop's settings if you do not want it to stay. (Portable mode on Wayland.)

Also on Wayland: pressing into the edge while dragging a file cancels the drag on GNOME 46. Wayland displays have scale 1.0 and no physical size.

### Linux, X11

No extra limitations are listed. Behaviour to know: Mouser cannot take the pointer grab while another program holds one, for example while a GTK menu is open; it succeeds as soon as the menu closes. Scale is fixed at 1.0 and one wheel click is 15 pixels.

### Windows

- every Windows computer: Some games and remote-desktop apps ignore Mouser's input on this PC.
- every Windows computer: Windows warns that Mouser comes from an unknown publisher when it is installed or first run.
- portable, not elevated: Portable Mouser can't control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator.
- portable, restarted as administrator: Portable Mouser can't control the login screen, permission prompts or Ctrl-Alt-Del.
- installed, with the service running, and policy blocks software Ctrl-Alt-Del: Ctrl-Alt-Del is blocked by this PC's policy.
- installed, without the service: Ctrl-Alt-Del can't be sent to this PC yet.

The installer and the exe are unsigned (a code-signing certificate is future work), so Windows shows its unknown-publisher warning ("Windows protected your PC"); see [releasing.md](releasing.md) for the click-through. Other Windows messages: "Other computers can't reach this PC because Windows Firewall blocks Mouser. Allow Mouser in Windows Security > Firewall & network protection > Allow an app through firewall." and "Mouser lost its hold on the keyboard and mouse and took it back; input may have briefly leaked to this PC." (raised only after real input was missed; software such as ASUS AsHotplugCtrl that swallows Mouser's own checks while your input keeps arriving no longer triggers it).

What an installed Windows Mouser is designed to do, and what is not available yet:

- Design: `MouserService` (LocalSystem, starts with Windows) runs a headless session engine on the console session, which follows the input desktop to the login screen and permission prompts, sends Ctrl-Alt-Del, and controls administrator applications. The code for this exists (service, session engine, `mouser-service.exe --install|--uninstall`); the per-machine installer that runs it is not built, and the checks on real hardware (login screen, permission prompts, user switching, dragging files as SYSTEM) are unverified. Do not rely on any of it yet.
- With Mouser installed and `MouserService` running, the Mouser window can't yet run a notice's fix, answer a file "replace?" prompt, show the landing flash, show QuickJump or the review-placement message.
- Uninstalling Mouser keeps `C:\ProgramData\Mouser` (the computer's keys and desk membership), so upgrades and reinstalls keep the computer on its desk. If you delete that folder after uninstalling, the computer forgets its desk and must join again with one approval. The folder holds secrets: only SYSTEM and Administrators should be able to open `secrets` in it (whether the installer sets this has not been confirmed on a real install).

## The diagnostic log

Logging is off by default. Turn it on with `mouser logs enable`, the logging switch in the Diagnostics section of the Mouser window, or `diagnostics.logging = true` in config.toml. Nothing is written to the system console or log.

Location (`mouser logs path` prints it):

- macOS: `~/Library/Application Support/Mouser/logs/mouser.log`
- Linux: `$XDG_CONFIG_HOME/mouser/logs/mouser.log` (or `~/.config/mouser/logs/mouser.log`)
- Windows: `%APPDATA%\Mouser\logs\mouser.log`
- Portable: `MouserData/logs/mouser.log`
- Windows with the service (installed): `C:\ProgramData\Mouser\logs\mouser.log`, plus the service's own `service.log` beside it (not confirmed on real hardware)

Rotation: there are two files, `mouser.log` and `mouser.log.1`, each at most half of `diagnostics.max_log_mb` (default 50, so 25 MB each), so together they stay within the limit. When `mouser.log` would pass its half, it is renamed `mouser.log.1`, replacing the previous one (the oldest records go first), and a new `mouser.log` starts. `service.log` rotates the same way.

Every file, including one started by a rotation, begins with a `log.header` record: the Mouser version, the operating system and its version, the computer's id and name, the desk's id and name (if any), the mode (`installed`, `portable`, `portable_elevated`, `service` or `headless`) and the log's path.

Each line is one JSON object, following `specs/001-mouser-v1/contracts/log-record.schema.json`, with `fields` limited per event by `specs/002-mouser-v2/contracts/log-events.schema.json`:

```
{"ts":"2026-10-22T03:51:59.657Z","level":"warn","computer":{"id":"9e4ac5618a56f17190c2af3dac2a571c","name":"Mac-mini"},"event":"problem.raised","peer":"9e4ac5618a56f17190c2af3dac2a571c","fields":{"kind":"platform_limitation","problem_id":"platform_limitation:inject","message":"Some games and remote-desktop apps ignore Mouser's input on this Mac."}}
```

- `ts`: RFC 3339 UTC time, to the millisecond.
- `level`: `error`, `warn`, `info`, `debug` or `trace`.
- `computer`: `id` (32 lowercase hex characters) and `name` of the computer that wrote the line.
- `event`: Dotted lowercase name.
- `peer`: Optional. The other computer's id, or for a problem the computer it is about.
- `fields`: Optional. Event details.

No other keys appear. The log never contains typed characters or which keys were pressed, clipboard contents, file contents or names, or window titles; applications are named by program name only (the executable's name, no path).

Events (level in brackets; `peer` is set where it says so):

- `log.header` (info): first record of every file, described above.
- `logging.enabled`, `logging.disabled` (info): the first and last lines of a logging session.
- `control.changed` (info): control moved. `from` and `to` are computer ids; `how` is `crossing`, `panic`, `quickjump`, `pause`, `link_lost`, `capture_lost`, `elevated_app`, `fault`, `quit` or `entered`. When this computer received control, `app` names the application in front and `app_admin` says whether it runs as administrator.
- `pointer.hidden` (info): `reason` is `crossing_out` or `quickjump`. `pointer.shown` (info): `reason` is `returned`, `capture_lost`, `fault`, `quit`, `startup_recovery` or `guard` (the Windows cursor guard restored the pointer after Mouser ended).
- `input.refused` (warn, `peer` = the controller): the operating system refused an input. `kind` is `key`, `button`, `motion`, `scroll`, `grab` or `warp`, with `app` and `app_admin`. Only the first refusal of a kind in a run is written; `input.refused_run_ended` (info) then gives the `count` for the run.
- `peer.connected` (info, with `peer`): a desk computer authenticated; `version` and `os` are given. `peer.disconnected` (info, with `peer`): the link ended; `reason` is `peer_left`, `peer_quit`, `removed`, `replaced`, `unreachable`, `incompatible`, `protocol`, `fault_here` or `quit_here`.
- `wire.unknown` (warn, with `peer`): a message from a newer Mouser that this one does not know was skipped.
- `fault` (error): something went wrong inside Mouser. `where` (thread or task), `location` (file:line) and `message` (Mouser's own text, otherwise "non-string payload"). `fault.recovered` (warn): `where` and `action` (`returned_local`, `closed_link` or `continued`) show Mouser carrying on.
- `capture.lost` (warn, Windows): `reason` is `elevated_foreground`, `other_desktop` or `hooks_silent`, with `foreground` (program name) when known. The pointer is already back on this computer. `capture.probe`, `capture.reinstall`, `capture.hook_thread`, `capture.start` (debug) are the Windows hook checks; `capture.desktop` (info) is the installed session engine changing desktop.
- `display.changed` (info): `computer`, `display` and `change` (`added`, `removed`, `rearranged`, `resized`). `layout.changed` (info): `by`, `computer` and `change` (`moved`, `display_removed`, `display_included`, `kept_placement`, `auto_placed`).
- `setting.changed` (info): `key`, `old`, `new`, `origin` (`app`, `cli`, `file` or `desk_mate`) and `from` for a desk-mate.
- `clipboard.sent`, `clipboard.received` (info, with `peer`): `from`, `to` and `reps`, each with a `kind` (`text`, `html`, `rtf`, `png`, `files`) and a `size` (bytes, or item count for files).
- `logs.collected` (info): `path`, `included` (count) and `missing` (computer and reason).
- `elevation.restart` (info): `outcome` is `prompted`, `declined` or `handed_over`.
- `problem.raised`, `problem.cleared` (warn): `kind`, `problem_id`, `computer` and `message`.
- `config.rejected` (warn): an edit to config.toml was refused whole. `fields.reason` says why.
- `update.*`: update checks and failures, as in v1.
- `service.session`, `service.engine_started`, `service.engine_exited`, `service.sas` (info, in `service.log` only): `MouserService` following the console session, starting and restarting the session engine, and Ctrl-Alt-Del requests. Repeated `service.engine_exited` records mean the engine keeps ending.

To reconstruct an incident, read the `control.changed`, `pointer.*`, `input.refused`, `peer.disconnected`, `fault` and `capture.lost` records by time. Other computers' logs are on those computers, or in the bundle below.

### Collecting the desk's logs

Run `mouser logs collect [--out DIR]` on any desk computer, or in the Mouser window choose Diagnostics, then "Collect logs from the desk". It asks every connected computer for its log and writes one file, `mouser-logs-<desk>-<yyyymmddThhmmssZ>.zip`, on this computer only (nothing is sent anywhere else; the window shows the file in the file manager). It goes to the folder after `--out`, otherwise to your Downloads folder; if that name exists, ` (2)` and so on is added. The command prints the file's path, then one line per computer: `<name>: included` or `<name>: not included (offline)`, `(logging off)` or `(older version)`. A computer is included only if logging is on there; a computer running a version below 0.3.0 contributes only a card.

Inside the zip: `bundle.json`, and per computer a folder with `computer.json`, `config.toml`, `mouser.log`, `mouser.log.1` when present and, on a Windows computer with the service, `service.log` and `service.log.1`. It never contains `state.json`, anything from `secrets/`, or desk keys. Send it to whoever is helping you only if you choose to.

On a Windows computer with Mouser installed, the engine that runs before anyone signs in never writes the file to a folder you chose: it streams the bundle to your own Mouser, which writes it as you. `mouser logs collect` does this itself. The window's button for it is not confirmed to work for an installed Windows Mouser until UI mode is finished (see the status paragraph at the top), so use the command there.

## Other things you might hit

- Settings are back at their defaults after a start, or `mouser config get` with Mouser stopped says "Mouser's settings file has a mistake: ..." and exits 3: The config.toml is invalid (not valid TOML, or a value out of range or an unparseable hotkey). Mouser leaves the file as it is, runs with the defaults and, if logging is on, writes `config.rejected` with the reason. A missing key is not an error: it takes its default. Fix the line the reason names and save; a running Mouser picks up the corrected file. See [configuration.md](configuration.md).
- Editing config.toml seems to do nothing: The edit was refused whole. Turn logging on and look for `config.rejected`.
- Desks are not listed on a computer, but others see them: Discovery uses mDNS (multicast). Some networks and VM networks block it (it did not work between two Lima VMs). Join by address: `mouser desk join <host>:<port>`. Also check the macOS Local Network permission.
- `mouser` says `Mouser is not running`: The app is not running for this user, or the control socket is elsewhere. On Linux it needs `XDG_RUNTIME_DIR`.
- `mouser pause` paused only one computer: By design in v1: pause is per computer. Run it on each computer.
- First paste of a big clipboard item is old: Items over 1 MiB, and files, are fetched when you paste, so the first paste can be stale. Paste again, or use the targeted-paste hotkey (default Ctrl+Alt+Shift+V).
- File dragged onto another computer does not arrive, or "There is not enough free space on that computer for these files, so nothing was copied.": The destination lacks space; free some and retry. On Linux file drag is not available; copy and paste files instead.
- "`<file>` already exists there. Replace it?": A name clash during a drag; choose Replace or Skip in the transfer window. Skipping leaves that file alone.
- "The copy stopped because `<name>` went offline. N of M files were copied.": The other computer disappeared mid-drag. Partial files are not cleaned up; delete them by hand.
- A computer's new name does not show on the others: Names are desk-wide: rename from any computer (Settings, or `mouser desk rename <computer> <name>`) and every computer shows it. A computer that was offline shows the new name when it reconnects. A computer on 0.2.x also adopts it.
- Pointer comes back to the other computer the moment I click an application: If it runs as administrator on a Windows computer, see `elevated_app` above.
- Pointer or keys stuck after Mouser was killed: Mouser puts the pointer back on every exit it can run (Quit, logoff, shutdown, a fault, a lost link, a lost hold). On Windows a small helper (`mouser-app.exe --cursor-guard <pid>`) restores the system cursors if Mouser is killed. If it ever happens, turn logging on, reproduce, collect the logs and look for `pointer.hidden` without a later `pointer.shown`.
- Changed `advanced.port` but nothing happened: Restart Mouser.
- Cannot remove another computer's display from the layout: Only the owning computer can remove its own displays; do it on that computer.
- The arranger opens by itself: A new computer was placed automatically and offered for review.
- A hotkey change does nothing: Keep to the `Ctrl+Alt+Shift+M` format. All four hotkeys are checked when you set them, including `hotkeys.enable_disable`. A combination the operating system refuses to register is reported in Settings next to that hotkey.
- After a Wayland login nothing crosses: Approve the "Approve Mouser on `<computer>`" dialog. It appears after every login and can take several seconds.
- X11: capture does not start: Another program holds the pointer or keyboard grab (an open menu is the usual one). Close it.
- "Allow control at the login screen" does nothing useful, or stops working after a desk change: The login-screen copy is a snapshot; run the option again after joining, leaving or rotating a desk. It has not been run on a real login screen.
- Updates do not arrive: Releases are pre-releases; the default `updates.release_url` lists them. `updates.automatic` must be on, or run `mouser update`. Only one desk computer downloads; the rest fetch from it.

## When you report a problem

Turn logging on first (`mouser logs enable`), reproduce the problem, then run `mouser logs collect` and send the zip. Include the output of `mouser status --json` from each computer, the Mouser version (`mouser --version`), the OS and desktop (for Linux: X11 or Wayland, GNOME or Plasma and version). If you cannot collect, send the last 100 lines of the log from each computer (`mouser logs tail -n 100`).
