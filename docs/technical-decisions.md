# Mouser — Technical Decisions

Decided 2026-10-21; updated for the as-built v1 and, further down, the as-built v2 work (feature 002, still in progress; the version is 0.2.x until the 0.3.0 release). See [product-requirements.md](product-requirements.md) for what is being built and what v1 delivers.

| Area | Decision | Notes |
|---|---|---|
| Core language | Rust | One engine for discovery, desks, transport, layout, clipboard, file transfer. |
| UI | Tauri | Tray/menu bar app, display arranger, health panel; one UI codebase for macOS, Windows, Linux. |
| Transport | QUIC | Connection migration survives Wi-Fi/Ethernet/IP changes; targets the "drops while typing" failure. |
| Input layers | Per-OS crates behind one trait | macOS: HID-level CGEvent tap with cursor disassociation, CGEventPost injection. Windows: low-level hooks + SendInput, SYSTEM service for login/UAC/Ctrl-Alt-Del (built in v2 as `MouserService` plus a session engine; the installer that deploys it is not built yet, see "As built in v2"). Linux: XInput2 + XTest on X11; InputCapture + RemoteDesktop portals with libei on GNOME/KDE Wayland. |
| Linux targets | X11, GNOME Wayland, KDE Wayland | User's machines are mixed. |
| macOS signing | Developer ID, signed and notarized | Keeps Accessibility permission across updates; required for LAN auto-update. |
| Process | Spec-kit (SDD) pipeline | specify → plan → tasks → implement, in phases. |
| CI | GitHub repository + GitHub Actions macOS, Windows, Ubuntu runners | Nothing links cross-OS from a Mac; owner's Windows/Linux machines run hardware probes. |
| Test doubles | `VirtualPlatform` (OS stand-in) and `SimNet` (network impairment relay) | Real backends pass the same contract tests on their own OS runners. |
| Signing | Apple Developer ID + notarization now; Windows Authenticode later; minisign update key | Windows builds show SmartScreen warnings until signed; v2 ships Windows unsigned (no certificate). |
| Updates | GitHub Releases `latest.json` (Tauri updater format), one desk computer downloads and shares over LAN | Only internet access Mouser makes. |
| Platform limitations | Shown in the app (see product requirements) | macOS/Windows apps ignoring simulated input; Wayland portal versions; no file drag on Linux; Windows portable limits; GNOME tray. |

## As built

### Workspace and toolchain

- Rust 1.99.0 (pinned in `rust-toolchain.toml`, with rustfmt, clippy and llvm-tools-preview), edition 2024, resolver 3. Workspace version 0.2.8 (0.3.0 is set at the end of v2), MIT, `publish = false`. Rust warnings and clippy `all` are denied workspace-wide.
- Node.js 22 for the UI tests and the Tauri CLI (`@tauri-apps/cli@2.12.4`).
- Members: 3 apps, 7 crates and `xtask`. `specs/` is excluded.

| Member | What it is |
|---|---|
| `crates/mouser-proto` | Wire protocol, control API types, config schema and validation, log record type. serde 1, serde_json 1, postcard 1.1.3, sha2 0.11, toml 1, ipnet 2.12. |
| `crates/mouser-platform` | The `Platform` trait every OS backend implements, plus the conformance checks (feature `conformance`). |
| `crates/mouser-testkit` | `VirtualPlatform` and `SimNet` test doubles (tokio 1.54). |
| `crates/mouser-core` | The engine. Adds `zip` =9.1.2 (deflate only) for log bundles in v2. quinn =0.11.12, rcgen =0.14.10, mdns-sd =0.21.6, if-addrs =0.15.0, ed25519-dalek 3, hmac 0.13, sha2 0.11, blake2 0.11, rustls-platform-verifier 0.7, tokio-rustls 0.26 (ring), tokio 1.54, postcard 1.1.3, toml 1, chrono 0.4, ipnet 2.12, libc =0.2.191 (Unix). |
| `crates/mouser-platform-macos` | core-graphics =0.25.0, core-foundation =0.10.1, objc2 =0.6.5, objc2-app-kit =0.3.2, objc2-foundation =0.3.2, block2 =0.6.2, dispatch2 =0.3.1, clipboard-rs =0.3.5, security-framework =3.7.0 (Keychain), smappservice-rs =0.1.3 (login item), system-configuration =0.7.0, if-addrs =0.15.0. |
| `crates/mouser-platform-linux` | Compiled only on Linux. ashpd =0.13.13 (portals), reis =0.7.1 (libei), zbus =5.19.0, x11rb =0.14.0 (randr, xfixes, xinput, xtest), wl-clipboard-rs =0.9.4, rustix =1.1.5, tokio =1.54.0, if-addrs =0.15.0. |
| `crates/mouser-platform-windows` | The Windows backend: low-level hooks and `SendInput`, clipboard, file drag, displays, autostart, firewall watch, portable mode, DPAPI machine secret store, and in v2 the cursor guard, foreground-app readout, desktop following and the service client. `windows` and `windows-core` =0.62.2, `png` =0.18.1. |
| `apps/mouser-app` | Tauri 2.12.5 (tray-icon, image-png, macos-private-api), plugins: updater 2.14.0, single-instance 2.5.4, notification 2.7.0, global-shortcut 2.4.1; tauri-build 2.7.4. Plain HTML/JS UI in `ui/`, tested with `node --test`. Builds without a window or tray unless the `gui` feature is on. |
| `apps/mouser-cli` | The `mouser` command: clap 4 (derive), tokio 1.54. |
| `apps/mouser-service-win` | `mouser-service.exe` (v2): `MouserService` supervisor, install/uninstall, Ctrl-Alt-Del, service log. `windows-service` =0.8.1, `windows` =0.62.2. Its `winapi_sys.rs` pass-through is the one owner-approved coverage exclusion (OD-5). |
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
| macos | macos-15 | proto, platform, testkit, core, platform-macos, app, cli, xtask |
| windows | windows-2025 | proto, platform, testkit, core, platform-windows, app, cli, service-win, xtask |
| linux | ubuntu-24.04 | proto, platform, testkit, core, platform-linux, app, cli, xtask |

