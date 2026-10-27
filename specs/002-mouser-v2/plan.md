# Implementation Plan: Mouser v2

**Branch**: `002-mouser-v2` | **Date**: 2026-10-26 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/002-mouser-v2/spec.md`. The plan is differential to `specs/001-mouser-v1/plan.md`.

## Summary

Mouser v2 (release 0.3.0) covers seven stories on top of v0.2.8:

| Story | What it delivers |
|---|---|
| 1 | Reliability: no fault or refused input can leave a pointer hidden or a desk wedged |
| 2 | Useful, private diagnostic logs and a one-step desk log bundle |
| 3 | One Mouser window with a sidebar, attributed notices and a tray icon that shows connection state |
| 4 | Hotkeys, friendly names, landing highlight and the Command/Control setting, all in Settings |
| 5 | Windows elevation: installed SYSTEM service + session engine, and "Restart as administrator" for portable mode |
| 6 | Headless mode on Windows and Linux |
| 7 | Real-hardware verification |

**Technical approach** (research.md V1–V20):

- **Core**
  - stops panicking on refused OS requests (V1);
  - stops poisoning its locks (V2);
  - runs every unit of work behind a fault boundary that returns control locally (V3).
- **App**
  - runs `Engine::shutdown` on every exit path (V4);
  - becomes one hash-routed window (V16).
- **Windows backend**
  - gets a cursor guard process (V5);
  - judges hook health on real input and reports capture loss (V7).
- **Elevated foreground**
  - When the foreground app outranks Mouser, the receiving computer hands control back with `Panic` and raises an attributed `elevated_app` problem (V6).
- **Installed Windows**
  - runs a Deskflow-style LocalSystem supervisor that starts the whole engine headless in the console session with a winlogon token, and calls `SendSAS` for it (V9–V12).
- **Logs**
  - get a header and a closed, privacy-checked event catalog (V14), plus a version-gated stream that gathers the whole desk into one zip (V15).
- **Desk-wide settings**
  - Names ride on the existing `DeskState`.
  - The new desk-shared toggles ride on a version-gated `DeskSettings` message (V17, V20).

**E2E (Principle 4.3, for the operator):** the inventory is J1 only. J1 is a **modified** journey for 002, decided by the owner (OD-4): its scenario text is unchanged, but the Windows computer it installs on now gets the machine-wide installer and `MouserService`. J1 is therefore re-run in Convergence (T-IDs in tasks.md "E2E"). No new journey is proposed.

## Technical Context

**Language/Version**: Rust 1.99 (edition 2024), unchanged. The UI is static HTML + ES modules served by Tauri, unchanged.

**Primary Dependencies**: as 001, plus:
- `zip` 9.1.2 (log bundle);
- `windows-service` 0.8.1 (service-win);
- `windows` 0.62.2 with service, RemoteDesktop (WTS), Security, Pipes and Threading features (service-win);
- tauri 2.12.5 and tray-icon 0.25.1, unchanged (click events verified in their docs).

**Storage**: files only, as 001, with these changes:
- the log rotation policy changes (V14);
- bundles are written as zip files (V15);
- the Windows installed engine uses `C:\ProgramData\Mouser` with machine-scope secrets (001 `FileSecrets` + ACL decision).

**Testing**: as 001 (cargo test, cucumber 0.23.0 `acc_`, `contract_` with jsonschema, cargo-llvm-cov 0.9.1 at 100% lines, node `--test` + happy-dom for the UI), plus:
- new VirtualPlatform knobs (contracts/platform-api-delta.md);
- a `WinApi` seam in service-win;
- the closed log-event schema used as a contract oracle;
- one coverage exclusion, approved by the owner (OD-5): `apps/mouser-service-win/src/winapi_sys.rs`, the thin SYSTEM-only pass-through to the real Windows APIs behind the `WinApi` seam. It is recorded as debt and verified by probe P2 on the VM and the manual rows R8/R9.

**Target Platform**: as 001. Windows installed mode needs an administrator once, at install.

**Project Type**: desktop tray app + CLI + Windows service, in a Cargo workspace (unchanged).

**Performance Goals**: no new targets (spec Assumptions; constitution 2.7). Reconnection after a lost link keeps 001 SC-008 (≤ 5 s), using the 001 R2 budget. Control returns and the pointer is restored with no time target beyond "without action by the desk user". No new timers are introduced: V7 runs on the existing 5 s watchdog cycle, and V15 waits on the existing link liveness.

**Constraints**:
- FR-003 privacy, enforced by a closed schema.
- No panic on any refused OS request (FR-002).
- Wire additions are version-gated to peers ≥ 0.3.0, because 0.2.x peers panic on unknown variants.
- Unsigned Windows build, so no uiAccess (FR-004).
- Never run real-input suites on the owner's Mac from an agent.

**Scale/Scope**: desks of 2–4 computers, unchanged.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-checked after Phase 1 design (see "Post-Design Constitution Re-check" below).*

| Principle | Status | How |
|---|---|---|
| I Reliability First | PASS | V1–V5, V7. Each fix ships with a regression scenario: `acc_v2_us1` reproduces the trigger mechanism with `refuse_input` / `panic_on_next_inject`. |
| II Discoverable, plain-language UI | PASS | One window, search, settings next to what they affect (display options stay in Displays), one-click fixes (`restart_elevated`, `keep_placement`, `update_computer`). |
| III Trusted LAN | PASS | The bundle is written only on the requesting computer. The log stream uses the existing encrypted desk link. No new internet access. |
| IV Agent-friendly ops | PASS | `mouser logs collect` and `mouser desk rename`. Every new setting has a config key. Logs stay JSONL with a header. |
| V Cross-platform parity | PASS with stated limitations | Every v2 behaviour exists on all three OSes or is stated per computer: Linux tray click, Linux login screen, Windows unsigned warning. |
| VI Test the seams | PASS | Simulated-network and VirtualPlatform acceptance tests. Version-gate contract tests. Real-hardware Story 7. |
| VII Simplicity | PASS (see Complexity Tracking) | The engine moves into the session (Deskflow model) instead of proxying the Platform trait over IPC. Existing patterns are reused: `Panic`, release streams, `DeskState`. |
| Dev workflow: Polish updates docs/ | PASS | Polish phase: troubleshooting, configuration, cli, releasing, technical-decisions, product-requirements. |
| 1.3 No open markers before tasks | PASS | No `[NEEDS CLARIFICATION]` markers. OD-1 to OD-3 and contract approval were decided by the owner on 2026-10-26, OD-4 to OD-7 on 2026-10-27 (see "Owner Decisions"). |
| 1.4 Deferred-work mechanism | PASS | Same as 001: the "Deferred to a future feature" section of tasks.md, plus "Open" items in this feature's notes file. No GitHub issues are in use. |
| 2.6 Handoff consumed | PASS | research.md disposition table H-1…H-12. |
| 3.1 External deps probed | PASS (OD-3: probes on the Parallels VM, risk accepted; OD-6: Linux checks on the Lima Ubuntu VM) | Windows APIs, desk research and source reading done. Probes P1–P4 are scheduled as Setup tasks that gate their lanes. |
| 3.2 Gap analysis | PASS | See "Gap Analysis" below. |
| 3.3 Contracts before implementation | PASS | `contracts/` is fixed. The frozen-crate changes are listed below for approval. |
| 3.4 Test infra present | GAP → setup tasks | The gate's evidence directory, `schemas_in_sync` and `lane_disjointness.py` are hard-wired to 001. VirtualPlatform needs the new knobs. A Windows runner exists (001 CI). |
| 3.5 Sequence diagrams | PASS | One diagram per story, below. |
| 3.8 Debt survey | PASS | See below. |
| 4.1 Test doubles are operator decisions | PASS (OD-5) | VirtualPlatform/SimNet are 001 D2 (sanctioned). The extended knobs are the same double. The new seams — service-win `WinApi`, app `Shell` (elevated relaunch), app service-query (UI mode), app file-manager reveal, platform-windows input-desktop seam — are sanctioned by the owner (OD-5). Real-config debt per seam: `WinApi` → probe P2 + R8/R9; `Shell` → probe P3 + R8 5g; service-query → R8 5a; file-manager → quickstart §3/§4 by hand; desktop seam → probe P2 item 2 + R8 5b/5c. |
| 4.1 Contracts both sides | PASS | Every seam has a `contract_` test on each side; the app's consumer side of status v2 is `ui/contract_status_v2.test.js`, and ui-navigation is `src/contract_navigation_tests.rs` / `ui/contract_navigation.test.js` (see Test Strategy). |
| 4.2 Coverage | PASS (OD-5) | 100% lines everywhere except the one owner-approved module `apps/mouser-service-win/src/winapi_sys.rs` (SYSTEM-only pass-through), added to the gate's exclusion list in Setup and recorded as debt. |
| 4.3 E2E admission | PASS (OD-4) | Inventory: J1 only. **No new journey is proposed.** J1 is a modified journey (OD-4: the Windows install becomes machine-wide with `MouserService`), so it re-runs in Convergence. Story 7 is manual real-hardware evidence. |
| 5.2 Workspace bootstrap | PASS | constitution v1.4.0, unchanged. |

## Project Structure

### Documentation (this feature)

```text
specs/002-mouser-v2/
├── spec.md, checklists/, reference/
├── plan.md              # this file
├── research.md          # Phase 0 (V1–V20, probes P1–P4, handoff disposition)
├── data-model.md        # Phase 1 (deltas)
├── quickstart.md        # Phase 1 (incl. Story 7 checklist R1–R12)
├── contracts/           # Phase 1 — only changed/new contracts
│   ├── wire-protocol-delta.md      control-api-delta.md     config-delta.md
│   ├── platform-api-delta.md       cli-delta.md             service-ipc.md
│   ├── log-events.md + log-events.schema.json
│   ├── log-bundle.md + log-bundle.schema.json
│   ├── status.schema.json          # full v2 schema (supersedes 001 copy)
│   └── ui-navigation.md
├── spikes/              # P1–P4 (Setup; excluded from coverage)
├── evidence/            # <lane>.jsonl via cargo xtask gate; quickstart.md (Story 7)
└── tasks.md             # /speckit-tasks
```

### Source Code (delta against 001 layout)

```text
crates/
├── mouser-proto/        # CHANGED (frozen; Foundational): wire.rs (+4 msgs, UnknownVariant), control.rs
│                        #   (methods, topic, Problem.app, fixes, status v2), config.rs (2 keys, validate),
│                        #   service_ipc.rs (NEW), schemas/{status v2, log-events, log-bundle}
├── mouser-platform/     # CHANGED (frozen): foreground_app, LocalInput::CaptureLost, Capability +3, rules 10–11
├── mouser-testkit/      # CHANGED (frozen): VirtualPlatform knobs
├── mouser-core/         # faults.rs (NEW), lock ext, logging/{events.rs NEW, header, rotation}, logs/ (NEW,
│                        #   collect + serve), crossing (V1/V6), session (close reasons, skip-unknown),
│                        #   desk (rename, DeskSettings), control (new methods/topic), problems (attribution)
│   └── tests/features/v2_us{1..6}.feature (NEW, Foundational) + acc_v2_us{1..6}.rs + steps/v2_us*.rs
├── mouser-platform-windows/  # watchdog (V7), cursor guard (V5), foreground_app, desktop following (V9),
│                             #   service_client.rs (NEW), limitations
├── mouser-platform-macos/    # foreground_app
└── mouser-platform-linux/    # foreground_app (X11), limitations (login screen, tray), tray capability fix
apps/
├── mouser-app/          # windows.rs → one window + navigate; tray (click, connection icons); host shutdown on
│                        #   Exit; headless on Win/Linux + --machine; --cursor-guard, --wait-for; UI mode
│                        #   (service present); elevation.rs (NEW); installer hooks (windows/hooks.nsh NEW)
│   └── ui/              # index.html, nav.js, search_index.js, format.js, sections/{desk,displays,health,
│                        #   settings,diagnostics,help,welcome}.js; old per-window pages removed
├── mouser-cli/          # logs collect, desk rename, origin=cli, service pipe
└── mouser-service-win/  # service.rs, session.rs, launch.rs, ipc.rs, sas.rs, install.rs, winapi.rs (seam)
xtask/                   # gate evidence dir from .specify/feature.json; disjointness script parameterised
```

**Structure Decision**: The structure is 001's workspace unchanged. No package is added. `mouser-service-win` (a stub until now) is filled in.

## Frozen Contract Changes (need orchestrator/owner approval before Foundational lands)

| Crate / file | Change | Why (spec) |
|---|---|---|
| `mouser-proto/src/wire.rs` | Append `LogsRequest`, `DeskSettings`, `RestartElevated`. Add the `LogsPart` stream header type, `FrameError::UnknownVariant`, `PeerStatus.mode` (sent only to ≥ 0.3.0) and `V2_SINCE`. | S2 sc.10, S4 sc.4–6, S5 sc.6–7, FR-002 |
| `mouser-proto/src/control.rs` | Methods `logs.collect`, `desk.rename`, `layout.keep_placement`, `elevation.restart`. `config.set.origin`. Topic `config`. `Problem.app` and `problem_id` in status. `ProblemKind::ElevatedApp`. Fix actions `restart_elevated`, `keep_placement` and `update_computer`. Status v2 (`tray_summary`, `mode`, `os_version`). | S2, S3, S4, S5 |
| `mouser-proto/src/config.rs` | `crossing.landing_highlight` and `keyboard.command_as_control` (both desk-shared). Validation of `enable_disable` and `max_log_mb`. | S4, FR-008, FR-012 |
| `mouser-proto/src/service_ipc.rs` (new) | Service ↔ engine IPC types | S5 |
| `mouser-proto/schemas/` | status v2, `log-events.schema.json`, `log-bundle.schema.json`. `schemas_in_sync` points at `specs/002-mouser-v2/contracts/` for these three and still at 001 for log-record and release-manifest. | S2, S3 |
| `mouser-platform/src/lib.rs`, `conformance.rs` | `foreground_app()` replaces `foreground_elevated()`. `LocalInput::CaptureLost`. `Capability::{Install, LoginScreen, Tray}`. Rules 10–11. | S1 sc.3, S2 sc.2/4, S5, S6 sc.2 |
| `mouser-testkit/src/virtual_platform.rs` | `refuse_input`, `panic_on_next_inject`, `set_foreground_app`, `lose_capture`, `pointer_hidden` | S1 tests |
| `mouser-proto/src/control.rs` (OD-7) | `logs.collect_stream` and the `logs.chunk {seq, data}` notification; `logs.collect` rejects every call in mode `service` | FR-007, S3 sc.12 |
| `mouser-platform-windows` cross-lane signatures (approved 2026-10-27) | `cursor_guard::run`, `ServiceClient::{connect, on_shutdown, stopping}`, `console_session_id`, `WinPlatform::with_service_client` (contracts/platform-api-delta.md "Frozen cross-lane Rust signatures"). Implemented by the platform-windows lane, called by the app lane | E1 |
| `.feature` files (new) | `crates/mouser-core/tests/features/v2_us1..v2_us6.feature`, `apps/mouser-service-win/tests/features/v2_us5_service.feature`, and `crates/mouser-platform-windows/tests/features/{v2_us1_guard,v2_us3_watchdog}.feature` (Story 1 sc. 6 and Story 3 sc. 16, which only the Windows backend can observe), each written verbatim from spec.md. The 001 `.feature` files are not edited. v2 amendments to 001 scenarios are expressed as new v2 scenarios. | 4.1 |
| Workspace dependencies | Every new dependency (`zip` in core, `windows-service` and `windows` features in service-win, cucumber in app and service-win) is added in one Foundational task, so `Cargo.lock` is written once and no lane writes it | 5.2 |

## Package Boundary Declaration (Principle 3.3)

| Story | Packages touched | Kind |
|---|---|---|
| US1 Reliability | proto (UnknownVariant), platform, testkit, core, platform-windows (V5, V7), platform-macos and platform-linux (`foreground_app`), app (V4, panic hook install) | multi |
| US2 Logging | proto, core (events, header, rotation, collect), platform-windows (`pointer.*`, `capture.lost`), cli (`logs collect`) | multi |
| US3 One window | proto (status v2, methods), core (attribution, tray connection, keep_placement), app (window, UI, tray), platform-windows (V7 real-input judgement), platform-linux (tray limitation, capability fix) | multi |
| US4 Settings | proto (config, DeskSettings, `desk.rename`), core (rename, DeskSettings, mapping, landing), app (Settings UI, hotkey re-register), cli (`desk rename`) | multi |
| US5 Windows elevation | proto (service_ipc, `RestartElevated`, `elevated_app`), core (V6 hand-back, `elevation.restart`), platform-windows (desktop following, service client, limitations), service-win, app (UI mode, V19, installer), cli (service pipe) | multi |
| US6 Headless | app (headless on Windows/Linux, `--machine`), platform-linux (login-screen limitation) | multi |
| US7 Real hardware | none (evidence only) | — |

All shared contracts are fixed in `contracts/`. Per 001 practice they are implemented in Foundational in `mouser-proto`, `mouser-platform` and `mouser-testkit`, together with the v2 `.feature` files.

## Sequence Diagrams (Principle 3.5)

### US1 — As a desk user, I want my pointer and keyboard to keep working whatever goes wrong inside Mouser
```mermaid
sequenceDiagram
  actor User
  participant CapA as Platform A capture (controller)
  participant CoreA as Core A
  participant CoreB as Core B (receiver, Windows)
  participant InjB as Platform B inject
  participant Guard as Cursor guard (A, Windows)
  CoreA->>CapA: set_grabbed(true) → pointer.hidden
  CapA-->>Guard: started (--cursor-guard pid)
  CoreA->>CoreB: Enter, Key, Button…
  CoreB->>InjB: key(...)
  InjB-->>CoreB: Err(Os) (refused)
  CoreB->>CoreB: skip, log input.refused (no panic, no poison)
  alt foreground outranks Mouser (receiver tick)
    CoreB->>CoreB: release held; raise elevated_app (self)
    CoreB->>CoreA: Panic
    CoreA->>CapA: set_grabbed(false) → pointer.shown; ReleaseAll → B
  else fault inside a unit of work
    CoreB->>CoreB: catch_unwind → fault; recover_after_fault → Local; fault.recovered
  else A's hooks go blind (elevated/secure desktop/silent)
    CapA->>CapA: ungrab + show pointer itself
    CapA->>CoreA: LocalInput::CaptureLost(reason)
    CoreA->>CoreA: return_local(capture_lost); ReleaseAll → B
  else link lost / peer faulted
    CoreA->>CoreA: on_link_closed(reason) → return_local(link_lost) (locks never poisoned)
    CoreA->>CoreB: redial (001 R2) ≤ 5 s after reachable
  else Quit / logoff / update restart
    CoreA->>CoreA: Engine::shutdown → return_local(quit); close links "quit"
  else A ends unexpectedly
    Guard->>Guard: parent gone → SPI_SETCURSORS if blank → pointer.shown(guard)
  end
