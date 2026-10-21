# Spike Findings: Windows and Linux platform risks (Mouser v1)

Feature 001-mouser-v1 | 2026-10-21 | **Desk research only** (vendor docs, upstream NEWS/MR records, crate sources, Deskflow and lan-mouse source). No Windows or Linux machine was available; nothing here was run (constitution Principle 3.1: no probe could run → recorded as risk). Saved by the plan orchestrator from the research agent's report.

Confidence: **documented** (vendor/upstream docs), **verified-in-source** (read in source/NEWS/MR), **uncertain** (needs a probe). Risk: HIGH / MEDIUM / LOW to shipping v1 as specified.

## Versions referenced
| Component | Version | Date |
|---|---|---|
| `windows` crate | 0.62.2 | 2025-10-06 |
| `ashpd` (portals) | 0.13.13 | 2026-07-17 |
| `reis` (libei/EIS) | 0.7.1 | 2026-07-30 |
| `input-capture` (lan-mouse) | 0.4.0 | 2026-06-12 |
| `x11rb` | 0.14.0 | 2026-07-16 |
| `wl-clipboard-rs` | 0.9.4 | 2026-09-28 |
| `ksni` | 0.3.7 | 2026-10-20 |
| `tray-icon` | 0.26.1 | 2026-10-07 |
| xdg-desktop-portal | 1.22.0 | 2026-06-08 |
| Deskflow | v1.27.0 | 2026-10-01 |

## Ranked risks
1. **HIGH — Wayland portal versions.** Every Wayland computer needs InputCapture (to share its mouse) and RemoteDesktop (to be driven). Capture without a prompt each login needs GNOME 51 / Plasma 6.7; clipboard on a capture session needs GNOME 51 (Deskflow says 50) / Plasma 6.7. Ubuntu 24.04 (GNOME 46) and 26.04 (GNOME 50) re-prompt every login and can't copy from the capturing machine.
2. **HIGH — approval dialog after reboot.** A Wayland computer with no local mouse cannot approve the portal dialog. A Plasma 6.7.4 + xdg-desktop-portal 1.22.1 regression stops the InputCapture dialog appearing.
3. **HIGH — drag-and-drop between computers (Story 7).** No prior art on X11, likely infeasible on Wayland, fiddly on Windows (Mouse Without Borders: single file only).
4. **HIGH — GNOME ≤ 50 as clipboard source.** mutter has no data-control; clipboard on capture sessions needs mutter 51. KDE allows ext-data-control for unsandboxed apps (wlr-data-control dropped in 6.5).
5. **HIGH (as worded) — apps ignoring simulated input (Story 2 sc. 7).** Windows marks injected input (`LLMHF_INJECTED`, Raw Input `hDevice == NULL`); a fix needs a signed kernel virtual-HID driver.
6. **HIGH unless the service ships — UIPI.** Non-elevated `SendInput` fails silently on elevated windows.
7. **MEDIUM — Windows SYSTEM service** for login/UAC/lock/Ctrl-Alt-Del (Deskflow pattern). Security-sensitive; desk keys must be readable by SYSTEM before login.
8. **MEDIUM — Windows portable mode** (no service, no elevated apps, firewall prompt needs admin, autostart vs "keeps no settings", WebView2 data folder).
9. **MEDIUM — hooks.** Raw Input cannot suppress input; LL hooks are required and are silently removed if a callback exceeds ~1 s.
10. **MEDIUM — `reis` 0.7.1 is pre-1.0** ("API is subject to change").
11. **MEDIUM — X11 capture must be written from scratch** (lan-mouse X11 capture returns `NotImplemented`); grabs fail while another client holds one.
12. **MEDIUM — Wayland edge behaviour**: no full-screen detection; compositor may ignore release position.
13. **MEDIUM — stock GNOME has no tray** without the AppIndicator extension.
14. **MEDIUM — pasting remote files**: X11/Wayland need local staging; Windows needs a virtual-file data object.
15. **LOW–MEDIUM**: EDID physical size unreliable on Windows; cursor parking by warping (Deskflow) rather than `ClipCursor`.
16. **LOW**: Linux Wayland login screen probably infeasible (stated limitation); Linux portable leaves portal permission behind; text/HTML/image clipboard and XTest injection are known patterns.

