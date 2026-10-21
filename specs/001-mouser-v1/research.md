# Research: Mouser v1

Phase 0 record for plan.md. Every external-dependency assumption the plan relies on is listed with its evidence (constitution 3.1). Probe code is under `spikes/spike_*`; full observations in `spikes/FINDINGS-*.md`.

## Plan-Phase Handoff disposition (spec.md → plan, Principle 2.6)

| Handoff hypothesis | Disposition | Evidence |
|---|---|---|
| Stack: Rust core, Tauri UI, QUIC, per-OS input crates | **Confirmed** (macOS); Windows/Linux by research only | FINDINGS-app-tooling A1; FINDINGS-network A1–A3; FINDINGS-macos-input 1–3 |
| Windows "Raw Input + SendInput" | **Refuted in part**: capture must use LL hooks (Raw Input can't suppress); Raw Input optional for deltas | FINDINGS-windows-linux W1 |
| QUIC connection migration handles address/adapter change | **Confirmed, client side only**; app must detect change and call `rebind`; accepting side must re-dial | FINDINGS-network A2 |
| VPN/VM breakage = adapter binding/advertising | **Supported**: mdns-sd defaults advertise loopback fe80 and cross-interface link-local; bridges/VM IPv4 not excluded; this Mac has Ethernet + Wi-Fi on the same /22 and utun interfaces. Root cause in ShareMouse not proven | FINDINGS-network A5–A6 |
| "Disconnects when typing" from keep-alive / VPN routes / Secure Input | **Open (not reproduced)**; design covers all three: idle timeout ≥ 3 s survives sub-idle flaps (A3), multi-interface advertising filtered (A5), Secure Input is a state not a disconnect (macOS 4) | FINDINGS-network A3; FINDINGS-macos-input 4 |
| Discovery via mDNS/DNS-SD with desk in TXT | **Confirmed** | FINDINGS-network A4 |
| Secure Input detectable and attributable | **Confirmed** (names the responsible app) | FINDINGS-macos-input 4 |
| Log at per-OS app-support path; CLI reads same state | **Superseded/decided** by contracts/control-api.md and config.md | — |
| ShareMouse defaults (panic Esc×2, etc.) | **Confirmed** as defaults in contracts/config.md | ShareMouse Defaults.plist (PanicKey keyCode 53, PanicCount 2) |

## Decisions

### R1 Transport: quinn 0.11.12 + rustls 0.23, self-signed device certificates
- **Decision:** One QUIC endpoint per computer, ALPN `mouser/1`; certs from the device key (rcgen 0.14.10); peer verification by pinning the cert key to the expected ComputerId (custom verifier). Pointer motion on datagrams (payload < 1100 B), everything else on streams.
- **Rationale:** loopback p50 0.20 / p99 0.35 ms; Wi-Fi p50 4.6 / p99 10.3 ms; 0/3000 datagrams lost (A1).
- **Alternatives:** TCP+TLS (no migration, head-of-line on every message); plain UDP + custom reliability (re-implements QUIC).

### R2 Liveness and recovery budget (SC-008 ≤ 5 s)
- **Decision:** `keep_alive_interval = 750 ms`, `max_idle_timeout = 3 s`. Local network-change detection (macOS SCDynamicStore/NWPathMonitor, Windows `NotifyIpInterfaceChange`, Linux netlink, plus 1 s polling fallback) triggers `Endpoint::rebind` immediately; the peer whose address changed re-dials if its role was the accepting side. Rediscovery via mDNS re-announce on change.
- **Rationale:** detection ≈ max_idle_timeout (1.22/3.08/5.07 s measured); sub-idle outages (0.6 s, 2 s) survived (A3); rebind costs ~15 µs + one RTT (A2). 3 s idle + ≤ 1 s re-dial + handshake ≈ 12 ms fits 5 s.
- **Alternatives:** 1 s idle (spurious drops on Wi-Fi roam); 5 s idle (blows SC-008).

### R3 Discovery and interface filtering
- **Decision:** mdns-sd 0.21.6 with `disable_interface(All)` + `enable_interface(Predicate(physical_lan))`; classification per OS (macOS `SCNetworkInterfaceGetInterfaceType`, Windows `GetAdaptersAddresses` IfType + name list, Linux `/sys/class/net/<if>/{device,type}`), name heuristics as fallback; prefer IPv4; fe80 only on the interface heard on; always unregister-and-wait on exit; `NSLocalNetworkUsageDescription` + `NSBonjourServices=_mouser._udp` in the macOS bundle.
- **Rationale:** A4–A7.
- **Alternatives:** UDP broadcast like ShareMouse (no per-interface control, blocked on many Wi-Fi APs).

### R4 macOS input
- **Decision:** One HID-level filtering CGEventTap; in remote mode `CGAssociateMouseAndMouseCursorPosition(false)`, park/hide cursor, read `kCGMouseEventDeltaX/Y`; inject with `CGEventPost` from an `HIDSystemState` source at `kCGHIDEventTap`, tagged via `kCGEventSourceUserData` to ignore our own events; re-enable tap on `TapDisabledByTimeout`; Secure Input via `IsSecureEventInputEnabled` + `kCGSSessionSecureInputPID`; displays via CGDisplay mode APIs with vendor/model/serial identity; full-screen via frontmost window bounds = display bounds or shielding level.
- **Rationale:** FINDINGS-macos-input 1–6 (swallowing MouseMoved does not stop the cursor — refuted assumption drove the disassociate design).
- **Open:** virtual-HID injection for apps that ignore CGEvents returned NULL without entitlements → decision D3 in plan.md.

### R5 Windows input (research only)
- **Decision:** LL hooks on a dedicated thread with watchdog; cursor parking by `SetCursorPos` warp at the crossing edge; `SendInput` injection; SYSTEM `mouser-service` + per-session agent with winlogon-derived token for login/UAC/lock and `SendSAS` (installer sets `SoftwareSASGeneration=1`); clipboard on the `windows` crate with delayed rendering and virtual-file data objects; Per-Monitor-V2 DPI.
- **Rationale/evidence:** FINDINGS-windows-linux W1–W9 (Deskflow source, Microsoft docs). **Not probed** → risk D1.

### R6 Linux input (research only)
- **Decision:** X11: XInput2 raw + grab + XTest (x11rb 0.14.0). Wayland: InputCapture + RemoteDesktop portals via ashpd 0.13.13, libei via reis 0.7.1 behind a Mouser trait; `.desktop` with stable app ID + host registry registration; `persist_mode=2`; runtime capability detection drives stated limitations. Floor: GNOME ≥ 46, Plasma ≥ 6.1.
- **Rationale/evidence:** FINDINGS-windows-linux X1–X4, L1–L9. **Not probed** → risk D1; limitations → D3.

### R7 App shell: Tauri 2.12.5, vanilla HTML/TS UI, no npm build step
- **Decision:** Tauri tray app (hidden main window), plugins single-instance 2.5.4 and autostart 2.7.0 (macOS: auto-launch SMAppService mode directly on macOS 13+), updater 2.14.0. UI is static HTML + plain ES modules (no bundler). tauri-cli only for release bundling.
- **Rationale:** A1 built with plain `cargo build` in 23 s; template tray icons on macOS.
- **Alternatives:** native per-OS UI (3× work); egui (no native tray/menus quality).

### R8 Clipboard: clipboard-rs 0.3.5 on macOS; custom `windows`-crate module on Windows; X11 selections / portal Clipboard / ext-data-control on Linux
- **Rationale:** A2 (RTF + multi-format + watcher only in clipboard-rs); W6; X3; L4.

### R9 Releases and updates (Story 10, FR-003)
- **Decision:** GitHub Releases; static `latest.json` (tauri-plugin-updater format) at `releases/latest/download/latest.json` (CDN redirect, not the 60/h API); minisign signatures; one desk computer (lowest ComputerId among online members with internet) fetches, verifies every platform entry, and re-serves manifest + bundles over the desk connection; peers point the updater endpoint at it.
- **Rationale:** A5. Contract: contracts/release-manifest.schema.json.

### R10 Testing stack
- **Decision:** `cargo test`; cucumber 0.23.0 for `acc_` suites (`harness=false`, `fail_on_skipped()`); `contract_` tests validate JSON against the schemas in `contracts/` (jsonschema crate) and the platform trait rules; coverage with cargo-llvm-cov 0.9.1 (`--fail-under-lines 100`, excluding test infra and `spikes/`).
- **Rationale:** A4 (cucumber works; llvm-tools present; cargo-llvm-cov missing → setup task).

### R11 Build and CI
- **Decision:** native runners per OS (macOS arm64, Windows x86_64, Ubuntu x86_64) for build, test, coverage and release bundling; core crates stay Tauri-free so they test everywhere.
- **Rationale:** A3 — nothing links cross-OS from this Mac; Tauri can't even `cargo check` for Windows without llvm-rc. No git remote or CI exists today → D1.

### R12 Code signing
- **Decision:** macOS Developer ID Application + hardened runtime + notarytool; Windows Authenticode; minisign key for updates. Dev builds ad-hoc signed.
- **Rationale:** A6 — tools present, 0 identities on this Mac → D4.

## Spike inventory
| Spike | Covers |
|---|---|
| spikes/spike_quic/ | R1, R2 |
| spikes/spike_mdns/ | R3 |
| spikes/spike_macos_input/ | R4 |
| spikes/spike_tauri_tray/ | R7 |
| spikes/spike_clipboard/ | R8 |
| spikes/spike_cross/ | R11 |
| spikes/spike_cucumber/ | R10 |
| (curl, updater source) | R9 |
| (desk research only) | R5, R6 |