```

### US2 — As a troubleshooting agent, I want the diagnostic log to show what Mouser did and why
```mermaid
sequenceDiagram
  actor Agent
  participant CLI as mouser CLI
  participant CoreA as Core A (requester)
  participant CoreB as Core B (≥0.3.0, connected)
  participant CoreC as Core C (0.2.x)
  Note over CoreA,CoreB: every log file starts with log.header; events per log-events.schema.json
  Agent->>CLI: mouser logs collect
  CLI->>CoreA: logs.collect {}
  CoreA->>CoreA: own part (card, config.toml, logs)
  CoreA->>CoreB: LogsRequest{request}
  CoreB->>CoreA: uni stream: id, LogsPart{card, config, logging, files}, bytes…
  CoreA->>CoreA: C: older_version (not asked); offline members: offline
  CoreA->>CoreA: write mouser-logs-<desk>-<ts>.zip + bundle.json; log logs.collected
  CoreA-->>CLI: {path, computers[]}
  Note over CLI,CoreA: Windows installed (OD-7): CLI/app → logs.collect_stream; engine (SYSTEM) sends logs.chunk…, result; the caller writes the zip as the user
```

### US3 — As a desk owner, I want one Mouser window where I can find everything
```mermaid
sequenceDiagram
  actor Owner
  participant Tray as Tray (app)
  participant Win as Main window (index.html)
  participant Core as Core (control API)
  participant Peer as Peer core
  Peer->>Core: PeerStatus (problems w/o start-up limitations, mode)
  Core->>Core: merge by (problem_id, computer); tray_summary.connection
  Core-->>Tray: status topic → icon {state × connection}, tooltip
  Owner->>Tray: left click
  Tray->>Core: active.pause / active.resume
  Owner->>Tray: right click → Arrange displays
  Tray->>Win: show/focus + emit mouser://navigate {#displays}
  Win->>Core: layout.get, status
  Owner->>Win: Keep this placement
  Win->>Core: layout.keep_placement {computer}
  Owner->>Win: search "logs" → #diagnostics
  Win->>Core: status → Health entries: problems once, limitations under own computer, rounded ms
