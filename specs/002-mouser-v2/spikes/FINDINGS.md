# Mouser v2 probes: observed results

Everything below was observed. Where something could not be exercised it says so and why. Each item is marked CONFIRMED, REFUTED or NOT RUN.

## P1 spike_uipi (2026-10-27)

Probe: `spikes/spike_uipi/` (standalone crate, `windows` 0.62.2), built natively (aarch64, debug). Windows 11 `10.0.26200.9711` ARM64 in Parallels, Parallels Tools running (`prl_cc`, `prl_tools`, `prl_tools_service`). The probe runs as the logged-in local administrator with its filtered token: `TokenIsElevated=0`, integrity `0x2000` (medium). Logs: `%TEMP%\spike_uipi_<label>.log`.

The owner runs were made in both Parallels mouse modes, set in VM config → Hardware → Mouse & Keyboard: **"Optimize for games"** (relative/captured pointer) and **"Don't optimize for games"** (SmartMouse, absolute/seamless pointer). Per OD-3 the hook-over-elevated and capture conclusions needed an owner at the keyboard; both modes were exercised and the UIPI and capture conclusions hold in **both**, so the Parallels mouse mode does not change them.

| # | Item | Status |
|---|---|---|
| 1 | LL hooks see input while an elevated window is in front | CONFIRMED (both mouse modes) |
| 2 | `SendInput` / `SetCursorPos` into an elevated foreground window under UIPI | CONFIRMED — both blocked |
| 3 | Foreground → pid → `QueryFullProcessImageNameW`, elevation, integrity | CONFIRMED, incl. elevated windows |
| 4 | Out-of-context `EVENT_SYSTEM_FOREGROUND` WinEvent hook fires for elevated windows | CONFIRMED |
| 5 | Cursor-guard child restores blank cursors after `TerminateProcess` of its parent | CONFIRMED |
| 6 | `swallow`: real input still arrives past a probe-swallowing LL hook | CONFIRMED |
| 7 | `capture`: raw-input capture and `ClipCursor` behaviour in a Parallels guest | CONFIRMED with caveats |

### Baseline for items 1–3 (medium-integrity foreground)

