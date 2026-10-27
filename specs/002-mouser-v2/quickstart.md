# Quickstart Validation: Mouser v2

These are the runnable scenarios that prove v2 works end to end. They are differential to `specs/001-mouser-v1/quickstart.md`, whose sections 1–8 still apply. Behaviour details are in `contracts/` and `data-model.md` and are not repeated here.

**Results.** Each real-hardware result goes into `specs/002-mouser-v2/evidence/quickstart.md`. Record the outcome, the hardware and OS, the Mouser version, the date and who ran it (Story 7). A failure is recorded as a defect against the story it breaks.

> Never run suites that inject real input on the owner's Mac from an agent session. `cargo xtask gate --all` on macOS drives real input (001 T495). Agents run package gates. `--all` runs only on CI runners or by the owner.

## Prerequisites

- Same toolchain as 001: `cargo fetch --locked && cargo xtask bootstrap`.
- Real machines:
  - the owner's two Macs;
  - the Parallels Windows 11 VM, with Parallels shared clipboard **off** for every clipboard check (OD-3);
  - optional and not gating (owner decision OD-3): Steve's two Windows PCs with VMware Workstation;
  - the Linux computer: the owner's Lima Ubuntu VM with a desktop session, with X11, GNOME Wayland and KDE Wayland sessions installed (owner decision OD-6; it gates every Linux row);
  - optional and not gating: a Linux user's own computer.
- Builds:
  - the 0.3.0 release candidate: Windows NSIS installer, Windows portable zip, macOS dmg, Linux AppImage/deb;
  - one 0.2.8 build, for the mixed-version checks.
- Turn on diagnostic logging on every computer before each scenario (`mouser logs enable`). Collect a bundle after each scenario (`mouser logs collect`).

## 1. Automated gates

```bash
cargo xtask gate --packages mouser-proto,mouser-platform,mouser-testkit --lane foundational
cargo xtask gate --packages mouser-core --lane core          # includes acc_v2_us1..us6
cargo xtask gate --packages mouser-app --lane app            # Rust + node UI (acc_v2_us3/us4 *.test.js)
cargo xtask gate --packages mouser-cli --lane cli
cargo xtask gate --packages mouser-service-win --lane service-win   # Windows runner
```

**Expected:** every gate passes with 100% line coverage. Evidence is appended to `specs/002-mouser-v2/evidence/<lane>.jsonl`. The gate now takes the evidence directory from `.specify/feature.json`.

## 2. Reliability (Story 1), simulated

`cargo test -p mouser-core --test acc_v2_us1`. The scenarios use VirtualPlatform knobs from contracts/platform-api-delta.md:

- `refuse_input([Key, Button])` while receiving → the link stays up, later accepted input arrives, and `input.refused` is logged.
- `panic_on_next_inject()` → `fault` and `fault.recovered` are logged. Crossing, panic key and pause still work. There is no restart.
- `lose_capture(HooksSilent)` while grabbed → `pointer_hidden()` is false, the state is Local, and `control.changed how=capture_lost` is logged.
- Link black-holed (SimNet) while Remote → the pointer is shown locally. After the network is restored, the desk user can cross again within 001 SC-008's 5 s.
- `Engine::shutdown` while Remote → the pointer is shown and the peer releases what it held.

## 3. Logging and desk bundle (Story 2)

On two computers with logging on:

1. Cross back and forth.
2. Copy text containing the marker `MOUSER-MARKER-7f3a`, and type the marker on both computers.
3. Change a setting in the app, then another from the CLI, then edit the config file.
4. Drag a computer in Displays.
5. Pull one computer's network.
6. Run `mouser logs collect --out /tmp/b`.

**Expected:**

- The bundle has one folder per computer, and `bundle.json` validates against `contracts/log-bundle.schema.json`.
- Every line validates against `contracts/log-events.schema.json`.
- `grep -r MOUSER-MARKER-7f3a` over the unzipped bundle and all log folders finds 0 matches (SC-004).
- Each file starts with `log.header`.
- The sequence of crossings, setting changes (with origin), layout changes and the `peer.disconnected reason=unreachable` can be reconstructed from the bundle alone (SC-003).