```

### US4 — As a desk owner, I want to tailor hotkeys, names and pointer behavior from the Mouser window
```mermaid
sequenceDiagram
  actor Owner
  participant UI as Settings section
  participant App as App (hotkeys)
  participant CoreA as Core A
  participant CoreB as Core B (receiver)
  Owner->>UI: rename Beta → "Studio PC"
  UI->>CoreA: desk.rename {Beta, name}
  CoreA->>CoreB: DeskState(epoch+1, members[].display_name)
  CoreB->>CoreB: adopt; write general.computer_name; setting.changed origin=desk_mate
  Owner->>UI: record panic key F12×3 / QuickJump chord
  UI->>CoreA: config.set {origin: app}
  CoreA-->>App: config topic {key, old, new, origin}
  App->>App: re-register OS hotkeys (refusal shown in Settings, no panic)
  Owner->>UI: landing highlight off; Command as Control off
  UI->>CoreA: config.set (desk-shared)
  CoreA->>CoreB: DeskSettings(epoch+1) [only ≥0.3.0]
  CoreB->>CoreB: no Landing event; map_usage disabled (Cmd→Win, Ctrl→Ctrl)
```

### US5 — As a desk user, I want to control a Windows computer at its login screen, elevation prompts and administrator applications
```mermaid
sequenceDiagram
  participant Ctl as Core (controller, Mac)
  participant Svc as MouserService (LocalSystem)
  participant Eng as Session engine (mouser-app --headless --machine, winlogon token)
  participant Desk as Input desktop (Default / Winlogon)
  participant UI as Tray app (UI mode)
  Svc->>Svc: wait console session; create \\.\pipe\mouser-svc-<s>
  Svc->>Eng: CreateProcessAsUserW(winlogon token, winsta0\default)
  Eng->>Svc: Hello
  Eng->>Ctl: QUIC Hello/DeskAuth (machine-scope identity)
  Eng->>Desk: poll OpenInputDesktop → SetThreadDesktop, reinstall hooks
  Ctl->>Eng: Enter / Key / Button
  Eng->>Desk: SendInput (SYSTEM: works on login, UAC, elevated apps)
  Ctl->>Eng: SecureAttention (tray "Send Ctrl-Alt-Del")
  Eng->>Svc: SendSas
  Svc->>Svc: SendSAS(FALSE)
  Svc-->>Eng: SasResult
  UI->>Eng: control API over \\.\pipe\mouser-service
  Note over Svc,Eng: SESSIONCHANGE → Shutdown → relaunch in new console session
  Note over Ctl,UI: Portable: receiver sees outranking app → Panic + elevated_app{fix restart_elevated}
  Ctl->>Eng: (portable) elevation.restart → RestartElevated
  Eng->>Eng: ShellExecuteEx runas --wait-for pid; UAC approved at the PC; old exits; new starts elevated
