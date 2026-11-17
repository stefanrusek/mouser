# Research: coexist with apps that capture the mouse

Probe evidence is in `spikes/FINDINGS.md`. External dependency here: the macOS window server's cursor APIs and what another app does with them.

## Diagnosis status (spec Background: "MUST confirm, MUST NOT assume")

| Claim in the spec | Status | Evidence |
|---|---|---|
| Mouser sets state once and never looks again | Confirmed | FINDINGS S0a |
| Another app can undo it (visible, associated, position) | Not yet observed | FINDINGS S1 not run; the diagnosis rests on recollection of SDL/GLFW |
| The game's windowing library captures the way recalled | Unchecked | The recalled calls (`CGAssociateMouseAndMouseCursorPosition(false)`, a hide, warp to window centre) are what `spike_foreign_capture.c` emulates; confirming needs the owner |
| Timing points at the game side | Confirmed | FINDINGS S0d |
| The Mac is the controller in the owner's setup | Confirmed from the log | `control.changed` shows `how: crossing` with the Mac as `from` and `app: Warp` on return; the Mac hides on `crossing_out`, so it is the controller |
| Recovery key is default QuickJump | Confirmed by owner | spec Background; `config.toml` has `quickjump = "Ctrl+Alt+Shift+J"` and `panic = "Escape*2"` |

## Decisions

### D1 Re-assert by watching, not by one more set
- **Decision**: while grabbed, a guard thread wakes every 20 ms, reads cursor visibility and position, and corrects any deviation; association cannot be read through public API, so it is re-asserted (`associate(false)` is idempotent) every 250 ms and whenever visibility or position deviated.
- **Rationale**: no notification exists for cursor visibility or association changes (the public CoreGraphics API offers only reconfiguration callbacks); polling at 20 ms gives a worst case near 45 ms against FR-001's 100 ms with margin for window-server latency. Cost is about 50 cheap reads a second, only while grabbed, so nothing is added to forwarding (FR-010): the guard shares no lock with the tap callback.
- **Alternatives**: a second event tap for the foreign warps (rejected: warps by `CGWarpMouseCursorPosition` generate no event, and visibility never does); `kCGSSetConnectionProperty` tricks to make the hide stick (rejected: private API on top of the private API already used, no evidence it holds); re-grab on every foreground-app change (rejected: a game can re-capture without a focus change).

### D2 Foreign motion is filtered by event source, with a plausibility fallback
- **Decision**: while grabbed, only motion events whose `kCGEventSourceStateID` is the HID system state become `LocalInput::Motion`; others are dropped and not forwarded. If S2 shows foreign events carry the HID state too, fall back to dropping motion whose absolute position equals the guard's last park or a foreign correction (same shape as `Echo`).
- **Rationale**: FR-002 and FR-003 say a foreign move must not move the remote pointer. The tap already ignores Mouser's own tag; this adds the mirror rule for other apps.
- **Alternatives**: trust deltas only (rejected: a game's warp to window centre can arrive as a large delta and fling the remote pointer, the "input going to the other computer" symptom).

### D3 Arrival on the Mac: a settle window, then yield
- **Decision**: on ungrab the guard stays in an "arrived" phase for 1000 ms: it re-reads visibility and position every 20 ms and re-applies show and associate if a capturing app undoes them, then stops. After that the foreign app wins (FR-006: the cursor behaves as the releasing app left it).
- **Rationale**: FR-005 says keep the state "if the game tries to re-capture", but a guard that never yields would make a game on the Mac unplayable for as long as Mouser runs. One second covers the observed tug of war right after a crossing (the game re-capturing on the focus change that the arrival causes). Needs the owner's confirmation; recorded in the Plan Summary.
- **Alternatives**: yield immediately (rejected: that is today's partial take-back); never yield (rejected above).

### D4 Hide counting is owned by Mouser
- **Decision**: Mouser records how many hides it has issued (`hides`). A correction while grabbed is "show `hides` times, then hide once" so the count stays at one; `set_grabbed(false)` shows `hides` times, never more. Visibility is read with `CGCursorIsVisible` (deprecated, but it is what the platform conformance test already uses and it still answers on this OS).
- **Rationale**: an unbalanced hide leaves the cursor stuck hidden, a Principle I failure. S3 confirms or replaces this rule; the guard's correction is one function so replacing it is local.
- **Alternatives**: unconditional repeated `hide()` (rejected: counted hides stack).

### D5 Logging of foreign changes needs no platform-trait change
- **Decision**: the macOS backend gets the same diagnostics sink the Windows backend has (`set_diagnostics`, app wires it in `apps/mouser-app/src/diagnostics.rs`); it writes one new catalog event, `cursor.foreign_change`, coalesced per aspect over 1 s with a count.
- **Rationale**: the `InputCapture` and `LocalInput` seam is frozen and every backend, the testkit and core test doubles implement it; the existing sink pattern reaches the log with no seam change.
- **Alternatives**: a `LocalInput::ForeignChange` variant (rejected: touches five crates for a log line).

### D6 Windows and Linux: documented limitation, not a guess
- **Decision**: this fix is macOS only. Windows (low-level hooks, blanked system cursors, a cursor-guard process) and Linux (X11, Wayland portals) hold the cursor in different ways, and whether a capturing game fights Mouser there has not been observed or read; the planner did not read those backends for this. The plan documents that in `docs/troubleshooting.md` and records it in `owner-confirmations.md`, with no VM work in this feature (VM rules).
- **Rationale**: FR-009 allows a documented limitation; inventing a user-visible warning for an unobserved Windows behaviour would be a guess shown to users. The proposal for the Plan Summary is to promote it to a user-visible limitation only if the owner sees the bug there.

## Risks (open until the owner runs S1-S4)

| id | Risk | If it goes the other way |
|---|---|---|
| R1 | A foreign capture changes aspects other than the three the guard re-reads | add the aspect to `Observation` (data-model.md); no design change |
| R2 | Foreign motion events carry the HID source state | use the plausibility fallback in D2 |
| R3 | Hide count semantics differ (clamps, or is per connection) | change `correct_hidden` only (D4) |
| R4 | The game's re-capture never lets the 1000 ms window pass quietly | lengthen the constant; if it needs to be unbounded, escalate to the owner |
