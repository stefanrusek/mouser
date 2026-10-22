# Troubleshooting

For desk owners and for agents helping them. Start with the first section, then look up the message you see.

Mouser v1 status in one paragraph: macOS is tested on real Macs. Linux is tested in virtual machines and against fake portals, not on real desktops. The Windows backend does not exist yet: the Windows app starts and stops with "the Windows platform backend is not implemented yet". Releases are GitHub pre-releases.

## First steps

1. Ask Mouser what is wrong:

   ```
   mouser status
   mouser status --json
   ```

   It lists the desk, each computer's connection (`connected`, `reconnecting`, `offline`, `incompatible`), any missing permission and every open problem with its fix. Platform limitations also show up under Problems; they are explanations, not faults. If it prints `Mouser is not running` (exit code 2), start the app.
2. If the cause is not obvious, turn on the log, reproduce the problem, and read the log:

   ```
   mouser logs enable
   mouser logs tail -n 50
   mouser logs path
   ```
3. Run `mouser status` on **each** computer on the desk. Each one only reports its own view and its own permissions.

Quick checks that fix many "it won't connect" reports: both computers are on the same network and awake; the UDP port (default 47801, `advanced.port`) is not blocked by a firewall; you are not running ShareMouse at the same time (two programs capturing input fight each other); both computers run compatible Mouser versions.

## Problem kinds

Every notice in the app, and every entry under Problems in `mouser status`, has one of these seven kinds. `fix` is the one-click action in the app; the CLI shows it as `(fix: ...)`.

### missing_permission

Mouser cannot do its job without an OS permission. Mouser does not start capturing or injecting input until nothing is Missing, and re-checks every 250 ms, so granting it works without a restart. Fix: `open_system_settings:<permission>`; in the app, click the fix and the right settings pane opens.

| Permission | Message | Where |
|---|---|---|
| accessibility | Mouser needs Accessibility access to move the pointer and type on this computer. | macOS: System Settings > Privacy & Security > Accessibility |
| input_monitoring | Mouser needs Input Monitoring access to see the mouse and keyboard on this computer. | macOS: Input Monitoring |
| screen_recording | Mouser needs Screen Recording access to see where full-screen windows are. | macOS: Screen Recording |
| full_disk_access | Mouser needs Full Disk Access to copy files to and from this computer. | macOS: Full Disk Access |
| input_capture_portal | Mouser needs permission to capture input through the desktop's input portal. | Linux Wayland |
| remote_desktop_portal | Mouser needs permission to control this computer through the desktop's remote-control portal. | Linux Wayland |

If you rebuilt or reinstalled an unsigned Mouser on macOS, the grants are reset; the released, notarized app keeps them across updates. Allow local network access when macOS asks, or computers cannot find each other.

### incompatible_version

> `<A>` runs Mouser `<X>` and `<B>` runs Mouser `<Y>`, and those versions cannot work together. Update `<the older computer>`.

Cause: the two computers speak different protocol versions. The computer shows as `incompatible` in `mouser status`; a join attempt fails with this text. Fix: update the older one. If it is this computer, the fix is `open_update`: run `mouser update` or use the tray menu. Install the same release on every computer.

### join_refused

Raised on the computer that asked to join, or on the desk computer that turned someone away. Messages:

| Message | Cause and fix |
|---|---|
| This computer was not admitted to `<desk>`. | Someone clicked Decline (or `mouser decline`). Ask again and approve on a computer already on the desk. |
| `<name>` (`<ip>`) was refused because its address is not on the allowlist. | The desk has `advanced.allowlist` set. Add the joiner's network to it on a desk computer (`mouser config set advanced.allowlist 192.168.1.0/24`) or empty it. |
| The request to join the desk was not answered. | The desk computer went away while waiting. Check it is on and try again. |
| The request to join the desk was interrupted when Mouser stopped. | Mouser on this computer was quit during the request. Join again. |
| (the incompatible_version text) | The desk computer runs an incompatible version. |

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

### platform_limitation

A thing this operating system does not allow. It is shown on the affected computer and is not a fault. There is no fix action. The exact texts are in the next section.

## Platform limitations, with their exact texts

These are the five limitations the owner accepted for v1, plus the detail texts the backends produce.

### macOS