```

### US6 — As a desk owner, I want Mouser to run without its window or tray on Windows and Linux
```mermaid
sequenceDiagram
  actor Owner
  participant App as mouser-app --headless
  participant Core as Core
  participant Peer as Desk-mate
  Owner->>App: start --headless (Windows/Linux; user or portable folder; --machine for service)
  App->>App: no window, no tray; install panic hook
  App->>Core: Engine::new(params) → join desk (existing membership)
  Core->>Peer: Hello / DeskAuth
  Peer-->>Peer: lists computer as connected; crossing works
  Note over Core: Linux: limitation "can't control this computer's login screen" in status
  Owner->>App: SIGTERM / Ctrl-C / service Shutdown → Engine::shutdown
```

### US7 — As a Mouser developer, I want every v1 behavior still unverified on real hardware checked on real hardware
```mermaid
sequenceDiagram
  actor Tester as Owner (Macs, Parallels VM, Lima VM; optional Steve / Linux user)
  participant HW as Real computers
  participant Bundle as mouser logs collect
  participant Ev as specs/002-mouser-v2/evidence/quickstart.md
  Tester->>HW: run quickstart §8 row (R1–R12)
  HW-->>Tester: observed outcome
  Tester->>Bundle: collect desk logs for the run
  Tester->>Ev: outcome, hardware/OS, version, date, who, bundle name
  alt failure
    Tester->>Ev: defect against the broken story (tasks.md follow-up)
  end
