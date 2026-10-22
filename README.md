# Mouser

Mouser shares one mouse and keyboard across the Macs, Windows PCs and Linux computers on your local network. Push the pointer off the edge of one screen and it appears on the next computer, and your keyboard follows it. Clipboard and file drags cross the edge too (built, not yet in a release). It is a from-scratch replacement for ShareMouse.

The reason it exists is reliability. ShareMouse either fails to connect or drops the connection the moment you start typing, and it breaks when a VPN or VM network adapter is active. Mouser uses QUIC so a connection survives Wi-Fi/Ethernet switches, IP changes, sleep and VPNs, and heals itself without restarting the app. Traffic is encrypted automatically.

Computers are grouped into **desks**, not joined with passwords or pairing codes. On first launch Mouser lists the desks it finds on your network and offers "Join *Home office*" or "Create a new desk". Joining takes one click of approval on a computer already on the desk. Several desks can share a network without seeing each other's input or clipboard.

## Status

Mouser is an **early pre-release**. Expect rough edges. Published versions are on the [Releases page](https://github.com/stefanrusek/mouser/releases) (currently v0.1.0 and v0.1.1, both marked pre-release).

| Platform | State |
|---|---|
| macOS | Works between two Macs as of v0.1.1. |
| Linux | In progress. The released AppImage and .deb do not yet give you a working Linux backend. |
| Windows | In progress. The released installer does not yet give you a working Windows backend. |

What is built (in source, not necessarily in a release):

- The core engine: desks and join approval, discovery, QUIC transport with self-healing, crossing between displays, corner blocking, clipboard sharing, file drag between computers with a progress window, and problem reporting (missing permissions, blocked crossings and so on).
- The app: tray/menu bar, first-run screen, join prompt, the display arranger (with per-edge options), settings, and the file transfer window.
- The `mouser` CLI: `status`, `pause`/`resume`, `desks`, `desk`, `requests`, `approve`, `decline`.
- The macOS backend (capture, injection, clipboard, secrets, permissions, displays).
- Linux backend pieces: session detection, portal permissions, network interface classification, secret store, autostart and tray detection, plus an HID-to-evdev key table and X11 groundwork. Input capture/injection, displays and clipboard for X11 and Wayland are not finished.

What is not done yet (tracked in `specs/001-mouser-v1/tasks.md`):

- Linux input capture and injection (X11 and Wayland), Linux displays, clipboard and a network-change watcher.
- The Windows backend beyond a skeleton; `mouser-service-win` (the service for the login screen and UAC) is a skeleton too.
- Health panel, QuickJump, Send Ctrl-Alt-Del, auto-update across the LAN, portable mode, the macOS login-window agent.
- CLI commands `config`, `logs`, `update` and `send-cad`.

Known limitations:

- Most testing has been against simulated platforms (`VirtualPlatform`) and a simulated network (`SimNet`), not real multi-computer setups. Only the macOS backend has been exercised between real machines, and only two Macs.
- Pause currently applies to the computer where you press the hotkey, not to every computer on the desk.
- Large clipboard items (over 1 MiB) are fetched when you paste, so the first paste can be stale.
- Windows builds are unsigned (SmartScreen will warn). Windows Authenticode signing is deferred.
- The Linux notes below come from VM experiments, not real hardware.

## Install

Download from [GitHub Releases](https://github.com/stefanrusek/mouser/releases). Install Mouser on every computer you want on the desk. Quit ShareMouse first; two programs capturing input fight each other.

### macOS

1. Download `Mouser_<version>_universal.dmg` (Apple silicon and Intel; signed and notarized).
2. Open it and drag Mouser to Applications.
3. Launch Mouser and grant **Accessibility** and **Input Monitoring** in System Settings > Privacy & Security. Mouser tells you which one is missing and opens the right pane.
4. Allow local network access when macOS asks.
5. Optional: put the command-line tool on your PATH:

   ```
   sudo ln -s /Applications/Mouser.app/Contents/MacOS/mouser /usr/local/bin/mouser
   ```

### Linux

Download `Mouser_<version>_amd64.AppImage` or `Mouser_<version>_amd64.deb`. The .deb installs the CLI as `/usr/bin/mouser`.

As of v0.1.1 the Linux backend is not functional: you can start the app, but input capture and injection are not implemented yet. Treat the next release as the first one worth trying, and expect problems. The AppImage and .deb bundles had not been verified on a real machine when this was written.

What the design assumes, based on experiments in virtual machines (Ubuntu 24.04 with GNOME 46, Ubuntu 26.04 with GNOME 50, Fedora 44 with Plasma 6.7; X11 only on Ubuntu 24.04):

- **Wayland (GNOME, KDE):** Mouser uses the RemoteDesktop and InputCapture portals plus libei. Expect an approval dialog ("Approve Mouser on <computer>") after **each login**: InputCapture permission cannot be remembered on GNOME 46/50, and no remembering was observed on Plasma 6.7.5. The dialog can take several seconds to appear.
- **X11:** XInput2/XTest and XFixes barriers. A grab can fail while another program holds one, for example while a GTK menu is open.
- Pressing into the edge while dragging a file cancels the drag on GNOME 46, so dragging files out of a Wayland computer that way does not work.
- Not tested: GNOME 51, Plasma 6.8, Plasma lock screen, display-manager greeters, real GPUs and physical input hardware.

### Windows

Download `Mouser_<version>_x64-setup.exe`, a per-user NSIS installer. It is unsigned, so Windows SmartScreen will warn you ("More info" then "Run anyway"). The Windows backend is not functional yet, so there is little point installing it today. To use the CLI, add `%LOCALAPPDATA%\Mouser` to your PATH.

## Using it

1. **Create a desk.** On the first computer, launch Mouser and choose **Create a new desk**, then give it a name. From a terminal: `mouser desk create "Home office"`.
2. **Join it.** On the second computer, Mouser lists the desks it finds. Choose yours. From a terminal: `mouser desks`, then `mouser desk join "Home office"`.
3. **Approve.** On a computer already on the desk, click **Allow** on the join prompt. From a terminal: `mouser requests`, then `mouser approve <request>`.
4. **Cross.** Move the pointer off a shared screen edge. It appears on the other computer, with a brief highlight where it lands, and your keyboard goes with it. New computers are placed automatically; open the **arranger** from the tray to drag displays into the real layout. Clicking an edge in the arranger sets that edge's options, such as blocking corners or full-screen windows.
5. **Corners.** Crossing is blocked in display corners (to stop accidental switches). Bump the edge twice quickly to override.
6. **Bring the pointer home.** Press **Escape twice** within half a second and control returns to the local computer.
7. **Pause.** The default hotkey is Ctrl+Alt+Shift+M (configurable) and `mouser pause` / `mouser resume` do the same. Today this applies to the computer where it is issued.
8. **Clipboard and files.** Copy on one computer and paste on another. Drag files across the edge; a transfer window shows progress and asks about overwrites. The targeted-paste hotkey defaults to Ctrl+Alt+Shift+V.

When something is wrong, the app says why and offers a fix (missing permission, blocked crossing, incompatible version). To check from a terminal:

```
mouser status          # desk, computers, connection state, permissions, problems
mouser status --json   # the same, machine-readable
```

`mouser` talks to a running Mouser; it exits with code 2 if Mouser is not running. All commands accept `--json`. The full CLI design is in `specs/001-mouser-v1/contracts/cli.md`; only the commands listed under Status are implemented so far.

## Building from source

You need the Rust toolchain pinned in `rust-toolchain.toml` (rustup picks it up automatically; it includes rustfmt, clippy and llvm-tools-preview) and Node.js (CI uses 22) for the UI tests.

```
cargo fetch --locked
cargo xtask bootstrap      # installs cargo-llvm-cov and the UI's npm dependencies
```

On Ubuntu, install the system packages CI uses first:

```
sudo apt-get install -y libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libxdo-dev libssl-dev libdbus-1-dev
```

Run the app with a real window and tray (without `gui` the app has no window or tray, which is what the gate builds):

```
cargo run -p mouser-app --features gui
```

Release bundles need extra steps; see [docs/releasing.md](docs/releasing.md).

### The quality gate

```
cargo xtask gate --packages mouser-core,mouser-cli   # chosen packages
cargo xtask gate --all                               # every workspace package
```

The gate runs `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test`, a **100% line-coverage** requirement via `cargo llvm-cov`, and the UI tests with their own coverage requirement. It appends a record of every check to `specs/001-mouser-v1/evidence/`. CI (`.github/workflows/ci.yml`) runs it on macOS, Windows and Ubuntu for each push to `main` and each pull request.

## Project layout

Apps:

- `apps/mouser-app` - the Tauri tray/menu bar app and its web UI (`ui/`): arranger, first run, join prompt, settings, transfer window.
- `apps/mouser-cli` - the `mouser` command-line client.
- `apps/mouser-service-win` - Windows service for the login screen and UAC (skeleton).

Crates:

- `crates/mouser-core` - the engine: desks, discovery, transport, layout, crossing, clipboard, file transfer, problems, control API.
- `crates/mouser-proto` - wire protocol, control API types and config schema.
- `crates/mouser-platform` - the trait every OS backend implements, plus conformance checks.
- `crates/mouser-platform-macos`, `-windows`, `-linux` - the OS backends.
- `crates/mouser-testkit` - `VirtualPlatform` and `SimNet` test doubles.
- `xtask` - `bootstrap` and `gate`.

Other: `docs/` (requirements, technical decisions, releasing), `specs/001-mouser-v1/` (the spec, plan and tasks).

## How it's developed

Mouser is built spec-first. `specs/001-mouser-v1/` holds the user stories (`spec.md`), the plan, the contracts, the task list (`tasks.md`) and a quickstart of validation scenarios. The project rules, including the coverage requirement, are in `.specify/memory/constitution.md`. Product goals are in `docs/product-requirements.md` and the technology choices (Rust, Tauri, QUIC, per-OS input layers) in `docs/technical-decisions.md`.

## License

License: not yet chosen.