Each lane installs the toolchain from `rust-toolchain.toml`, Node 22, runs `cargo fetch --locked && cargo xtask bootstrap`, then `cargo xtask gate --packages ... --lane ci-<lane>`. The gate runs `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test`, a 100% line-coverage check (`cargo llvm-cov`) and the UI tests. Ubuntu also installs `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libxdo-dev libssl-dev libdbus-1-dev xvfb dbus-daemon`; the Linux contract tests need Xvfb and a D-Bus daemon. Evidence files are uploaded as the `evidence-<lane>` artifact.

`.github/workflows/release.yml` runs when a tag `v*` is pushed (and on manual dispatch as a dry run that publishes nothing):

1. `bundle` job on macos-15 (universal arm64 + x86_64 `.app` and `.dmg`), windows-2025 (NSIS installer) and ubuntu-24.04 (AppImage and `.deb`). It checks the tag equals `v` + the `tauri.conf.json` version, builds the CLI sidecar, stages the UI, and runs `tauri build --features gui` with `tauri.release.conf.json`. On macOS it imports the Developer ID certificate into a temporary keychain, signs, notarizes and staples, then verifies with `codesign`, `stapler` and `spctl`.
2. `publish` job on ubuntu-24.04: generates `latest.json` (`apps/mouser-app/scripts/make-latest-json.mjs`), validates it against `release-manifest.schema.json` with ajv-cli 5.0.0, and on a tag push creates a GitHub **pre-release** with every asset (`gh release create --prerelease`).

The release procedure and secrets are in [releasing.md](releasing.md).

### Signing state

- **macOS: signed and notarized.** Developer ID Application certificate, hardened runtime, notarized and stapled in CI. This keeps Accessibility and Input Monitoring grants across updates.
- **Windows: unsigned.** Authenticode is deferred (decision D4) and v2 ships without a signature (FR-004), so SmartScreen warns; this is an accepted limitation and docs/releasing.md has the click-through. Nothing in v2 may depend on a signature, which rules out `uiAccess` manifests. Free OSS signing (SignPath Foundation) is future work.
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

**Windows (001)**: probes 1 and 3 (hooks, injection, Explorer drag) ran in a Parallels Windows 11 ARM64 VM (`crates/mouser-platform-windows/probes/FINDINGS.md`); probe 2 (service, login screen, UAC, SendSAS) was not run. v2 probe outcomes are below.


## As built in v2 (feature 002)

Decisions V1 to V20 are in `specs/002-mouser-v2/research.md`; the code is authoritative. What was built, and what was not:

**Reliability**
- V1: every inject, warp and grab result is handled in core; a refusal skips that input and is logged (`input.refused`), never a panic. V2: core's locks never poison (`LockExt::guard`). V3: a process panic hook logs `fault`; units of work (reconcile tick, each inbound message, each input event, each control request) run under `catch_unwind`, and a caught fault forces the crossing state to Local, shows the pointer, releases everything held and logs `fault.recovered`. Unknown wire messages are skipped (`wire.unknown`).
- V4: Quit, `RunEvent::Exit` and session end run `Engine::shutdown` (stop input, return local, show the pointer, release held input, close links with reason `quit`). V5: the cursor guard, a second `mouser-app.exe --cursor-guard <pid>` process, restores the Windows system cursors if Mouser ends without cleaning up; macOS and Linux rely on the window server and the compositor.
- V6: `Platform::foreground_app()` returns the program name, whether it is administrator and whether it outranks Mouser (UIPI). A receiver whose foreground outranks it hands control back (reusing `Panic`, no wire change) and raises `elevated_app`; V7: the Windows backend gives up capture (`capture.lost`) when the foreground outranks Mouser, another desktop takes input, or one real miss occurs, and the watchdog counts a miss only when real input was used and the hook saw nothing, so a third-party hook swallowing Mouser's probes (ASUS AsHotplugCtrl) raises nothing. V8: `SetCursorPos` motion and VMware's own capture are left as accepted limitation 1.