```

## Test Strategy (Principles 4.1–4.3)

**Acceptance (`acc_`)**

- *Core features.* `crates/mouser-core/tests/features/v2_us{1..6}.feature` cover every acceptance scenario of spec Stories 1–6 that core can observe. They are authored verbatim in Foundational and run by `acc_v2_usN.rs` against the multi-node harness: VirtualPlatform with the new knobs, plus SimNet.
- *UI scenarios.* These are Story 3 sc. 1–3, 5–7, 9–15, 17 and Story 4 sc. 1–6 as seen in the window. They live in `apps/mouser-app/ui/acc_v2_us3.test.js` and `acc_v2_us4.test.js`, with Given/When/Then test names taken verbatim. Node has no Gherkin runner, per constitution 4.1.
- *Service and headless.* Story 5 sc. 1–3 at the service seam are in `apps/mouser-service-win/tests/features/v2_us5_service.feature` with `acc_v2_us5_service.rs`, run on the Windows runner through the `WinApi` seam. Story 6 is in `apps/mouser-app/tests/acc_v2_us6.rs`.

**Contract (`contract_`), each side**

| Seam | Producer side | Consumer side |
|---|---|---|
| wire | proto | core send-gating, skip-unknown |
| control / status v2 | core | cli, app |
| log events | core (`contract_log_events`: every record emitted during the acc suites validates against `log-events.schema.json`) | platform-windows diag names |
| log bundle | core | cli |
| platform rules 10–11 | each backend on its OS runner | VirtualPlatform |
| service IPC | service-win | platform-windows |
| ui-navigation | `src/contract_navigation_tests.rs` | `ui/contract_navigation.test.js` |
| control / status v2 (app consumer) | core | `ui/contract_status_v2.test.js` (every `status_fixture.js` fixture validates against `schemas/status.schema.json`) |

**Privacy (SC-004).** `contract_log_privacy` (core) runs a desk session in which a marker is typed and copied, collects a bundle, and asserts zero matches across logs and the zip.

**Implementation tests** reach 100% lines per package, as in 001.

**Regression (constitution VI).** The trigger mechanism (refused inject while the crossing is held, then link close, then reconnect) is reproduced in `acc_v2_us1`.

**Evidence.** `cargo xtask gate` → `specs/002-mouser-v2/evidence/<lane>.jsonl`, which is resolved from `.specify/feature.json`.

**Acceptance for Windows-only observations.** Story 1 sc. 6 (unexpected end: the cursor guard) and Story 3 sc. 16 (probes swallowed by another hook) can only be observed in the Windows backend. They get Foundational `.feature` files under `crates/mouser-platform-windows/tests/features/` and `acc_` runners in the platform-windows lane (the guard scenario spawns a real guard child and terminates its parent on the Windows VM/runner; the watchdog scenario runs over the `HookApi` seam).

**E2E.** The inventory is J1 only, and nothing is added. J1 is modified for 002 (OD-4) and re-runs in Convergence. Absorption was considered: J1 could absorb "quit while remote → pointer visible" (Story 1 sc. 5), but nothing that J1 runs can check cursor visibility from outside on macOS. That assertion therefore sits lower: platform rule 10 conformance on each OS runner, plus Story 7 R4. No journey is proposed.

## Infrastructure Detection (Principle 3.4)

| Need | Present? | Action (Setup, before all feature tasks) |
|---|---|---|
| Gate evidence directory per feature | **no**: `xtask/src/gate.rs:56-57` hardcodes `specs/001-mouser-v1/evidence` | Read `feature_directory` from `.specify/feature.json` |
| CI evidence artifact | `.github/workflows/ci.yml` uploads `specs/001-mouser-v1/evidence/` | Upload the evidence of the active feature |
| Lane disjointness for 002 | **no**: `xtask/scripts/lane_disjointness.py` hardcodes `## Phase 54`, and ignores root files such as `Cargo.lock` | Parameterise the end marker and the frozen prefixes (add `apps/mouser-service-win/tests/features/` and `crates/mouser-platform-windows/tests/features/`), and count root files (`Cargo.toml`, `Cargo.lock`) |
| Coverage exclusion for the SYSTEM-only pass-through | **no** (`xtask/src/gate.rs:9` `EXCLUDE`) | Add `winapi_sys\.rs` to `EXCLUDE` (OD-5; one file) |
| `schemas_in_sync` | hardwired to the 001 contracts path (`crates/mouser-proto/tests/schemas_in_sync.rs:4-17`) | Foundational (frozen crate): per-schema source paths |
| VirtualPlatform failure / blind / foreground knobs | **no** (`virtual_platform.rs:493-566`) | Foundational (testkit) |
| Windows CI runner (service-win, platform-windows) | yes (001 CI, windows-2025) | — |
| Windows hardware for P1–P3 | Parallels Windows 11 VM (ARM64, driven by the Windows Claude session; Parallels shared clipboard off) | Probe tasks per OD-3 |
| `zip`, `windows-service` crates | not yet dependencies | Added in their lanes' first tasks |
| Tray icon assets with connection badges | **no** (6 PNGs, `apps/mouser-app/icons/`) | App lane asset task |

