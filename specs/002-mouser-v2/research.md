# Research: Mouser v2

Phase 0 record for plan.md, **differential** to `specs/001-mouser-v1/research.md`: decisions R1–R12 and findings W1–W9, X1–X4, L1–L9 stay valid unless a V-decision below says it supersedes them. Code citations are against HEAD `bd97abe`. Web sources were read on 2026-10-26.

Evidence labels: **read**: confirmed by reading the code at the cited line. **doc**: vendor documentation. **src**: read in third-party source (Deskflow at commit 38e0df5, PowerToys Mouse Without Borders, local crate sources in `~/.cargo/registry`). **probe**: needs a hardware probe (P1–P4 below) before the dependent lane tasks run. **infer**: our inference from the above.

## Plan-Phase Handoff disposition (spec.md → plan, Principle 2.6)

| # | Handoff hypothesis | Disposition | Evidence |
|---|---|---|---|
| H-1 | Trigger case is H1: an injected input refused under UIPI → `.expect` panic while `crossing` is locked → `crossing` and then `core` poisoned → `drop_lost` never runs, input thread and `reconcile_loop` die, no reconnect | **Mechanism confirmed. Trigger partly refuted.** The panic and poison cascade is real: `.expect` on inject at `crossing/receiver.rs:85,127,170,188,193,203,209`, `controller.rs:126,131,189` and `quickjump/mod.rs:137`; `crossing` is held across inject at `session/mod.rs:95,350,404-408`; `LinkGuard::drop` (`session/mod.rs:60-71`) logs `peer.disconnected` (:313-314) and then `refresh_map` re-locks `crossing` (`crossing/mod.rs:314`) while holding `core` (:308); the first lock in `reconcile_loop` is now `config/watch.rs:50`, not `engine.rs:697`, with the same result. What is refuted is the trigger: Microsoft says UIPI-blocked `SendInput` is **not** reported through its return value or `GetLastError`. So H1 needs some *other* refusal or panic on the receiving PC. These were found and all lead to the same cascade: `supported()` `assert_eq!(e, Unsupported)` (`crossing/mod.rs:267-274`) on any `Os` error from `set_grabbed`; `WinInject::button` returning `Err` for buttons above 5 (`platform-windows/src/inject.rs:102`), which reaches receiver.rs:209; `SetCursorPos` failing; unchecked `map.wires[&peer]` (`controller.rs:116,140,180,182,250,307`); `.expect("peer followed the protocol")` (`session/mod.rs:87`); the malformed-frame `panic!` (`transport/mod.rs:352`). H2 (the other PC dropped first) cannot be decided without the other PC's log. **Design consequence:** V1–V3 remove the whole class (no panic on any OS refusal, no lock poisoning, fault boundary), whichever trigger it was. | read; doc https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput |
| H-2 | No panic hook, so panics are never logged | **Confirmed.** No `set_hook` anywhere; the only non-test `catch_unwind` is `session/mod.rs:67` | read |
| H-3 | `SetSystemCursor` is global and outlives the process; `recover_at_startup` runs only at the next launch, so an unexpected end needs something outside the process | **Confirmed** (V5) | read `platform-windows/src/cursor.rs:77-126`; doc SetSystemCursor / SPI_SETCURSORS; persistence after exit: community-reported |
| H-4 | Tray Quit calls `app.exit(0)` without `Engine::shutdown`; `WinCapture::drop` does not run | **Confirmed.** `shell.rs:306`; `RunEvent` handler handles only `ExitRequested{code:None}` (`shell.rs:348-353`); `App::run` ends with `std::process::exit`, which runs no destructors | read; doc tauri 2.12.5 `App::run`, Rust `std::process::exit` |
| H-5 | The watchdog skips its probe when the foreground is elevated; `foreground_elevated()` is never called outside tests; the W3 problem is not implemented | **Confirmed.** `watchdog.rs:225-228,371`; the callers of `foreground_elevated` are only trait impls and tests (`testing.rs:872,909`, `tests/support/picky.rs:93`, `apps/mouser-app/src/support_tests.rs:236`); `uipi.rs:1-3` comment is false | read |
| H-6 | Hooks do not check `LLMHF_INJECTED`; motion is injected with `SetCursorPos`; injected keys carry `LLKHF_INJECTED` | **Confirmed** (`capture.rs:169-204`, `inject.rs:145-151`). V8 keeps `SetCursorPos` and adds no injected-flag filter. Neither is needed by a v2 story, and 001 accepted limitation 1 already covers VMs that read raw input | read; probe 1 FINDINGS (`crates/mouser-platform-windows/probes/FINDINGS.md`) |
| H-7 | Third-party LL hooks (ASUS AsHotplugCtrl) swallow probes; a miss should be judged against real input still arriving | **Confirmed as a gap.** Watchdog counters count only probe-tagged events (`capture.rs:347-352`); real input is never treated as a sign of life | read; V7 |
| H-8 | Story 5 design = T367 + T415–T426, a SYSTEM service plus a winlogon-token agent; uiAccess is ruled out because the build is unsigned | **Confirmed, with changes** (V9–V12). Change: the agent is the whole headless engine, as in Deskflow, not just an input proxy. Change: `SendSAS` must be called by the service (doc). Change: the installer becomes per-machine | doc SendSAS, CreateProcessAsUserW, WTSQueryUserToken; src Deskflow `MSWindowsWatchdog.cpp`, `MSWindowsDesks.cpp` |
| H-9 | SignPath is future work | **Recorded**, out of scope | — |
| H-10 | Config keys exist; a desk-shared friendly name likely moves into desk state; rotation keeps one previous file | **Confirmed.** `Member.display_name` (`proto/model.rs:122-131`) is set only at join (`desk/join.rs:189-196`) and never updated, so renames do not reach the desk today. Rotation: `logging/mod.rs:142-148` keeps `mouser.log` plus `mouser.log.1`, each up to `max_log_mb`, which is up to 2× the limit and violates FR-008 | read; V14, V17 |
| H-11 | macOS headless reads a machine-folder copy; Windows headless is expected to be the service | **Confirmed and generalised** (V13) | read `apps/mouser-app/src/headless.rs:22-60` |
| H-12 | `docs/troubleshooting.md` is stale | **Confirmed**, and so are `docs/releasing.md:63`, `docs/cli.md:162` and `docs/configuration.md:11,26` → Polish | read |

