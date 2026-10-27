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
