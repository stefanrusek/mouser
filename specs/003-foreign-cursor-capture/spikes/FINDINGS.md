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
| S1 | After a capture while the pointer is remote, is the cursor visible? Which of visible / associated / position changed? | NOT RUN | Decides which aspects the guard must re-read (R1). |
| S2 | Does the capture's warp or posted event reach Mouser's tap as a motion event, and what is its `kCGEventSourceStateID`? | NOT RUN | Decides how FR-002 is met: filter by source (preferred) or by plausibility (R2). |
| S3 | Hide counting: with the probe's `hides` at 1 and Mouser's at 1, after the probe releases, is the cursor still hidden? Does one extra `CGDisplayShowCursor` from Mouser clamp at zero? | NOT RUN | Decides whether a correction may be "show, then hide" or must count (R3). Getting this wrong leaves the cursor stuck hidden. |
| S4 | Mode `hold`: with the pointer moved onto the Mac, does Mouser's show+associate break the capture, and does the probe's next call undo it? | NOT RUN | Decides the settle window in Story 2 (D3). |

Until S1-S4 are run the plan holds R1-R4 open as risks (plan.md "Risks"). The design below is built so that every outcome of S1-S4 is handled by a constant or a one-line branch, not a redesign.
