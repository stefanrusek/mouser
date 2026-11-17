# Owner confirmations: batched for after implementation

The owner's rule (2026-10-29): implementation never waits on the owner. Checks that need a person are collected here and done together once implementation is finished. Each one should take under 2 minutes unless it says otherwise.

| # | Where | What to do | What to look for | Source |
|---|-------|------------|------------------|--------|
| 1 | Mac | Run the tray-click spike (`specs/002-mouser-v2/spikes/spike_tray_click/RUNBOOK.md`) | A left click toggles the icon and doesn't open the menu; a right click opens the menu | P4 / T015 |
| 2 | Windows VM | The same tray-click spike | Same as row 1. (The menu and Quit already work on 0.2.7, as the owner reported.) | P4 / T015 |
| 3 | Windows VM | Turn the UAC prompt back on (ConsentPromptBehaviorAdmin), then start the relaunch as administrator and click **No** | Mouser reports that it stayed unelevated; it doesn't hang or crash | P3 Scenario E / T014 |
| 4 | Windows VM, installed 0.3.0 | Lock the PC and log in again, using only the Mac's keyboard and mouse | The login screen can be controlled from the Mac (needs `MouserService`) | P2 / T012, Story 5 sc. 1 |
| 5 | Windows VM, installed 0.3.0 | Trigger a UAC prompt and answer it from the Mac | The pointer follows onto the secure desktop within about one watchdog cycle (≤ 5 s) | P2 / T012, Story 5 sc. 2 |
| 6 | Windows VM, installed 0.3.0 | Fast user switching to a second account and back | Control follows the active session | P2 / T012 |
| 7 | Windows VM, installed 0.3.0 | Drag a file from the Mac onto the Windows desktop while it's controlled through the service | The file arrives. If it doesn't, the record becomes "drag as SYSTEM: refuted" and the limitation text changes | P2 item 4 / T013 |
| 8 | Windows VM, installed 0.3.0 | Send Ctrl-Alt-Del from the Mac | The secure attention screen opens, or the policy message appears | Story 5 |
| 9 | Windows VM | Answer the Windows Firewall "allow on public networks" dialog left by a test run (Cancel or Allow) | The dialog is gone | win-mouser-safe 2026-10-29 |
| 10 | Mac | Press Ctrl+Alt+Shift+J | QuickJump appears, and the focused app doesn't receive the keys | 002 regression fix fb51a93 |
| 11 | Mac and Windows | Run the J1 two-computer journey on 0.3.0 | Every J1 step passes | T496+ (OD-4) |
| 12 | Windows VM, 0.3.0 | With the Macs offline, look at the log and Task Manager | No flood of peer.disconnected messages several times a second, and no steady CPU use (0.2.7 showed both) | win-mouser-safe 2026-10-29 |
| 13 | Windows VM, installed build with T538–T540 | Tray → Quit, then start Mouser from the Start menu | Quit: the app exits and `MouserService` stops (`sc query MouserService` shows STOPPED). Start menu: Mouser and the service come back with no UAC prompt | win-mouser-safe 2026-10-29; owner 2026-11-03 |
| 14 | Lima Ubuntu VM (GNOME), Mac, Windows VM | Left-click and right-click the tray icon | A left click toggles pause and a right click opens the menu. On Linux the appindicator backend may not deliver clicks at all; if so, record it. The red and green connection dots are legible at tray size | T259–T262 built from the tray-icon 0.25.1 docs; P4 not run |
| 15 | Windows VM, installed 0.3.0 | Run `icacls C:\ProgramData\Mouser\secrets` in an admin prompt, then try to open a file in that folder as a normal user | Only SYSTEM and Administrators are listed (protected, no inherited Users entry); the normal user gets access denied | Phase 27 finding: FileSecrets sets no ACL on Windows |
| 16 | Mac (real hardware) | `cargo test -p mouser-platform-macos --features real-drag --test contract_platform_macos_us7`, hands off the mouse | Four `ok` lines and exit 0. Not in hosted CI: macOS 15/26 runners never deliver mouse events to the drag loop. Passed on the owner's Mac 2026-10-31 | us7 CI exception |
| 17 | Mac | Feature 003 open item D3: with the pointer on the other computer, run the probe `hold` mode (or the game), cross onto the Mac | The cursor is visible and under control within 100 ms with no flicker, and a game that re-captures after the crossing is only fought for 1 s (settle window `SETTLE`, R4); lengthen the constant if the game keeps winning | 003 D3 / R4, quickstart section 3 step 3 |
| 18 | Mac | Feature 003 probe S2 (R2) and the T035 filter choice: run `spike_foreign_capture` (capture-while-remote) with a real foreign app moving the cursor while the pointer is remote | The foreign move is NOT forwarded to the other computer. If it is, switch `FILTER_IN_FORCE` in `capture.rs` to 1 (plausibility) and record the choice in `specs/003-foreign-cursor-capture/research.md` | 003 T014 / T035, quickstart section 3 step 1 |
| 19 | Mac | Feature 003 probe S4 (R4): mode `hold`, move the pointer onto the Mac and watch whether the show and associate break the capture and whether the probe's next call undoes it | Records the settle behaviour that decides D3 | 003 S4 |
| 20 | Mac, Mouser.app quit | Feature 003 SC-001: 100 start/pause/quit cycles with the pointer remote | The Mac cursor is never visible and no input reaches the Mac | 003 SC-001 |
| 21 | Mac | Feature 003 SC-003: a full working session beside the game | No recovery key (QuickJump, Escape twice) was needed | 003 SC-003 |
| 22 | Windows and Linux | Feature 003 D6 / FR-009: no work done; the limitation is documented only | If the owner sees the cursor-capture bug on Windows or Linux, promote it to a listed limitation or a fix | 003 D6 |

Owner's acceptance (2026-11-17): feature 003 proceeds with the diagnosis only partly confirmed. S5 (the real game) showed the cursor becoming visible with Mouser unaware; S2 and S4 were never run, and S1 and S3 were only partly confirmed by emulation. Rows 17 to 21 are the checks that close it, together with the full gate (OD-1) and the Windows-side compile of `crates/mouser-platform-windows/src/diag.rs`.