**Windows installed mode (built, not packaged)**
- V9 to V11: `MouserService` (LocalSystem, automatic) is a supervisor that tracks the console session with `SERVICE_CONTROL_SESSIONCHANGE` (no polling, no timers) and starts the session engine, `mouser-app.exe --headless --machine`, with a `winlogon.exe` token on `winsta0\default`; the engine follows the input desktop on the watchdog cycle. The service and engine talk over `\\.\pipe\mouser-svc-<session>` (SYSTEM-only DACL, client PID check, postcard frames: `Hello`, `SendSas`, `SasResult`, `Shutdown`, `Stopping`). Ctrl-Alt-Del is `SendSAS(FALSE)` from the service with `SoftwareSASGeneration=1`. The user's tray app is meant to run in UI mode against the engine over `\\.\pipe\mouser-service`.
- Not done: the per-machine NSIS installer, its hooks and the `mouser-service` sidecar packaging (T490 to T493); the control methods that let UI mode run fixes, answer file-replace prompts, show the landing flash, QuickJump and the review prompt (T530 to T532); the version bump to 0.3.0 (T494).
- V12: the installer design is `installMode: perMachine`, hooks running `mouser-service.exe --install` and `--uninstall`, unsigned. Decided 2026-10-30: uninstalling keeps `C:\ProgramData\Mouser` so upgrades keep the desk membership. V12a: `--install` copies the installing user's desk membership to the machine scope when it can; otherwise the computer rejoins with one approval (OD-2).
- V13: `--headless` runs on all three OSes; `--headless --machine` uses the machine folder.

**Diagnostics, window, settings**
- V14, V15: typed log events with closed field lists (`log-events.schema.json`), a `log.header` record at the start of every file, two files of `max_log_mb/2`, and `logs.collect` (zip written with `zip` 9.1.2, `LogsRequest`/`LogsPart` over the existing release-stream pattern, peers 0.3.0 or newer only). OD-7: on Windows installed the SYSTEM engine never writes a user-chosen path; it streams the zip (`logs.collect_stream`, `logs.chunk`) to the user's own Mouser or CLI.
- V16 to V18: one `main` window with a sidebar, search and hash routes; limitations are listed under their own computer and never raised as problems; problems are keyed by (id, computer); numbers are rounded (`ui/format.js`); the tray left-click pauses and the icon shows connection state with a shape change as well as colour. V19: portable "Restart Mouser as administrator" (`ShellExecuteExW runas`, `--wait-for`, `MouserData/elevated.pid`). V17, V20: friendly names ride `DeskState`; `landing_highlight` and `command_as_control` ride the new `DeskSettings` message, gated to peers 0.3.0 or newer (`V2_SINCE`), with `PROTOCOL_VERSION` still 1.
- New dependencies: `zip` =9.1.2 (core), `windows-service` =0.8.1 (service-win). Tauri and tray-icon are unchanged.

**Probe outcomes (specs/002-mouser-v2/spikes/FINDINGS.md; Parallels Windows 11 ARM64 VM, Windows probes run there by decision OD-3)**
- P1 `spike_uipi`: hooks keep seeing real input over an elevated foreground window (confirmed in both Parallels mouse modes); `SendInput` and `SetCursorPos` into an elevated foreground are blocked by UIPI, and `SendInput` still returns success with no error, so its return value is not a delivery signal and the elevated case must be detected from the foreground readout; the foreground program, elevation and integrity are readable for elevated user apps (system and service processes cannot be opened from medium integrity, so an unreadable process is treated as outranking Mouser); the out-of-context foreground WinEvent fires for elevated windows; the cursor guard restored the cursors in about 50 ms after its parent was terminated; real input still arrives past a hook that swallows only probe events; raw-input capture works in the VM, but Parallels defeats `ClipCursor` (the pointer must be hidden another way) and `Ctrl+Alt` is Parallels' own release chord (capture release must avoid it). A job object with kill-on-close could kill the guard with its parent; not exercised.
- P3 `spike_elevate`: a `runas` relaunch with `--wait-for` hands over the single-instance mutex cleanly; injection from a medium process into an elevated app is silently dropped; injection from an elevated process into an elevated app is delivered; a medium second launch does see an elevated first instance's mutex (no duplicate instance), but its `WM_COPYDATA` hand-off is blocked by UIPI, hence the `elevated.pid` check in V19. Declining the UAC prompt (`ERROR_CANCELLED`) was not run because the VM auto-elevates.
- P2 `spike_service` (cold boot, login screen, UAC, user switching, `SendSAS` policy values, clipboard and drag as SYSTEM, NSIS per-machine): not run. The service was built to the research and these items are unverified on real hardware (owner confirmations rows 4 to 8).
- P4 `spike_tray_click` (left versus right tray click): not run. The tray code was written from the tray-icon 0.25.1 documentation; owner confirmation rows 1, 2 and 14 check it. Linux's appindicator emits no click events, so Linux states a limitation.
