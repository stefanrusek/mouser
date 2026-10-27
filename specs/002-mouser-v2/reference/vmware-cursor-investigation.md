# VMware Workstation: cursor vanishes, link drops (Steve, v0.2.8, two Windows PCs)

This was a read-only investigation. Paths are relative to `/Users/stefanrusek/Code/mouser`.
Labels used below: **PROVEN** means the code shows it. **HYPOTHESIS** means it depends on Windows or VMware behaviour that nobody has observed on hardware. (FINDINGS.md:41-44 says probe E1/E2 for an elevated foreground was never run, and probe 2 / T367 was never run.)

## The log (`/Volumes/home/mouser (3).log`, PC STVH8-ASUS-V2, id 601b…)

- 21:54:35 the PC connects to 1e88…. At 21:54:40.6 it disconnects. This was the manual restart. At 21:55:36.9 it connects again.
- 21:55:51.581 probe `seen`, foreground explorer.exe.
- 21:55:56.586 probe `skipped reason=elevated_foreground foreground=vmware.exe`.
- 21:55:56.652 `peer.disconnected` 1e88…, 66 ms later.
- After that there is no `peer.connected` for at least 90 s. Platform watchdog lines keep coming every 5 s.
- Nothing about crossings, grab, or cursor hide/show is logged. Panics are not logged either: the code has no panic hook (grep for `set_hook` finds nothing), and a Windows GUI process has no stderr to write to.

Id ordering matters here. Only the lower `ComputerId` dials (engine.rs:695-707). 1e88… < 601b…, so the **other** PC has to redial, and this PC has to accept.

---

## Q1. Cursor hide/show on Windows

**Mechanism (PROVEN).** `cursor.rs:77-95` `hide()` replaces each of 14 system cursor shapes with a blank one through `SetSystemCursor`. `cursor.rs:110-117` `show()` restores them through `SystemParametersInfoW(SPI_SETCURSORS)` (`recover()`, cursor.rs:163-172). The change is **global and outlives the process**. The `HIDDEN` flag (cursor.rs:48) makes both calls idempotent.

**The only path that hides the cursor:** `WinCapture::set_grabbed(true)` (capture.rs:584-592) → `set_pointer_hidden` → `hide_or_show` (capture.rs:459-477). It has two callers in core:
- `Crossing::begin` (controller.rs:147), when crossing out of this PC
- `can_grab` (quickjump/mod.rs:157-160), which is always followed by `begin`.

The receiver side never hides the cursor.

**Paths that show it again:**
- `set_grabbed(false)` from `return_local` (controller.rs:172-191). `return_local` is reached from panic_desk (controller.rs:57-66), on_panic (controller.rs:70-73), crossing back, quickjump (quickjump/mod.rs:129), on_enter (receiver.rs:74), set_paused (pause.rs:29-32), drop_lost (loss.rs:24-44), and `Crossing::stop` (crossing/mod.rs:247-254).
- `WinCapture::stop` and `Drop` (capture.rs:600-621).
- `recover_at_startup` in `WinPlatform::with_settings` (lib.rs:181-191, cursor.rs:121-126). This runs at the next launch.

**Can it stay blank?**

- **(a) The link drops while Remote.** In normal operation it does not (PROVEN): on_link_closed (session/mod.rs:306-319) → refresh_map → `set_map` → `drop_lost` → `return_local(Lost)` → `set_grabbed(false)`.
  **It does stay blank if the drop is caused by a panic inside the crossing lock (PROVEN).** The receive path holds `ctx.crossing.lock().unwrap()` while it injects (session/mod.rs:350, :95, :404-408). Every injection failure is an `.expect` panic:
  - receiver.rs:85, 127 (SetCursorPos)
  - receiver.rs:170, 188, 193, 203, 209 (SendInput)
  - controller.rs:126, 131 (SendInput)
  - controller.rs:189 `warp(...).expect("pointer can be placed")`
  - quickjump/mod.rs:136.

  The panic poisons `crossing`. The reader task unwinds, and `LinkGuard::drop` (session/mod.rs:53-71) runs `on_link_closed` inside `catch_unwind`. That code logs `peer.disconnected` **first** (line 313–314), while it holds `core`. It then calls refresh_map, and the `crossing.lock().unwrap()` at crossing/mod.rs:314 panics on the poisoned mutex. That panic happens while `core` is held, so **`core` is now poisoned too**. Consequences:
  - `set_map`/`drop_lost` never runs, so if this PC was Remote, the cursor stays blank **and** `grabbed` stays true, and the hooks keep swallowing local input.
  - The `mouser-input` thread dies at its next event (crossing/mod.rs:347-351).
  - `reconcile_loop` dies at `core.lock().unwrap()` (engine.rs:697), and every accept or dial on this PC panics. **No reconnect, ever.**
  - The platform watchdog does not use the engine locks, so it keeps logging.

  This matches the log exactly: disconnect, then silence, with watchdog lines continuing.