## Decisions

### V1 Injection refusal never panics (Story 1 sc. 1, FR-002) — supersedes the "fail loudly" rule for inject/warp/grab results
- **Decision:** In core, every `InputInject` and `InputCapture::{warp,set_grabbed}` result is handled. An `Err` skips that one input and writes `input.refused` (see contracts/log-events.md for how repeats are grouped), and processing continues. Releases that fail are dropped; the receiver's held-set is cleared anyway, so no key stays recorded as held. `supported()` stops asserting: an `Os` error from `set_grabbed(true)` when crossing out means the crossing does not happen (pointer stays, `input.refused kind=grab`). `map.wires[&peer]` lookups become `get`; a missing peer is handled as `Lost`.
- **Rationale:** FR-002 makes refusal a specified condition, so handling it is not unspecified error handling (constitution 5.6). UIPI refusals are usually silent (doc), so this protects against the refusals Windows *does* report and against backend errors such as buttons above 5.
- **Alternatives:** keep the panics and rely on V3 recovery alone. Rejected: every refused key would tear down the link.

### V2 Locks never poison (Story 1 sc. 2, 7, FR-002)
- **Decision:** Core's `std::sync::Mutex` locks (`core`, `crossing`, `secrets`, `release_waiters`, `release_serving`, `problems.all`, logging, discovery, transfer ids/waiters/inbox) are taken through one extension, `LockExt::guard()` = `lock().unwrap_or_else(PoisonError::into_inner)`. This replaces the 115 `lock().unwrap()` sites in `crates/mouser-core/src` (count read at HEAD). It is mechanical and uniform.
- **Rationale:** a poisoned lock is what turns one fault into a permanently dead engine. State consistency after a fault is restored by V3, not by poisoning.
- **Alternatives:** `parking_lot::Mutex`. Rejected: a new dependency and an API churn for the same effect.

### V3 Fault boundary and recovery (Story 1 sc. 2, 4, 7, 8; Story 2 sc. 6)
- **Decision:**
  1. **Process panic hook** `mouser_core::faults::install(log)`, installed by app and headless at start. It writes `fault` (thread or task name, `file:line`, the panic message) to the diagnostic log if logging is on, then chains to the default hook.
  2. **Units of work run under `catch_unwind`:** each tick of `reconcile_loop`, each inbound message in `read_loop` and `motion_loop`, each event on the `mouser-input` thread, and each control-API request.
  3. **What happens on a caught panic:**
     - `Crossing::recover_after_fault()` runs. It forces the crossing state to `Local`, ungrabs and shows the pointer, and releases everything held. As controller it sends `ReleaseAll` to the computer it was driving; as receiver it releases the injected keys and buttons and drops the session.
     - Then `fault.recovered {where, action}` is written, and the loop continues.
     - A link whose reader faulted is closed with reason `fault`, and the normal redial (001 R2) reconnects it.
  4. **Inbound-frame errors:**
     - The malformed-frame `panic!` (`transport/mod.rs:352`) and `on_message .expect` (`session/mod.rs:87`) become: an unknown message variant is **skipped** and logged (`wire.unknown`). Frames are length-prefixed, so skipping one is safe. This is the "skip-unknown" v2 item in 001 foundational-notes, "Pause isolation".
     - Any other decode or protocol error closes that link with reason `protocol` and is logged.
  5. **`Ctx::persist`** (`state.json writable` expect, about 12 sites under the core lock, an open item in foundational-notes) returns a `Result`. A failure is logged as `fault` and the in-memory state is kept.
  6. **Logging writer failures** (`logging/mod.rs:139-150` expects) drop the record. There is nowhere to report them.
- **Rationale:** FR-002 and Story 2 sc. 6 ("a record describes the fault … a later record shows Mouser carrying on"). Recovery that returns control locally satisfies Story 1 sc. 4 and 8 even when the fault was inside the crossing state.
- **Alternatives:**
  - `panic = "abort"` plus an external restarter. Rejected: the desk would drop for every fault, and portable mode has no supervisor.
  - Restarting the whole engine on any fault. Rejected: it is heavier than per-unit recovery and loses links.

