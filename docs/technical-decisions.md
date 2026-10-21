# Mouser — Technical Decisions

Decided 2026-10-21. See [product-requirements.md](product-requirements.md) for what is being built.

| Area | Decision | Notes |
|---|---|---|
| Core language | Rust | One engine for discovery, desks, transport, layout, clipboard, file transfer. |
| UI | Tauri | Tray/menu bar app, display arranger, health panel; one UI codebase for macOS, Windows, Linux. |
| Transport | QUIC | Connection migration survives Wi-Fi/Ethernet/IP changes; targets the "drops while typing" failure. |
| Input layers | Per-OS crates | macOS: CGEvent tap + IOHID injection. Windows: Raw Input + SendInput (service later for login/UAC). Linux: libei/InputCapture portal on GNOME and KDE Wayland, plus X11. |
| Linux targets | X11, GNOME Wayland, KDE Wayland | User's machines are mixed. |
| macOS signing | Developer ID, signed and notarized | Keeps Accessibility permission across updates; required for LAN auto-update. |
| Process | Spec-kit (SDD) pipeline | specify → plan → tasks → implement, in phases. |
