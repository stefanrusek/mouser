# Mouser — Technical Decisions

Decided 2026-10-21; updated for the as-built v1 (below the table). See [product-requirements.md](product-requirements.md) for what is being built and what v1 delivers.

| Area | Decision | Notes |
|---|---|---|
| Core language | Rust | One engine for discovery, desks, transport, layout, clipboard, file transfer. |
| UI | Tauri | Tray/menu bar app, display arranger, health panel; one UI codebase for macOS, Windows, Linux. |
| Transport | QUIC | Connection migration survives Wi-Fi/Ethernet/IP changes; targets the "drops while typing" failure. |
| Input layers | Per-OS crates behind one trait | macOS: HID-level CGEvent tap with cursor disassociation, CGEventPost injection. Windows: low-level hooks + SendInput, SYSTEM service for login/UAC/Ctrl-Alt-Del (the service is deferred to a future feature (owner decision 2026-10-22)). Linux: XInput2 + XTest on X11; InputCapture + RemoteDesktop portals with libei on GNOME/KDE Wayland. |
| Linux targets | X11, GNOME Wayland, KDE Wayland | User's machines are mixed. |
| macOS signing | Developer ID, signed and notarized | Keeps Accessibility permission across updates; required for LAN auto-update. |
| Process | Spec-kit (SDD) pipeline | specify → plan → tasks → implement, in phases. |
| CI | GitHub repository + GitHub Actions macOS, Windows, Ubuntu runners | Nothing links cross-OS from a Mac; owner's Windows/Linux machines run hardware probes. |
| Test doubles | `VirtualPlatform` (OS stand-in) and `SimNet` (network impairment relay) | Real backends pass the same contract tests on their own OS runners. |
| Signing | Apple Developer ID + notarization now; Windows Authenticode later; minisign update key | Windows builds show SmartScreen warnings until signed. |
| Updates | GitHub Releases `latest.json` (Tauri updater format), one desk computer downloads and shares over LAN | Only internet access Mouser makes. |
| Platform limitations | Shown in the app (see product requirements) | macOS/Windows apps ignoring simulated input; Wayland portal versions; no file drag on Linux; Windows portable limits; GNOME tray. |

## As built

### Workspace and toolchain

- Rust 1.99.0 (pinned in `rust-toolchain.toml`, with rustfmt, clippy and llvm-tools-preview), edition 2024, resolver 3. Workspace version 0.1.1, MIT, `publish = false`. Rust warnings and clippy `all` are denied workspace-wide.
- Node.js 22 for the UI tests and the Tauri CLI (`@tauri-apps/cli@2.12.4`).
- Members: 3 apps, 7 crates and `xtask`. `specs/` is excluded.

| Member | What it is |
|---|---|
| `crates/mouser-proto` | Wire protocol, control API types, config schema and validation, log record type. serde 1, serde_json 1, postcard 1.1.3, sha2 0.11, toml 1, ipnet 2.12. |
| `crates/mouser-platform` | The `Platform` trait every OS backend implements, plus the conformance checks (feature `conformance`). |
| `crates/mouser-testkit` | `VirtualPlatform` and `SimNet` test doubles (tokio 1.54). |
| `crates/mouser-core` | The engine. quinn =0.11.12, rcgen =0.14.10, mdns-sd =0.21.6, if-addrs =0.15.0, ed25519-dalek 3, hmac 0.13, sha2 0.11, blake2 0.11, rustls-platform-verifier 0.7, tokio-rustls 0.26 (ring), tokio 1.54, postcard 1.1.3, toml 1, chrono 0.4, ipnet 2.12, libc =0.2.191 (Unix). |
| `crates/mouser-platform-macos` | core-graphics =0.25.0, core-foundation =0.10.1, objc2 =0.6.5, objc2-app-kit =0.3.2, objc2-foundation =0.3.2, block2 =0.6.2, dispatch2 =0.3.1, clipboard-rs =0.3.5, security-framework =3.7.0 (Keychain), smappservice-rs =0.1.3 (login item), system-configuration =0.7.0, if-addrs =0.15.0. |
| `crates/mouser-platform-linux` | Compiled only on Linux. ashpd =0.13.13 (portals), reis =0.7.1 (libei), zbus =5.19.0, x11rb =0.14.0 (randr, xfixes, xinput, xtest), wl-clipboard-rs =0.9.4, rustix =1.1.5, tokio =1.54.0, if-addrs =0.15.0. |
| `crates/mouser-platform-windows` | **Empty.** No Windows backend exists. |
| `apps/mouser-app` | Tauri 2.12.5 (tray-icon, image-png, macos-private-api), plugins: updater 2.14.0, single-instance 2.5.4, notification 2.7.0, global-shortcut 2.4.1; tauri-build 2.7.4. Plain HTML/JS UI in `ui/`, tested with `node --test`. Builds without a window or tray unless the `gui` feature is on. |
| `apps/mouser-cli` | The `mouser` command: clap 4 (derive), tokio 1.54. |
| `apps/mouser-service-win` | A 13-line stub; the service is deferred to a future feature (owner decision 2026-10-22). |
| `xtask` | `bootstrap` and `gate`. |

