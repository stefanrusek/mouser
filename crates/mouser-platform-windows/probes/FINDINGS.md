# Windows hardware probes 1–3 (T366–T368): observed results

Everything below was observed. Where something could not be exercised it says so and why.

## Environment

One machine: Windows 11 Pro 10.0.26200 (build 26200), ARM64, a Parallels ARM virtual machine on an Apple Silicon Mac. It is not the owner's x86_64 Windows hardware. Input reaches the VM through Parallels (mouse integration on); there is no physical HID device attached to Windows.

The probes are standalone crates using `windows` 0.62.2 (pinned), built natively for `aarch64-pc-windows-msvc` with `CARGO_TARGET_DIR=%USERPROFILE%\target-probes`. They were run from the Claude Code shell at medium integrity in session 1 (`WinSta0\Default`), inside a job object without UI restrictions.

### Build prerequisites on Windows ARM64 (beyond the README)

- `aws-lc-sys` (the rustls crypto backend) cannot assemble its ARM64 `.S` files with MSVC. It needs Clang and CMake: the Visual Studio Build Tools components `VC.Llvm.Clang`, `VC.Llvm.ClangToolset` and `VC.CMake.Project`, with `VC\Tools\Llvm\ARM64\bin` and the bundled CMake `bin` on `PATH`. The ARM64 compiler (`VC.Tools.ARM64`) is needed too.
- MSVC `link.exe` on aarch64 produces coverage profiles that `llvm-profdata merge` rejects ("malformed instrumentation profile data: symbol name is empty"), for every crate. Linking with `rust-lld` fixes it (`mouser-proto` then reports 100% lines). Set in `%USERPROFILE%\.cargo\config.toml`:

  ```
  [target.aarch64-pc-windows-msvc]
  linker = "rust-lld"
  ```
- An empty crate (the `mouser-platform-windows` stub) fails the gate's coverage step with "no coverage data found"; that is expected until the crate has code.

## Probe 1: LL hooks, Raw Input, elevation, ClipCursor (T366), `spike_hooks/`

Injected input: 20 relative moves of ±1 px and 5 presses of F15 (scan 0x66, so nothing types into a window), every event tagged with `dwExtraInfo = 0x4D4F5553` ("MOUS"). Raw Input was registered with `RIDEV_INPUTSINK` for mouse and keyboard on a hidden window on the hook thread.

| Observation | Result |
|---|---|
| LL hooks see input injected by another process | yes: 20/20 mouse, 10/10 key events, all with `LLMHF_INJECTED` / `LLKHF_INJECTED` |
| LL hooks see input injected by another thread of the hook-owning process | yes, 30/30, while the hook thread pumps messages |
| `dwExtraInfo` tag reaches the hook | yes, on all 30 events, so Mouser can recognise its own injected events |
| Raw Input for injected events | 20 mouse + 10 key events, all with `hDevice == NULL` |
| Raw Input for events the LL hooks swallow (hook returns 1) | still delivered: 20 mouse + 10 key `WM_INPUT`, `hDevice == NULL`. Raw Input observes swallowed input; it cannot be used to suppress it |
| Cursor while swallowing injected relative moves | unchanged |
| Hook thread stops pumping while input is injected | `SendInput` in the injector **blocks** until the hook thread pumps again or the hook call times out. Events whose hook call times out are passed on to the system **without the hook seeing them**. Delivered to the hook after a stall of 100/250/500/1000/2000/5000 ms: 30/30/29/27/24/15 of 30, same for a child-process and a same-process injector |
| Hook callback itself slow (sleep per mouse event) | 100 ms: 20/20 called; 250 ms: 20/20; 400 ms: 16/20; 600 ms: 11/20; 1000 ms: 6/20; 1500 ms: 4/20. The skipped events go to the system unseen. `LowLevelHooksTimeout` is not set in `HKCU\Control Panel\Desktop` |
| Hook silently removed after timeouts | **not observed** on this build: after every stall and slow-callback run the next batch reached the hook 30/30 and `UnhookWindowsHookEx` succeeded |

Consequences for capture (T385/T386): the hook thread must never block, because a stalled hook lets grabbed input leak to local apps rather than removing the hook. The watchdog should detect "hook stopped delivering" (the documented removal), but on this build stalls show up as skipped events, not removal.

Not yet run (needs the owner at the keyboard; attempts recorded no real input and the cursor did not move):

- real (non-injected) mouse and keyboard input through the hooks and Raw Input, pass-through and swallowed (`spike_hooks hooks`, phases P3/P4);
- LL hooks and `SendInput` with an elevated foreground window (`spike_hooks elevated`, phases E1/E2, needs a UAC approval);
- `ClipCursor` across the UAC secure-desktop switch (same run).

## Probe 2: winlogon-token agent and SendSAS (T367), `spike_service/`