### V4 Quit and session end run full cleanup (Story 1 sc. 5)
- **Decision:**
  - The app calls `Host::shutdown()` → `Engine::shutdown()` (stop input → `return_local(Quit)` → ungrab, show the pointer, release held input, close links with reason `quit`) from a `RunEvent::Exit` handler. This covers tray Quit, `app.exit`, the updater restart, and logoff/shutdown: tao 0.37.1 emits `Exit` on `WM_ENDSESSION`.
  - Tray Quit calls `Host::shutdown()` *before* `app.exit(0)`, because restart paths can skip `Exit` (tauri#12310).
  - Peers already release held input when the link closes (001 Story 3 sc. 5), and the reason `quit` lets them log "the other computer quit".
- **Rationale:** `std::process::exit` runs no destructors (doc). tao handles `WM_ENDSESSION(TRUE)` by calling `loop_destroyed()` and then `exit(0)` (src).
- **Alternatives:** relying on `Drop` of managed state. Refuted (H-4).

### V5 Pointer is never left hidden after an unexpected end (Story 1 sc. 6) — Windows cursor guard
- **Decision:**
  - **Windows:** when capture starts, the Windows backend starts a **cursor guard**: the same executable re-run as `<exe> --cursor-guard <pid>`, in the same session, with no window. It waits on the parent's process handle. When the parent ends for any reason, it checks whether the arrow is the blank cursor (the test `recover_at_startup` already uses, `cursor.rs:121-126`), restores with `SPI_SETCURSORS` if so, writes `pointer.shown {reason: guard}` to the log if logging is on, and exits. A normal stop also ends the guard.
  - **macOS:** the cursor is hidden through the window server per connection, and the window server restores it when the process ends.
  - **Linux X11:** grabs and XFixes cursor hiding end when the client disconnects.
  - **Linux Wayland:** the compositor ends the InputCapture session when the client goes away.
  - The macOS and Linux claims are infer/doc-level. Story 7 checks them on real hardware (quickstart §R4).
- **Rationale:** SetSystemCursor changes are global and outlive the process. Deskflow's alternative (a ShowCursor counter plus a 1×1 topmost hider window that takes the foreground) changes focus and is a much bigger rewrite. A watcher process is small, has no IPC, and works in portable mode.
- **Alternatives:**
  - Deskflow's hider window. Rejected: focus side effects and a rewrite of capture.
  - In installed mode, letting the service restore the cursor. Rejected: portable mode has no service, and one mechanism should serve both.

### V6 Elevated or secure foreground policy (Story 1 sc. 3; Story 5 sc. 5–7)
- **Decision:**
  - The platform trait's `foreground_elevated() -> bool` is **replaced** by `foreground_app() -> Option<ForegroundApp { name, admin, outranks_mouser }>` (contracts/platform-api-delta.md). `name` is the program's executable name without its path. `admin` means the app runs elevated or as SYSTEM. `outranks_mouser` means the app's integrity is higher than Mouser's own, so UIPI blocks Mouser.
  - **As receiver**, on each reconcile tick while receiving: if `outranks_mouser`, the receiver releases what it holds, sends `Panic` to the controller only, and raises the problem `elevated_app` on itself, attributed to itself and naming the app. Under 001 semantics, a controller that receives `Panic` returns to Local and sends `ReleaseAll`, and every 0.2.x peer understands this. The fix is `restart_elevated` while portable and not elevated; installed Mouser never outranks the app. The problem reaches the controller through `PeerStatus`, so the controller shows it under the Windows computer (Story 5 sc. 6). The problem clears when control next enters this computer without an outranking foreground.
  - **As controller (grabbed)**, the backend reports `LocalInput::CaptureLost` (V7). Core returns control locally with `how = capture_lost`.
  - Installed (SYSTEM agent) and "restarted as administrator" builds never see `outranks_mouser = true` except for System-integrity UI on another desktop, which V10 covers.
- **Rationale:**
  - The trigger case, and W3's planned mitigation, which was never built.
  - Checking per tick, rather than per injected event, keeps the hot path free of token queries.
  - Reusing `Panic` needs no wire change for the hand-back.
- **Alternatives:**
  - A new hand-back wire message. Rejected: a 0.2.x controller would crash on it.
  - Refusing to cross onto a computer with an elevated app in front. Rejected: it is not in the spec, and the controller can't know.

### V7 Capture loss detection, and watchdog judged on real input (Story 1 sc. 3; Story 3 sc. 16) — amends the 001 watchdog decision of 2026-10-23
- **Decision:**
  - The Windows hook thread counts **every** event its hooks see, not only probe-tagged ones.
  - Each watchdog cycle judges a hook healthy if it saw a probe *or* any real event since the last cycle.
  - A miss is counted only when the system's last-input tick advanced (someone produced input) and the hook saw nothing.
  - So a probe eaten by another program's hook (AsHotplugCtrl) no longer counts while real input keeps arriving, and Health shows no `LOST_HOLD` (Story 3 sc. 16).
  - **While grabbed**, the backend sends `LocalInput::CaptureLost{reason}` to core, shows the pointer and ungrabs itself immediately, when any of these happens:
    - (a) the foreground outranks Mouser, checked every cycle and on `EVENT_SYSTEM_FOREGROUND` from an out-of-context WinEvent hook on the hook thread;
    - (b) another desktop takes input (`OpenInputDesktop` is not `Default`);
    - (c) one real miss.
  - The ungrab does not wait for core, so the pointer is visible even if core is wedged.
- **Rationale:** a medium-IL LL hook sees nothing over a high-IL foreground (doc, UIAccess abilities; community). Hooks are per-desktop (doc).
- **Alternatives:**
  - A timer that ungrabs after N seconds without hook events. Rejected: it would invent a timing target (Principle 2.7), and it would fire while the user simply isn't moving.

### V8 Not changed
- `SetCursorPos` motion, no `LLMHF_INJECTED` filter, and VMware's own capture are left as 001 accepted limitation 1. The edge case in spec.md says so.

### V9 Windows installed mode: service + session engine (Story 5 sc. 1–5, FR-013) — supersedes 001 R5's split "service owns network, agent injects"
- **Decision (Deskflow model):**
  - **`MouserService`** is `apps/mouser-service-win` built as `mouser-service.exe`. It runs as LocalSystem with automatic start, and is a **supervisor only**. It tracks the active console session with `SERVICE_CONTROL_SESSIONCHANGE` plus `WTSGetActiveConsoleSessionId`. While that returns `0xFFFFFFFF` the service waits in `WaitingForSession`; only the next `SERVICE_CONTROL_SESSIONCHANGE` (console connect or logon) makes it read the id again. There is no polling loop, sleep or timer.
  - In that session it starts the **session engine**: `mouser-app.exe --headless --machine`, started with `CreateProcessAsUserW` and a duplicate of that session's `winlogon.exe` token, `lpDesktop = "winsta0\\default"`. It restarts the engine when the console session changes or the engine exits.
  - The session engine is the whole engine with no UI, using the machine folder `C:\ProgramData\Mouser` and machine-scope secrets (001 `FileSecrets` and its ACL decision). Its Windows backend thread follows the input desktop: it polls `OpenInputDesktop(DF_ALLOWOTHERACCOUNTHOOK, …)`, and on a change calls `SetThreadDesktop`, reinstalls the hooks and re-attaches injection (src: Deskflow `MSWindowsDesks::checkDesk`). The check runs on the existing watchdog cycle (no new timer), so a newly shown login or UAC desktop can wait up to one cycle before it accepts input; probe P2 records the delay it observes, and the spec sets no time target (constitution 2.7).
  - It runs as SYSTEM, so its hooks and `SendInput` work over elevated apps, the UAC secure desktop and Winlogon (doc: UI Automation security overview).
  - **The user's tray app** detects an installed and running `MouserService` and starts in **UI mode**: no engine of its own. It is a control-API client of the session engine over `\\.\pipe\mouser-service`, whose DACL allows SYSTEM, Administrators and the console session's interactive user (`PIPE_REJECT_REMOTE_CLIENTS`, `FILE_FLAG_FIRST_PIPE_INSTANCE`). The CLI prefers that pipe when it exists (contracts/cli-delta.md).
- **Rationale:**
  - One engine per machine, so there is one identity and no hand-over between a login-screen engine and a user engine.
  - A small service↔engine IPC (V11) instead of proxying the whole Platform trait over a pipe.
  - The engine restarting at a user switch costs one reconnect, well inside 001 SC-008.
- **Alternatives:**
  - The service runs the engine and the agent proxies capture and inject (001 T415–T422). Rejected: it needs the whole Platform trait as IPC, plus clipboard and drag from another process.
  - Mouse Without Borders' one-process-per-desktop model. Rejected: an engine restart at every UAC prompt.
- **Risks (probe P2):**
  - Clipboard and file drop from a SYSTEM process in the user's session. Deskflow does clipboard this way; drag is unverified.
  - Files written by a drop are owned by SYSTEM. They inherit the folder's ACL (infer).
  - The probe decides. If drag fails as SYSTEM, the installed engine states the 001 limitation 3 text for drag on Windows. That needs an owner decision at that point.

### V10 Ctrl-Alt-Del (Story 5 sc. 3)
- **Decision:**
  - The session engine's `InputInject::secure_attention()` asks the service through the IPC (V11). The service calls `SendSAS(FALSE)` from `sas.dll`.
  - The installer sets `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System\SoftwareSASGeneration = 1`.
  - If the policy is managed to `0`, the platform states the limitation "Ctrl-Alt-Del is blocked by this PC's policy." (Story 5 sc. 3) under `Capability::SecureAttention`.
- **Rationale:** SendSAS must be called by a service (or a uiAccess app, which is ruled out) (doc). Deskflow does exactly this (src).
- **Not yet verified:** the meaning of policy value 2 (probe P2).

### V11 Service ↔ session-engine IPC (T421/T422 revived)
- **Decision:**
  - A named pipe `\\.\pipe\mouser-svc-<session id>` created by the service with SDDL `D:P(A;;GA;;;SY)` (SYSTEM only), `PIPE_REJECT_REMOTE_CLIENTS` and `FILE_FLAG_FIRST_PIPE_INSTANCE`.
  - The service checks the client with `GetNamedPipeClientProcessId` against the process it launched.
  - Length-prefixed postcard frames, the same framing as the wire protocol. Messages in contracts/service-ipc.md: `Hello{version}`, `SendSas`, `SasResult`, `Stopping{reason}`, `Shutdown`.
- **Rationale:** it is the minimum the session engine needs from the service. The engine is not a service, so it cannot call SendSAS itself.

### V12 Installer (FR-004, FR-013, SC-010)
- **Decision:**
  - The Tauri NSIS bundle switches to `installMode: "perMachine"`, which needs one UAC approval.
  - `installerHooks`: `NSIS_HOOK_POSTINSTALL` runs `mouser-service.exe --install`. This creates `MouserService` (LocalSystem, automatic start), sets `SoftwareSASGeneration=1`, adds the firewall rule for UDP 47801, migrates the identity (V12a) and starts the service.
  - `NSIS_HOOK_PREUNINSTALL` runs `mouser-service.exe --uninstall`. This stops and deletes the service, ends the session engine and removes the firewall rule, so nothing of Mouser keeps running (FR-013).
  - `mouser-service.exe` is shipped as a Tauri sidecar binary, the existing `binaries/` mechanism.
  - Unsigned: the release notes and `docs/releasing.md`/install instructions describe SmartScreen's "More info → Run anyway" (doc).
- **V12a Identity migration:**
  - When `--install` runs, it reads the installing user's existing identity: the device key and desk key from the user-scope store plus `state.json`. NSIS runs elevated as the same user when that user is an administrator; with over-the-shoulder elevation it is a different account (infer).
  - If the identity is readable, it is copied to the machine scope (`Engine::export_machine_scope`, 001 core US12 decision).
  - Otherwise the session engine starts without a desk, and the UI shows first run. The computer then rejoins with one click of approval (001 Story 1).
  - **Owner decision OD-2 (2026-10-26, approved):** one rejoin approval is accepted for this case.
- **Alternatives:** a WiX `ServiceInstall`. Rejected: the bundle already uses NSIS.

### V13 Headless on Windows and Linux (Story 6, FR-006) — supersedes the 001 T516 refusal
- **Decision:** `mouser-app --headless` runs the engine with no window or tray on all three OSes.
  - On **macOS**, `--headless` keeps its 001 meaning: the machine folder, used by the login-window LaunchAgent.
  - On **Windows and Linux**, `--headless` uses the current user's folder, or `MouserData/` when portable. `--headless --machine` uses the machine folder; the Windows service passes it.
  - Linux headless runs only in a signed-in session. Its secrets go to Secret Service on the session bus, falling back to the 0600 file.
  - Linux always states the limitation "Mouser can't control this computer's login screen." (FR-006).
- **Rationale:** reuses `headless.rs`. Windows installed headless *is* the session engine (V9).

### V14 Diagnostic log v2 (Story 2, FR-003, FR-008)
- **Decision:**
  - **Header record.** Every newly opened log file starts with `log.header`: Mouser version, OS name and version, computer id and name, desk id and name (or none), and `mode` (`installed` / `portable` / `portable_elevated` / `service` / `headless`, as data-model.md). It is also written after rotation.
  - **Rotation.** Two files, each at most `max_log_mb / 2` MiB (`mouser.log`, `mouser.log.1`), so their sum stays within the limit. Rotation drops the oldest file. `diagnostics.max_log_mb` is validated to be ≥ 1.
  - **Event catalog.** New records for control moves, pointer hide/show, refused input, link loss with reason, fault and recovery, display and layout changes, setting changes with their origin, and clipboard transfers. Every event has a closed field list (contracts/log-events.md plus `log-events.schema.json`). Core writes records only through typed constructors in `mouser_core::logging::events`, so free text cannot reach the log. The only names logged are application names, computer names and display names.
  - **Platform diagnostics** keep the Windows `diag` sink and gain `pointer.hidden`/`pointer.shown` from `capture.rs`.
  - **Link-loss reason.** `on_link_closed` gets a reason taken from quinn's `ConnectionError`/`close_reason()` and the app close codes (`done`, `replaced`, `shutdown`→`quit`, `not a member`→`removed`), plus new close reasons `fault`, `protocol` and `incompatible`.
- **Rationale:** SC-003 (cause, computer, time and application recoverable from logs alone) and SC-004 (zero marker matches) need both completeness and a closed vocabulary.
- **Alternatives:**
  - Free-form `fields` with a review checklist. Rejected: nothing would enforce FR-003.

### V15 Collect logs from the whole desk (Story 2 sc. 10, Story 3 sc. 12, FR-007)
- **Decision:**
  - Control method `logs.collect {dir?}` → `{path, computers:[{id,name,included,reason?}]}`. CLI: `mouser logs collect [--out DIR]`. UI: Diagnostics → "Collect logs from the desk", which then reveals the file.
  - **Wire (sent only to peers ≥ 0.3.0):** `LogsRequest{request}`. The peer answers on one uni stream with the existing release-stream routing pattern (`release/serve.rs:119-123,204-233`; `session/mod.rs:100-118`): a postcard `LogsPart` header (its computer card, its `config.toml` text, file names and lengths), then the raw bytes of each log file.
  - **Which computers are asked:** only connected peers. A peer that is not connected is listed with reason `offline`. A peer older than 0.3.0 is listed with reason `older_version`. A peer whose link drops mid-stream is listed as `offline`. With logging off, a peer sends no files and reason `logging_off`.
  - No new timeout: every wait ends either when the stream finishes or when the link is lost, and link liveness is 001 R2's existing `max_idle_timeout`.
  - **Output:** one file `mouser-logs-<desk>-<UTC yyyymmddThhmmssZ>.zip`, written with the `zip` crate 9.1.2, deflate only. It goes to the requested dir or the user's Downloads, or for a headless `--machine` engine on macOS/Linux to `<machine dir>/bundles`; the CLI prints the path.
  - **Windows installed (owner decision OD-7, 2026-10-27):** the SYSTEM session engine never writes a bundle to a folder a user chose. In mode `service`, `logs.collect` called with `dir` is rejected (`-32602`). The caller (the user's UI-mode tray app or the CLI, running as that user, not elevated) calls `logs.collect_stream {}` instead: the engine gathers the parts exactly as above, builds the zip in memory and returns it over the control pipe as a sequence of `logs.chunk` notifications followed by the result; the caller writes the zip, as the user, to the user's Downloads folder or the folder the user chose, and the UI reveals it (Story 3 sc. 12). contracts/control-api-delta.md and cli-delta.md define it. Layout and `bundle.json` manifest are in contracts/log-bundle.md.
  - Never included: `state.json`, `secrets/`, desk keys.
- **Rationale:** reuses a proven streaming pattern. A zip opens natively on all three OSes.
- **Alternatives:**
  - Pulling logs over the control API. Rejected: it only reaches the local computer.
  - A tar.gz bundle. Rejected: Windows can't open it natively.

### V16 One window (Story 3, FR-009, FR-010, FR-011)
- **Decision:**
  - The six normal windows become one `main` window, `ui/index.html`, with a sidebar, a search box and hash routes (contracts/ui-navigation.md). The windows were `main`/first_run, `settings`, `arranger`, `health`, `join`, `transfer` (`apps/mouser-app/src/windows.rs:12-65`).
  - The join prompt becomes an approval card in Desk, which brings the window to the front.
  - Transfer progress becomes a panel in Desk, which brings the window to the front when a question needs an answer.
  - Overlays stay separate windows: `landing` and `quickjump-*`.
  - Tray items call `windows::navigate(Section)`: unminimize, show and focus the existing window, then `emit_to("main", "mouser://navigate", route)`. If the window doesn't exist, it is created at `index.html#route` (doc: tauri `WebviewWindow`, `Emitter`).
  - On macOS, the activation policy is Regular while the window is shown and Accessory while it is hidden.
  - **Attribution rules (fix for the screenshot):**
    1. Startup platform limitations are no longer raised as problems (`platform_limitation:<capability>` ids). They appear only in `computers[].limitations`, under "What doesn't work here" in that computer's Health entry. Problems from 0.2.x peers with those ids are filtered out when status is merged.
    2. Problems are keyed by `(problem_id, computer)` everywhere: core merge, `Toasts` and `notify.js`.
    3. Problem banners carry the computer's name and appear only in Health and as counts on Desk.
    4. Limitation texts are worded about "this computer" but render only under their own computer.
  - **Rounding:** one `ui/format.js` (whole ms; sizes to whole or one-decimal units; display scale as a percentage) is used by every section.
  - **Review placement:** a message naming the computer plus "Keep this placement" (`layout.keep_placement {computer}`). It clears `needs_review` for that computer's displays.
  - **Arranger labels:** tiles wrap their text. A tile too small for its name gets an external label with a leader line. Full names are always in the tile's accessible name and tooltip.
  - **Help:** stage-ui copies `docs/troubleshooting.md` into `ui/help/`, rendered by a small Markdown subset renderer (headings, paragraphs, lists, code).
- **Rationale:** the causes were read from code: duplicate limitation ids per capability (`limitations.rs:9-43`), limitations raised as problems *and* listed (`status.rs:73,108-111`), banners without a computer name (`notify.js:19-44`), and every page rendering all desk problems (`desk_settings.js:77,91`, `first_run.js:117,148`). The raw RTT comes from `status.rs:96-98` → `health.js:25-28`.
- **Alternatives:** keep several windows that can navigate to each other. Rejected by Story 3 sc. 1–2.

### V17 Settings (Story 4, FR-012)
- **Decision:**
  - **Friendly names:** `desk.rename {computer, name}` sets `Member.display_name` in `DeskState` (epoch+1). No wire change, and 0.2.x peers adopt it because the shape is unchanged. Setting `general.computer_name` on a computer renames that computer desk-wide. A computer that is offline during a rename adopts the higher epoch when it reconnects (spec edge case). Status, the arranger, problems, notifications and the CLI use the member name.
  - **Landing highlight** `crossing.landing_highlight` (default true) and **Command as Control** `keyboard.command_as_control` (default true) are desk-shared. They travel in a new version-gated wire message `DeskSettings{epoch, author, landing_highlight, command_as_control}`, last writer wins by (epoch, author). A new field on `DeskState` would crash 0.2.x peers.
  - The receiver applies `map_usage` (`crossing/modifiers.rs:18-32`) only when `command_as_control` is on. Off: Command maps to the Windows/Super key and Control to Control.
  - The landing event is suppressed when the highlight is off.
  - **Per computer:** the panic key (`hotkeys.panic` `<key>*<count>`) and the chord hotkeys keep their 001 keys. The UI records them with a key recorder, and `Config::validate` now also validates `enable_disable`.
  - The app re-registers OS hotkeys on the new `config` control topic, which also fires for file and CLI edits (today it re-registers only after a UI `config.set`: `commands.rs:47-58`). A hotkey the OS refuses is reported in Settings next to that hotkey instead of panicking (`shell.rs:91`), because FR-002 covers refused OS requests.
- **Rationale:** DeskState already carries member names. Version gating follows the `Paused` precedent (`crossing/pause.rs:26,104-107`).

### V18 Tray connection state and left-click pause (Story 3 sc. 13–15, SC-008)
- **Decision:**
  - `tray_summary.connection ∈ {connected, alone, disconnected}`. It is `disconnected` when at least one member that has not been removed is `reconnecting`, `offline` or `incompatible`. It is `alone` with no other members or no desk. Pause does not change it.
  - Icons: {active, paused} × {connected: no badge, disconnected: badge} × {light, dark}, plus macOS template variants. The badge is a *hollow ring with a slash*, red in colour builds, so it can be told apart in greyscale. "Connected" adds a small filled dot, green in colour builds. "Alone" has no badge. The PNGs are generated by an asset task.
  - Click handling: `show_menu_on_left_click(false)` plus `on_tray_icon_event` → `Click{button: Left, button_state: Up}` toggles pause; right click opens the menu (doc, tauri 2.12.5).
  - Linux (libappindicator) emits no click events (doc), so Linux states the limitation "Clicking the tray icon doesn't pause Mouser here; use Pause in its menu." (spec Assumptions; 001 FR-001). This also fixes `NO_TRAY` being filed under `Capability::Capture` (`platform-linux/src/tray.rs:24`).
- **Rationale:** the API was verified in the docs for the pinned versions.

### V19 Portable "Restart Mouser as administrator" (Story 5 sc. 6–8, FR-005)
- **Decision:**
  - Fix action `restart_elevated:<computer id>`. When the computer is this one, the app runs the restart itself. When it is another computer, `elevation.restart {computer}` sends the version-gated wire message `RestartElevated` to that computer's engine, whose app runs the restart.
  - **Restart sequence:**
    1. `ShellExecuteExW(verb="runas", exe, "--wait-for <pid>")`.
    2. If UAC is declined (`ERROR_CANCELLED`), nothing changes and the problem stays.
    3. If accepted, the old instance shuts down (V4) and exits.
    4. The new instance waits for that pid to exit *before* Tauri's single-instance plugin initialises. The plugin's mutex would otherwise make it exit (src: tauri-plugin-single-instance 2.5.4 `windows.rs:78-130`).
  - The data folder is unchanged (`MouserData/`), so the computer stays portable.
  - While elevated, `foreground_app().outranks_mouser` is false for administrator apps.
  - Health's limitation text depends on mode, per Story 5 sc. 8.
- **Owner decision OD-1 (2026-10-26, approved):** the prompt is approved at the Windows PC's own keyboard. Windows shows the UAC prompt on the Windows computer's secure desktop. Portable Mouser cannot drive the secure desktop (that is installed mode, V9), so the prompt is approved at that computer's own keyboard and mouse. A medium-IL second launch cannot signal an elevated instance either (UIPI blocks WM_COPYDATA, infer; probe P3 confirms).
- **Second launch while elevated (spec edge case, 2026-10-27):** the elevated instance writes `MouserData/elevated.pid` (its pid) when it starts and deletes it on exit. A second launch that is not elevated reads that file before Tauri initialises; if the pid is alive it shows the message "Mouser is already running as administrator on this computer. Open it from the tray icon." and exits 0, instead of handing over to the single-instance plugin. When the second launch is itself elevated, the single-instance plugin focuses the running window as in 001.

### V20 Version gating for v2 wire additions
- **Decision:**
  - v2 ships as Mouser **0.3.0**, and `V2_SINCE = "0.3.0"`.
  - `LogsRequest`, `LogsPart` (stream header), `DeskSettings` and `RestartElevated` are appended last to `ControlMessage` and sent only to peers whose `Hello.mouser_version ≥ 0.3.0`, the `Paused` precedent.
  - `PROTOCOL_VERSION` stays 1, so 0.2.x and 0.3.x stay compatible.
  - From 0.3.0 on, unknown variants are skipped (V3), so later additions no longer need gating against 0.3.x peers.
  - Story 3 sc. 8 (versions shown side by side; "update <computer>" when incompatible) uses the existing `version` and `connection: incompatible`.

## Probes (constitution 3.1)

No Windows or Linux hardware is reachable from the planning Mac, and the planning session must not inject input on the Mac. **Owner decision OD-3 (2026-10-26)** sets where the probes run:

- P1–P3 run **only on the owner's Parallels Windows 11 VM** (ARM64), driven by the Windows Claude session. 001 probes 1 and 3 ran in the same environment (`crates/mouser-platform-windows/probes/FINDINGS.md`).
- Steve's PCs gate nothing.
- The probes are scheduled as Setup tasks and gate their lanes.
- Probe code goes under `specs/002-mouser-v2/spikes/spike_*`. Observations go in `specs/002-mouser-v2/spikes/FINDINGS.md`.

| Probe | Validates | Gate for |
|---|---|---|
| P1 `spike_uipi` (Parallels Windows 11 VM) | Medium-IL hooks over an elevated foreground (001 probe 1 E1/E2, never run). `SendInput`/`SetCursorPos` return values under UIPI. `foreground_app` token queries. `EVENT_SYSTEM_FOREGROUND` out-of-context delivery. A cursor guard restoring after `TerminateProcess` of the parent. A `swallow` mode (Story 3 sc. 16 stand-in) and a `capture` mode that captures the pointer itself the way a VM application does (`ClipCursor` to its window, raw input with `RIDEV_CAPTUREMOUSE`, released by Ctrl+Alt): the gating stand-in for the spec's VM-capture edge case (quickstart R10a) | platform-windows US1/US3/US5 phases |
| P2 `spike_service` (= 001 T367, extended; Parallels VM) | Starting the winlogon-token engine at cold boot, login, UAC and fast user switching. Following the input desktop by polling. `SendSAS` under `SoftwareSASGeneration` 0/1/2/3. Clipboard and Explorer drop from a SYSTEM process in the user session. NSIS `perMachine` with hooks installing and uninstalling the service unsigned | service-win lane, installer task |
| P3 `spike_elevate` (Parallels VM) | Portable `runas` relaunch with `--wait-for`, the single-instance mutex hand-over, that an elevated app's hooks and injection work over elevated apps, and whether a medium-IL second launch can reach the elevated instance's single-instance signal | app US5 phase |
| P4 `spike_tray_click` (macOS, then Windows) | `TrayIconEvent::Click` left vs right with `show_menu_on_left_click(false)`, runtime `set_icon` with badge variants. No input is injected: the owner clicks by hand | app US3 tray tasks |

**What the VM cannot show, and how each gap is covered (OD-3):**

| VM limitation | Effect | How it is validated instead |
|---|---|---|
| Parallels shared clipboard and Tools sync | They would mask or duplicate the clipboard results of P2's SYSTEM-process clipboard check | Shared clipboard must be **off** (VM Configuration → Options → Sharing) for P2 and for every Windows clipboard check. FINDINGS.md records the setting. |
| Parallels mouse integration | Pointer input reaches Windows from Parallels' input path, not a physical HID. Hooks still see it as non-Mouser input (001 probe 1). Absolute-position integration can fight `SetCursorPos` parking. | P1 runs once with mouse integration on and once with it off ("Optimize for games" / pointer capture), and FINDINGS records both. Hook-over-elevated and capture-loss results are taken from the run with integration off. Real HID behaviour is confirmed on real hardware only optionally (Steve, Story 7, not gating). |
| Ctrl-Alt-Del from the host keyboard | Parallels intercepts the host combination | P2 judges SAS only by `SendSAS` called from the probe service, never by host keys. Secure-desktop operation is driven by the probe's own injected input from the SYSTEM engine. |
| No VMware Workstation on Windows ARM64 | The trigger case cannot run with VMware in the VM | Any elevated application gives the same UIPI condition: `cmd.exe` "Run as administrator", or Task Manager when elevated. P1 and quickstart R9 use one of these. The VMware-specific variants (R9-vmware, R10) are optional extra confirmation on Steve's PCs and gate nothing. The spec's VM-capture edge case is gated on the VM by `spike_uipi capture` (R10a), which models an application capturing the pointer itself; it is a stand-in, not VMware. |
| No ASUS AsHotplugCtrl | Story 3 sc. 16 cannot be reproduced with the real culprit | P1 installs its own probe-swallowing LL hook after Mouser's hook (a `spike_uipi swallow` mode), so probes are eaten while real input keeps arriving. Story 3 sc. 16 is judged on that. |
| Cold boot and fast user switching | Supported in the VM (restart and switch user work normally) | P2 runs them in the VM. |
| ARM64 build | Probes and the release build are x86_64 on CI. The VM runs ARM64 natively (001 FINDINGS build notes: rust-lld, Clang for aws-lc). | The ARM64 build notes from 001 probe FINDINGS are reused. An x86_64 release candidate also runs under the VM's x64 emulation for P2's installer check. |

Linux tray clicks are documented as unsupported, so there is nothing to probe. Linux checks run on the owner's Lima Ubuntu VM with a desktop session (owner decision OD-6, 2026-10-27); a Linux user's own computer is optional confirmation. The macOS and Linux "pointer restored at process end" behaviour (V5) is checked in Story 7 (quickstart §R4).

## Dependencies (new or changed)

| Dependency | Version | Use | Evidence |
|---|---|---|---|
| `zip` | 9.1.2 (`default-features = false`, `deflate`) | log bundle | crates.io (cargo search, 2026-10-26) |
| `windows` | 0.62.2 (already pinned for the platform crate): `Win32_System_Services`, `Win32_System_RemoteDesktop`, `Win32_Security`, `Win32_System_Pipes`, `Win32_System_Threading` | service-win | 001 W4 |
| `windows-service` | 0.8.1 | SCM dispatcher in service-win | crates.io |
| tauri / tray-icon | 2.12.5 / 0.25.1 (unchanged) | tray click events | docs.rs |

## Sources

- SendInput: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput
- SetWindowsHookExW (hooks are per-desktop): https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowshookexw
- LowLevelMouseProc / LowLevelKeyboardProc: https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc
- UIAccess abilities: https://technet.microsoft.com/en-us/library/jj852244.aspx
- UI Automation security overview: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview
- SendSAS: https://learn.microsoft.com/en-us/windows/win32/api/sas/nf-sas-sendsas
- WTSGetActiveConsoleSessionId: https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-wtsgetactiveconsolesessionid
- WTSQueryUserToken: https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsqueryusertoken
- HandlerEx / session change: https://learn.microsoft.com/en-us/windows/win32/api/winsvc/nc-winsvc-lphandler_function_ex
- CreateProcessAsUserW: https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessasuserw
- CreateNamedPipeW: https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew
- GetNamedPipeClientProcessId: https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid
- SetSystemCursor: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setsystemcursor
- SystemParametersInfoW: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-systemparametersinfow
- SmartScreen reputation: https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation
- Tauri config (NSIS installerHooks, installMode): https://v2.tauri.app/reference/config/ and https://v2.tauri.app/distribute/windows-installer/
- Tauri tray: https://docs.rs/tauri/2.12.5/tauri/tray/struct.TrayIconBuilder.html and https://docs.rs/tauri/2.12.5/tauri/tray/enum.TrayIconEvent.html
- Tauri RunEvent / App::run: https://docs.rs/tauri/2.12.5/tauri/enum.RunEvent.html ; restart skipping Exit: https://github.com/tauri-apps/tauri/issues/12310
- Deskflow: https://github.com/deskflow/deskflow/blob/master/src/lib/platform/MSWindowsWatchdog.cpp and MSWindowsDesks.cpp
- Mouse Without Borders: https://github.com/microsoft/PowerToys/tree/main/src/modules/MouseWithoutBorders
- SoftwareSASGeneration values: https://knowledge.broadcom.com/external/article/21517/ctrlaltdel-does-not-work-on-remote-contr.html
