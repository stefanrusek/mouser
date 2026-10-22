# Mouser — Product Requirements

Mouser replaces ShareMouse: one mouse and keyboard driving several computers over the local network, by moving the pointer across screen edges.

Why replace it: ShareMouse either fails to connect, or connects and then drops the connection as soon as the user starts typing. It also breaks when a work VPN/VM network adapter is active. Many of its features are hard to discover or painful to set up.

Source of the feature inventory: sharemouse.com, the ShareMouse edition matrix, and the installed ShareMouse 7.0.29 app (its preference panes, strings and defaults). Feature IDs in brackets match the requirements picker artifact.

## What v1 delivers (read this first)

The requirement text below is the owner's original wording and is unchanged. Each item has a **Delivered:** note saying what actually shipped in v1, including accepted platform limitations and behaviour that changed during implementation. Where a note and the original wording disagree, the note is correct.

Where v1 stands per operating system:

- **macOS:** the full feature set is built and has been tested on real Macs (two Macs).
- **Linux:** input capture and injection (X11 and GNOME/KDE Wayland), displays, clipboard, network-change watching and portable mode are built. Tested in aarch64 virtual machines and against fake portals, not on real x86_64 desktops or physical input hardware.
- **Windows:** the backend is **not implemented**. The Windows build starts, then stops with "the Windows platform backend is not implemented yet". It waits for the owner's hardware. Everything marked "Windows" below is therefore not delivered.
- Releases are GitHub pre-releases. v0.1.1 is the latest published; v0.2.0 is coming.

The five accepted platform limitations (operator decision 2026-10-21), each shown in the app on the affected computer:

1. Apps that ignore simulated input are not reached on macOS or Windows (for example some games and remote-desktop apps).
2. Wayland: GNOME before 51 and Plasma before 6.7 ask for permission at each login, and cannot share the clipboard from the computer that is sharing its mouse. Desktops without input-capture support (Sway, Hyprland, COSMIC) are unsupported. Full-screen blocking is unavailable.
3. File drag between computers is unavailable on Linux. Copy and paste is the alternative.
4. Windows portable mode cannot control the login screen, permission prompts, Ctrl-Alt-Del or applications running as administrator.
5. GNOME without the AppIndicator extension shows no tray icon. The tray actions are in the Mouser window instead.

The exact texts are in [troubleshooting.md](troubleshooting.md). Operator references: [cli.md](cli.md), [configuration.md](configuration.md).

## Target setup

- **Operating systems:** macOS, Windows, Linux — all first-class.
- **Scale:** typically 2–3 computers and 5 displays; no hard limit on computers or displays.
- **Network:** Wi-Fi and/or Ethernet, all on the same network. Other adapters (VPN, VM, Docker bridges) are present and must not break anything.
- **Multiple independent desks** may share one network (e.g. two separate clusters of computers).

## Product principles

1. **Reliability first.** A connection that drops while typing is the failure we are replacing. Connections heal themselves after sleep, Wi-Fi/Ethernet changes, IP changes and VPN changes, without restarting the app.
2. **Better UI than ShareMouse.** Every feature is discoverable where it applies, not buried in a preferences tab:
   - Settings live next to what they affect: edge and corner behaviour is set by clicking that edge in the display arranger; a display's options are on the display.
   - The core experience needs zero configuration. Defaults are good enough that most people never open settings.
   - Plain language, no jargon. Nothing called "IOHID simulation" or "UDP broadcast" in the main UI; technical detail lives behind an "Advanced" disclosure or in the config file.
   - When something doesn't work, the app says why and offers the fix in one click (missing permission, Secure Input held by an app, peer on an incompatible version, switching blocked by a corner rule).
3. **Trusted LAN, low ceremony.** The user's network is trusted. Grouping computers is a usability feature, not a security ritual: no passwords or pairing codes as the way to form a group. Traffic is still encrypted, automatically, with no user-facing setup.
4. **Agent-friendly operations.** Config is a plain-text file, there is a CLI, and logs are JSON Lines in a file.
  - Delivered: config.toml with live reload (a bad edit is refused whole), the `mouser` CLI (see cli.md) and a JSON Lines log (see troubleshooting.md).