## Windows
### W1 Capture: LL hooks vs Raw Input — MEDIUM
Raw Input (`RIDEV_INPUTSINK`) observes but cannot suppress; swallowing requires `WH_MOUSE_LL`/`WH_KEYBOARD_LL` returning non-zero. An LL hook exceeding `LowLevelHooksTimeout` is silently removed (Win7+; capped at 1000 ms from Win10 1709). Deskflow (`MSWindowsHook.cpp` 670–672) and lan-mouse (`input-capture` 0.4.0 `src/windows/event_thread.rs:178-179`) use LL hooks on a dedicated thread. Source: learn.microsoft.com lowlevelkeyboardproc (2025-07-16). documented + verified-in-source; whether Raw Input still sees hook-swallowed events: uncertain. **Mitigation:** dedicated hook thread with I/O-free pump + lock-free queue; watchdog reinstalls removed hooks and raises a Problem.
### W2 Cursor confinement — LOW–MEDIUM
Deskflow warps to centre with `SetCursorPos` after each move (`MSWindowsComputer.cpp` 294, 470–475, 1255, 1364–1398), not `ClipCursor` (reset on desktop switch). **Mitigation:** warp design, park at the crossing edge.
### W3 SendInput and UIPI — HIGH without service
`SendInput` only reaches equal-or-lower integrity and fails silently. Remedies: run elevated/SYSTEM, or `uiAccess=true` (Authenticode-signed binary in Program Files/System32). Deskflow starts its core with a winlogon-derived token and `TokenUIAccess=1` (`MSWindowsWatchdog.cpp` getUserToken, 288–294). **Mitigation:** SYSTEM service launches per-session agent; detect foreground integrity mismatch and raise "Mouser can't control <app> because it runs as administrator."
### W4 Login screen, UAC, user switching, Ctrl-Alt-Del (Story 12) — MEDIUM
Secure desktop requires a SYSTEM process in the active session attached to the input desktop. Deskflow: service tracks console session → duplicates winlogon.exe token → `CreateProcessAsUser` → `OpenInputDesktop(DF_ALLOWOTHERACCOUNTHOOK)` + `SetThreadDesktop` on desktop change (`MSWindowsDesks.cpp` 658, 918). Ctrl-Alt-Del cannot be injected; needs `SendSAS(FALSE)` from a service with `HKLM\...\Policies\System\SoftwareSASGeneration` = 1 (or 3). `windows` 0.62.2 binds SendSAS, WTSQueryUserToken, CreateProcessAsUserW, OpenInputDesktop, SetThreadDesktop. **Mitigation:** `mouser-service` (SYSTEM) owns the network endpoint, per-session `mouser-agent` owns hooks/injection, tray UI on top; installer sets SoftwareSASGeneration=1; show if Group Policy overrides; keys in machine-scope DPAPI; security review of service IPC.
### W5 Apps ignoring simulated input (Story 2 sc. 7) — HIGH as worded / LOW with limitation
Injected flags are detectable; real fix = kernel virtual HID (EV signing + attestation). **Mitigation:** user-mode only in v1 with a stated limitation.
### W6 Clipboard formats — LOW
CF_UNICODETEXT, "HTML Format", "Rich Text Format", CF_DIB/DIBV5 + "PNG", CF_HDROP via `windows`; `AddClipboardFormatListener`; delayed rendering. Deskflow converters. arboard covers text/image/HTML only. **Mitigation:** custom module on `windows`.
### W7 Remote files via paste and drag (Stories 6.2, 7) — MEDIUM paste / HIGH drag
Virtual-file data object: `CFSTR_FILEDESCRIPTORW` + `CFSTR_FILECONTENTS` (IStream) + `IDataObjectAsyncCapability` → Explorer extracts with its own progress/overwrite/disk-space dialogs. Drag in: local `DoDragDrop` started by injected button-down over a helper window; drag out: `IDropTarget` on transparent edge window. **Mitigation:** virtual-file paste first; probe drag before committing Story 7 Windows tasks.
### W8 Monitors with DPI — LOW–MEDIUM
Per-Monitor-V2 in manifest; `EnumDisplayMonitors`/`GetMonitorInfoW`; `GetDpiForMonitor(MDT_EFFECTIVE_DPI)`; physical mm only from EDID (unreliable). **Mitigation:** `SetCursorPos` for placement; DPI-derived size fallback; re-enumerate on WM_DISPLAYCHANGE/WM_DPICHANGED.
### W9 Portable mode (Story 13) — MEDIUM
No service (no login/UAC/lock/Ctrl-Alt-Del); no elevated windows; firewall rule needs admin (inbound UDP may be blocked; outbound-initiated works); autostart writes HKCU Run (conflicts with 13.2); WebView2 not guaranteed on Win10, data folder in %LOCALAPPDATA%; Mark-of-the-Web SmartScreen warning. **Mitigation:** fixed limitation list in portable build; portable dials out; redirect all state incl. WebView2 data to the drive; optionally bundle fixed-version WebView2 (~150–200 MB).

## Linux — X11
### X1 Capture — MEDIUM
XInput2 `XI_RawMotion` on root + `XGrabPointer`/`XGrabKeyboard` + `XWarpPointer` (Deskflow `XWindowsComputer.cpp` 1170, 1806, 1861–1876, 1952); grabs fail when another client holds one; XFixes barriers for corner blocking; under XWayland capture is not global — choose backend from `XDG_SESSION_TYPE`/`WAYLAND_DISPLAY`. **Mitigation:** retry grabs with backoff; port design not code (Deskflow GPL-2.0).
### X2 Injection — LOW
XTest with `XTestGrabControl(True)`; uinput deferred.
### X3 Clipboard — LOW (text/HTML/image), MEDIUM (files)
Own CLIPBOARD, TARGETS, INCR; `UTF8_STRING`, `text/html`, `image/png`, `text/uri-list`, `x-special/gnome-copied-files`, `application/x-kde-cutselection`; XFixes owner-change; remote files must be staged locally.
### X4 Drag-and-drop — HIGH
XDND source/target at edge; no prior implementation. **Mitigation:** probe, else stated limitation.