| Capability | Text |
|---|---|
| inject | Some games and remote-desktop apps ignore Mouser's input on this Mac. |
| secure attention | Ctrl-Alt-Del can only be sent to Windows computers. |
| capture (portable mode only) | macOS permissions must be granted again on each computer you run Mouser from this drive on. |

Why: macOS only lets an unentitled app post events with `CGEventPost`; apps that read hardware-level input (some games, remote-desktop clients) never see them.

### Linux (all sessions)

| Capability | Text |
|---|---|
| file drag | Dragging files between computers isn't available on Linux; use copy and paste |
| secure attention | Ctrl-Alt-Del can only be sent to Windows computers. |
| tray (no tray host on the desktop) | This desktop shows no tray icon for Mouser. Install the AppIndicator extension to get one; the tray menu actions are also in the Mouser window. |

### Linux, Wayland (GNOME and KDE)

| Text | Meaning |
|---|---|
| This desktop cannot share its mouse and keyboard with other computers. Mouser needs GNOME 46 or later or KDE Plasma 6.1 or later on Wayland. | The input-capture portal is missing. Sway, Hyprland, COSMIC and older desktops are unsupported. |
| This desktop cannot share its keyboard with other computers. / This desktop cannot share its mouse with other computers. | The portal offers only one of the two. |
| This desktop asks you to approve Mouser each time you sign in before it can share this computer's mouse and keyboard. | Expected on GNOME 46/50 and Plasma 6.7.5: the permission cannot be remembered. The approval dialog appears when capture starts and can take several seconds (6 to 40 seconds were seen). |
| This desktop does not let Mouser move the mouse or type here, so other computers cannot control this one. | The remote-desktop portal is missing. |
| Mouser cannot tell when an app is full-screen on Wayland, so the pointer may still cross to another computer during a game or video. | Full-screen blocking is unavailable. |
| QuickJump and hotkey crossings can't start from this computer on Wayland; push the pointer off the screen edge instead. | On Wayland only an edge push starts a crossing. |
| Mouser cannot share the clipboard from this computer while it shares its mouse. This needs GNOME 51 or later. | GNOME before 51 (or an unknown version). |
| This desktop keeps Mouser's permission after you remove the drive; remove it in the desktop's settings if you do not want it to stay. | Portable mode on Wayland. |

Also on Wayland: pressing into the edge while dragging a file cancels the drag on GNOME 46. Wayland displays have scale 1.0 and no physical size.

### Linux, X11

No extra limitations are listed. Behaviour to know: Mouser cannot take the pointer grab while another program holds one, for example while a GTK menu is open; it succeeds as soon as the menu closes. Scale is fixed at 1.0 and one wheel click is 15 pixels.

### Windows

The Windows backend is not implemented. Starting Mouser on Windows stops with "the Windows platform backend is not implemented yet". Nothing else about Windows can be diagnosed in v1. When it exists, the installer is unsigned, so SmartScreen will warn ("More info", then "Run anyway"), and portable mode will carry this limitation: "Portable mode cannot control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator."

## The diagnostic log

Logging is off by default. Turn it on with `mouser logs enable`, the checkbox in the health window, or `diagnostics.logging = true` in config.toml. Nothing is written to the system console or log.

Location (`mouser logs path` prints it):

| OS | File |
|---|---|
| macOS | `~/Library/Application Support/Mouser/logs/mouser.log` |
| Linux | `$XDG_CONFIG_HOME/mouser/logs/mouser.log` (or `~/.config/mouser/logs/mouser.log`) |
| Windows | `%APPDATA%\Mouser\logs\mouser.log` |
| Portable | `MouserData/logs/mouser.log` |

When the file would pass `diagnostics.max_log_mb` (default 50 MB), it is renamed `mouser.log.1`, replacing the previous one, and a new `mouser.log` starts.

Each line is one JSON object, following `specs/001-mouser-v1/contracts/log-record.schema.json`:

```
{"ts":"2026-10-22T03:51:59.657Z","level":"warn","computer":{"id":"9e4ac5618a56f17190c2af3dac2a571c","name":"Mac-mini"},"event":"problem.raised","peer":"9e4ac5618a56f17190c2af3dac2a571c","fields":{"kind":"platform_limitation","problem_id":"platform_limitation:inject","message":"Some games and remote-desktop apps ignore Mouser's input on this Mac."}}
```

