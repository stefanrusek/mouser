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
| 13 | Windows VM, 0.3.0 | Tray → Quit | The app exits (0.2.7 didn't; T235/T236 fix this) | win-mouser-safe 2026-10-29 |
| 14 | Lima Ubuntu VM (GNOME), Mac, Windows VM | Left-click and right-click the tray icon | A left click toggles pause and a right click opens the menu. On Linux the appindicator backend may not deliver clicks at all; if so, record it. The red and green connection dots are legible at tray size | T259–T262 built from the tray-icon 0.25.1 docs; P4 not run |