- **(b) Engine teardown.** `Engine::shutdown` → `stop_input` → `Crossing::stop` → `return_local` → show (engine.rs:627-629). This is PROVEN to be fine, but only when the mutexes are not poisoned (otherwise `stop_input` panics at crossing/mod.rs:359). **Tray Quit does not call `shutdown`.** `tray::Action::Quit => app.exit(0)` (shell.rs:306), and the Engine lives in Tauri managed state (host.rs:21,69), so `WinCapture::drop` does not run when the process exits. Quitting while Remote leaves the cursor blank until the next launch runs `recover_at_startup`. That this destructor does not run is HYPOTHESIS, based on how Tauri exits; it is not verified.
- **(c) The hooks stop receiving events** (UIPI with an elevated foreground, or VMware's own LL hook placed ahead of ours that does not call `CallNextHookEx`). Nothing hides or shows the cursor in response to hook delivery, and the watchdog only probes, logs and reinstalls (watchdog.rs:186-246). **PROVEN:** if this happens while Remote, the cursor stays blank. The panic key, crossing back and pointer movement all depend on hook events reaching `Crossing::local_input` (controller.rs:33-51). An elevated foreground makes the watchdog skip its probe (watchdog.rs:225-228, 371), so it does not even detect blindness. That a medium-IL LL hook goes blind over an elevated foreground window is HYPOTHESIS: it is well-known behaviour (AutoHotkey and others document it), but E2 was never probed. The same applies to VMware's hook swallowing.
- **(d) Crash or exit.** The blank cursor persists (cursor.rs:9-11 says so). The only remedy is `recover_at_startup` at the next launch. A panic inside a tokio task does not exit the process; it wedges the process as in (a). So in practice "crash" usually means "a running zombie with poisoned locks", and recover-at-startup never runs until the user quits and relaunches. PROVEN by code structure.

## Q2. Can VMware's warping be taken for user movement and push across an edge?

**PROVEN:**
- The hooks filter only Mouser's own tags. `on_mouse` (capture.rs:169-188) ignores `PROBE_TAG` and `MOUSER_TAG`. **`LLMHF_INJECTED` is never checked**, so input that another process injects with `SendInput` counts as user motion.
- `SetCursorPos` produces no LL hook event (capture.rs:10-12, inject.rs:1-2). So VMware's recentring cannot produce a motion event by itself.
- The delta is `hook point − GetCursorPos()` (capture.rs:180-183, `mouse_raw` capture.rs:95-101). After VMware recentres, the next real move's `from` is the recentred position, so the recentre jump is **not** added to dx/dy. The new `abs` is the recentred point (the window centre). A crossing needs `abs` on our display and `abs+dx` past an edge (controller.rs:76-107). With VMware recentring to the VM window centre, the pointer never reaches the screen edge.

**Conclusion:** VMware's SetCursorPos/ClipCursor recentring will not, by itself, trigger a crossing.

**Residual risks (HYPOTHESIS):**
1. With a full-screen VM, or a ClipCursor rect that touches a screen edge, the unclamped hook point (capture.rs:13-14) lets a real push at that edge cross out while VMware holds the grab. Raw input still reaches VMware for events we swallow (FINDINGS.md:33), so the VM guest would keep moving too.
2. If VMware, or anything else, ever injects moves with `SendInput`, they would count as user motion.

## Q3. Why the link dropped 66 ms after the elevated foreground

**PROVEN:**
- The `elevated_foreground` observation has no effect on the session. `Skip::Elevated` only produces a log line (watchdog.rs:225-228).
- `Platform::foreground_elevated()` is **never called by core or app**. It is used only in `testing.rs:872-911`.
- The planned W3 mitigation (FINDINGS-windows-linux.md:45 "detect foreground integrity mismatch and raise …") is not implemented, even though uipi.rs:1-3 claims "the core is told".

So the probe line is a **correlate, not a cause**.

**Most likely cause (HYPOTHESIS H1, strong):** this PC was receiving, and Steve clicked VMware through Mouser. The click made VMware (high IL) the foreground. The next injected event (a button-up, or a key, within ~66 ms — about the length of a click) was refused by UIPI. `SendInput` returns 0, so `check_sent` returns Err (inject.rs:73-88). Then `expect("button can be injected")` (receiver.rs:209), or the key variant (receiver.rs:203), panics in `read_loop`. The chain from there is PROVEN: LinkGuard → `peer.disconnected` → `core` and `crossing` poisoned → reconcile and accept dead → no reconnect (Q1a).

A `SetCursorPos` refusal would take the same route (receiver.rs:127). The open question is whether SendInput/SetCursorPos actually *return failure* under UIPI, or "fail silently" as research W3 says. Microsoft's docs are ambiguous on this.

**Alternative (H2):** the other PC closed or panicked. The deciding evidence is the other PC's log. It would show whether it saw a disconnect first and whether it kept trying to redial; the redial is its job, because it has the lower id.

## Q4. Why Esc×2 did not work

**PROVEN:** the panic key is detected only in `Crossing::local_input` (controller.rs:46, 216). That needs (1) our keyboard LL hook to receive the key, (2) the `mouser-input` thread to be alive, and (3) the `crossing` mutex to be unpoisoned. If the wedge in Q1a happened, (2) and (3) fail, so Esc×2 can never work on that PC, whatever the hooks do. On the *other* PC, Esc×2 sends `Panic` over a link that is already gone (controller.rs:61-65).

**HYPOTHESIS:** (1) also fails. Under an elevated foreground, a medium-IL LL keyboard hook gets nothing. With VMware's VM holding the grab, VMware's own LL hook (installed after ours, so called first) may swallow keys without calling `CallNextHookEx`. Our watchdog would reinstall our hook on a miss and win the race again, but it skips probes when the foreground is elevated.

**HYPOTHESIS (VMware's own grab):** if VMware hid the cursor itself (the VM took the input after an injected click), Mouser's Esc×2 cannot release VMware's grab. Only VMware's Ctrl+Alt can. Mouser injects motion with `SetCursorPos` (inject.rs:145-151), which generates no raw input, so a VMware VM that reads raw input sees no motion from Mouser at all. Injected keys carry `LLKHF_INJECTED`, and VMware may ignore them.

**Recovery paths that do not depend on our hooks:**
- Tray **Pause** and the **enable/disable hotkey** (default `Ctrl+Alt+Shift+M`, mouser-proto config.rs:114; registered as an OS hotkey, hotkeys.rs). Both go through `set_paused` → `return_local` (pause.rs:29-49). They work only if the locks are not poisoned. While grabbed with *working* hooks, our hook swallows the hotkey's keystrokes (HYPOTHESIS about ordering: LL hooks run before hotkey processing). While blind, the hotkey probably works.
- **Tray Quit** does not restore the cursor (Q1b). A relaunch does, through `recover_at_startup`.
- There is no timer, no hook-health-based release, and no "remote silence → return local" path that bypasses poisoned locks.

## Q5. Overlap with deferred UAC/service work (T367, T415–T426)

What the deferred lane is: a SYSTEM service plus a per-session agent started with a winlogon-derived token, following the input desktop, with SendSAS (tasks.md:885-906, research R5, FINDINGS W3/W4, plan.md:465). The spec defers every Windows part of Story 12 (spec.md:240, 250).

**Work that belongs to it:**
- Injecting into and capturing over elevated windows. This needs a high-IL or `uiAccess=true` agent (a Deskflow-style token with `TokenUIAccess`, W3). It is not literally in T415–T426, but T417–T420 (the agent and its token) are where it would live, and plan.md:465 names UIPI as a reason for the service. This is the real cure for "VMware as admin".
- Covering the secure desktop and UAC prompts (T419/T420) and Ctrl-Alt-Del (T423/T424).
- Probe 2 (T367), plus the never-run probe 1 E1/E2. These would confirm the UIPI hypotheses.

**Work independent of the service, which can ship now:** everything in plan A below. None of it needs the service, and it is needed even after the service ships, because portable mode never gets the service (spec.md:334).

## Q6. Fix plan

### A. Small independent fixes (shippable now)

1. **Injection failure must never panic or poison.**
   - Change: replace the `.expect`s on inject/warp in receiver.rs, controller.rs and quickjump with error handling that logs, drops the event and keeps going. A failed release is retried or ignored.
   - Contracts: none (core only). An existing .feature edge case already covers it (spec.md:274 "no key stuck"). Unit tests can use a testkit inject that fails, which needs a VirtualPlatform knob in the **frozen mouser-testkit**, so orchestrator approval. Alternatively, put a failing inject double in core's own `testing.rs`.
2. **Poison-proof locking and recovery.**
   - Change: `lock().unwrap_or_else(PoisonError::into_inner)` on `crossing` and `core`, at least in LinkGuard / on_link_closed / reconcile / input thread / stop_input. Also have `on_link_closed` call refresh_map in a way that cannot leave `core` poisoned. Add a panic hook that writes `engine.panic` to the Mouser log.
   - Contracts: none. A new log event name might need a data-model or contract note if log events are listed there.
3. **Fail-safe cursor in the platform.**
   - Change, Windows backend only: restore the cursor (`show`) and clear `grabbed` whenever the watchdog sees a hook missing while grabbed. Optionally add a backstop timer that ungrabs when grabbed and no hook event has arrived for N s.
   - Contracts: none in the trait. Behaviour is within InputCapture's doc ("swallowed … held still and hidden").
4. **Elevated or foreign foreground policy in core.**
   - Change: core actually calls `platform.foreground_elevated()`. As controller: do not cross *out* while the local foreground is elevated (hooks are untrustworthy). As receiver: on an elevated foreground, raise the W3 Problem "Mouser can't control <app> because it runs as administrator" and hand control back (send `ReleaseAll`, or a new message). Without a new message, use the existing `Panic`/`ReleaseAll` semantics. While grabbed, poll the elevated foreground from the reconcile tick (or the watchdog) and `return_local` when it turns true.
   - Contracts: probably a new spec edge case and a .feature scenario in us2 or us3 (**orchestrator approval**). Possibly a new problem kind. The `mouser-platform` trait already has the method, so no trait change. A notice to the controller ("returned because the target is elevated") would be a new wire message in the **frozen mouser-proto**. Avoid that by reusing `ReleaseAll` plus the receiver's own Problem.
5. **Tray Quit runs `engine.shutdown()`** (or at least `stop_input`) before `app.exit`. App only.
6. **Logging.** Log `crossing.enter` / `crossing.leave` / `crossing.return{how}`, `cursor.hide` / `cursor.show` (diag::note in capture.rs), and `inject.refused`. A spec/data-model note only if the log vocabulary is specified.
7. **Optional.** Ignore `LLMHF_INJECTED` events that are not ours for edge-crossing decisions (capture.rs:169). Inject motion with `SendInput` (absolute, `MOUSEEVENTF_VIRTUALDESK`) instead of `SetCursorPos`, so raw-input apps such as VMware see motion. The second change needs conformance rule 7 to be re-checked: own events must still be ignored, and the tag already handles that.

### B. Belongs to the UAC/service feature (deferred)

- A high-IL or uiAccess agent so hooks and injection work over elevated windows (VMware as admin), plus secure desktop, UAC, lock screen and SendSAS (T367, T415–T426). It needs spec Story 12 un-deferred, plus the us12.feature scenarios (already written: us12.feature:14-22), plus `apps/mouser-service-win`. Signing is needed for uiAccess.

### Steve's next tests, and what each confirms

- **Quit Mouser, start a VM, click into it.** If the cursor still vanishes, that is VMware's normal grab (Ctrl+Alt releases it). The non-admin symptom is then mostly VMware behaviour combined with H4 (Mouser's SetCursorPos-only motion and injected keys that VMware ignores). If it does not vanish, Mouser is the cause.
- **Mouser running, cursor gone: press Ctrl+Alt on that PC's own keyboard.**
  - If it comes back, it was VMware's grab, not Mouser's blank cursor.
  - If it does not, but tray Pause or Ctrl+Alt+Shift+M restores it, it was Mouser's grab with blind hooks.
  - If nothing helps and the log shows no reconnect, it was the poisoned-engine wedge (H1). Quitting and relaunching should fix it, through `recover_at_startup`.
- **Logs from both PCs for the same minute**, plus which PC had the pointer at that moment. These separate H1 (this PC panicked as receiver) from H2 (the other side dropped).
- **Admin VMware home screen, receiver role:** a single click on VMware's window should reproduce the disconnect and wedge if H1 holds.