## Desks (replaces ShareMouse's password-based grouping)

- Computers belong to a named **desk** (e.g. "Home office", "Lab bench").
- On first launch, Mouser lists desks it finds on the network and offers "Join *Home office*" or "Create a new desk". One click.
- Several desks coexist on one network without seeing each other's input or clipboard.
- A computer can be moved to another desk or removed from one in the UI.
- Joining needs one click of approval on a computer already on the desk ("Allow *Laptop* to join Home office?").
- Encryption keys are exchanged automatically when a computer joins; no password or code to type.
  - Delivered: yes. Desks, first-run "Join / Create", approval, move and remove all work; the CLI equivalents are `mouser desks`, `desk create|join|leave|remove`, `requests`, `approve`, `decline`. Joining by name, desk id or `host:port` is supported. A computer's rename only reaches its desk-mates when it next reconnects. mDNS discovery between two Lima VMs did not work (probably no multicast on that VM network; unverified), so on Linux VMs join by address.

## Requirements

### Must

**Mouse & keyboard sharing**
- Edge switching [edge-switch]
  - Delivered: built (macOS tested on real Macs; Linux in VMs).
- Keyboard follows the pointer [kbd-follows]
  - Delivered: built. Modifiers held at the moment of a crossing are released locally, not carried across.
- No fixed server — any computer can control any other [bidirectional]
  - Delivered: built; there is no server.
- macOS ↔ Windows ↔ Linux, with modifier mapping between platforms [cross-platform, linux]
  - Delivered: macOS and Linux are built. **Windows is not delivered** (no backend). Linux: X11, GNOME Wayland (46 and later) and KDE Wayland (Plasma 6.1 and later); other Wayland desktops show a limitation. On Wayland the pointer can only start a crossing by being pushed through a screen edge; hotkey and QuickJump crossings cannot start from a Wayland computer.
- Multiple monitors per computer [multi-monitor]
  - Delivered: built. Wayland displays come from the input-capture zones, with scale 1.0 and no physical size.
- No limit on computers or displays [scale]
  - Delivered: no hard limit in code; testing targeted 2-3 computers and 5 displays.
- DPI / Retina-aware pointer movement [dpi]
  - Delivered: built on macOS. On Linux the scale is fixed at 1.0 on both X11 and Wayland.
- Low-level input injection so input works in apps that ignore synthetic events [hid-sim]
  - Delivered: **changed.** macOS injects with CGEventPost; a lower-level virtual HID device needs entitlements Mouser does not have. This is accepted limitation 1: some games and remote-desktop apps ignore Mouser's input. The app shows "Some games and remote-desktop apps ignore Mouser's input on this Mac." Not delivered on Windows.
- Secure Input detection on macOS, naming the app that holds it [secure-input]
  - Delivered: built. Typing is blocked while an app holds Secure Input; Mouser says which app, and typing works again when you switch away from it or close it.

**Display layout**
- Visual display arranger: every computer's displays, drag to match the desk [display-manager]
  - Delivered: built (arranger window).
- New computers are positioned automatically, then offered for review [auto-position]
  - Delivered: built. The arranger window opens when a review is offered. Core does not yet mark displays as "needs review" or clear that mark, so a per-display review badge may not appear.
- Display details: resolution, scale, computer, online/offline, last seen [display-info]
  - Delivered: built (`layout.get` returns these per display).

**Switching**
- Block switching in display corners [no-corners]
  - Delivered: built. Corners are 8 logical pixels. Corner blocking applies to every crossing, including on remote displays. A block raises a "crossing blocked" notice with a fix that opens that edge in the arranger.
- Double-tap an edge to override a block [double-tap]
  - Delivered: built. Push the same edge twice within 600 ms (a bump is one continuous contact; back off to re-arm).
- Pointer locator: brief highlight where the pointer lands [locate]
  - Delivered: built (a click-through overlay window).