## 4. One window (Story 3)

On a desk of two Macs, Windows and Linux:

- Open from the tray. Visit every sidebar item and search for "panic", "landing" and "logs".
- From the tray, choose Arrange displays while the window is open. There is still one window, now showing Displays.
- Count the notices. Each limitation appears once, under its own computer, in Health only. No raw decimals appear (SC-006). Responsiveness reads like "Responds quickly (9 ms)".
- With a 0.2.8 peer, both versions are shown. With an incompatible peer, the computer to update is named.
- Tray: a left click pauses and then resumes, and a right click opens the menu. Unplug a desk-mate's network and the badge appears. Reconnect it and the badge goes away. Remove that computer from the desk and no badge is shown. Take a greyscale screenshot and check that the states can still be told apart (SC-008).
- In the Parallels VM, start `spike_uipi swallow`, a third-party-style LL hook installed after Mouser's that eats Mouser's probes. Use the VM for 10 minutes (a test duration chosen by the plan so that many 5 s watchdog cycles pass; it is not an owner timing target). Health shows no "lost its hold" problem (Story 3 sc. 16). Optionally confirm on Steve's PC with ASUS AsHotplugCtrl (not gating).

## 5. Settings (Story 4)

In Settings:

- Set the panic key to F12×3. Pressing F12 three times returns control, and Esc×2 no longer does.
- Record a new QuickJump chord.
- Rename Beta from Alpha. Check the name in the arranger, Health, notifications and `mouser status` on every computer, including one that was offline during the rename.
- Turn the landing highlight off.
- Turn "Use Command as Control" off. On Windows, Cmd+R opens Run (Win+R) and Ctrl+C copies.

**Expected:** every change takes effect without a restart. SC-007: a new owner does all of the above without documentation.

## 6. Windows elevation (Story 5), on the Parallels Windows 11 VM (OD-3)