Not run. Writing this probe was stopped in the session that ran probes 1 and 3, so nothing was installed and no service code exists. The owner decides how this probe and the US12-service-win lane are handled. Cold boot, UAC, lock screen, fast user switching and `SendSAS` under `SoftwareSASGeneration` 1 vs 3 therefore remain unverified (desk research only, FINDINGS-windows-linux W4).

## Probe 3: drag (T368), `spike_drag/`

The process is Per-Monitor-V2 DPI aware; all pointer input is `SendInput` absolute moves and left button down/up, tagged "MOUS".

### Part A: `DoDragDrop` started by an injected button-down (`spike_drag own`)

- The injected button-down reached the source window as `WM_LBUTTONDOWN` (`GetMessageExtraInfo` = 0x4D4F5553); `DoDragDrop` called from that handler started a drag.
- The first `IDropSource::QueryContinueDrag` saw `MK_LBUTTON` set; after the injected button-up it saw it clear and the drag dropped (35–36 calls over ~1.8 s of injected moves).
- Our `IDropTarget` window got `DragEnter` (with `FileGroupDescriptorW` offered), 12 `DragOver` and `Drop`; `DoDragDrop` returned `DRAGDROP_S_DROP` (0x00040100) with `DROPEFFECT_COPY`.
- While the drag passed over other windows, another application read **both `FileContents` streams in full** (8 MiB) before any drop. A virtual-file data object must expect contents to be read during hover, not only on drop.
- `windows`-rs `#[implement]` objects are agile: OLE and the drag-image helper called the data object from threads other than the STA thread that called `DoDragDrop`. The data object must be thread-safe (or opt out of agility).

### Part B: virtual-file drop into Explorer (`spike_drag explorer`)

Data object: `FileGroupDescriptorW` (HGLOBAL; `FD_FILESIZE | FD_PROGRESSUI`) and `FileContents` per `lindex` (IStream from `SHCreateMemStream`), files `hello.txt` (34 bytes) and `big.bin` (8 MiB pattern). Each run used a fresh folder under `%TEMP%\spike_drag`, opened with `explorer.exe <folder>` (window class `CabinetWClass`, title `<folder> - File Explorer`; launched from the probe it ran as its own `explorer.exe` process), and dropped at 70%/60% of the window.

| Observation | async offered (`IDataObjectAsyncCapability`, `GetAsyncMode` = TRUE) | sync (`GetAsyncMode` = FALSE) |
|---|---|---|
| Explorer queries `IDataObjectAsyncCapability` | yes, `GetAsyncMode` right after the drop | yes, `GetAsyncMode` |
| `StartOperation` | called | not called |
| `DoDragDrop` returns | ~35 ms after the button-up, **before** any `FileContents` read | after both `FileContents` reads |
| `FileContents` reads | after `DoDragDrop` returned, on two Explorer threads | before `DoDragDrop` returned |
| `EndOperation` | `hr = S_OK, effects = 1` (copy), ~140 ms after the drop | not called |
| `SetData` from Explorer | `TargetCLSID` = `F3364BA0-65B9-11CE-A9BA-00AA004AE837` (CLSID_ShellFSFolder), `Performed DropEffect` = 1, `Logical Performed DropEffect` = 1, `DropDescription` | same |
| Other formats requested | `Shell IDList Array`, `CF_HDROP` (15), `FileNameW`, `FileName`, `Preferred DropEffect`, `UniformResourceLocator`, `Net Resource`, `Shell Object Offsets`, `ZoneIdentifier`, `EnterpriseDataProtectionId`, `chromium/x-file-system-files`, drag-image helper formats | same |
| Files on disk | both present, contents byte-exact | both present, contents byte-exact |
| No overwrite, progress or error dialog | none appeared (fresh folders) | none |

Drop location: Explorer does not tell the source where it dropped. After `DoDragDrop` returned, `WindowFromPoint(drop point)` → `GetAncestor(GA_ROOT)` gave the `CabinetWClass` window. In `IShellWindows`, the entry whose `IWebBrowserApp::HWND` equals that window then yielded the folder (`IServiceProvider::QueryService(SID_STopLevelBrowser, IShellBrowser)` → `QueryActiveShellView` → `IFolderView::GetFolder(IPersistFolder2)` → `GetCurFolder` → `SHGetPathFromIDListW`), which was exactly the drop folder in both runs. `IShellBrowser::GetWindow` returns an inner window, not the `CabinetWClass` window, so it cannot be used for the match.

Not exercised: dropping on the desktop, into a folder that already holds the names (overwrite prompt), cancelling Explorer's copy, and drops into non-Explorer targets.

### Added with US2 capture (`spike_hooks setcursor`, `spike_hooks edge`)

Same VM and conditions as above.

| Observation | Result |
|---|---|
| `SetCursorPos` reaches the LL mouse hook | **no**: 11 calls, 0 hook events. Parking the cursor does not echo into capture |
| Hook point `pt` at the right screen edge, pass-through, +10 relative moves | not clamped: `pt.x` 2572, 2580, 2581 with the screen's last pixel 2559 and the cursor clamped to 2557, 2559, 2559 |
| Same, hook swallowing | cursor stays put (2557); `pt.x` 2578 every time, i.e. cursor plus the accelerated delta. So a swallowed move's delta is `pt` minus the parked point, also at an edge |
| A zero-length tagged relative move reaches the LL mouse hook | **no**; a one-pixel one does. The capture watchdog's probe is a tagged one-pixel move, which the hook swallows so the cursor never moves |