**Clipboard & files**
- Clipboard sync: plain text, rich text, images, files [clipboard]
  - Delivered: built on macOS and Linux. **Changed:** representations over 1 MiB, and files, are fetched when you paste, so the first paste of a large item can be stale. On Wayland, GNOME before 51 cannot share the clipboard from the computer that shares its mouse (accepted limitation 2). Received clipboard files are kept under `<config dir>/clipboard/` and are never cleaned up. Not delivered on Windows.
- Targeted clipboard paste hotkey [clip-hotkey]
  - Delivered: built; default Ctrl+Alt+Shift+V (`hotkeys.targeted_paste`).
- Drag & drop files and folders across the edge [dnd]
  - Delivered: macOS built. **Linux: unavailable** (accepted limitation 3; use copy and paste). On GNOME, pressing into the edge also cancels an in-progress drag. Dragging a file *into* a Mac is verified only in the harness; verify it in the real app. Not delivered on Windows.
- Transfer progress with speed, time left, overwrite prompt, disk-space check [transfer-ui]
  - Delivered: built (transfer window). If a computer goes offline mid-transfer you get "The copy stopped because <name> went offline." Partial and completed files are not cleaned up.

**Network**
- Zero-config discovery, scoped to a desk (see Desks) [autodiscover]
  - Delivered: built (mDNS). See the Desks note about VM networks.
- Manual peers by address, and an allowlist [manual-ip]
  - Delivered: built: `mouser desk join host:port`, and `advanced.allowlist` (CIDR list; a joiner outside it is refused with a notice). Changing `advanced.port` needs a restart.
- Encrypted traffic, automatic [encryption]
  - Delivered: built. QUIC with pinned certificates and a per-desk key; nothing to configure.
- LAN only; nothing goes to the internet, except one desk computer checking for and downloading new Mouser releases [lan-only]
  - Delivered: as stated. The only internet access is the update check (`updates.release_url`, default the GitHub releases list, which includes pre-releases).
- Version compatibility check with a clear message [version-check]
  - Delivered: built: "<A> runs Mouser X and <B> runs Mouser Y, and those versions cannot work together. Update <the older one>." The release manifest's `min_protocol` is parsed but not enforced.
- Extra network adapters (VPN, VM, container bridges) never break connectivity, with no configuration [from adapter note]
  - Delivered: built (adapter classification; connections migrate and reconnect). Verified in simulation and between real Macs; not exercised against every VPN product.

**App & operations**
- Menu bar / system tray app with clear state [menubar]
  - Delivered: built. On GNOME without the AppIndicator extension there is no tray icon (accepted limitation 5); use the Mouser window.
- Light/dark tray icon following the system theme [theme]
  - Delivered: built (macOS uses a template image; other systems follow the system appearance).
- Panic key returns control to the local computer [panic]
  - Delivered: built; default Escape pressed twice within 500 ms (`hotkeys.panic`, written `Escape*2`).
- Explanatory notifications when something is blocked or broken [bubbles]
  - Delivered: built (problems with a one-click fix where one exists). Setting `general.notifications` to false silences system notifications live; windows such as the join prompt and the arranger still open.
- Launch at login [autostart]
  - Delivered: built (`general.start_at_login`, default on). It is applied once the computer is on a desk, and never in portable mode.
- Guided permissions setup per OS [permissions]
  - Delivered: built on macOS (Accessibility, Input Monitoring, Screen Recording, Full Disk Access; the app opens the right pane). On Wayland the per-login approval dialog appears when capture starts and can take several seconds.
- Diagnostic logging: one checkbox (and a CLI/config equivalent); logs go to a file in JSON Lines format so agents can read them. No OS console logging, no launch-time key combos [diagnostics]
  - Delivered: built: `diagnostics.logging`, `mouser logs enable|disable|path|tail`. Records follow log-record.schema.json (see troubleshooting.md). Crossing events are not logged yet.
- Auto-update across the LAN: one computer downloads new releases, the rest update from it over the LAN [auto-update]
  - Delivered: built. Automatic when `updates.automatic` is on; manual with `mouser update`. Releases are signed with a minisign key and re-verified by each computer before installing. macOS releases are signed and notarized so permissions survive updates. Windows builds are unsigned (SmartScreen warns).