| # | Steps | Expected |
|---|---|---|
| 5a | Install the NSIS build unsigned on a clean Windows. Follow only the release-notes text past "Windows protected your PC". | One UAC approval. `MouserService` is running. The tray app is in UI mode (SC-010). |
| 5b | Cold boot. From a Mac, cross to the PC at the login screen. | You can sign in (sc. 1). |
| 5c | Trigger a UAC prompt, then switch users, then lock the PC. | Each screen can be operated, and the new user's session is controllable (sc. 2). |
| 5d | Tray → Send Ctrl-Alt-Del while the PC is active. | The security screen appears. With `SoftwareSASGeneration=0` forced by policy, Health states the limitation (sc. 3). |
| 5e | Start an elevated application (`cmd.exe` or Task Manager "Run as administrator"; VMware Workstation as administrator on Steve's PC as an optional extra). Type, click, scroll and move into it from a Mac. Then sit at the PC: push past an edge, press the panic key and press a hotkey. | Everything works (sc. 4, 5). |
| 5f | Portable zip, with an elevated application in front. Cross onto the PC. | Control returns to the Mac. The Mac shows the message naming the application and the PC, with "Restart Mouser as administrator" and the hint to install (sc. 6). |
| 5g | Choose Restart. Approve UAC **at the PC's own keyboard** (in the VM: the Parallels window; owner decision OD-1). Cross again. | The elevated application receives the input. The edge, panic key and hotkeys work there (sc. 7). Health lists the portable limitations (sc. 8). |
| 5h | Uninstall (with the tray app open in the signed-in session). | No Mouser process remains — no session engine, tray app or `--cursor-guard` process — and no `MouserService` exists (FR-013). |
| 5i | With Mouser installed, Diagnostics → "Collect logs from the desk", then `mouser logs collect --out C:\Users\<you>\Desktop`. | Each zip is written by the signed-in user (file owner is that user, not SYSTEM) to Downloads / the chosen folder; File Explorer opens on it (OD-7, Story 3 sc. 12). |
| 5j | Portable restarted as administrator (after 5g), then start `mouser-app.exe` again from Explorer (not elevated). | The second launch shows "Mouser is already running as administrator on this computer. Open it from the tray icon." and exits; one Mouser keeps running (spec edge case). |

## 7. Headless (Story 6)

- **Windows portable:** `mouser-app.exe --headless`. **Linux:** `mouser-app --headless` in a signed-in session.
- **Expected:** no window or tray. Other computers list the computer as connected, and you can cross to it. Linux Health lists "Mouser can't control this computer's login screen."

## 8. Real-hardware checklist (Story 7, SC-011)

Record every row in `specs/002-mouser-v2/evidence/quickstart.md`.

| ID | Check | Hardware |
|---|---|---|
| R1 | Wi-Fi to Ethernet switch, and Wi-Fi toggle while controlling a computer → crossing works within 5 s | Mac pair (gating); real Windows PC (optional, Steve) |
| R1w | While the Mac controls the Parallels VM, switch the VM's network adapter between Shared and Bridged (Wi-Fi) → crossing works within 5 s. This tests the Windows guest's interface and address change, not a real Wi-Fi/Ethernet switch (OD-6) | Parallels VM (gating) |
| R2 | VPN connect and disconnect | Mac |
| R3 | Unplug the network while the computer has the pointer → control returns, no key is stuck | Mac ↔ Windows |
| R4 | Kill Mouser (Activity Monitor / Task Manager → End task, `kill -9`) while the pointer is remote → pointer visible on that computer (SC-002; research V5) | Mac, Parallels VM, Lima VM X11, Lima VM Wayland |
| R5 | Portable from USB: join, eject, nothing left behind | Windows |
| R6 | Linux on X11, GNOME Wayland and KDE Wayland: join, cross, clipboard; on Wayland Health lists "Mouser can't tell which application is in front here, so its diagnostic log doesn't name it." | Lima VM (gating); a Linux user's computer (optional) |
| R7 | Lock and unlock a Mac, and the Mac login window, from the shared keyboard | Mac |
| R8 | Section 6 rows 5a–5h | Parallels VM (gating); Steve's PC (optional) |
| R9 | **Trigger case (FR-011, SC-001), gating form:** the owner's Mac and the Parallels Windows 11 VM, with Mouser installed on Windows. The VM receives the pointer from the Mac. Bring an elevated application to the front with a click (`cmd.exe` Run as administrator). Then type, press Esc×2, cross back and cross again. Repeat 20 times. Then repeat with a non-elevated application, then do both again with portable Mouser (portable and not elevated → sc. 6 behaviour). **Expected:** in 20 of 20 runs per variant, the pointer is visible and working on a computer the user can use. After that, the user can cross both ways without restarting Mouser, within 5 s of the computers being able to reach each other. The bundle names the computer, the time, the cause and the application (SC-003). **Optional, not gating:** the original two-PC VMware Workstation form on Steve's PCs (VMware does not run on Windows ARM64). | Mac + Parallels VM (gating); Steve's two PCs (optional) |
| R10a | Run `spike_uipi capture` (captures the pointer itself: `ClipCursor` to its window plus raw input with `RIDEV_CAPTUREMOUSE`, released by Ctrl+Alt) on the VM while it has the pointer from the Mac; click into it, then press Ctrl+Alt → the stand-in releases its capture and Mouser never leaves the pointer hidden on either computer (spec VM-capture edge case; a stand-in for a VM application, not VMware) | Parallels VM (gating) |
| R10 | VMware VM captures the pointer (click into a running VM) → Ctrl+Alt releases VMware's capture, and Mouser never leaves the pointer hidden (spec edge case) | Steve's PC (optional, not gating; no VMware on the ARM64 VM) |
| R11 | Installing past the unknown-publisher warning using only the release notes (SC-010) | Parallels VM, by someone new to Mouser if possible |
| R12 | Mixed desk 0.2.8 + 0.3.0: crossing, pause, and `logs collect` lists the 0.2.8 computer as "older version" | Mac + Mac |