## Linux — Wayland
### L1 Portal version matrix — HIGH
| Capability | GNOME | KDE Plasma |
|---|---|---|
| RemoteDesktop + EIS (injection) | 45 | 6.0 |
| RemoteDesktop persistence | 45 | fixed 6.6/6.7 |
| InputCapture (capture) | 45 (Deskflow: 46 practical) | 6.1 |
| InputCapture persistence | 51 (needs x-d-p ≥ 1.21.1/1.22.0) | 6.7 |
| Cursor hidden while captured | 51 | fixed |
| Clipboard on InputCapture | 51 (Deskflow says 50) | 6.7 |
| Clipboard on RemoteDesktop | 45 | 6.4 |
| GlobalShortcuts portal | 48 | available |

wlroots/Sway, COSMIC, niri, Hyprland lack InputCapture (Deskflow #7499, 2026-10-08). **Mitigation:** floor GNOME ≥ 46 / Plasma ≥ 6.1; detect portal `version` and `SupportedCapabilities` at runtime; per-feature limitations shown in app.
### L2 Rust crates — MEDIUM
`ashpd` 0.13.13: full InputCapture v2 (`create_session2`, `start` with persist_mode/restore_token/clipboard_enabled, zones, barriers, `connect_to_eis`), RemoteDesktop (persistence, EIS), Clipboard. `reis` 0.7.1 pre-1.0 (used in production by lan-mouse). **Mitigation:** pin versions; EI behind a Mouser trait; lan-mouse (GPL-3.0) reference only.
### L3 Permission dialogs and persistence — HIGH
Both portals prompt at `Start`; restore tokens are single-use (store each new one). Unsandboxed apps identified via systemd scope / `.desktop` (host app registry, x-d-p ≥ 1.19). KDE pre-authorization (6.3), notification for pre-authorized apps (6.7). **Mitigation:** ship `.desktop` with stable app ID, call `org.freedesktop.host.portal.Registry.Register`, `persist_mode=2`, detect a pending dialog and raise "Approve Mouser on <computer>"; state limitation for GNOME < 51 / Plasma < 6.7.
### L4 Wayland clipboard for a background app — HIGH (GNOME ≤ 50 as source) / MEDIUM
KDE: ext-data-control since 6.4, wlr dropped 6.5, from 6.8 allowed for unsandboxed clients. GNOME: no data-control; only portal Clipboard on RemoteDesktop (45+) or InputCapture (51+, only while active). Portal file transfer `application/vnd.portal.filetransfer`. **Mitigation:** portal clipboard first, ext-data-control fallback on KDE, stated limitation on GNOME ≤ 50.
### L5 Drag-and-drop on Wayland — HIGH, probably infeasible on GNOME
**Mitigation:** stated limitation; use file copy/paste.
### L6 Edge behaviour — MEDIUM
Barriers are axis-aligned segments (corners blockable with gaps); `Release` cursor_position is a hint the compositor may ignore; no full-screen detection; setting barriers suspends the session until `Enable()`. **Mitigation:** barrier gaps for corners; stated limitation for full-screen blocking; probe double-bump release.
### L7 Login screen — LOW (not in Story 12 scope) but stated limitation
GDM/SDDM Wayland greeters have no third-party input path; X11 greeters drivable by root XTest.
### L8 Autostart and tray — MEDIUM
XDG autostart works; stock GNOME needs AppIndicator extension (Ubuntu yes, Fedora no); `tray-icon` uses libayatana-appindicator; `ksni` pure-Rust SNI. **Mitigation:** detect `org.kde.StatusNotifierWatcher`; fall back to notifications + app window with a hint.
### L9 Linux portable — LOW
AppImage; portal grants persist in the desktop. **Mitigation:** stated limitation.

## Probes to schedule on hardware
1. Windows 11 hooks: Raw Input vs LL-swallowed events; LL hooks with an elevated foreground window; ClipCursor across desktop switch.
2. Windows 11 service: winlogon-token agent at cold boot, UAC, fast user switching; SendSAS with policy 1 vs 3.
3. Windows drag: `DoDragDrop` from injected button-down; virtual-file drop into Explorer with IDataObjectAsyncCapability.
4. Wayland: Ubuntu 24.04 (GNOME 46), Ubuntu 26.04 (GNOME 50), Fedora (GNOME 51), Plasma 6.7/6.8 — both portals start, persistence across logout/reboot with registered app ID, clipboard per session type.
5. GNOME/KDE: RemoteDesktop/EIS input on the lock screen; InputCapture during a drag.
6. X11: grab failures with menus open; XFixes barriers for corner blocking.
