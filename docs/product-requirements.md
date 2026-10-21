# Mouser — Product Requirements

Mouser replaces ShareMouse: one mouse and keyboard driving several computers over the local network, by moving the pointer across screen edges.

Why replace it: ShareMouse either fails to connect, or connects and then drops the connection as soon as the user starts typing. It also breaks when a work VPN/VM network adapter is active. Many of its features are hard to discover or painful to set up.

Source of the feature inventory: sharemouse.com, the ShareMouse edition matrix, and the installed ShareMouse 7.0.29 app (its preference panes, strings and defaults). Feature IDs in brackets match the requirements picker artifact.

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

## Desks (replaces ShareMouse's password-based grouping)

- Computers belong to a named **desk** (e.g. "Home office", "Lab bench").
- On first launch, Mouser lists desks it finds on the network and offers "Join *Home office*" or "Create a new desk". One click.
- Several desks coexist on one network without seeing each other's input or clipboard.
- A computer can be moved to another desk or removed from one in the UI.
- Joining needs one click of approval on a computer already on the desk ("Allow *Laptop* to join Home office?").
- Encryption keys are exchanged automatically when a computer joins; no password or code to type.

## Requirements

### Must

**Mouse & keyboard sharing**
- Edge switching [edge-switch]
- Keyboard follows the pointer [kbd-follows]
- No fixed server — any computer can control any other [bidirectional]
- macOS ↔ Windows ↔ Linux, with modifier mapping between platforms [cross-platform, linux]
- Multiple monitors per computer [multi-monitor]
- No limit on computers or displays [scale]
- DPI / Retina-aware pointer movement [dpi]
- Low-level input injection so input works in apps that ignore synthetic events [hid-sim]
- Secure Input detection on macOS, naming the app that holds it [secure-input]

**Display layout**
- Visual display arranger: every computer's displays, drag to match the desk [display-manager]
- New computers are positioned automatically, then offered for review [auto-position]
- Display details: resolution, scale, computer, online/offline, last seen [display-info]

**Switching**
- Block switching in display corners [no-corners]
- Double-tap an edge to override a block [double-tap]
- Pointer locator: brief highlight where the pointer lands [locate]

**Clipboard & files**
- Clipboard sync: plain text, rich text, images, files [clipboard]
- Targeted clipboard paste hotkey [clip-hotkey]
- Drag & drop files and folders across the edge [dnd]
- Transfer progress with speed, time left, overwrite prompt, disk-space check [transfer-ui]

**Network**
- Zero-config discovery, scoped to a desk (see Desks) [autodiscover]
- Manual peers by address, and an allowlist [manual-ip]
- Encrypted traffic, automatic [encryption]
- LAN only; nothing goes to the internet, except one desk computer checking for and downloading new Mouser releases [lan-only]
- Version compatibility check with a clear message [version-check]
- Extra network adapters (VPN, VM, container bridges) never break connectivity, with no configuration [from adapter note]

**App & operations**
- Menu bar / system tray app with clear state [menubar]
- Light/dark tray icon following the system theme [theme]
- Panic key returns control to the local computer [panic]
- Explanatory notifications when something is blocked or broken [bubbles]
- Launch at login [autostart]
- Guided permissions setup per OS [permissions]
- Diagnostic logging: one checkbox (and a CLI/config equivalent); logs go to a file in JSON Lines format so agents can read them. No OS console logging, no launch-time key combos [diagnostics]
- Auto-update across the LAN: one computer downloads new releases, the rest update from it over the LAN [auto-update]
- Portable mode: run without installing [portable]
- Self-healing connections [reconnect]
- Status & health panel: who's connected, latency, last error, missing permissions, and why the pointer isn't crossing [health]
- Plain-text config file and CLI [config-file]

### Nice to have

- Mouse buttons 4/5: configurable passthrough or Back/Forward mapping, so a separate mapping tool (e.g. 4/5 → switch virtual desktops) keeps working [btn45]
- Remove a display from the layout [remove-display]
- Equalize display dimensions [equalize]
- Block switching in full-screen windows [no-fullscreen]
- QuickJump, redesigned: one hotkey shows a letter on every display across all computers; press a letter to jump there. No setup [quickjump]
- Control the Mac login window [mac-login-control]
- Windows login screen after cold boot [win-cold-login]
- Windows UAC prompts and user switching [win-uac]
- Send Ctrl-Alt-Del [cad]
- Configurable ports [ports]
- Enable/disable hotkey [enable-hotkey]

### Out of scope

Remote mouse/scroll speed tuning (unless trivial), multimedia keys, lock-key sync, keyboard language sync, named layout profiles, location-based profiles, shared displays, DDC/CI monitor input switching, toggle-monitors hotkey, wrap-around, require-a-key-to-switch, block in maximized windows, block in chosen apps, per-display jump hotkeys (covered by QuickJump), dim inactive displays, hardware dimming, all power/lock/screensaver sync, remote Mac login with stored credentials, reset-to-defaults, settings UI lock, licensing, Wake-on-LAN, password-based grouping, pairing codes.
