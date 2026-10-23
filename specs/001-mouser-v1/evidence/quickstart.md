# Quickstart sections 2-8: real-machine validation outcomes (T503)

Recorded 2026-10-23. Outcome codes:

- **VERIFIED-BY-J1**: exercised on real machines by the J1 run (Mac mini + Windows 11 Parallels VM, Mouser 0.2.4, 2026-10-23). Steps are in `apps/mouser-cli/tests/e2e/features/e2e_j1_two_computers.feature`; passing record is the e2e line in `convergence.jsonl` from commit 3ca6338.
- **VERIFIED-BY-OWNER**: confirmed by hand by the owner during this run (not scripted).
- **VERIFIED-BY-TESTS**: covered only by the automated simulated acceptance suites (`crates/mouser-core/tests/features/usN.feature`, run by `acc_usN.rs`) or CLI contract tests. These use the testkit, not real machines and real input, so they are not real-machine evidence.
- **PENDING-OWNER**: needs hands on real hardware.

Owner-confirmed this run: Mac-to-Mac daily use (0.2.0 to 0.2.1 auto-update) across a desk; display grouping matches the OS arrangement; landing highlight; copy/paste across Macs (after the Cmd-modifier fix); Mac-to-Windows connection, crossing indicator and keystrokes arriving (owner plus logger; the VM cursor not being drawn while Parallels is in the background is a VM artifact); LAN auto-update propagating between Macs.

## 2. Two computers, one desk

| # | Check | Outcome | Evidence |
|---|---|---|---|
| 1 | Launch A, grant permissions, Create desk | VERIFIED-BY-J1 | J1 "the owner chooses Create desk "Home office" on computer A"; also owner (Macs) |
| 2 | B sees "Home office", chooses it | VERIFIED-BY-J1 | J1 "the owner chooses "Home office" on computer B" |
| 3 | A clicks Allow | VERIFIED-BY-J1 | J1 "the owner clicks Allow on computer A" (decline path also in J1) |
| 4 | Both trays show 2 computers | VERIFIED-BY-J1 | J1 "both computers list each other as connected members of "Home office""; tray text itself not separately asserted: PENDING-OWNER (glance at both trays) |
| 4 | `mouser status --json` validates against `status.schema.json` | VERIFIED-BY-TESTS | `apps/mouser-cli/tests/contract_cli_status.rs`; not run against the live pair: PENDING-OWNER |
| 5 | Pointer crosses to B, typing works, cross back | VERIFIED-BY-J1 | J1 Story 2 scenarios 2 and 3 steps (type, save, file contents match, cross back); also owner (Mac-Mac, Mac-Windows) |
| 5 | Landing highlight | VERIFIED-BY-OWNER | Mac-Mac; `us2.feature` scenario 9 for logic |
| 6 | Esc twice returns control to A | VERIFIED-BY-J1 | J1 "presses Escape twice on computer A" |

## 3. Self-healing

| # | Check | Outcome | Evidence |
|---|---|---|---|
| 1 | 60 s continuous typing, every character in order | VERIFIED-BY-J1 | J1 "types a continuous passage for 60 seconds ... contains the continuous passage exactly" |
| 2 | Sleep/wake B, wait 5 s, cross | VERIFIED-BY-J1 | J1 "computer B is put to sleep / wakes / 5 seconds pass" then cross and type (Windows VM) |
| 3 | Wi-Fi to Ethernet / Wi-Fi toggle, crossing within 5 s | VERIFIED-BY-TESTS (simulated) | `us3.feature` scenario 3 and SC-008. PENDING-OWNER: toggle Wi-Fi on B while controlling it and cross within 5 s |
| 4 | VPN connect/disconnect | VERIFIED-BY-TESTS (simulated) | `us3.feature` scenario 4. PENDING-OWNER: connect then disconnect a VPN on B and cross |
| 5 | Unplug network while B has the pointer: control returns to A, no stuck keys | VERIFIED-BY-TESTS (simulated) | `us3.feature` scenario 5 and key-held edge case. PENDING-OWNER: pull B's network while it holds the pointer |

## 4. Two desks on one network