The pins in `Cargo.lock` are exact; versions written `=` above are also exact in `Cargo.toml`. Tests: cucumber 0.23.0 for the behaviour suites, jsonschema 0.59.0 for contract tests.

### Behaviour that differs from the original plan

These are as-built decisions; the code is authoritative. The longer list of per-story decisions and open items is in `specs/001-mouser-v1/foundational-notes.md`.

- Pause (hotkey or `mouser pause`) applies to one computer, not the whole desk. Making it desk-wide needs a new wire message.
- Wayland capture is edge-driven: no local motion until the pointer pushes through an outer-edge barrier. `set_grabbed` is unsupported, so hotkey and QuickJump crossings cannot start from a Wayland computer.
- The control socket is `~/Library/Application Support/Mouser/mouser.sock` (macOS), `$XDG_RUNTIME_DIR/mouser.sock` (Linux) and `\\.\pipe\mouser-<USERNAME>` (Windows; the contract said the user's SID).
- Config keys are re-read on the engine's 250 ms tick; the file is applied whole or refused whole.
- Update bundles travel over the LAN; each computer re-verifies the minisign signature, and the app re-signs the verified bundle with a one-time key to drive the Tauri updater on loopback.
- The login-window mode is `mouser-app --headless` with secrets in 0600 files in a machine directory (no user Keychain exists at the login window).

### CI layout

`.github/workflows/ci.yml` runs on every push to `main` and every pull request. One job, `gate`, runs three lanes in parallel with `fail-fast: false`:

| Lane | Runner | Packages gated |
|---|---|---|
| macos | macos-14 | proto, platform, testkit, core, platform-macos, app, cli, xtask |
| windows | windows-2025 | proto, platform, testkit, core, platform-windows, app, cli, service-win, xtask |
| linux | ubuntu-24.04 | proto, platform, testkit, core, platform-linux, app, cli, xtask |

Each lane installs the toolchain from `rust-toolchain.toml`, Node 22, runs `cargo fetch --locked && cargo xtask bootstrap`, then `cargo xtask gate --packages ... --lane ci-<lane>`. The gate runs `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test`, a 100% line-coverage check (`cargo llvm-cov`) and the UI tests. Ubuntu also installs `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libxdo-dev libssl-dev libdbus-1-dev xvfb dbus-daemon`; the Linux contract tests need Xvfb and a D-Bus daemon. Evidence files are uploaded as the `evidence-<lane>` artifact.

`.github/workflows/release.yml` runs when a tag `v*` is pushed (and on manual dispatch as a dry run that publishes nothing):

1. `bundle` job on macos-14 (universal arm64 + x86_64 `.app` and `.dmg`), windows-2025 (NSIS installer) and ubuntu-24.04 (AppImage and `.deb`). It checks the tag equals `v` + the `tauri.conf.json` version, builds the CLI sidecar, stages the UI, and runs `tauri build --features gui` with `tauri.release.conf.json`. On macOS it imports the Developer ID certificate into a temporary keychain, signs, notarizes and staples, then verifies with `codesign`, `stapler` and `spctl`.
2. `publish` job on ubuntu-24.04: generates `latest.json` (`apps/mouser-app/scripts/make-latest-json.mjs`), validates it against `release-manifest.schema.json` with ajv-cli 5.0.0, and on a tag push creates a GitHub **pre-release** with every asset (`gh release create --prerelease`).

The release procedure and secrets are in [releasing.md](releasing.md).

### Signing state

- **macOS: signed and notarized.** Developer ID Application certificate, hardened runtime, notarized and stapled in CI. This keeps Accessibility and Input Monitoring grants across updates.
- **Windows: unsigned.** Authenticode is deferred (decision D4), so SmartScreen warns on the installer. No Windows backend exists in any case.
- **Linux: unsigned** AppImage and `.deb`. Not verified on a real machine.
- **Update bundles:** minisign-signed on all platforms. The public key is `plugins.updater.pubkey` in `apps/mouser-app/tauri.conf.json` and is also compiled into `crates/mouser-core/src/release/pubkey.rs`. If the private key is lost, installed apps can never accept another update.

### Probe outcomes

Plan-phase probes (in `specs/001-mouser-v1/spikes/`) and the Linux hardware probes (`crates/mouser-platform-linux/probes/FINDINGS.md`) decided the following. Each line says what was observed, not what was hoped.

**macOS input (spike, macOS 26.5, arm64)**
- A listen-only CGEventTap works once permission is granted; non-prompting preflight calls report state without a dialog.
- A filtering tap hides a move from apps but does not stop the cursor. Remote mode therefore uses one HID filtering tap, `CGAssociateMouseAndMouseCursorPosition(false)`, a parked cursor and the raw deltas.
- `CGEventPost` injection works. A lower-level virtual HID device is refused for an unentitled binary (`IOHIDUserDeviceCreateWithProperties` returned NULL). This is why some apps ignore Mouser's input on macOS (accepted limitation 1).
- Secure Input is detectable (`IsSecureEventInputEnabled`) and the holder's pid is in the session dictionary, so Mouser can name the app.
- Display scale comes from the display mode, not `CGDisplayPixelsWide`. Coordinates are points, may be negative.
- Full-screen detection: the data is available; the positive case was not exercised in the spike.

**Network (spike)**
- quinn + rcgen: loopback p50 0.2 ms, p99 0.35 ms; Wi-Fi p50 4.6 ms, p99 10 ms; no loss.
- Connection migration works through `Endpoint::rebind`, from the client side only, so the app must trigger it.
- Idle timeout and keep-alive detect a hard path loss in about the idle timeout; shorter outages survive.
- mdns-sd with the desk name in TXT works; unregister and wait before re-registering.
- Filtering VPN and virtual adapters needs OS hardware-type information; defaults leak, so Mouser classifies interfaces itself.
- macOS needs `NSLocalNetworkUsageDescription` and `NSBonjourServices` (`_mouser._udp`) and shows a Local Network prompt on first use.

**App tooling (spike)**
- Nothing links cross-OS from a Mac, so releases and CI use native runners.
- Tauri v2 tray app with a hidden window builds and runs.
- GitHub Releases plus the Tauri updater format work; `releases/latest/download/latest.json` redirects to the CDN and is not rate-limited, but only resolves for non-pre-releases (so Mouser reads the releases list through the API by default).

**Linux hardware probes (aarch64 VMs: Ubuntu 24.04 GNOME 46, Ubuntu 26.04 GNOME 50, Fedora 44 Plasma 6.7.5; no real GPU or input hardware)**
- RemoteDesktop portal persists with a restore token on all three (silent after the first grant). InputCapture cannot persist on GNOME 46/50 and none was observed on Plasma 6.7.5, so "approve Mouser after each login" is the normal case.
- Clipboard on Wayland must ride the RemoteDesktop session and be requested up front (an existing grant made without it does not allow clipboard). Clipboard on an InputCapture session works only on Plasma 6.7+.
- Barriers: GNOME accepts a barrier at `x+width`; `x+width-1` is rejected.
- Pressing into the edge during a GTK drag cancels the drag on GNOME 46 (so file drag out of Wayland does not work this way).
- Lock screen: on GNOME 46/50 a persisted session started silently and absolute pointer moves worked on the lock screen; keyboard typing could not be verified. Plasma lock screen not exercised.
- X11: grabs fail with `AlreadyGrabbed` for as long as another client (for example an open GTK menu) holds one. XFixes barriers block pointer motion (XTest too) when `directions` is set correctly, but a diagonal move into an exact corner leaks through unless the barriers overlap by about 20 px.
- Crate defects worked around: ashpd fails to decode InputCapture `SupportedCapabilities` on GNOME 46/50 (the raw u32 is read instead); `CreateSessionOptions` is private in ashpd 0.13.13; after `ConnectToEIS` the portal rejects direct `Notify*` calls so injection goes through EIS; reis 0.7.1's event stream is not `Send` and runs on its own thread.
- Not run: GNOME 51, Plasma 6.8, Plasma lock screen, KDE X11, real greeters, real GPUs.

**Windows**: researched only (specs/001-mouser-v1/spikes/FINDINGS-windows-linux.md); no probe has run on Windows hardware and no backend exists.