- Hooks saw real input with a medium-integrity window in front: 235 real mouse and 4 real key events in 8 s.
- Same-integrity injection, 4 rounds: `SetCursorPos` returned ok with `GetLastError` 0 and the cursor moved each time; `SendInput` returned 1 for each of the 4 events per round, error 0, and the hook saw every tagged event.
- Foreground readout, e.g. `pid=2388 exe=claude.exe (C:\Program Files\WindowsApps\Claude_...\claude.exe) TokenIsElevated=0 IL=0x2000 (medium) outranks_own=false`.
- Side finding: from medium integrity, `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` failed with `Access is denied. (0x80070005)` for 10 of 12 service and system processes sampled (winlogon, csrss, services, lsass, several svchost), so their exe, elevation and integrity cannot be read. explorer.exe and one svchost read as medium, not elevated. A process that cannot be opened must be treated as outranking Mouser (as 001's `uipi.rs` does). Elevated **user** apps (an admin `cmd.exe`, Task Manager) opened fine — see items 3/4.

### Item 5: cursor guard

All 13 system cursors were blanked in a victim process. A child `spike_uipi --cursor-guard <victim pid>` waited on the victim's process handle. `TerminateProcess` of the victim was followed by `guard: parent exited, calling SPI_SETCURSORS`; result `hidden-before-kill=true restored-by-guard=true within 50.5ms`, `arrow blank now=false`.

Not exercised: a launcher job object with kill-on-close would kill the guard together with its parent; the guard may need `CREATE_BREAKAWAY_FROM_JOB`.

### Item 6: swallow

Hook B is installed after the probe's hook A and swallows only tagged probe events.

- Control (B passing through): A saw 8/8 tagged mouse, 8/8 tagged key, 8/8 foreign-tagged mouse, and 85 real mouse events.
- B swallowing: B ate 24 of 24 probes; A saw 0 tagged mouse, 0 tagged key, 12/12 foreign-tagged mouse and 169 real mouse events.

So probes can vanish while real input keeps arriving (Story 3 scenario 16 shape).

### Owner runs: items 1–4, injection over an elevated foreground window

Two `observe 130` runs with an owner driving the desktop: `abs-observe` (SmartMouse/absolute) and `on-observe2` (games/relative). In each the owner opened an **elevated** admin `cmd.exe` and elevated **Task Manager** and gave each window the foreground.

- **Item 4 — WinEvent fires for elevated windows: CONFIRMED.** The out-of-context `EVENT_SYSTEM_FOREGROUND` hook fired for the admin `cmd.exe` and for Task Manager, e.g. `WINEVENT EVENT_SYSTEM_FOREGROUND ... class='ConsoleWindowClass' title='Command Prompt' exe=cmd.exe | TokenIsElevated=1 | IL=0x3000 (high) | own IL=0x2000 (medium) | outranks_own=true` and `class='TaskManagerWindow' ... Taskmgr.exe | TokenIsElevated=1 | IL=0x3000 (high)`.
- **Item 3 — elevation/integrity readout for elevated windows: CONFIRMED.** Both elevated user windows resolved to pid, exe path, `TokenIsElevated=1`, `IL=0x3000 (high)`, `outranks_own=true`. (System/service processes still deny `OpenProcess` — the baseline side finding — but elevated user apps read fine.)
- **Item 1 — LL hooks keep seeing real input while an elevated window is in front: CONFIRMED (both modes).** Throughout the elevated-foreground stretches the LL hook reported real mouse input (e.g. `mouse real=20..48` per second in `abs-observe`), i.e. the user's pointer motion is still delivered to the hook.
- **Item 2 — injection into an elevated foreground window is blocked under UIPI: CONFIRMED.** Every `[FG OUTRANKS probe]` fired while a high-IL window held the foreground was identical:
  - `SetCursorPos(x+1): ok=false GetLastError=0 ... moved=false` — the call fails and the cursor does not move.
  - `SendInput mouse=(1,err 0) key=(1,err 0)` — the API **reports success** (count 1, error 0), but `hook saw tagged mouse +0 key +0` — the events are **silently swallowed** and never reach the elevated queue. (Same-IL / lower-IL probes in the same run saw `+2/+2`.)
  - Reproduced ~16× across the admin `cmd.exe` and Task Manager in `abs-observe`, plus once in `on-observe2`; zero exceptions. END totals e.g. `abs-observe`: mouse real=1324, tagged=88; key real=101, tagged=88; 11 foreground events.

  Takeaway for the product: `SendInput`'s return value is not a delivery signal under UIPI — it returns success while the input is dropped. Detection of the elevated-foreground case must come from the foreground readout (items 3/4), not from `SendInput`'s result.

### Owner runs: item 7, capture mode (both mouse modes)

`capture 40` with `ClipCursor` to an off-screen rect `(-32000,-32000)` plus `RegisterRawInputDevices(RIDEV_CAPTUREMOUSE | RIDEV_NOLEGACY)`.

- **Raw-input capture works in both modes.** `abs-capture` received `raw WM_INPUT mouse=1106` over a clean 40 s (auto-release on timeout); `games-capture` received `raw WM_INPUT mouse=1214`, `key real=38` and was released early by Ctrl+Alt at 21 s. (An earlier games run, `on-capture`, logged 0 mouse events and stopped at 13 s — a Parallels **guest stall** when the probe window lost focus, not a capability gap; superseded by `games-capture`.)
- **Input is swallowed away from the foreground app — CONFIRMED by the owner.** While capture was active the cursor *shape* stopped changing and typing did **not** appear in Notepad, i.e. `RIDEV_NOLEGACY` suppressed legacy mouse/keyboard to other windows while the probe received the raw stream.
- **CAVEAT — `ClipCursor` is defeated by Parallels in both modes.** The requested off-screen clip never took; every sample logged the clip rect as the full screen `(0,0)-(1474,979)` and the cursor roamed freely (`abs-capture`) or was only frozen by the stall (`on-capture`). Parallels Tools owns absolute cursor positioning and resets the clip each frame. **The capture feature cannot rely on `ClipCursor` to pin or hide the pointer inside a Parallels guest** — it must hide/park the cursor another way.
- **CAVEAT — release hotkey collision.** `Ctrl+Alt` (the probe's release chord) is also Parallels' own mouse-release chord in games mode; the two fire together. The product's capture-release hotkey must avoid `Ctrl+Alt`.

## P3 spike_elevate (2026-10-28)

Probe: `spikes/spike_elevate/` (standalone crate, `windows` 0.62.2), built natively (aarch64, debug). Same VM as P1. The probe replicates `tauri-plugin-single-instance` 2.5.4 (mutex `com.mouser.spike-elevate-sim`, a hidden message window, `FindWindowW` + `AllowSetForegroundWindow` + `WM_COPYDATA` with `dwData` 1543) plus an elevation relaunch via `ShellExecuteExW(verb=runas)` and a `--wait-for <pid>` handover that opens the parent with `PROCESS_SYNCHRONIZE` and waits before touching the mutex. Logs: `%TEMP%\spike_elevate\<timestamp>-pid<pid>-<mode>.log`.

**UAC note (affects which paths are reachable on this VM).** The VM is configured `EnableLUA=1`, `ConsentPromptBehaviorAdmin=0` (elevate without prompting). So `runas` auto-elevates **silently** — no consent dialog. This lets the accept path (Scenario A) run unattended but means the user-decline path (`ERROR_CANCELLED` 1223) cannot be exercised here without an owner raising `ConsentPromptBehaviorAdmin` first (a security-setting change); see Scenario E.

| # | Scenario | Status |
|---|----------|--------|
| A | `runas` relaunch + `--wait-for` handover (medium → elevated, old instance holds the mutex until the child waits it out) | CONFIRMED |
| B | Inject text/mouse into an **elevated** foreground window from the medium probe | CONFIRMED — silently dropped |
| C | Single-instance forward, medium first ← medium second (`WM_COPYDATA`) | CONFIRMED — delivered |
| D | Single-instance forward, **elevated** first ← **medium** second | mutex seen; `WM_COPYDATA` blocked by UIPI |
| E | `runas` **declined** → `ERROR_CANCELLED` (1223) | NOT RUN (UAC auto-elevates on this VM) |

### Scenario A: elevation relaunch with `--wait-for` handover — CONFIRMED

A medium first instance (`pid 19132`, `integrity=medium 0x2000`) took the mutex fresh, then after 3 s relaunched itself elevated:

- `RELAUNCH: ShellExecuteExW(verb=runas, ... params=run --wait-for 19132 --exit-after 12)` → `ShellExecuteExW OK (UAC accepted); new process pid 2672` (accepted silently per the UAC note).
- The old instance deliberately **kept the mutex for 3 s** after the relaunch, then released it just before exit.
- The elevated child (`pid 2672`, `integrity=high 0x3000`) logged `--wait-for 19132: opening process (SYNCHRONIZE) before touching the single-instance mutex`, then `WaitForSingleObject -> 0 (parent exited) after 2864 ms`, and only *then* `CreateMutexW(...) -> GetLastError 0 (fresh)` and `RUNNING as the first instance ... integrity=high (0x3000)`.

So the `PROCESS_SYNCHRONIZE` + `WaitForSingleObject` handover works: the elevated successor does not race the medium predecessor for the single-instance mutex — it blocks until the old process exits and then acquires the mutex cleanly. This is the pattern a real elevate-in-place relaunch must use.

### Scenario B: injection into an elevated foreground window — CONFIRMED (silently dropped)

The medium probe (`integrity=medium 0x2000`) called `SendInput` to type `SPIKE-ELEVATE-OK` and nudge the mouse while different windows held the foreground. The probe's own `WH_KEYBOARD_LL`/`WH_MOUSE_LL` hook is the delivery oracle: an injected event that reaches the input queue is seen by the hook (`injected=true`); one dropped before the queue is not seen at all.

- **Same-integrity control (Notepad, `integrity=medium 0x2000`):** `SendInput(text, 32 events) returned 32, GetLastError 0`, and the hook then logged **16** `HOOK key down vk=0xE7 ... injected=true` lines plus the injected mouse move — every event reached the queue.
- **Elevated target (`Taskmgr.exe`, `elevated=true integrity=high 0x3000`):** `SendInput(text 'SPIKE-ELEVATE-OK', 32 events) returned 32, GetLastError 0` and `SendInput(mouse move +1,-1) returned 2` — **identical success return** — but the hook logged **zero** `injected=true` key events afterward (the only nearby hook line was a real `msg=0x204 injected=false` button-up). The 32 events were accepted by the API and then **silently discarded by UIPI**; they never reached the input queue, so not even the injecting process saw them.

This reproduces P1 item 2 through the plugin-shaped `inject()` path: **`SendInput`'s return value is not a delivery signal** when the foreground window outranks Mouser. Mouser must detect the elevated-foreground case from the foreground readout (P1 items 3/4), never from `SendInput`'s result.

### Scenario C: single-instance forward, medium → medium — CONFIRMED (delivered)

First instance `pid 2168` (`integrity=medium 0x2000`) took the mutex and ran a message loop. A second launch `pid 9636` (also medium):

- `CreateMutexW(...) -> GetLastError 183 (ERROR_ALREADY_EXISTS)` — detects the running instance.
- `FindWindowW -> 0x405de`, owner `pid 2168 ... integrity=medium`.
- `AllowSetForegroundWindow -> ok=true`.
- `SendMessageTimeoutW(WM_COPYDATA) DELIVERED, result 1`.

The first instance logged `*** WM_COPYDATA RECEIVED from second launch: cwd|args = ...`. So the normal (non-elevated) single-instance forward works end to end: the second invocation hands its cwd+args to the first and exits.

### Scenario D: single-instance forward, elevated first ← medium second — mutex seen, `WM_COPYDATA` blocked by UIPI

First instance launched elevated (`pid 14224`, `elevated=true integrity=high 0x3000`), took the mutex fresh. A **medium** second launch (`pid 2168`, `integrity=medium 0x2000`):

- `CreateMutexW(...) -> GetLastError 183 (ERROR_ALREADY_EXISTS)` — **the medium instance does detect the elevated singleton.** (A named mutex's default DACL grants `SYNCHRONIZE`/`QUERY` broadly, so there is no `ACCESS_DENIED` on the mutex. The earlier worry that a medium second instance would fail to see an elevated first instance and start a **duplicate** is **REFUTED**.)
- `FindWindowW -> 0x8900ae`, owner `pid 14224 ... elevated=true integrity=high (0x3000)`.
- `AllowSetForegroundWindow -> ok=true`.
- `SendMessageTimeoutW(WM_COPYDATA) FAILED, result 0, GetLastError 5 (ERROR_ACCESS_DENIED)`.

So the real cross-integrity hazard is **not** a duplicate instance; it is that **argument/URL hand-off from a medium relaunch to an already-running elevated instance is silently blocked by UIPI** (a medium process cannot post `WM_COPYDATA` to a high-IL window). `tauri-plugin-single-instance` would log "failed to forward arguments" and the payload is lost — the second instance still suppresses itself (mutex said already-exists), but whatever the user passed (a file, a URL, a command) never reaches the running instance. For an elevated instance to accept the forward it would have to opt in with `ChangeWindowMessageFilterEx(hwnd, WM_COPYDATA, MSGFLT_ALLOW)`, which the plugin does not do. Mouser should either run both instances at the same integrity or add that filter (and a payload-size/validation guard) if it ever runs elevated.

### Scenario E: `runas` declined (`ERROR_CANCELLED` 1223) — NOT RUN

The probe's `relaunch`/`--relaunch-after` path logs `ERROR_CANCELLED` (1223) when the user declines the UAC prompt. On this VM `ConsentPromptBehaviorAdmin=0` makes `runas` elevate without ever prompting, so there is no decline to capture. Exercising it needs the owner to temporarily set `ConsentPromptBehaviorAdmin` to a prompting value (a security-setting change the owner must make, not the probe) and then decline a relaunch. Deferred until an owner can do that; the handling is a single `GetLastError == ERROR_CANCELLED` branch and is low risk.