### Network profile after a Parallels suspend/resume (J1, 2026-10-22)

Same VM. Before the J1 run the network was "Network", category Private, and the Private firewall profile is disabled on this VM, so mouser-app's UDP 47801 socket drew no prompt. J1 suspends and resumes the VM with `prlctl`; on resume Windows identified the adapter as a new network, "Network 2", category **Public**, where the firewall is on.

| Observation | Result |
|---|---|
| Network after resume | "Network 2", Public (was "Network", Private) |
| mouser-app started 17:47:14 | Windows' own prompt opened 17:47:19: `PickerHost.exe FirewallNotificationDialogServer`, window title "Windows Security" |
| While the prompt is open | a full-screen `Shell_SystemDim` window (PickerHost) covers the display; `DoDragDrop` cannot start ("the drag could not be started") and the edge-strip drag ends at once |

So the J1 sleep step reaches a firewall-on profile on every run, and a real Windows user who joins a new network sees the prompt again. Firewall detection has to re-check on network change, not only at start.

### A stalled guest looks like lost hooks (`probe_logger`, 2026-10-22)

Same VM (Windows 11 ARM64 under Parallels). The watchdog sometimes raised "Mouser lost its hold on the keyboard and mouse..." although the hooks were fine. `probe_logger` repeats the watchdog's probe (a tagged one-pixel move and an F24 release through `SendInput`, then waits for both hooks) every 5 s for 6 minutes (71 lines, 17:39:21 to 17:45:27) and logs what happened.

| Observation | Result |
|---|---|
| Loop interval | meant to be 5 s; stretched to 8-12 s at times, with single gaps of 14 s and 20 s |
| `GetTickCount` inside one probe iteration (capped at 2 s) | jumped by 4.8-7.8 s several times (once 11.7 s): the whole guest was not running |
| Missed probes | every one was seen by the hook later, and the next probe went through: late, not lost |
| `SendInput` | never failed (0 errors in the log) |
| `GetLastInputInfo` | advanced on every probe, the missed ones too, so it cannot tell late from lost |
| `LowLevelHooksTimeout` | not set (neither `HKCU\Control Panel\Desktop` nor `HKLM`), so the default applies and the system did not drop the hooks |
| Cursor position, foreground window | varied across the misses and the hits; neither correlates with a miss |

So a probe that misses its 500 ms window is not evidence that the hooks were removed: the guest itself stalled for seconds, and the hook callback ran when it woke up. Nothing in the process can observe the difference by time or by input state (`GetLastInputInfo` counts the injected probe whether or not a hook saw it).

The rule that does work counts probes, not time. A missed probe is remembered together with what the hooks had seen when it was sent. The hooks are judged lost only if, when the next cycle begins, the counters have not moved past that point, that is the earlier probe is still unseen after a whole further interval. A guest stall delays both the probe and the cycle, so the late probe has arrived by the time the next cycle looks. A failed `SendInput` is a separate outcome: it says nothing about the hooks, so it neither reinstalls nor raises. When a probe is seen after an alarm, the alarm is withdrawn. The cost is that a hook the system really removed is reinstalled about one interval (5 s) later than before.

### Windows Firewall as seen from `INetFwPolicy2` (2026-10-22)

Same VM. What the firewall detection (`src/firewall.rs`) reads and what it saw here:

| Observation | Result |
|---|---|
| Private profile | firewall **disabled** on this VM |
| Public profile | firewall **enabled** |
| Current network after the Parallels resume | Public ("Network 2"), so `CurrentProfileTypes` is Public and the firewall is on for it |
| Prompt from Windows when mouser-app first listened | opened (see the J1 section); until it is answered there is no Allow rule for mouser-app on Public and the app is unreachable |
| Access | `CoCreateInstance(NetFwPolicy2)` and enumerating `Rules` work unelevated; reading is enough, no rule is written |

Matching: a rule counts if it is enabled, inbound and names mouser-app's path (compared case-insensitively after expanding `%VARIABLES%`, ignoring quotes and the kind of slash). For each profile that is both current and firewalled, Mouser is let in only with an Allow rule covering that profile and no Block rule covering it (Block wins in Windows; cancelling the prompt leaves a Block rule). Rules that name no program (ports, services) are ignored in both directions, and the profile's default inbound action is not consulted; a PC configured to allow inbound by default would still be told Mouser is blocked. The check runs at start, on every network change (`netwatch`: interface and address changes, resume), and every 60 s. The periodic check is there because a switch of the profile of a network that stays connected (Public to Private in Settings) is not an interface or address change, so no notification is guaranteed for it; this was not verified here because changing the network category is a system setting.