## Gap Analysis (Principle 3.2)

### Package pass

Findings come from reading the code at HEAD bd97abe. Every one is **confirmed-by-reading** at the cited lines.

| # | Package | Finding | Impact |
|---|---|---|---|
| G1 | core | `.expect` on inject, warp and grab results while `crossing` is held. See research H-1 for the full list. | CRITICAL |
| G2 | core | All 115 `lock().unwrap()` calls. The poison cascade runs `session/mod.rs:308,319` → `crossing/mod.rs:314`. | CRITICAL |
| G3 | core | No panic hook. Tasks die silently in `engine.rs:276-281`. | HIGH |
| G4 | core | Malformed or unknown frame causes `panic!` at `transport/mod.rs:352`. Protocol `.expect` at `session/mod.rs:87`. | CRITICAL (mixed-version desks) |
| G5 | core | `on_link_closed` has no close reason. `transport/mod.rs:354` discards `ConnectionError`. | HIGH (S2 sc.5) |
| G6 | core | `foreground_elevated` is never called (H-5). | HIGH |
| G7 | core | Rotation keeps 2× `max_log_mb`. There is no header. Writer expects at `logging/mod.rs:139-150`. | MEDIUM |
| G8 | core | `Member.display_name` is never updated after join. | HIGH (S4 sc.3) |
| G9 | core | Limitations are raised as problems with per-capability ids and also listed per computer (`limitations.rs:9-43`, `status.rs:73,108-111`). | HIGH (S3 sc.5–6) |
| G10 | core | `rtt_ms` is sent unrounded. The UI rounds it (decision V16). | LOW |
| G11 | core | `needs_review` cannot be cleared without moving a display (`control/layout.rs:85-90`). | MEDIUM (S3 sc.11) |
| G12 | core | `Ctx::persist` expects under the core lock (001 open item). | MEDIUM |
| G13 | platform-windows | Watchdog counts only probes, never real input (`capture.rs:347-352`). It never touches the grab (`watchdog.rs:186-251`). | HIGH |
| G14 | platform-windows | `SetSystemCursor` hide persists after the process dies (`cursor.rs`). | HIGH |
| G15 | platform-windows | `uipi.rs:1-3` comment is false. `window_elevated` `.expect` at `uipi.rs:54-62`. | LOW |
| G16 | platform-windows | `#[allow(dead_code)]` on `LiveInput` at `lib.rs:384`. | LOW (debt) |
| G17 | app | Quit calls `app.exit(0)` with no shutdown (`shell.rs:306`, `:348-353`). | HIGH |
| G18 | app | Six windows. Tray theme listener tied to `main` (`tray.rs:238-241`, `shell.rs:319-325`). | HIGH (S3) |
| G19 | app | Hotkeys are re-registered only after a UI `config.set` (`commands.rs:47-58`). A registration failure panics (`shell.rs:91`). The quickjump and targeted_paste OS registrations emit an event nobody listens to (`shell.rs:262-266`). | MEDIUM |
| G20 | app | `app.rs:72,137` panics on a lagged broadcast. `tray::action` panics on an unknown id (`tray.rs:266`). | MEDIUM (FR-002) |
| G21 | app | `notify.rs:64-87` `Toasts` is keyed by id only. `notify.js:19-50` shows no computer name and filters by id only. | HIGH (S3 sc.5) |
| G22 | app | `headless.rs:22-27` refuses on Windows and Linux. | HIGH (S6) |
| G23 | app | NSIS `installMode: currentUser`. No installer hooks. No `installer.nsi` was ever committed (001 notes). | HIGH (S5, FR-013) |
| G24 | app | `CONNECTION_TEXT` and permission labels are duplicated (`health.js:6-21`, `desk_settings.js:7-13`, `first_run.js:6-31`). | LOW (debt) |
| G25 | platform-linux | `NO_TRAY` is filed under `Capability::Capture` (`tray.rs:24`). There is no login-screen limitation. | LOW |
| G26 | service-win | Stub (`src/main.rs` is `fn main() {}`). | — (new work) |
| G27 | cli | No collect or rename commands. The pipe name differs from the contract (`control/server.rs:258-279`). | MEDIUM |
| G28 | xtask | Evidence directory and disjointness script are hardwired to 001. | HIGH (infra) |
| G29 | docs | Stale Windows statements: `troubleshooting.md:5,132-134`, `configuration.md:11,26`, `releasing.md:63`, `cli.md:162`. | LOW → Polish |