| Field | Meaning |
|---|---|
| `ts` | RFC 3339 UTC time, to the millisecond. |
| `level` | `error`, `warn`, `info`, `debug` or `trace`. |
| `computer` | `id` (32 lowercase hex characters) and `name` of the computer that wrote the line. |
| `event` | Dotted lowercase name. |
| `peer` | Optional. The other computer's id, or for a problem the computer it is about. |
| `fields` | Optional. Event details. |

No other keys appear. Events written in v1:

| Event | Level | Meaning |
|---|---|---|
| `logging.enabled`, `logging.disabled` | info | The first and last lines of a logging session. |
| `peer.connected`, `peer.disconnected` | info | A desk computer authenticated or its link ended. `peer` is that computer. |
| `problem.raised`, `problem.cleared` | warn | A problem opened or closed. `fields` has `kind`, `problem_id` and `message`. |
| `config.rejected` | warn | An edit to config.toml was refused whole. `fields.reason` says why. |

Crossing events are not logged yet. Other computers' logs are on those computers; run the commands there.

## Other things you might hit

| Symptom | Cause and what to do |
|---|---|
| Mouser will not start and complains about config.toml (or `mouser config get` with Mouser stopped panics with "config.toml is valid TOML" or "config.toml: ...") | At start an invalid config.toml is not refused, it stops Mouser. Most often a key is missing (every key must be present), or a value is out of range. Fix the line it names, or move the file aside so a fresh one with the defaults is written. A running Mouser only refuses a bad edit and keeps going; see [configuration.md](configuration.md). |
| Editing config.toml seems to do nothing | The edit was refused whole. Turn logging on and look for `config.rejected`. |
| Desks are not listed on a computer, but others see them | Discovery uses mDNS (multicast). Some networks and VM networks block it (it did not work between two Lima VMs). Join by address: `mouser desk join <host>:<port>`. Also check the macOS Local Network permission. |
| `mouser` says `Mouser is not running` | The app is not running for this user, or the control socket is elsewhere. On Linux it needs `XDG_RUNTIME_DIR`. |
| `mouser pause` paused only one computer | By design in v1: pause is per computer. Run it on each computer. |
| First paste of a big clipboard item is old | Items over 1 MiB, and files, are fetched when you paste, so the first paste can be stale. Paste again, or use the targeted-paste hotkey (default Ctrl+Alt+Shift+V). |
| File dragged onto another computer does not arrive, or "There is not enough free space on that computer for these files, so nothing was copied." | The destination lacks space; free some and retry. On Linux file drag is not available; copy and paste files instead. |
| "`<file>` already exists there. Replace it?" | A name clash during a drag; choose Replace or Skip in the transfer window. Skipping leaves that file alone. |
| "The copy stopped because `<name>` went offline. N of M files were copied." | The other computer disappeared mid-drag. Partial files are not cleaned up; delete them by hand. |
| A computer's new name does not show on the others | A rename reaches desk-mates when they next reconnect. |
| Changed `advanced.port` but nothing happened | Restart Mouser. |
| Cannot remove another computer's display from the layout | Only the owning computer can remove its own displays; do it on that computer. |
| The arranger opens by itself | A new computer was placed automatically and offered for review. |
| A hotkey change does nothing | Keep to the `Ctrl+Alt+Shift+M` format; `hotkeys.enable_disable` is not checked when you set it. |
| After a Wayland login nothing crosses | Approve the "Approve Mouser on `<computer>`" dialog. It appears after every login and can take several seconds. |
| X11: capture does not start | Another program holds the pointer or keyboard grab (an open menu is the usual one). Close it. |
| "Allow control at the login screen" does nothing useful, or stops working after a desk change | The login-screen copy is a snapshot; run the option again after joining, leaving or rotating a desk. It has not been run on a real login screen. |
| Updates do not arrive | Releases are pre-releases; the default `updates.release_url` lists them. `updates.automatic` must be on, or run `mouser update`. Only one desk computer downloads; the rest fetch from it. |

## When you report a problem

Include the output of `mouser status --json` from each computer, the Mouser version (`mouser --version`), the OS and desktop (for Linux: X11 or Wayland, GNOME or Plasma and version), and the last 100 lines of the log (`mouser logs tail -n 100`).
