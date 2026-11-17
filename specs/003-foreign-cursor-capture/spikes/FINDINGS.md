# 003 probes: observed results

Each item is CONFIRMED, REFUTED or NOT RUN. Nothing here was inferred.

## S0 Diagnosis checks the spec asked for (planner, 2026-11-17, read-only)

| # | Check | Status | Observed |
|---|---|---|---|
| S0a | Mouser sets the Mac cursor state once and never looks again | CONFIRMED by reading | `crates/mouser-platform-macos/src/capture.rs` `set_grabbed` (lines 354-370): returns early when the state is unchanged, otherwise calls warp, `associate(false)`, `hide()` once. No thread, timer or tap event re-reads cursor visibility, position or association while grabbed. The tap thread (`tap.rs`) only runs the event callback. |
| S0b | The diagnostic log can show a foreign change | REFUTED (it cannot) | `~/Library/Application Support/Mouser/logs/mouser.log` (1.9 MB, 2026-10-23 to 2026-11-17): 1435 `pointer.hidden` and 1434 `pointer.shown`, no double hide, none out of order; the one unmatched hide is the current remote crossing. The log records only what Mouser did, never what the Mac cursor did, so the owner's incident leaves no trace. FR-007 is therefore a real gap, not a nicety. |
| S0c | Idle-Mac check: with Mouser holding the pointer remote and no capturing app, the Mac cursor is hidden and still | CONFIRMED | `spike_cursor_state.c`, output `spike_cursor_state.out`: six samples over 3 s, `visible=0 pos=(4.0,1147.0)` every time, while the log's last event was `control.changed` to the other computer. The baseline is clean, so any later change is foreign. |
| S0d | No recent commit changed this behaviour | CONFIRMED | `git log` on `capture.rs`: the last changes are 2a89ba9 (SetsCursorInBackground), 2aea0c9 (App Nap opt-out), 6fbf794/51d52b8 (refused warp is an error, rule 10). None touches how grab state is held. The "about a week" timing therefore points at the game side, as the spec suspected. |

## S1-S4 What a capturing app does to Mouser (owner-run)

Probe: `spike_foreign_capture.c` (emulates the calls SDL and GLFW make; needs a person watching the screen, so the planner did not run it). Not run: it changes live cursor state, and the owner does the spikes during plan (project rule).

| # | Question | Status | Why it matters |
|---|---|---|---|
| S1 | After a capture while the pointer is remote, is the cursor visible? Which of visible / associated / position changed? | PARTLY CONFIRMED (2026-11-17, run unattended by the orchestrator at the owner's request; Mouser's log confirmed the pointer stayed remote for both runs) | `capture-while-remote`: before the capture `visible=0 pos=(0,954)`; at the capture, position jumped to the screen centre `(1280,720)` and stayed there through ten re-captures; `visible` stayed 0 throughout. So POSITION changed and visibility did not. Association is not readable, so not observed. **The owner's symptom (cursor visible on this screen) was NOT reproduced by this emulation.** Caveat: the probe is a command-line process with no window, so it is not a faithful stand-in for a foreground game; a real game's activation may be what reveals the cursor. Decides which aspects the guard must re-read (R1): position is confirmed, visibility is not. |
| S2 | Does the capture's warp or posted event reach Mouser's tap as a motion event, and what is its `kCGEventSourceStateID`? | NOT RUN | Decides how FR-002 is met: filter by source (preferred) or by plausibility (R2). |
| S3 | Hide counting: with the probe's `hides` at 1 and Mouser's at 1, after the probe releases, is the cursor still hidden? Does one extra `CGDisplayShowCursor` from Mouser clamp at zero? | PARTLY CONFIRMED (same runs) | `release-while-remote`: the probe hid once (`our_hides=1`), released at t=6000 ms (`our_hides=0`, probe called show once per hide), and `visible=0` held to the end of the run (t=19250 ms). So the probe's balanced show did NOT unhide the cursor while Mouser's own hide was outstanding. The release did not restore the position either: it stayed at `(1280,720)`. NOT observed: whether an extra show from Mouser clamps at zero (not tested). Caveat: the probe's hide may have had no effect of its own as a background process, so this shows only that a balanced foreign show/hide leaves Mouser's hide intact. Decides whether a correction may be "show, then hide" or must count (R3). |
| S4 | Mode `hold`: with the pointer moved onto the Mac, does Mouser's show+associate break the capture, and does the probe's next call undo it? | NOT RUN | Decides the settle window in Story 2 (D3). |

Until S1-S4 are run the plan holds R1-R4 open as risks (plan.md "Risks"). The design below is built so that every outcome of S1-S4 is handled by a constant or a one-line branch, not a redesign.

## S5 Real game (cobolcraft) while the pointer is remote (orchestrator-run, owner away, 2026-11-17)

Setup: Mouser's log showed `control.changed` to the other computer at 17:57:39.641Z; the pointer stayed remote for the whole run (no `pointer.shown`, no `control.changed` back). A read-only sampler (`sample_cursor`, 250 ms) started 2 s before the game. The game was launched with the owner's exact command (`cobolcraft --name Stefan --gamedir /tmp/cc-test`), left running 25 s, then killed. Samples: `cobolcraft_real_game.samples.out`.

| Observation | Result |
|---|---|
| Cursor visibility | `visible=0` before launch, flipped to `visible=1` between t=3750 ms and t=5750 ms (about 2-4 s after the game started) and stayed 1 to the end of the 30 s sampling, including after the game was killed. |
| Cursor position | Unchanged at `(0,792)` the whole run. No warp to the screen centre, unlike the S1 emulation. |
| Mouser's log | Nothing: no `pointer.shown`, no `control.changed`. Mouser still believed the pointer was remote while the Mac cursor was visible. |
| Game output | `language: 2 lang files, no problem`, then only the kill signal. Not observed: whether a window opened, or whether the game captured the mouse. |

Verdict: **CONFIRMED** that launching the real game while the pointer is remote makes the Mac cursor visible with Mouser unaware, which is the owner's "visible on both screens" symptom. The cause is the game launching or activating (a visibility change), NOT a capture warp: position never moved. This corrects S1 (visibility, not position, is the aspect the guard must re-read first) and means R1 resolves toward visibility. The cursor was left visible after the run (Mouser still believed remote); QuickJump or double-Escape recovers.

## T001 Baseline before any 003 change (implementer, 2026-11-17)

Pre-existing failure, no 003 code touched yet: `cargo test -p mouser-platform-macos --test contract_platform_macos_us2` fails `a_grab_swallows_local_keys_and_reports_pointer_motion_until_released` (crates/mouser-platform-macos/tests/contract_platform_macos_us2.rs:253, left 0, right 2: no pointer motion reported). It drives the real event tap, so it likely needs input/Accessibility permission or a quiet foreground; not investigated. Everything else passed: other macOS tests (8 of 9 in that file), `cargo test -p mouser-app` (203), `cargo clippy --workspace --all-targets`, `cargo fmt --check`.