### Seam pass

There is one check per contract.

- **Wire.** The producer (core) sends `Paused` gated. The consumer (0.2.x reader) panics on unknown variants (G4). **Confirmed mismatch risk.** Every v2 message is gated, and the reader skips unknown variants from 0.3.0 on.
- **Control/status.** The UI assumes the problem id is unique, but core makes `(id, computer)` unique (G9, G21). **Confirmed.** Both sides change to key on `(id, computer)`.
- **Platform.** Core never consumes `foreground_elevated` (G6). Backends provide no capture-loss signal (G13). **Confirmed.** Both are replaced by `foreground_app` and `CaptureLost`.
- **Config.** CLI and file edits are not seen by the app's hotkeys (G19). **Confirmed.** Fixed by the `config` topic.
- **Log-record.** The Windows diag events are free-form, and core's `fields` are free-form. There is no privacy guard. **Confirmed.** Fixed by the closed catalog.
- **Service IPC and log bundle.** These are new contracts with no existing producer or consumer, so there are no findings.

### Rollup and coverage critic

- The findings above are already deduplicated. Every finding is confirmed, so none is held at "suspected".
- Touched packages are covered: proto, platform, testkit, core, platform-windows, platform-macos, platform-linux, app, cli, service-win and xtask.
- Contracts are covered: wire, control, config, platform, service-ipc, log-events, log-bundle, status and ui-navigation.

### Routing

Each gap is resolved by a V-decision within the plan's authority. Three items need owner confirmation (below).

## Technical Debt Survey (Principle 3.8)

### Tier 1: suppressions

- G16 `#[allow(dead_code)]` (`platform-windows/src/lib.rs:384`): rename the field to `_guard` so the allow is no longer needed.
- `#[allow(clippy::zombie_processes)]` (`platform-windows/src/drag_tests.rs:699`): wait for the Explorer child process in test teardown and remove the allow.
- No coverage-ignore markers, skipped tests or JS ignore comments were found.

### Tier 2: local debt

- G2's 115 `lock().unwrap()` calls → `LockExt::guard()`.
- G15's false comment.
- G19's dead OS registrations for quickjump and targeted_paste. Core detects both in the captured stream, so the registrations are deleted.
- G24's duplicated label tables become one `ui/labels.js`.
- `tray_model_tests.rs` tests `tray.rs`, so it is renamed `tray_tests.rs`.
- `shell.rs` (355 lines, mixing setup, the menu closure and plugins) is split into `shell.rs` (builder) and `tray_runtime.rs`.
- G12's `persist` expects.
- G20's app panics on lag (resync from status instead) and on an unknown tray id.

### Tier 3: structural — complexity deleted, not rearranged

1. Six windows plus seven pages become one routed page. This deletes `Which` and its per-window plumbing, the capabilities entries, the per-page 2–3 s `status` polling (one subscription replaces it), and `open_home` branching.
2. Limitations are no longer raised as problems. This deletes the `platform_limitation:<capability>` raise path, the startup-limitation exclusions in `notify.rs:42-48` and the merge special-cases.
3. `foreground_elevated` is replaced by `foreground_app`. One method serves logging, the V6 policy and V7.
4. Installed Windows reuses the existing headless engine as the session engine (V9). This avoids a Platform-over-IPC proxy layer entirely.
5. Close-reason plumbing replaces the reason-less `on_link_closed`. That lets the tray, logs and problems share one `LinkLossReason`.

## Execution Mode (Principle 3.6)

**Declared: package-parallel.** Every story except US7 crosses packages. The package work is large and disjoint, the Windows lanes run on a different host, and the contract changes are fixed now. This is the same reasoning as 001.

**Lanes**: `core`, `app`, `cli`, `platform-windows`, `platform-macos`, `platform-linux` and `service-win`. Within each lane, phases follow story order US1→US6.

**Setup** covers:
- xtask fixes (G28);
- probes P1–P4 (OD-3);
- the 002 evidence directory.

**Foundational** covers:
- the frozen-crate changes listed above;
- the v2 `.feature` files;
- every new workspace dependency (one task, so `Cargo.lock` is written once);
- tests for the placeholder match arms the frozen changes force into lane packages;
- the lane disjointness check.

**Convergence** covers:
- the NSIS installer hooks and `perMachine` mode (app + service-win sidecar);
- release-workflow changes for the `mouser-service.exe` sidecar;
- an all-lanes gate.

**Polish** covers:
- `docs/` updates (G29, plus the logging catalog, the bundle, Windows install, headless, settings);
- the Story 7 run (quickstart §8), with evidence;
- the final gate.

**Gates by phase (mode-consistent):**