- Portable mode: run without installing [portable]
  - Delivered: built for macOS and Linux (a `MouserData/` folder beside the app). Windows portable is not delivered (no backend), and a portable zip is not yet packaged by the release workflow. Limitations shown: on macOS permissions must be granted again on each computer; on Wayland the desktop keeps Mouser's permission after the drive is removed; on Windows (accepted limitation 4). Details in configuration.md.
- Self-healing connections [reconnect]
  - Delivered: built (QUIC migration, reconnect, offline notice after a grace period). Tested through a simulated network and between real Macs.
- Status & health panel: who's connected, latency, last error, missing permissions, and why the pointer isn't crossing [health]
  - Delivered: built (health window and `mouser status`).
- Plain-text config file and CLI [config-file]
  - Delivered: built. See configuration.md and cli.md. **Changed:** `mouser pause` and the enable/disable hotkey pause only the computer where they are used, not the whole desk.

### Nice to have

- Mouse buttons 4/5: configurable passthrough or Back/Forward mapping, so a separate mapping tool (e.g. 4/5 → switch virtual desktops) keeps working [btn45]
  - Delivered: built (`mouse.buttons_4_5` = `passthrough` or `back_forward`; Back/Forward is Alt-Left/Right on Windows and Linux and Cmd-[ and Cmd-] on macOS).
- Remove a display from the layout [remove-display]
  - Delivered: built, but only the computer that owns a display can remove it; a remote member's display is refused with "<display> belongs to another computer; remove it on that computer".
- Equalize display dimensions [equalize]
  - Delivered: built (arranger checkbox, `crossing.equalize_dimensions`). Shared with the desk.
- Block switching in full-screen windows [no-fullscreen]
  - Delivered: macOS and Linux X11 built (`crossing.block_fullscreen`, default on). **Unavailable on Wayland** (accepted limitation 2): "Mouser cannot tell when an app is full-screen on Wayland, so the pointer may still cross to another computer during a game or video."
- QuickJump, redesigned: one hotkey shows a letter on every display across all computers; press a letter to jump there. No setup [quickjump]
  - Delivered: built (`hotkeys.quickjump`, default Ctrl+Alt+Shift+J; letters A-Z then 0-9). Cannot start from a Wayland computer.
- Control the Mac login window [mac-login-control]
  - Delivered: code built (a login-window agent plus an "Allow control at the login screen" admin prompt). **Never run for real**: the admin prompt, the root install and a real login window are unverified. The machine-scope copy of the desk keys is not refreshed automatically after joining, leaving or key rotation.
- Windows login screen after cold boot [win-cold-login]
  - Delivered: **not delivered**; deferred to a future feature (owner decision 2026-10-22) (no Windows backend; `mouser-service-win` is a stub).
- Windows UAC prompts and user switching [win-uac]
  - Delivered: **not delivered**; deferred to a future feature (owner decision 2026-10-22).
- Send Ctrl-Alt-Del [cad]
  - Delivered: the command and wire message exist (`mouser send-cad`). It can only target Windows computers, and there is no Windows backend, so in v1 it always answers "Ctrl-Alt-Del cannot be sent to <name>.". Windows support for it is deferred to a future feature (owner decision 2026-10-22).
- Configurable ports [ports]
  - Delivered: built (`advanced.port`, default 47801, UDP). Needs a restart to take effect.
- Enable/disable hotkey [enable-hotkey]
  - Delivered: built (`hotkeys.enable_disable`, default Ctrl+Alt+Shift+M). Per computer, not per desk.

### Out of scope

Remote mouse/scroll speed tuning (unless trivial), multimedia keys, lock-key sync, keyboard language sync, named layout profiles, location-based profiles, shared displays, DDC/CI monitor input switching, toggle-monitors hotkey, wrap-around, require-a-key-to-switch, block in maximized windows, block in chosen apps, per-display jump hotkeys (covered by QuickJump), dim inactive displays, hardware dimming, all power/lock/screensaver sync, remote Mac login with stored credentials, reset-to-defaults, settings UI lock, licensing, Wake-on-LAN, password-based grouping, pairing codes.