| # | Check | Outcome | Evidence |
|---|---|---|---|
| 1 | Second desk exists, neither lists the other's computers | VERIFIED-BY-J1 | J1 "Lab bench" on B, "neither desk lists the other's computer as a member" (B stood in for C) |
| 2 | Copy on C leaves A and B clipboards unchanged | VERIFIED-BY-TESTS (simulated) | `us6.feature` "Story 1 scenario 4: clipboard isolation between desks". PENDING-OWNER: with a real third desk or separate desk on a spare computer, copy text and confirm the others are unchanged |

## 5. Data exchange

| # | Check | Outcome | Evidence |
|---|---|---|---|
| 1 | Copy/paste formatted text A to B | VERIFIED-BY-OWNER | Mac-Mac, after the Cmd-modifier fix; formatting fidelity not specifically recorded |
| 2 | Image copy/paste | VERIFIED-BY-TESTS (simulated) | `us6.feature` scenario 1. PENDING-OWNER: copy an image on A, paste on B |
| 3 | Folder copy/paste | VERIFIED-BY-TESTS (simulated) | `us6.feature` scenario 2. PENDING-OWNER: copy a folder in Finder on A, paste on B |
| 4 | Drag folder to B's desktop, progress, overwrite prompt | VERIFIED-BY-TESTS (simulated) | `us7.feature` scenarios 1-3. PENDING-OWNER: drag a folder A to B, watch progress, repeat for the overwrite prompt |

## 6. Arranger, crossing control, QuickJump

| # | Check | Outcome | Evidence |
|---|---|---|---|
| 1 | Display grouping matches OS arrangement | VERIFIED-BY-OWNER | After fix; `us2.feature` scenarios 1, 6 |
| 2 | Drag B to the other side of A, crossing follows | VERIFIED-BY-TESTS (simulated) | `us4.feature` scenario 1. PENDING-OWNER: drag B across A in the arranger and cross on the new side |
| 3 | Corner blocking; double bump crosses | VERIFIED-BY-TESTS (simulated) | `us5.feature` scenarios 1, 3. PENDING-OWNER: push into a corner (stays), bump twice (crosses) |
| 4 | QuickJump letters on every display, press one to jump | VERIFIED-BY-TESTS (simulated) | `us11.feature` scenarios 1-3. PENDING-OWNER: press the hotkey, press a letter |

## 7. Diagnostics and agent access

| # | Check | Outcome | Evidence |
|---|---|---|---|
| 1 | Revoke a permission on B: health panel names it with Open settings | VERIFIED-BY-TESTS (simulated) | `us8.feature` scenarios 1-3. PENDING-OWNER: revoke Accessibility/Input Monitoring for Mouser on B and read the panel |
| 2 | `mouser logs enable`, crossing, `logs tail -n 20` lines validate against `log-record.schema.json` | VERIFIED-BY-TESTS | `apps/mouser-cli/tests/contract_cli_logs.rs`, `us9.feature` scenario 3. Owner also used a logger on the Mac-Windows run (not a schema check). PENDING-OWNER: run the command pair against the live app |
| 3 | `mouser config set crossing.block_fullscreen false` shows in GUI | VERIFIED-BY-TESTS | `apps/mouser-cli/tests/contract_cli_config.rs`, `us9.feature` scenario 2. PENDING-OWNER: run it and look at the GUI |

## 8. Updates, secure screens, portable

| # | Check | Outcome | Evidence |
|---|---|---|---|
| 1 | One computer downloads, the other updates over the LAN, permissions intact | VERIFIED-BY-OWNER | LAN auto-update 0.2.0 to 0.2.1 propagated between Macs with permissions intact; `us10.feature` scenarios 1-3. Not yet on Windows |
| 2 | Windows login with shared keyboard / Send Ctrl-Alt-Del | DEFERRED | Owner decision 2026-10-22: deferred to a future feature; `us12.feature` kept for scope only |
| 3 | Mac: lock and unlock from the shared keyboard | VERIFIED-BY-TESTS (simulated) | `us12.feature` scenario 1. PENDING-OWNER: lock the Mac, unlock from the shared keyboard |
| 4 | Portable build from USB, join desk, remove drive, no files remain | VERIFIED-BY-TESTS (simulated) | `us13.feature` scenarios 1-2 and the Windows limitation. PENDING-OWNER: run the portable build from USB on Windows, join, eject, confirm nothing left |

## Not yet verified by hand (summary)

Wi-Fi network switch, VPN, USB portable mode on Windows, Linux real hardware, Windows-to-Windows, login-window/secure desktop (deferred feature).