| Phase | Gate command |
|---|---|
| Setup | `cargo xtask gate --all --lane setup`, green on the three CI runners (001 T012 form). It is not run locally on the owner's Mac, because the macOS suites drive real input |
| Foundational | `cargo xtask gate --packages mouser-proto,mouser-platform,mouser-testkit --lane foundational`, on all three CI runners |
| Lane phase | `cargo xtask gate --packages <lane package> --lane <lane>`. Windows lanes (platform-windows, service-win, and the Windows parts of core/app/cli) run the gate on the owner's Parallels Windows 11 VM (ARM64) from the Windows Claude session, as in 001, which writes the lane evidence; the pushed lane commit must also be green on the CI `windows-2025` runner. macOS-input suites run on CI or by the owner, never from an agent on the owner's Mac |
| Convergence and terminal | `cargo xtask gate --all --lane convergence` / `--lane polish`, on the three CI runners. J1 is re-run because Convergence touches packaging |

**Workspace bootstrap**: `cargo fetch --locked && cargo xtask bootstrap` (unchanged).

## Owner Decisions (2026-10-26)

All decisions below were made by the owner and relayed by the coordinator on 2026-10-26 (CA, OD-1 to OD-3) and 2026-10-27 (OD-4 to OD-7).

| ID | Decision |
|---|---|
| CA | **Frozen-crate changes approved** as listed in "Frozen Contract Changes" above: `mouser-proto` (wire, control, config, `service_ipc`, schemas), the `mouser-platform` delta, the `mouser-testkit` knobs, and the new v2 `.feature` files. The 001 `.feature` files stay untouched. |
| OD-1 | **Approved.** The UAC prompt raised by portable "Restart Mouser as administrator" is approved at the Windows PC's own keyboard. Windows shows UAC on that PC's secure desktop, which portable Mouser cannot drive (research V19). The message says "Approve the Windows prompt on <computer>". |
| OD-2 | **Approved.** When a different administrator account installs the per-machine upgrade from a 0.2.x per-user install, the computer rejoins with one approval (research V12a). |
| OD-4 | **J1 is a modified journey** (2026-10-27). The Windows install it performs becomes machine-wide with `MouserService`, so J1 re-runs in Convergence (constitution 4.3). Its scenario text is unchanged. |
| OD-5 | **New seams approved** (2026-10-27): `WinApi` (service-win), `Shell`, service-query and file-manager (app), input-desktop (platform-windows), each with the real-config debt named in the Constitution Check 4.1 row. The thin SYSTEM-only pass-through `apps/mouser-service-win/src/winapi_sys.rs` is excluded from coverage, recorded as debt, and verified by probe P2 on the VM and manual rows R8/R9. |
| OD-6 | **Linux on a Lima Ubuntu VM** (2026-10-27). A Lima Ubuntu VM with a desktop session (X11, GNOME Wayland and KDE Wayland sessions) counts as "the Linux computer" for every gating Story 7 row; a Linux user's own computer is optional. The Windows part of R1 is not a real Wi-Fi/Ethernet switch on the VM: the gating Windows row is R1w (switch the VM's Parallels adapter between Shared and Bridged while it is controlled), which tests the guest's interface and address change only; a real Windows Wi-Fi/Ethernet switch is optional (Steve). |
| OD-7 | **The SYSTEM side never writes a user-chosen path** (2026-10-27). In mode `service` the session engine builds the bundle and returns it over the control pipe (`logs.collect_stream`); the user's own non-elevated app or CLI writes it to Downloads or the chosen folder. `logs.collect` is rejected in mode `service`. contracts/control-api-delta.md, cli-delta.md, service-ipc.md and log-bundle.md are updated (owner-approved contract changes). |
| OD-3 | **VM only.** Windows probes P1–P3 run on the owner's Parallels Windows 11 VM, driven by the Windows Claude session. Steve and his PCs gate no phase. His real-PC checks stay optional, as extra confirmation in Story 7, and gate nothing. VM limits and how each is covered are in research.md "Probes". |

## Post-Design Constitution Re-check

The design artifacts introduce no unjustified violations:

- Every frozen-contract change is listed above and routed through Foundational.
- No new timing target or retry policy is introduced:
  - V15 waits on existing link liveness.
  - V7 uses the existing watchdog cycle.
  - The service's engine relaunch is supervision specified by Story 5 (control must be available at all times), not a test accommodation.
- Error handling added in V1 and V3 is specified by FR-002 and Story 1. It is not invented (5.5, 5.6).
- No open markers remain. OD-1 to OD-7 and the contract changes are owner-approved.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Windows SYSTEM service + session engine (kept from 001, now built) | Story 5 sc. 1–5: login screen, UAC, Ctrl-Alt-Del and elevated apps are unreachable from a user process, and uiAccess is ruled out unsigned (FR-004) | User-level only: covers none of sc. 1–5. A Platform-over-IPC proxy: much larger contract |
| Cursor guard process (Windows) | Story 1 sc. 6: the cursor hide is global and outlives a crashed process | Deskflow hider window: focus side effects and a capture rewrite. "Restore at next launch" (001): the pointer stays invisible until a relaunch |
| Closed log-event catalog with a schema per event | FR-003 / SC-004 must be enforceable, not reviewed by eye | Free-form fields: no guard against a future leak |
| Version-gated wire additions + skip-unknown reader | 0.2.x peers panic on unknown variants (G4), and desks update one computer at a time | A protocol version bump: would make 0.2.x and 0.3.x incompatible mid-update |
| Per-unit `catch_unwind` fault boundary | FR-002 / Story 1 sc. 2: a fault must not end crossing, reconnecting or the panic key | Abort and restart the app: drops the desk on every fault, and portable mode has no supervisor |
