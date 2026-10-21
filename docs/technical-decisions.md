# Mouser — Technical Decisions

Decided 2026-10-21. See [product-requirements.md](product-requirements.md) for what is being built.

| Area | Decision | Notes |
|---|---|---|
| Core language | Rust | One engine for discovery, desks, transport, layout, clipboard, file transfer. |
| UI | Tauri | Tray/menu bar app, display arranger, health panel; one UI codebase for macOS, Windows, Linux. |
| Transport | QUIC | Connection migration survives Wi-Fi/Ethernet/IP changes; targets the "drops while typing" failure. |
| Input layers | Per-OS crates behind one trait | macOS: HID-level CGEvent tap with cursor disassociation, CGEventPost injection. Windows: low-level hooks + SendInput, SYSTEM service for login/UAC/Ctrl-Alt-Del. Linux: XInput2 + XTest on X11; InputCapture + RemoteDesktop portals with libei on GNOME/KDE Wayland. |
| Linux targets | X11, GNOME Wayland, KDE Wayland | User's machines are mixed. |
| macOS signing | Developer ID, signed and notarized | Keeps Accessibility permission across updates; required for LAN auto-update. |
| Process | Spec-kit (SDD) pipeline | specify → plan → tasks → implement, in phases. |
| CI | GitHub repository + GitHub Actions macOS, Windows, Ubuntu runners | Nothing links cross-OS from a Mac; owner's Windows/Linux machines run hardware probes. |
| Test doubles | `VirtualPlatform` (OS stand-in) and `SimNet` (network impairment relay) | Real backends pass the same contract tests on their own OS runners. |
| Signing | Apple Developer ID + notarization now; Windows Authenticode later; minisign update key | Windows builds show SmartScreen warnings until signed. |
| Updates | GitHub Releases `latest.json` (Tauri updater format), one desk computer downloads and shares over LAN | Only internet access Mouser makes. |
| Platform limitations | Shown in the app (see product requirements) | macOS/Windows apps ignoring simulated input; Wayland portal versions; no file drag on Linux; Windows portable limits; GNOME tray. |
