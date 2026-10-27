---
description: "Task list for Mouser v2 (002-mouser-v2)"
---

# Tasks: Mouser v2

**Input**: Design documents from `specs/002-mouser-v2/` — plan.md, spec.md, research.md (V1–V20, probes P1–P4), data-model.md, contracts/, quickstart.md

**Prerequisites**: plan.md (package-parallel mode, Owner Decisions CA, OD-1, OD-2, OD-3 of 2026-10-26), spec.md (7 stories), constitution v1.4.0. Differential to `specs/001-mouser-v1/tasks.md`: everything 001 delivered stays, and its tests keep running in every gate (FR-001).

**Tests**: Required. Constitution 4.1 mandates acceptance (`acc_`, Gherkin), contract (`contract_`) and implementation tests with 100% line coverage; every test task precedes the implementation task it verifies (5.1 rule 3). Write the test, run it, see it fail, then implement. If a new test passes before its implementation task, record that in the task's completion note and still do the implementation task's check.

**Organization**: Package-parallel (plan.md "Execution Mode", constitution 5.2). Setup, Probes and Foundational run first. Then a PARALLEL GROUP of lanes, one per package; within a lane, phases follow story order (US1→US6); across lanes, execution is concurrent. Convergence and Polish follow. Story 7 (real hardware) is verification only and runs in Polish.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel within its phase (different files, no dependency on an incomplete task)
- **[Story]**: `USn-<lane>` for lane phases (composite tag, 5.2); story completion = all `USn-*` phases + Convergence passed. `[US7]` marks the Story 7 real-hardware verification tasks in Polish
- Gate commands write evidence via `cargo xtask gate`, the only sanctioned evidence source (5.4); never hand-write evidence. After Setup the evidence goes to `specs/002-mouser-v2/evidence/<lane>.jsonl`
- Phase-boundary commits (5.3) are verification-only tasks; commit only after the phase `Gate:` task passes
- No behavior beyond spec.md acceptance criteria and edge cases (5.5); no error handling for unspecified conditions — fail loudly (5.6). FR-002 makes refused OS requests and faults inside Mouser specified conditions (research V1, V3): handling them is required, not invented
- Lanes MUST NOT write `specs/002-mouser-v2/contracts/`, `crates/mouser-proto/**`, `crates/mouser-platform/**`, `crates/mouser-testkit/**`, any `.feature` file, `crates/mouser-core/tests/features/**` or `apps/mouser-service-win/tests/features/**`; a needed contract change returns to the plan
- **Never run real-input suites on the owner's Mac from an agent session.** `cargo xtask gate --all` and `cargo xtask gate --packages mouser-platform-macos` drive real input on macOS: they run on the CI runners or by the owner only (001 T495, quickstart.md)

## Path Conventions

Cargo workspace at repo root, unchanged from 001: `crates/mouser-{proto,platform,testkit,core,platform-macos,platform-windows,platform-linux}`, `apps/mouser-{app,cli,service-win}`, `xtask/`. v2 acceptance features: `crates/mouser-core/tests/features/v2_usN.feature` (N = 1–6) and `apps/mouser-service-win/tests/features/v2_us5_service.feature`, all authored in Foundational. Core acceptance runners `crates/mouser-core/tests/acc_v2_usN.rs`, step definitions `crates/mouser-core/tests/steps/v2_usN.rs` (registered in `crates/mouser-core/tests/steps/mod.rs`). UI acceptance tests `apps/mouser-app/ui/acc_v2_us3.test.js`, `apps/mouser-app/ui/acc_v2_us4.test.js` (Node has no Gherkin runner, constitution 4.1: test names are the Given/When/Then text verbatim). The 001 `.feature` files are never edited.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Point the gate tooling, CI evidence upload and lane-disjointness check at the active feature instead of 001 (plan "Infrastructure Detection", gap G28; constitution 3.4: infrastructure precedes all feature tasks)

- [ ] T001 Write unit tests in `xtask/src/feature_dir_tests.rs` (module declared from `xtask/src/feature_dir.rs`): `evidence_dir()` reads `feature_directory` from `.specify/feature.json` (currently `{"feature_directory": "specs/002-mouser-v2"}`) and returns `<feature_directory>/evidence`; a missing file or missing key panics with a message naming `.specify/feature.json`
- [ ] T002 Implement `evidence_dir()` in `xtask/src/feature_dir.rs` and use it in `xtask/src/gate.rs` in place of the hard-coded `Path::new("specs/001-mouser-v1/evidence")` at lines 56–57; register the module in `xtask/src/main.rs`
- [ ] T003 [P] Change the evidence upload step in `.github/workflows/ci.yml` (currently `path: specs/001-mouser-v1/evidence/`) to upload `<feature_directory>/evidence/`, reading `feature_directory` from `.specify/feature.json` with `jq -r .feature_directory` in a preceding step
- [ ] T004 [P] Parameterise `xtask/scripts/lane_disjointness.py`: take the tasks.md path as `argv[1]`; derive the feature directory from that path; find the parallel group with line-anchored regexes (`re.M`): it starts at the level-2 heading that begins with "PARALLEL GROUP" and ends at the next level-2 heading that begins with "Phase " (instead of splitting on the first plain-substring occurrence and the literal Phase 54 heading, which break on any task text quoting a heading); frozen prefixes become `<feature dir>/contracts/`, `crates/mouser-proto/`, `crates/mouser-platform/`, `crates/mouser-testkit/`, `crates/mouser-core/tests/features/`, `apps/mouser-service-win/tests/features/`. Verify `python3 xtask/scripts/lane_disjointness.py specs/001-mouser-v1/tasks.md` still prints `RESULT: disjoint`
- [ ] T005 [P] Create `specs/002-mouser-v2/evidence/quickstart.md` with one table row per quickstart.md §8 check R1–R12 and per §3–§7 scenario, columns: check, outcome (`PENDING`), hardware and OS, Mouser version, date, who ran it, bundle file name, defect (Story 7)
- [ ] T006 Gate: `cargo fetch --locked && cargo xtask bootstrap && cargo xtask gate --all --lane setup` green on all three CI runners (`macos-14`, `windows-2025`, `ubuntu-24.04`) and the uploaded artifact contains `specs/002-mouser-v2/evidence/setup.jsonl`; not run locally on the owner's Mac (plan "Gates by phase")
- [ ] T007 Commit: phase boundary "Setup" (5.3)

---

## Phase 2: Probes (constitution 3.1; owner decision OD-3)

**Purpose**: Empirically validate the external Windows and tray behaviour the lanes depend on. P1–P3 run **only on the owner's Parallels Windows 11 VM** (ARM64), driven by the Windows Claude session; Steve's PCs gate nothing. P4 runs on macOS, then Windows; no input is injected, the owner clicks by hand. Probe code lives under `specs/002-mouser-v2/spikes/spike_*/` (excluded from coverage); observations go in `specs/002-mouser-v2/spikes/FINDINGS.md`. Reuse the ARM64 build notes in `crates/mouser-platform-windows/probes/FINDINGS.md` (rust-lld, Clang for aws-lc).

**Execution note**: This phase writes only under `specs/002-mouser-v2/spikes/` and may run concurrently with Phase 3 (Foundational). Each probe gates the lane phases named in its task; a refuted assumption stops that lane and returns to the plan (contracts are frozen; changes are plan-level, 3.3). All probe tasks are verification only.

- [ ] T008 Run probe P1 `specs/002-mouser-v2/spikes/spike_uipi/` on the Parallels VM, once with Parallels mouse integration on and once with it off ("Optimize for games" / pointer capture), and record both runs in `specs/002-mouser-v2/spikes/FINDINGS.md`: (1) whether medium-IL low-level hooks see input while an elevated `cmd.exe` ("Run as administrator") or elevated Task Manager is in front; (2) `SendInput` and `SetCursorPos` return values and `GetLastError` under UIPI; (3) `GetForegroundWindow` → process id → `QueryFullProcessImageNameW` file name plus token elevation and integrity compared with the probe's own; (4) whether an out-of-context `EVENT_SYSTEM_FOREGROUND` WinEvent hook on the hook thread fires for elevated windows; (5) a cursor-guard child (`--cursor-guard <pid>` style) restoring blank system cursors with `SPI_SETCURSORS` after `TerminateProcess` of its parent; (6) a `swallow` mode that installs a probe-swallowing LL hook after the probe's hook and shows real input still arriving (Story 3 sc. 16 stand-in for ASUS AsHotplugCtrl). Hook-over-elevated and capture-loss conclusions come from the integration-off run. Gates: Phase 19 (US1-platform-windows), Phase 22 (US5-platform-windows), Phase 21 (US3-platform-windows)
- [ ] T009 Run probe P2 `specs/002-mouser-v2/spikes/spike_service/` (001 T367, extended) on the Parallels VM with Parallels shared clipboard **off** (VM Configuration → Options → Sharing; record the setting), and record in `specs/002-mouser-v2/spikes/FINDINGS.md`: (1) a LocalSystem probe service starting a process with a duplicated `winlogon.exe` token via `CreateProcessAsUserW` (`lpDesktop = "winsta0\\default"`) at cold boot, at the login screen, on a UAC prompt and across fast user switching (`SERVICE_CONTROL_SESSIONCHANGE`, `WTSGetActiveConsoleSessionId` returning `0xFFFFFFFF` while no session); (2) following the input desktop by polling `OpenInputDesktop(DF_ALLOWOTHERACCOUNTHOOK)` + `SetThreadDesktop` and reinstalling hooks, with injected input operating the Winlogon and UAC desktops; (3) `SendSAS(FALSE)` from the service under `SoftwareSASGeneration` 0, 1, 2 and 3 (judged only by `SendSAS` from the service, never host keys); (4) clipboard read/write and an Explorer file drop from the SYSTEM process in the user session; (5) an x86_64 NSIS `perMachine` build with `installerHooks` installing and uninstalling the service unsigned, under the VM's x64 emulation. Gates: Phase 22 (US5-platform-windows), Phase 27 (US5-service-win), and the Convergence installer tasks
- [ ] T010 Run probe P3 `specs/002-mouser-v2/spikes/spike_elevate/` on the Parallels VM and record in `specs/002-mouser-v2/spikes/FINDINGS.md`: a portable relaunch with `ShellExecuteExW(verb="runas", exe, "--wait-for <pid>")`; `ERROR_CANCELLED` when the UAC prompt is declined; the new instance waiting for `<pid>` to exit before tauri-plugin-single-instance 2.5.4 initialises (its mutex would otherwise make it exit); and that the elevated instance's hooks and injection work over an elevated application. The UAC prompt is approved at the VM's own keyboard (the Parallels window; OD-1). Gates: Phase 13 (US5-app)
- [ ] T011 Run probe P4 `specs/002-mouser-v2/spikes/spike_tray_click/` on macOS, then on the Parallels VM, with the owner clicking by hand (no injected input), and record in `specs/002-mouser-v2/spikes/FINDINGS.md`: tauri 2.12.5 `TrayIconEvent::Click` reports left vs right with `show_menu_on_left_click(false)`; `button_state: Up` arrives once per click; runtime `set_icon` swaps between badge variants without flicker. Gates: the tray tasks of Phase 11 (US3-app)
- [ ] T012 Commit: phase boundary "Probes" (5.3) — after T008–T011 are recorded; a refuted probe is recorded as such and its gated lane phases wait for the plan change

---

## Phase 3: Foundational (Blocking Prerequisites — contracts surface only)

**Purpose**: Land the owner-approved frozen-crate changes (plan "Frozen Contract Changes", Owner Decision CA) and the v2 `.feature` files. No lane phase begins until this phase's gate passes. Lanes read these files; they never write them. The 001 `.feature` files are not edited; v2 amendments to 001 scenarios are new v2 scenarios.

### Contract crate `mouser-proto` (contracts/wire-protocol-delta.md, control-api-delta.md, config-delta.md, service-ipc.md, schemas)

- [ ] T013 Update `crates/mouser-proto/tests/schemas_in_sync.rs` so each schema has its own source: `status.schema.json`, `log-events.schema.json` and `log-bundle.schema.json` are compared with `specs/002-mouser-v2/contracts/`; `log-record.schema.json` and `release-manifest.schema.json` are still compared with `specs/001-mouser-v1/contracts/` (test fails until T014)
- [ ] T014 Copy `specs/002-mouser-v2/contracts/{status,log-events,log-bundle}.schema.json` into `crates/mouser-proto/schemas/` byte for byte (status v2 replaces the 001 copy) (uniform mechanical change)
- [ ] T015 [P] Extend `crates/mouser-proto/tests/contract_wire.rs`: a snapshot of every existing `ControlMessage` postcard variant index (unchanged from 0.2.8, `Paused` last); `LogsRequest { request: [u8; 16] }`, `DeskSettings { epoch: u64, author: ComputerId, landing_highlight: bool, command_as_control: bool }` and `RestartElevated` round-trip and take the next three indexes after `Paused`, in that order; `V2_SINCE == "0.3.0"`; `PROTOCOL_VERSION` is still `1`
- [ ] T016 Append `LogsRequest`, `DeskSettings`, `RestartElevated` to the end of `ControlMessage` and add `pub const V2_SINCE: &str = "0.3.0"` in `crates/mouser-proto/src/wire.rs`
- [ ] T017 [P] Extend `crates/mouser-proto/tests/contract_wire.rs`: a frame with a valid length prefix whose postcard body names an unknown `ControlMessage` variant index decodes to `FrameError::UnknownVariant(index)`; any other undecodable body decodes to `FrameError::Malformed`; the two are distinct
- [ ] T018 Implement `FrameError::UnknownVariant(u32)` and its detection in `decode_frame` in `crates/mouser-proto/src/wire.rs`
- [ ] T019 [P] Extend `crates/mouser-proto/tests/contract_wire.rs`: `LogsPart { card: LogsCard { id, name, os, os_version, version, mode }, config_toml: String, logging: bool, files: Vec<(String, u64)> }` round-trips through postcard; `logs_stream_id(request)` equals `sha256("logs\n" ‖ request)[..16]`
- [ ] T020 Implement `LogsPart`, `LogsCard` and `logs_stream_id` in `crates/mouser-proto/src/wire.rs` per contracts/wire-protocol-delta.md "Logs stream"
- [ ] T021 [P] Extend `crates/mouser-proto/tests/contract_wire.rs`: the peer `Status` message (`PeerStatus`) gains `mode` as its last field with values `installed | portable | portable_elevated | service | headless`; a 0.2.8-shaped `PeerStatus` (no `mode`) still encodes and decodes with the 001 shape through the separate legacy constructor used for peers below `V2_SINCE`
- [ ] T022 Implement `ComputerMode` and `PeerStatus.mode` (last field) plus the 001-shape encoding for peers below `V2_SINCE` in `crates/mouser-proto/src/wire.rs`
- [ ] T023 [P] Extend `crates/mouser-proto/tests/contract_control.rs`: params/results of the new methods serialize exactly as contracts/control-api-delta.md — `logs.collect {dir?}` → `{path, computers: [{id, name, included, reason?}]}`; `desk.rename {computer, name}` → `{}`; `layout.keep_placement {computer}` → `{}`; `elevation.restart {computer}` → `{}`; `config.set` accepts optional `origin: "app" | "cli"` defaulting to `"cli"`
- [ ] T024 Implement the `logs.collect`, `desk.rename`, `layout.keep_placement`, `elevation.restart` request/response types and `config.set.origin` in `crates/mouser-proto/src/control.rs`
- [ ] T025 [P] Extend `crates/mouser-proto/tests/contract_control.rs`: the `config` topic notification is `{key, old, new, origin, from?}` with `origin ∈ {app, cli, file, desk_mate}` (`SettingChange`, data-model "Setting change"); the `problem` notification carries `problem_id`, `computer` and optional `app`
- [ ] T026 Implement `SettingChange`, `SettingOrigin`, the `config` topic and the extended `problem` notification in `crates/mouser-proto/src/control.rs`
- [ ] T027 [P] Extend `crates/mouser-proto/tests/contract_control.rs`: `ProblemKind` enumerates exactly `missing_permission, incompatible_version, join_refused, secure_input, crossing_blocked, peer_offline, platform_limitation, elevated_app`; `Problem` has optional `app` (program name); `FixAction` adds `restart_elevated:<computer id>`, `keep_placement:<computer id>`, `update_computer:<computer id>`
- [ ] T028 Implement `ProblemKind::ElevatedApp`, `Problem.app` and the three new `FixAction` values in `crates/mouser-proto/src/control.rs`
- [ ] T029 [P] Extend `crates/mouser-proto/tests/contract_control.rs`: a populated v2 `Status` validates against `schemas/status.schema.json` (v2) with the `jsonschema` crate — `active_computer` always present, `tray_summary {state: active|paused, connection: connected|alone|disconnected, computers ≥ 1, displays ≥ 0}`, `computers[].mode`, `computers[].os_version`, `problems[].problem_id`, `problems[].app`; a status missing `tray_summary` fails validation
- [ ] T030 Implement Status v2 (`TraySummary`, `TrayConnection`, `computers[].mode`, `computers[].os_version`, `problems[].problem_id`, `problems[].app`, non-optional `active_computer`) in `crates/mouser-proto/src/control.rs`
- [ ] T031 [P] Extend `crates/mouser-proto/tests/contract_config.rs`: `crossing.landing_highlight` and `keyboard.command_as_control` default to `true` and are in `DESK_SHARED_KEYS`; a missing key takes its default (`schema` stays `1`); `Config::validate` rejects a non-boolean with exactly `crossing.landing_highlight: must be true or false` / `keyboard.command_as_control: must be true or false`; validates `hotkeys.enable_disable` with the same chord parser as `quickjump` and `targeted_paste`, failing with `hotkeys.enable_disable: <chord parse reason>`; rejects `diagnostics.max_log_mb = 0` with exactly `diagnostics.max_log_mb: must be 1 or more`
- [ ] T032 Implement the two keys, defaults, `DESK_SHARED_KEYS` additions and the `enable_disable` / `max_log_mb` validation in `crates/mouser-proto/src/config.rs`
- [ ] T033 [P] Write `crates/mouser-proto/tests/contract_service_ipc.rs`: `Hello { version: String }`, `SendSas`, `SasResult(Sent | PolicyBlocked | Failed(String))`, `Shutdown`, `Stopping { reason: Quit | Fault }` round-trip through 4-byte big-endian length + postcard framing (the `mouser_proto::wire` framing); `engine_pipe_name(7) == r"\\.\pipe\mouser-svc-7"`; `SERVICE_CONTROL_PIPE == r"\\.\pipe\mouser-service"`
- [ ] T034 Implement the OS-neutral serde types, framing and pipe-name helpers in `crates/mouser-proto/src/service_ipc.rs` and export `service_ipc` from `crates/mouser-proto/src/lib.rs`

### Contract crate `mouser-platform` (contracts/platform-api-delta.md)

- [ ] T035 In `crates/mouser-platform/src/lib.rs` (declarations only): remove `fn foreground_elevated(&self) -> bool`; add `fn foreground_app(&self) -> Option<ForegroundApp>` defaulting to `None`; add `pub struct ForegroundApp { pub name: String /* executable file name, no path (FR-003) */, pub admin: bool, pub outranks_mouser: bool }`; add `LocalInput::CaptureLost(CaptureLossReason)` and `pub enum CaptureLossReason { ElevatedForeground, OtherDesktop, HooksSilent }`; add `Capability::{Install, LoginScreen, Tray}`
- [ ] T036 Add conformance checks for rule 10 ("After `CaptureLost` is sent, or after `InputCapture::stop()`, or after the capture is dropped while grabbed, the pointer is visible and input acts locally") and rule 11 ("Every inject and capture method returns `Err`, never panics, when the OS refuses": a button above 5 returns `Err(Os(_))` or `Ok(())`, never a panic; `warp` outside every display returns `Err`) as public check functions in `crates/mouser-platform/src/conformance.rs`; rule 10's check takes a backend-supplied "pointer hidden" query

### Test infrastructure crate `mouser-testkit` (sanctioned doubles, 001 D2)

- [ ] T037 [P] Extend `crates/mouser-testkit/tests/virtual_platform.rs`: `refuse_input(&[InputKind::Key, InputKind::Button])` makes those inject methods return `Err(Os("refused"))` and `accept_input()` undoes it; `panic_on_next_inject()` makes the next inject call panic once; `set_foreground_app(Some(ForegroundApp{..}))` is returned by `foreground_app()`; `lose_capture(reason)` while grabbed ungrabs, then delivers `LocalInput::CaptureLost(reason)`; `pointer_hidden()` is true only while grabbed and not lost
- [ ] T038 Implement the knobs `refuse_input`/`accept_input`, `panic_on_next_inject`, `set_foreground_app` (replacing `set_foreground_elevated`), `lose_capture`, `pointer_hidden` in `crates/mouser-testkit/src/virtual_platform.rs`
- [ ] T039 Extend `crates/mouser-testkit/tests/contract_platform_virtual.rs` to run the rule 10 and rule 11 conformance checks against `VirtualPlatform`, with `pointer_hidden()` as rule 10's query

### Workspace stays compiling (mechanical follow-through of the frozen changes)

- [ ] T040 Replace every implementation and use of `foreground_elevated` / `set_foreground_elevated` outside the frozen crates with `foreground_app` returning `None` / `set_foreground_app(None)` respectively (uniform mechanical change): `crates/mouser-platform-macos/src/lib.rs`, `crates/mouser-platform-macos/src/lib_tests.rs`, `crates/mouser-platform-linux/src/lib.rs`, `crates/mouser-platform-linux/src/lib_tests.rs`, `crates/mouser-platform-windows/src/lib.rs`, `crates/mouser-platform-windows/src/lib_tests.rs`, `crates/mouser-core/src/testing.rs`, `crates/mouser-core/tests/support/picky.rs`, `crates/mouser-core/tests/steps/us12.rs`, `apps/mouser-app/src/support_tests.rs`; leave a `// v2: real foreground_app in <lane> US1` comment at each backend impl. `crates/mouser-platform-windows/src/watchdog.rs` and `uipi.rs` keep their elevation helper as a private function. Verified by `cargo check --workspace --all-targets` on all three CI runners
- [ ] T041 Add the minimal arms required by exhaustive `match`es on the new variants (`ControlMessage::{LogsRequest, DeskSettings, RestartElevated}` ignored on receipt, `LocalInput::CaptureLost(_)` ignored, `ProblemKind::ElevatedApp` and the new `Capability` variants mapped like their nearest 001 neighbour), each with a `// v2: replaced in <task id>` comment naming the lane task below that replaces it (uniform mechanical change). Verified by `cargo check --workspace --all-targets` and `cargo test --workspace --no-run` on all three CI runners

### Acceptance features (spec-derived; authored here because lanes may not write `.feature` files)

- [ ] T042 [P] Write `crates/mouser-core/tests/features/v2_us1.feature` with one Scenario per Story 1 acceptance scenario 1, 2, 3, 4, 5, 7, 8 (sc. 6 "ends unexpectedly" is a process death, verified by the cursor-guard tasks and quickstart R4) and the edge cases "a key or mouse button is held when the operating system starts refusing input" and "Mouser quits on a computer that has the pointer from another computer", Given/When/Then verbatim from spec.md, plus the regression Scenario reproducing the trigger mechanism (constitution VI): an inject refused while the crossing is held, then the link closes, then the two computers reach each other again and the desk user crosses without restarting Mouser
- [ ] T043 [P] Write `crates/mouser-core/tests/features/v2_us2.feature` with one Scenario per Story 2 acceptance scenario 1–10 and the edge cases "the diagnostic log rotates in the middle of an incident" and "a desk computer is offline, or has logging off, when logs are collected", verbatim from spec.md
- [ ] T044 [P] Write `crates/mouser-core/tests/features/v2_us3.feature` with the Story 3 scenarios core can observe through `status` and the control API — sc. 4, 5, 6, 8, 11, 14, 15 — and the edge cases "a desk computer is mid-update to a newer version" and "a desk-mate goes offline while this computer is paused", verbatim from spec.md (the window-only scenarios are UI tests in the app lane)
- [ ] T045 [P] Write `crates/mouser-core/tests/features/v2_us4.feature` with Story 4 scenarios 1, 3, 4, 5, 6, the QuickJump and targeted-paste parts of sc. 2 (detected by core in the captured stream), and the edge case "a computer is renamed while another desk computer is offline", verbatim from spec.md
- [ ] T046 [P] Write `crates/mouser-core/tests/features/v2_us5.feature` with Story 5 scenarios 4, 5, 6, 7, 8 as core observes them over `VirtualPlatform` (`set_foreground_app`, mode `installed` / `portable` / `portable_elevated`), verbatim from spec.md
- [ ] T047 [P] Write `crates/mouser-core/tests/features/v2_us6.feature` with Story 6 scenarios 1–2, verbatim from spec.md (run by the app lane's `acc_v2_us6.rs`)
- [ ] T048 [P] Write `apps/mouser-service-win/tests/features/v2_us5_service.feature` with Story 5 scenarios 1–3 as the service seam observes them (session engine started in the console session with a winlogon token, relaunched across a user switch, `SendSas` honoured or reported as blocked by policy) and FR-013 ("uninstalling it leaves no part of Mouser running on that computer"), verbatim from spec.md
- [ ] T049 Verify lane disjointness: `python3 xtask/scripts/lane_disjointness.py specs/002-mouser-v2/tasks.md` prints `RESULT: disjoint` (every lane's backticked path set is pairwise disjoint and disjoint from `specs/002-mouser-v2/contracts/`, the frozen crates and both features directories); record the output (5.2; verification only)
- [ ] T050 Gate: `cargo xtask gate --packages mouser-proto,mouser-platform,mouser-testkit --lane foundational` exits 0 on all three CI runners, and `cargo test --workspace --no-run` compiles on all three
- [ ] T051 Commit: phase boundary "Foundational" (5.3)

**Checkpoint**: Contracts frozen. All lanes may start (each also waiting on the probe its first phase names).

---

## PARALLEL GROUP — User story lanes

Disjointness (5.2): each lane writes only under its own package directory — core `crates/mouser-core/**` (except `tests/features/**`), app `apps/mouser-app/**`, cli `apps/mouser-cli/**`, platform-windows `crates/mouser-platform-windows/**`, platform-macos `crates/mouser-platform-macos/**`, platform-linux `crates/mouser-platform-linux/**`, service-win `apps/mouser-service-win/**` (except `tests/features/**`) — plus its own `specs/002-mouser-v2/evidence/<lane>.jsonl`. Disjointness is verified mechanically at the end of Foundational (T049), before any lane is dispatched.

### Lane: core

Package `crates/mouser-core`. Gate per phase: `cargo xtask gate --packages mouser-core --lane core` (runs on any OS; CI runs it on all three). Acceptance runners use cucumber 0.23 with `harness = false` and `fail_on_skipped()`; steps drive N in-process engines through the 001 multi-node harness (`crates/mouser-core/tests/support/desk.rs`) over `VirtualPlatform` + `SimNet`, and assert on control-API results/notifications and on the log files. Peers below 0.3.0 are simulated by setting `EngineParams.mouser_version = "0.2.8"`.

#### Phase 4: US1-core — As a desk user, I want my pointer and keyboard to keep working whatever goes wrong inside Mouser (P1) 🎯 MVP

**Independent test**: `cargo test -p mouser-core --test acc_v2_us1` passes: with `refuse_input`, `panic_on_next_inject`, `lose_capture` and SimNet black-holes, control always returns locally with the pointer shown, nothing stays held, and crossing works again without restarting an engine.

- [ ] T052 [US1-core] Write the acceptance runner `crates/mouser-core/tests/acc_v2_us1.rs` and step definitions `crates/mouser-core/tests/steps/v2_us1.rs` (registered in `crates/mouser-core/tests/steps/mod.rs`) for every scenario in `tests/features/v2_us1.feature`; "the pointer is visible" is asserted with `VirtualPlatform::pointer_hidden() == false`
- [ ] T053 [P] [US1-core] Write unit tests in `crates/mouser-core/src/lock_tests.rs`: `LockExt::guard()` on a `std::sync::Mutex` returns the guard; after a thread panicked while holding it (poisoned), `guard()` still returns the inner guard and does not panic (research V2)
- [ ] T054 [US1-core] Implement `LockExt::guard()` = `lock().unwrap_or_else(PoisonError::into_inner)` in `crates/mouser-core/src/lock.rs` and declare it in `crates/mouser-core/src/lib.rs`
- [ ] T055 [US1-core] Replace every `lock().unwrap()` in non-test files under `crates/mouser-core/src` (115 sites at HEAD bd97abe: `core`, `crossing`, `secrets`, `release_waiters`, `release_serving`, `problems.all`, logging, discovery, transfer ids/waiters/inbox) with `.guard()` (uniform mechanical change); verified by `grep -rn 'lock().unwrap()' crates/mouser-core/src --include='*.rs' | grep -v _tests` printing nothing and the core tests passing
- [ ] T056 [P] [US1-core] Write unit tests in `crates/mouser-core/src/crossing/receiver_tests.rs`: with `refuse_input` on key, button, motion and scroll in turn, the receiver skips that input, every later accepted input is injected in order, the link stays up, and a refused release still clears the key/button from the receiver's held set (research V1; Story 1 sc. 1; edge case "held when the operating system starts refusing input")
- [ ] T057 [US1-core] Replace the `.expect` on inject results in `crates/mouser-core/src/crossing/receiver.rs` (lines 85, 127, 170, 188, 193, 203, 209 at bd97abe) with skip-and-continue handling
- [ ] T058 [P] [US1-core] Write unit tests in `crates/mouser-core/src/crossing/controller_tests.rs`: an `Err(Os(_))` from `set_grabbed(true)` when crossing out means the crossing does not happen and the pointer stays local; an `Err` from `warp` is skipped; neither panics (`supported()` no longer asserts `e == Unsupported`)
- [ ] T059 [US1-core] Change `supported()` in `crates/mouser-core/src/crossing/mod.rs` (lines 267–274) and the grab/warp `.expect`s in `crates/mouser-core/src/crossing/controller.rs` (lines 126, 131, 189) to return without crossing on `Os` errors
- [ ] T060 [P] [US1-core] Write unit tests in `crates/mouser-core/src/crossing/controller_tests.rs`: a send to a peer that is no longer in `map.wires` is handled as that peer being `Lost` (control returns locally) instead of panicking
- [ ] T061 [US1-core] Replace the unchecked `map.wires[&peer]` lookups in `crates/mouser-core/src/crossing/controller.rs` (lines 116, 140, 180, 182, 250, 307) with `get` and the `Lost` path
- [ ] T062 [P] [US1-core] Write unit tests in `crates/mouser-core/src/quickjump/tests.rs`: a refused warp or inject during a QuickJump jump is skipped and QuickJump stays usable
- [ ] T063 [US1-core] Replace the `.expect` at `crates/mouser-core/src/quickjump/mod.rs:137` with skip handling
- [ ] T064 [P] [US1-core] Write unit tests in `crates/mouser-core/src/faults_tests.rs`: `faults::run_unit(where, f)` returns `f`'s value when it does not panic; when `f` panics it returns `Faulted`, emits `EngineEvent::Fault { where }`, and the calling loop can continue
- [ ] T065 [US1-core] Implement `run_unit` (a `catch_unwind` boundary) in `crates/mouser-core/src/faults.rs`, add `EngineEvent::Fault { where: String }` in `crates/mouser-core/src/engine.rs`, and declare `faults` in `crates/mouser-core/src/lib.rs`
- [ ] T066 [P] [US1-core] Write unit tests in `crates/mouser-core/src/crossing/recover_tests.rs`: `Crossing::recover_after_fault()` as controller forces state `Local`, calls `set_grabbed(false)` (pointer shown) and sends `ReleaseAll` to the computer it was driving; as receiver releases every injected key and button and drops the receiving session; afterwards "the crossing state is `Local` and nothing is held" (data-model invariant)
- [ ] T067 [US1-core] Implement `Crossing::recover_after_fault()` in `crates/mouser-core/src/crossing/recover.rs`
- [ ] T068 [P] [US1-core] Write unit tests in `crates/mouser-core/src/engine_tests.rs`: a panic inside one `reconcile_loop` tick (driven with `panic_on_next_inject`) runs `recover_after_fault`, and the next tick still runs (pause, panic key and crossing keep working)
- [ ] T069 [US1-core] Wrap each `reconcile_loop` tick (`crates/mouser-core/src/engine.rs:664`) in `run_unit("reconcile", …)` followed by `recover_after_fault` on `Faulted`
- [ ] T070 [P] [US1-core] Write unit tests in `crates/mouser-core/src/session/fault_tests.rs`: a panic while handling one inbound message in `read_loop` or one datagram in `motion_loop` runs `recover_after_fault` and closes that link with close reason `fault`; the 001 redial (R2) reconnects it and the desk user can cross again (Story 1 sc. 7)
- [ ] T071 [US1-core] Wrap per-message handling in `read_loop` and `motion_loop` (`crates/mouser-core/src/session/mod.rs:74,92`) in `run_unit` and close a faulted link with reason `fault`
- [ ] T072 [P] [US1-core] Write unit tests in `crates/mouser-core/src/crossing/input_thread_tests.rs`: a panic while handling one captured event on the `mouser-input` thread runs `recover_after_fault` and the thread keeps handling later events (the panic key still works)
- [ ] T073 [US1-core] Wrap per-event handling on the `mouser-input` thread (spawned at `crates/mouser-core/src/crossing/mod.rs:344`) in `run_unit("mouser-input", …)`
- [ ] T074 [P] [US1-core] Write unit tests in `crates/mouser-core/src/control/server_tests.rs`: a panic while handling one control-API request returns a JSON-RPC `-32603` error for that request and the server keeps serving later requests
- [ ] T075 [US1-core] Wrap each control-API request in `run_unit("control", …)` in `crates/mouser-core/src/control/server.rs`
- [ ] T076 [P] [US1-core] Write unit tests in `crates/mouser-core/src/transport/tests.rs`: a frame decoding to `FrameError::UnknownVariant` is skipped and the link stays up; a `FrameError::Malformed` frame closes the link with close reason `protocol` instead of `panic!` (`transport/mod.rs:352`)
- [ ] T077 [US1-core] Implement skip-unknown and close-on-malformed in the frame reader in `crates/mouser-core/src/transport/mod.rs`
- [ ] T078 [P] [US1-core] Write unit tests in `crates/mouser-core/src/session/fault_tests.rs`: a message that violates the protocol (the case behind `.expect("peer followed the protocol")` at `session/mod.rs:87`) closes that link with reason `protocol` and does not panic
- [ ] T079 [US1-core] Replace the `.expect("peer followed the protocol")` in `crates/mouser-core/src/session/mod.rs` with closing the link with reason `protocol`
- [ ] T080 [P] [US1-core] Write unit tests in `crates/mouser-core/src/engine_tests.rs`: `Ctx::persist` returns a `Result`; when `state.json` is not writable it returns `Err`, the in-memory state is kept and no panic occurs (research V3 item 5)
- [ ] T081 [US1-core] Change `Ctx::persist` (`crates/mouser-core/src/engine.rs:352`) and its ~12 callers to return/propagate `Result` without `expect`
- [ ] T082 [P] [US1-core] Write unit tests in `crates/mouser-core/src/logging/tests.rs`: when the log file cannot be written the record is dropped and nothing panics (research V3 item 6)
- [ ] T083 [US1-core] Replace the writer `expect`s in `crates/mouser-core/src/logging/mod.rs` (lines 139–150) with dropping the record
- [ ] T084 [P] [US1-core] Write unit tests in `crates/mouser-core/src/crossing/capture_lost_tests.rs`: `LocalInput::CaptureLost(reason)` while `Remote(c)` moves the state to `Local` with how `capture_lost` and sends `ReleaseAll` to c; it does not call `set_grabbed` again (the backend already ungrabbed, rule 10) (Story 1 sc. 3)
- [ ] T085 [US1-core] Handle `LocalInput::CaptureLost` in `crates/mouser-core/src/crossing/capture_lost.rs`, replacing the T041 placeholder arm
- [ ] T086 [P] [US1-core] Write unit tests in `crates/mouser-core/src/engine_tests.rs`: `Engine::shutdown()` while `Remote(c)` stops input, returns locally (pointer shown), releases held input, and closes every link; c releases what it holds when the link closes (Story 1 sc. 5)
- [ ] T087 [US1-core] Make `Engine::shutdown()` in `crates/mouser-core/src/engine.rs` run stop-input → return_local(quit) → close links, in that order
- [ ] T088 [US1-core] Gate: `cargo xtask gate --packages mouser-core --lane core` (acc_v2_us1, all 001 acceptance suites and all mouser-core tests pass, 100% coverage)
- [ ] T089 [US1-core] Commit: phase boundary "US1-core" (5.3)

#### Phase 5: US2-core — As a troubleshooting agent, I want the diagnostic log to show what Mouser did and why (P1)

**Independent test**: `cargo test -p mouser-core --test acc_v2_us2 --test contract_log_events --test contract_log_bundle --test contract_log_privacy` passes: every record validates against the closed catalog, files start with `log.header`, rotation keeps the total within `max_log_mb`, and `logs.collect` writes one zip with every computer listed and no marker.

- [ ] T090 [US2-core] Write the acceptance runner `crates/mouser-core/tests/acc_v2_us2.rs` and steps `crates/mouser-core/tests/steps/v2_us2.rs` for every scenario in `tests/features/v2_us2.feature`
- [ ] T091 [US2-core] Write the contract test `crates/mouser-core/tests/contract_log_events.rs` (producer side): a scripted two-node desk session through the harness triggers every core-emitted event of contracts/log-events.md; every line of both nodes' log files validates against the `mouser-proto` schema `schemas/log-events.schema.json` (with `schemas/log-record.schema.json` registered as its referenced resource); every catalog event core emits appears at least once
- [ ] T092 [US2-core] Add `EngineParams.mode: ComputerMode` (`installed | portable | portable_elevated | service | headless`) in `crates/mouser-core/src/engine.rs`, defaulting test harness nodes to `installed`, and report it in `status.computers[]` for this computer (needed by `log.header`, `LogsCard`, `PeerStatus.mode`)
- [ ] T093 [P] [US2-core] Write unit tests in `crates/mouser-core/src/logging/events_tests.rs`: one typed constructor per catalog row of contracts/log-events.md (`log.header`, `logging.enabled/disabled`, `control.changed`, `pointer.hidden`, `pointer.shown`, `input.refused`, `input.refused_run_ended`, `peer.connected`, `peer.disconnected`, `wire.unknown`, `fault`, `fault.recovered`, `display.changed`, `layout.changed`, `setting.changed`, `clipboard.sent/received`, `logs.collected`, `elevation.restart`, `problem.raised/cleared`); each produces exactly the catalog's fields and enum values; no constructor accepts a free-form map (FR-003)
- [ ] T094 [US2-core] Implement the constructors in `crates/mouser-core/src/logging/events.rs` and make `crates/mouser-core/src/logging/mod.rs` accept only records built by them
- [ ] T095 [P] [US2-core] Write unit tests in `crates/mouser-core/src/logging/header_tests.rs`: every newly opened log file, including one opened by rotation, starts with `log.header {mouser_version, os, os_version, computer_id, computer_name, desk_id?, desk_name?, mode, log_path}` (Story 2 sc. 1)
- [ ] T096 [US2-core] Implement header writing on open and after rotation in `crates/mouser-core/src/logging/header.rs`
- [ ] T097 [P] [US2-core] Write unit tests in `crates/mouser-core/src/logging/rotation_tests.rs`: "`mouser.log` is rotated to `mouser.log.1` when the next record would make it exceed `max_log_mb/2` MiB, and the previous `mouser.log.1` is deleted"; "The two files together are always ≤ `max_log_mb` MiB" (FR-008)
- [ ] T098 [US2-core] Implement the two-file rotation in `crates/mouser-core/src/logging/rotation.rs`, replacing the 001 rotation in `logging/mod.rs:142-148`
- [ ] T099 [P] [US2-core] Write unit tests in `crates/mouser-core/src/session/close_reason_tests.rs`: quinn `ConnectionError` / `close_reason()` plus application close bytes map to `LinkLossReason` — `done` or left the desk → `peer_left`; `quit` or `shutdown` (0.2.x) → `peer_quit`; `not a member of this desk` → `removed`; `replaced` → `replaced`; idle timeout or path lost → `unreachable`; protocol mismatch → `incompatible`; undecodable frame → `protocol`; own faulted link task → `fault_here`; own shutdown → `quit_here`
- [ ] T100 [US2-core] Implement `LinkLossReason` and its derivation in `crates/mouser-core/src/session/close_reason.rs`, and pass it to `on_link_closed(peer, link_id, reason)` (`crates/mouser-core/src/session/mod.rs:306`) instead of discarding `ConnectionError` (`transport/mod.rs:354`)
- [ ] T101 [P] [US2-core] Write unit tests in `crates/mouser-core/src/session/close_reason_tests.rs`: this computer sends close reason bytes `quit` (replacing `shutdown`) on shutdown, `fault` for a faulted link, `protocol` for an undecodable frame and `incompatible` for a protocol mismatch; `peer.disconnected {reason}` is written for every lost link (Story 2 sc. 5)
- [ ] T102 [US2-core] Send the new close reasons and write `peer.disconnected` in `crates/mouser-core/src/session/mod.rs`
- [ ] T103 [P] [US2-core] Write unit tests in `crates/mouser-core/src/crossing/state_tests.rs`: every control move writes `control.changed {from, to, how}` with how ∈ {crossing, panic, quickjump, pause, link_lost, capture_lost, elevated_app, fault, quit, entered}; on the computer that is `to`, `app` and `app_admin` come from `foreground_app()` (Story 2 sc. 2)
- [ ] T104 [US2-core] Write `control.changed` from the transitions in `crates/mouser-core/src/crossing/state.rs`
- [ ] T105 [P] [US2-core] Write unit tests in `crates/mouser-core/src/crossing/pointer_tests.rs`: every grab writes `pointer.hidden {reason: crossing_out | quickjump}`; every transition to `Local` shows the pointer first, then writes `pointer.shown {reason: returned | capture_lost | fault | quit | startup_recovery}` (data-model invariant; Story 2 sc. 3)
- [ ] T106 [US2-core] Route every `set_grabbed` call through a logging wrapper in `crates/mouser-core/src/crossing/pointer.rs`
- [ ] T107 [P] [US2-core] Write unit tests in `crates/mouser-core/src/crossing/refusals_tests.rs`: `input.refused {kind, app?, app_admin?}` (peer = the controller) is "Written for the first refusal of a kind after an accepted input of that kind or after control changed"; the next accepted input of that kind, or a control change, writes `input.refused_run_ended {kind, count}` with the run's count ≥ 1; kinds key/button/motion/scroll/grab/warp (Story 2 sc. 4)
- [ ] T108 [US2-core] Implement refusal-run tracking in `crates/mouser-core/src/crossing/refusals.rs` and call it from the skip paths of T057, T059, T063
- [ ] T109 [P] [US2-core] Write unit tests in `crates/mouser-core/src/transport/tests.rs`: a skipped unknown-variant frame writes `wire.unknown {variant}` with the peer set
- [ ] T110 [US2-core] Write `wire.unknown` from the skip path in `crates/mouser-core/src/transport/mod.rs`
- [ ] T111 [P] [US2-core] Write unit tests in `crates/mouser-core/src/faults_tests.rs`: `faults::install(log)` sets a process panic hook that writes `fault {where (thread or task name), location (file:line), message}` when logging is on — `message` is the payload when it is a `&str`/`String`, otherwise `"non-string payload"` — and then calls the previously installed hook (Story 2 sc. 6)
- [ ] T112 [US2-core] Implement `faults::install` in `crates/mouser-core/src/faults.rs`
- [ ] T113 [P] [US2-core] Write unit tests in `crates/mouser-core/src/faults_tests.rs`: after a caught fault, `fault.recovered {where, action: returned_local | closed_link | continued}` is written after the `fault` record (Story 2 sc. 6 "a later record shows Mouser carrying on")
- [ ] T114 [US2-core] Write `fault.recovered` from the recovery paths of `run_unit` callers in `crates/mouser-core/src/faults.rs`
- [ ] T115 [P] [US2-core] Write unit tests in `crates/mouser-core/src/layout/displays_tests.rs`: a display added, removed, rearranged or resized on a computer writes `display.changed {computer, display (name), change}` (Story 2 sc. 7)
- [ ] T116 [US2-core] Write `display.changed` in `crates/mouser-core/src/layout/displays.rs`
- [ ] T117 [P] [US2-core] Write unit tests in `crates/mouser-core/src/control/layout_tests.rs` and `crates/mouser-core/src/layout/autoplace_tests.rs`: `layout.set` writes `layout.changed {by, computer, change: moved | display_removed | display_included}`; auto-placement writes `change: auto_placed` (Story 2 sc. 7)
- [ ] T118 [US2-core] Write `layout.changed` in `crates/mouser-core/src/control/layout.rs` and `crates/mouser-core/src/layout/autoplace.rs`
- [ ] T119 [P] [US2-core] Write unit tests in `crates/mouser-core/src/config/changes_tests.rs`: after any config apply, one `SettingChange {key, old, new, origin}` is produced per key that differs; each is published on the `config` control topic and written as `setting.changed {key, old, new, origin, from?}` (Story 2 sc. 8)
- [ ] T120 [US2-core] Implement the config diff, `config` topic publication and `setting.changed` in `crates/mouser-core/src/config/changes.rs`
- [ ] T121 [P] [US2-core] Write unit tests in `crates/mouser-core/src/control/config_api_tests.rs`: `config.set` records `origin` from its `origin` param (`app` or `cli`, default `cli`)
- [ ] T122 [US2-core] Pass `config.set.origin` into the config apply in `crates/mouser-core/src/control/config_api.rs`
- [ ] T123 [P] [US2-core] Write unit tests in `crates/mouser-core/src/config/watch_tests.rs`: a file-watch reload of `config.toml` produces changes with origin `file`
- [ ] T124 [US2-core] Apply file reloads with origin `file` in `crates/mouser-core/src/config/watch.rs`
- [ ] T125 [P] [US2-core] Write unit tests in `crates/mouser-core/src/clipboard/watch_tests.rs`: each `ClipboardOffer` sent writes `clipboard.sent {from, to, reps: [{kind: text|html|rtf|png|files, size}]}` — size in bytes, or item count for files — and never content (Story 2 sc. 9; FR-003)
- [ ] T126 [US2-core] Write `clipboard.sent` in `crates/mouser-core/src/clipboard/watch.rs`
- [ ] T127 [P] [US2-core] Write unit tests in `crates/mouser-core/src/clipboard/apply_tests.rs`: each applied offer writes `clipboard.received` with the same field shape
- [ ] T128 [US2-core] Write `clipboard.received` in `crates/mouser-core/src/clipboard/apply.rs`
- [ ] T129 [P] [US2-core] Write unit tests in `crates/mouser-core/src/session/handshake_tests.rs`: `peer.connected` carries the peer's `version` and `os`
- [ ] T130 [US2-core] Add `version` and `os` to `peer.connected` in `crates/mouser-core/src/session/handshake.rs`
- [ ] T131 [US2-core] Add `zip = { version = "=9.1.2", default-features = false, features = ["deflate"] }` to `crates/mouser-core/Cargo.toml` and `Cargo.lock` (research "Dependencies")
- [ ] T132 [P] [US2-core] Write unit tests in `crates/mouser-core/src/logs/serve_tests.rs`: on `LogsRequest {request}` from an authed desk member (the `ReleaseFetch` check), the peer opens one uni stream with id `logs_stream_id(request)`, a `u32` BE header length, a postcard `LogsPart {card, config_toml (verbatim), logging, files}` and then each file's bytes in order; with logging off `files` is empty and `logging = false`; a requester not authed on this desk gets nothing
- [ ] T133 [US2-core] Implement the logs stream producer in `crates/mouser-core/src/logs/serve.rs` and route its streams through `file_loop` (`crates/mouser-core/src/session/mod.rs:100`)
- [ ] T134 [P] [US2-core] Write unit tests in `crates/mouser-core/src/logs/collect_tests.rs`: the requester sends `LogsRequest` only to connected peers with `Hello.mouser_version ≥ V2_SINCE`, parks one waiter per asked peer, and records each computer per the collection states — stream complete → `included`; not connected or link lost mid-stream → `offline`; peer < 0.3.0 → `older_version` (never requested, card built from what the requester knows); `logging = false` → `included = false, reason logging_off` with card and settings still present; no timer (waits end on stream finish or link loss)
- [ ] T135 [US2-core] Implement collection in `crates/mouser-core/src/logs/collect.rs` and declare `logs` in `crates/mouser-core/src/lib.rs`
- [ ] T136 [P] [US2-core] Write unit tests in `crates/mouser-core/src/logs/bundle_tests.rs`: the zip is named `mouser-logs-<desk>-<yyyymmddThhmmssZ>.zip` with every desk-name character outside `[A-Za-z0-9_-]` replaced by `-` (or `no-desk`); written to `dir` if given, otherwise the user's Downloads folder, or `<machine dir>/bundles/` for mode `service`/`--machine`; an existing name gets ` (2)`, ` (3)` … before `.zip`; layout `bundle.json`, `<name>-<id[0..8]>/computer.json`, `config.toml`, `mouser.log`, `mouser.log.1` (when it exists), `service.log` (Windows installed, when included); the requesting computer is listed first; never `state.json`, any `secrets/` file, the desk key or the device key (FR-007)
- [ ] T137 [US2-core] Implement the zip writer in `crates/mouser-core/src/logs/bundle.rs` (deflate only)
- [ ] T138 [P] [US2-core] Write unit tests in `crates/mouser-core/src/control/logs_tests.rs`: `logs.collect {dir?}` returns `{path, computers: [{id, name, included, reason?}]}`, writes `logs.collected {path, included, missing: [{computer, reason}]}`, and returns `-32603` with the OS reason in `message` when the bundle cannot be written
- [ ] T139 [US2-core] Implement the `logs.collect` method in `crates/mouser-core/src/control/logs.rs` and register it in `crates/mouser-core/src/control/server.rs`
- [ ] T140 [US2-core] Write the contract test `crates/mouser-core/tests/contract_log_bundle.rs` (producer side): a three-node desk (one at 0.2.8, one with logging off) collects a bundle whose `bundle.json` validates against the `mouser-proto` schema `schemas/log-bundle.schema.json`, whose `included: false` entries all carry a `reason`, and which contains none of the never-present files
- [ ] T141 [US2-core] Write the privacy contract test `crates/mouser-core/tests/contract_log_privacy.rs` (SC-004): a two-node session with logging on in which the marker `MOUSER-MARKER-7f3a` is typed (as key events) and copied (as clipboard text and as a copied file name) on both nodes, then a bundle is collected; zero matches of the marker across every log file and every entry of the unzipped bundle
- [ ] T142 [US2-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [ ] T143 [US2-core] Commit: phase boundary "US2-core" (5.3)

#### Phase 6: US3-core — As a desk owner, I want one Mouser window where I can find everything (P2)

**Independent test**: `cargo test -p mouser-core --test acc_v2_us3` passes: status v2 lists each problem once under its computer, limitations only in `computers[].limitations`, `tray_summary.connection` follows the rule, and `layout.keep_placement` clears the review.

- [ ] T144 [US3-core] Write the acceptance runner `crates/mouser-core/tests/acc_v2_us3.rs` and steps `crates/mouser-core/tests/steps/v2_us3.rs` for every scenario in `tests/features/v2_us3.feature`
- [ ] T145 [P] [US3-core] Write unit tests in `crates/mouser-core/src/problems/limitations_tests.rs`: start-up `PlatformInfo.limitations` are never raised as problems ("The ids `platform_limitation:<capability>` are never raised in v2"); they appear only in `status.computers[].limitations` of their own computer (FR-010; Story 3 sc. 6)
- [ ] T146 [US3-core] Delete the `platform_limitation:<capability>` raise path in `crates/mouser-core/src/problems/limitations.rs` (lines 9–43 at bd97abe), keeping the listing
- [ ] T147 [P] [US3-core] Write unit tests in `crates/mouser-core/src/problems/peer_status_tests.rs`: problems with ids `platform_limitation:<capability>` arriving from a 0.2.x peer are dropped on merge; this computer sends `PeerStatus.mode` only to peers ≥ `V2_SINCE` and the 001 shape to older peers; a peer's `mode` appears in its `status.computers[].mode`
- [ ] T148 [US3-core] Implement the merge filter and version-gated `PeerStatus` in `crates/mouser-core/src/problems/peer_status.rs`
- [ ] T149 [P] [US3-core] Write unit tests in `crates/mouser-core/src/problems/tests.rs`: "Problems are unique per `(problem_id, computer)` everywhere"; the same `problem_id` on two computers is two problems, each attributed to its own computer; `status.problems[]` carries `problem_id` (Story 3 sc. 5)
- [ ] T150 [US3-core] Key the registry and the status merge by `(problem_id, computer)` in `crates/mouser-core/src/problems/mod.rs`
- [ ] T151 [P] [US3-core] Write unit tests in `crates/mouser-core/src/control/tray_connection_tests.rs` for `TrayConnection`: "no desk, or desk with no other member → alone"; "every non-removed member is connected (paused counts) → connected"; "≥1 non-removed member is reconnecting/offline/incompatible → disconnected"; pausing this computer does not change it; a removed member or leaving the desk never counts (Story 3 sc. 14–15; edge case paused + disconnected)
- [ ] T152 [US3-core] Implement `TrayConnection` computation in `crates/mouser-core/src/control/tray_connection.rs`
- [ ] T153 [P] [US3-core] Write unit tests in `crates/mouser-core/src/control/status_tests.rs`: `status` validates against status v2 — `tray_summary {state, connection, computers, displays}`, `active_computer` always filled (this computer's id when Local), `computers[].os_version`, `computers[].mode`, `computers[].version` for every computer, `rtt_ms` raw (Story 3 sc. 4, 8)
- [ ] T154 [US3-core] Build status v2 in `crates/mouser-core/src/control/status.rs`
- [ ] T155 [P] [US3-core] Write unit tests in `crates/mouser-core/src/control/layout_tests.rs`: `layout.keep_placement {computer}` clears `needs_review` for that computer's displays without moving them, replicates via DeskState, and writes `layout.changed change=kept_placement` (Story 3 sc. 11)
- [ ] T156 [US3-core] Implement `layout.keep_placement` in `crates/mouser-core/src/control/layout.rs`
- [ ] T157 [P] [US3-core] Write unit tests in `crates/mouser-core/src/session/handshake_tests.rs`: the `incompatible_version` problem names which computer to update; it carries fix `update_computer:<computer id>` only when the incompatible computer is this one and an update is available locally (Story 3 sc. 8)
- [ ] T158 [US3-core] Add the `update_computer` fix in `crates/mouser-core/src/session/handshake.rs`
- [ ] T159 [US3-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [ ] T160 [US3-core] Commit: phase boundary "US3-core" (5.3)

#### Phase 7: US4-core — As a desk owner, I want to tailor hotkeys, names and pointer behavior from the Mouser window (P2)

**Independent test**: `cargo test -p mouser-core --test acc_v2_us4` passes: a rename from any node reaches every node (including one offline during it), `DeskSettings` reaches ≥ 0.3.0 peers only, landing and modifier mapping follow it, and a changed panic key works without restart.

- [ ] T161 [US4-core] Write the acceptance runner `crates/mouser-core/tests/acc_v2_us4.rs` and steps `crates/mouser-core/tests/steps/v2_us4.rs` for every scenario in `tests/features/v2_us4.feature`
- [ ] T162 [P] [US4-core] Write unit tests in `crates/mouser-core/src/desk/rename_tests.rs`: `desk.rename {computer, name}` trims the name and requires "1–64 chars, trimmed", sets `Member.display_name` in `DeskState` with `epoch + 1` and broadcasts it; status, problems, notifications and `layout.get` use the member name; an invalid name returns `-32602`; with no desk it returns `-32001` `data.kind = join_refused` (Story 4 sc. 3)
- [ ] T163 [US4-core] Implement `desk.rename` in `crates/mouser-core/src/desk/rename.rs` and register the method in `crates/mouser-core/src/control/server.rs`
- [ ] T164 [P] [US4-core] Write unit tests in `crates/mouser-core/src/desk/replication_tests.rs`: setting `general.computer_name` renames this computer desk-wide (empty means the OS name); adopting a winning `DeskState` that renamed this computer writes the new name into this computer's `general.computer_name` with origin `desk_mate`; a computer offline during the rename adopts the higher epoch on reconnect (edge case)
- [ ] T165 [US4-core] Implement rename adoption and `general.computer_name` write-back in `crates/mouser-core/src/desk/replication.rs`
- [ ] T166 [P] [US4-core] Write unit tests in `crates/mouser-core/src/desk/settings_tests.rs`: a change to `crossing.landing_highlight` or `keyboard.command_as_control` sends `DeskSettings {epoch, author, landing_highlight, command_as_control}` to every authed peer ≥ `V2_SINCE`, and after each new connection; "The higher `(epoch, author)` wins"; the receiver adopts it, writes `config.toml` and logs `setting.changed origin=desk_mate from=<author>`; a 0.2.8 peer never receives it (FR-012)
- [ ] T167 [US4-core] Implement `DeskSettings` send/adopt in `crates/mouser-core/src/desk/settings.rs`, replacing the T041 placeholder arm
- [ ] T168 [P] [US4-core] Write unit tests in `crates/mouser-core/src/crossing/receiver_tests.rs`: with `crossing.landing_highlight = false` no `Landing` event is emitted on `Enter`; with `true` (default) it is (Story 4 sc. 4)
- [ ] T169 [US4-core] Suppress the `Landing` event when the highlight is off in `crates/mouser-core/src/crossing/receiver.rs`
- [ ] T170 [P] [US4-core] Write unit tests in `crates/mouser-core/src/crossing/modifiers_tests.rs`: with `keyboard.command_as_control = true` a Mac controller's Command acts as Control on Windows/Linux (001 behaviour); with `false`, "Command maps to the Windows/Super key and Control to Control" (Story 4 sc. 5–6)
- [ ] T171 [US4-core] Apply `map_usage` only when `command_as_control` is on in `crates/mouser-core/src/crossing/modifiers.rs` (lines 18–32)
- [ ] T172 [P] [US4-core] Write unit tests in `crates/mouser-core/src/crossing/panic_tests.rs`: changing `hotkeys.panic` (format `<Key>*<count>`, count 1–5) through `config.set` takes effect without restart; the previous panic key no longer returns control (Story 4 sc. 1)
- [ ] T173 [US4-core] Re-read the panic key on each `SettingChange` for `hotkeys.panic` in `crates/mouser-core/src/crossing/panic.rs`
- [ ] T174 [P] [US4-core] Write unit tests in `crates/mouser-core/src/quickjump/tests.rs`: changing `hotkeys.quickjump` takes effect without restart and the previous chord no longer shows the letters (Story 4 sc. 2)
- [ ] T175 [US4-core] Re-read the QuickJump chord on each `SettingChange` in `crates/mouser-core/src/quickjump/mod.rs`
- [ ] T176 [P] [US4-core] Write unit tests in `crates/mouser-core/src/clipboard/targeted_tests.rs`: changing `hotkeys.targeted_paste` takes effect without restart and the previous chord no longer pastes (Story 4 sc. 2)
- [ ] T177 [US4-core] Re-read the targeted-paste chord on each `SettingChange` in `crates/mouser-core/src/clipboard/targeted.rs`
- [ ] T178 [US4-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [ ] T179 [US4-core] Commit: phase boundary "US4-core" (5.3)

#### Phase 8: US5-core — As a desk user, I want to control a Windows computer at its login screen, elevation prompts and administrator applications (P2)

**Independent test**: `cargo test -p mouser-core --test acc_v2_us5` passes: a portable receiving node whose foreground outranks Mouser hands control back with `Panic` and raises one `elevated_app` problem shown under it on the controller; `elevation.restart` reaches the right node; installed and elevated nodes keep receiving input.

- [ ] T180 [US5-core] Write the acceptance runner `crates/mouser-core/tests/acc_v2_us5.rs` and steps `crates/mouser-core/tests/steps/v2_us5.rs` for every scenario in `tests/features/v2_us5.feature`
- [ ] T181 [P] [US5-core] Write unit tests in `crates/mouser-core/src/crossing/elevated_tests.rs`: on each reconcile tick while receiving from k, if `foreground_app()` has `outranks_mouser = true` the receiver releases what it holds, sends `Panic` to k only, and writes `control.changed how=elevated_app`; k returns to Local and sends `ReleaseAll` (001 semantics, works with peers ≥ 0.2.6); when `outranks_mouser = false` (installed, or `admin` but not outranking) input keeps arriving (Story 5 sc. 4–7)
- [ ] T182 [US5-core] Implement the receiver hand-back in `crates/mouser-core/src/crossing/elevated.rs`, called from the reconcile tick
- [ ] T183 [P] [US5-core] Write unit tests in `crates/mouser-core/src/problems/elevated_app_tests.rs`: the hand-back raises `elevated_app` on this computer with `computer` = this computer, `message` = "Mouser on <computer> can't control <app> because it runs as administrator.", `app` = the program name; fix `restart_elevated:<this id>` when mode is `portable`; with no fix the message also names installing Mouser; it reaches the controller through `PeerStatus` and is shown there under this computer; it clears when control next enters this computer without an outranking foreground (Story 5 sc. 6)
- [ ] T184 [US5-core] Implement the `elevated_app` problem in `crates/mouser-core/src/problems/elevated_app.rs`, replacing the T041 placeholder arm
- [ ] T185 [P] [US5-core] Write unit tests in `crates/mouser-core/src/control/elevation_tests.rs`: `elevation.restart {computer}` for this computer emits `EngineEvent::RestartElevated`; for a peer ≥ 0.3.0 sends wire `RestartElevated`; returns `-32001` `data.kind=platform_limitation` when the target is not portable Windows or is a peer older than 0.3.0 (FR-005)
- [ ] T186 [US5-core] Implement `elevation.restart` and `EngineEvent::RestartElevated` in `crates/mouser-core/src/control/elevation.rs` and `crates/mouser-core/src/engine.rs`
- [ ] T187 [P] [US5-core] Write unit tests in `crates/mouser-core/src/session/elevation_tests.rs`: an inbound `RestartElevated` raises `EngineEvent::RestartElevated` when this computer is portable Windows and not elevated, and is ignored otherwise
- [ ] T188 [US5-core] Handle inbound `RestartElevated` in `crates/mouser-core/src/session/mod.rs`, replacing the T041 placeholder arm
- [ ] T189 [P] [US5-core] Write unit tests in `crates/mouser-core/src/control/server_tests.rs`: `control::default_path()` returns `\\.\pipe\mouser-service` when that pipe exists and otherwise the per-user path, which on Windows is `\\.\pipe\mouser-<USERNAME>` (contract text corrected, G27); an engine started with mode `service` serves the service pipe with a DACL allowing SYSTEM, Administrators and the console session's interactive user, `PIPE_REJECT_REMOTE_CLIENTS` and `FILE_FLAG_FIRST_PIPE_INSTANCE` (Windows-only tests behind `cfg(windows)`)
- [ ] T190 [US5-core] Implement `default_path()` selection and the service-pipe listener in `crates/mouser-core/src/control/server.rs` and `crates/mouser-core/src/control/mod.rs`
- [ ] T191 [US5-core] Gate: `cargo xtask gate --packages mouser-core --lane core` (including the Windows runner for the `cfg(windows)` tests)
- [ ] T192 [US5-core] Commit: phase boundary "US5-core" (5.3)

### Lane: app

Package `apps/mouser-app` (Tauri 2.12.5 host + `ui/` static ES modules). Gate per phase: `cargo xtask gate --packages mouser-app --lane app` (Rust tests + `node --test` UI tests, 100% coverage both). Each UI module exports pure render/update functions tested with happy-dom; the Tauri side keeps logic in testable modules. Each app phase starts after the same-numbered core phase is merged (app tests use `mouser-core` + `VirtualPlatform`).

#### Phase 9: US1-app — As a desk user, I want my pointer and keyboard to keep working whatever goes wrong inside Mouser (P1) 🎯 MVP

**Independent test**: app tests pass: the panic hook is installed at start, every exit path runs `Host::shutdown()` once before the process ends, and `--cursor-guard <pid>` starts no window.

- [ ] T193 [P] [US1-app] Write unit tests in `apps/mouser-app/src/app_tests.rs`: app start and headless start call `mouser_core::faults::install(log)` before the engine starts
- [ ] T194 [US1-app] Install the panic hook at start in `apps/mouser-app/src/app.rs` and `apps/mouser-app/src/headless.rs`
- [ ] T195 [P] [US1-app] Write unit tests in `apps/mouser-app/src/exit_tests.rs`: the `RunEvent::Exit` handler calls `Host::shutdown()` (covers `app.exit`, the updater restart and logoff/shutdown via `WM_ENDSESSION`); tray Quit calls `Host::shutdown()` *before* `app.exit(0)` (tauri#12310); `Host::shutdown()` runs `Engine::shutdown()` once even when called twice (research V4; Story 1 sc. 5)
- [ ] T196 [US1-app] Implement the exit handling in `apps/mouser-app/src/exit.rs` and wire it in `apps/mouser-app/src/shell.rs` (replacing the bare `app.exit(0)` at line 306 and the `ExitRequested`-only handler at lines 348–353) and `apps/mouser-app/src/host.rs`
- [ ] T197 [P] [US1-app] Write unit tests in `apps/mouser-app/src/app_tests.rs`: a lagged engine broadcast receiver resyncs from `status` instead of panicking (`app.rs:72,137`; FR-002)
- [ ] T198 [US1-app] Replace the lag panics in `apps/mouser-app/src/app.rs` with a resync from `status`
- [ ] T199 [P] [US1-app] Write unit tests in `apps/mouser-app/src/args_tests.rs`: `--cursor-guard <pid>` is parsed before any Tauri initialisation and selects guard mode (Windows only; on other OSes the flag is rejected with exit code 3)
- [ ] T200 [US1-app] Implement argument dispatch in `apps/mouser-app/src/args.rs` and in `apps/mouser-app/src/main.rs` call `mouser_platform_windows::cursor_guard::run(pid)` for guard mode with no window (depends on Phase 19 T335)
- [ ] T201 [US1-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [ ] T202 [US1-app] Commit: phase boundary "US1-app" (5.3)

#### Phase 10: US2-app — As a troubleshooting agent, I want the diagnostic log to show what Mouser did and why (P1)

**Independent test**: app tests pass: the engine is started with the right `ComputerMode`.

- [ ] T203 [P] [US2-app] Write unit tests in `apps/mouser-app/src/mode_tests.rs`: the app passes `EngineParams.mode` = `portable` when `MouserData/` is beside the executable, `portable_elevated` when portable and the process token is elevated (Windows), `headless` for `--headless`, `service` for `--headless --machine`, otherwise `installed`
- [ ] T204 [US2-app] Implement mode selection in `apps/mouser-app/src/mode.rs` and pass it from `apps/mouser-app/src/host.rs` and `apps/mouser-app/src/headless.rs`
- [ ] T205 [US2-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [ ] T206 [US2-app] Commit: phase boundary "US2-app" (5.3)

#### Phase 11: US3-app — As a desk owner, I want one Mouser window where I can find everything (P2)

**Independent test**: `node --test` passes `acc_v2_us3.test.js`, `nav.test.js` and `search_index.test.js`; Rust tests pass `windows_tests.rs` and `tray_tests.rs`: one `main` window navigated by hash route, notices once per `(problem_id, computer)`, rounded numbers, tray left-click pause and connection badges.

- [ ] T207 [US3-app] Write UI acceptance tests `apps/mouser-app/ui/acc_v2_us3.test.js`, one test per Story 3 scenario 1, 2, 3, 5, 6, 7, 9, 10, 11, 12, 13, 14, 15, 17 named with its Given/When/Then text verbatim from spec.md, rendering `ui/index.html` with happy-dom over `ui/status_fixture.js` fixtures (two Macs, a Windows and a Linux computer; a 0.2.8 peer)
- [ ] T208 [P] [US3-app] Write unit tests in `apps/mouser-app/src/windows_tests.rs` (producer side of contracts/ui-navigation.md): `navigate(route)` unminimizes, shows and focuses `main` and emits `mouser://navigate {route}`; if `main` does not exist it is created at `index.html#<route>` (960×680, resizable, minimum 720×520); no call ever creates a second normal window; closing hides it; on macOS the activation policy is Regular while shown and Accessory while hidden
- [ ] T209 [US3-app] Rewrite `apps/mouser-app/src/windows.rs` as one `main` window with `navigate(Section)`, deleting `Which` and the per-window plumbing for `settings`, `arranger`, `health`, `join`, `transfer`
- [ ] T210 [US3-app] Update `apps/mouser-app/tauri.conf.json` and `apps/mouser-app/capabilities/default.json` to declare only `main` (`index.html`, 960×680, min 720×520), `landing` and `quickjump-*`
- [ ] T211 [US3-app] Rename `apps/mouser-app/src/tray_model_tests.rs` to `apps/mouser-app/src/tray_tests.rs` (it tests `tray.rs`) and update its module declaration (debt Tier 2)
- [ ] T212 [US3-app] Split `apps/mouser-app/src/shell.rs` into `apps/mouser-app/src/shell.rs` (builder and plugins) and `apps/mouser-app/src/tray_runtime.rs` (the tray menu closure and tray event handling), behaviour unchanged; verified by the existing app tests passing (debt Tier 2)
- [ ] T213 [P] [US3-app] Write unit tests in `apps/mouser-app/src/tray_tests.rs`: menu items are the summary, the version/update line, Open Mouser (`#desk`, or `#welcome` while setup is needed), Pause/Resume, Arrange displays (`#displays`), Mouser health (`#health`), Send Ctrl-Alt-Del (when available), Quit; each navigating item calls `windows::navigate`; an unknown menu id is ignored instead of panicking (`tray.rs:266`)
- [ ] T214 [US3-app] Implement the menu and its navigation in `apps/mouser-app/src/tray.rs`
- [ ] T215 [P] [US3-app] Write unit tests in `apps/mouser-app/src/tray_tests.rs` (needs probe P4, T011): the icon is chosen from `tray_summary.state` × `tray_summary.connection` × theme (light, dark, macOS template); the tooltip names both states, e.g. "Mouser is paused · not connected to Beta" (Story 3 sc. 14–15)
- [ ] T216 [US3-app] Implement icon and tooltip selection in `apps/mouser-app/src/tray.rs`
- [ ] T217 [P] [US3-app] Write unit tests in `apps/mouser-app/src/tray_tests.rs` (needs P4): the tray is built with `show_menu_on_left_click(false)`; `TrayIconEvent::Click { button: Left, button_state: Up }` toggles `active.pause` / `active.resume`; right click opens the menu (Story 3 sc. 13)
- [ ] T218 [US3-app] Implement click handling in `apps/mouser-app/src/tray_runtime.rs`
- [ ] T219 [P] [US3-app] Write unit tests in `apps/mouser-app/src/tray_icons_tests.rs`: an icon file exists under `apps/mouser-app/icons/` for every {active, paused} × {connected, disconnected, alone} × {light, dark, template} variant; converted to greyscale, the connected, disconnected and alone variants of each state differ pixel-wise (SC-008)
- [ ] T220 [US3-app] Generate the tray PNGs with `apps/mouser-app/scripts/make-tray-icons.mjs` into `apps/mouser-app/icons/tray-<theme>-<state>-<connection>.png`: disconnected adds a hollow ring with a slash (red in colour builds), connected adds a small filled dot (green in colour builds), alone has no badge; template variants are monochrome
- [ ] T221 [P] [US3-app] Write unit tests in `apps/mouser-app/src/notify_tests.rs`: `Toasts` are keyed by `(problem_id, computer)`; a toast names its computer; the startup-limitation exclusions (`notify.rs:42-48`) are gone because limitations never arrive as problems; the `elevated_app` toast navigates to `#health/<computer id>`
- [ ] T222 [US3-app] Re-key and attribute toasts in `apps/mouser-app/src/notify.rs`
- [ ] T223 [P] [US3-app] Write unit tests in `apps/mouser-app/src/host_tests.rs`: a second launch, a `join_request`, a transfer question, `ReviewOffered` and the crossing-blocked fix each call `windows::navigate` with `#desk`, `#desk`, `#desk`, `#displays`, `#displays` respectively
- [ ] T224 [US3-app] Replace per-window opens with `windows::navigate` in `apps/mouser-app/src/host.rs`
- [ ] T225 [P] [US3-app] Write unit tests in `apps/mouser-app/src/commands_tests.rs`: `reveal_in_file_manager(path)` opens the OS file manager at `path` (Finder, Explorer, xdg-open of the folder) through a seam
- [ ] T226 [US3-app] Implement `reveal_in_file_manager` in `apps/mouser-app/src/commands.rs`
- [ ] T227 [P] [US3-app] Write UI tests `apps/mouser-app/ui/nav.test.js` (consumer side of contracts/ui-navigation.md): the sidebar shows "Your desk" with Desk, Displays, Health; "Configure" with Settings; "Support" with Diagnostics and Help; routes `#desk` (default), `#displays`, `#health`, `#health/<id>` (scrolls to and highlights that entry), `#settings`, `#settings/<key>`, `#diagnostics`, `#help`, `#welcome` (sidebar hidden; goes to `#desk` when finished); a `mouser://navigate {route}` event switches the section in place
- [ ] T228 [US3-app] Implement `apps/mouser-app/ui/index.html` and `apps/mouser-app/ui/nav.js` (sidebar with the search field at the top, sections grouped under headings, home overview first; layout after `reference/fluid-voice-style.png`, FR-011)
- [ ] T229 [P] [US3-app] Write UI tests `apps/mouser-app/ui/search_index.test.js`: entries are `{route, title, section, keywords[]}`, one per section and one per setting control; case-insensitive substring match on title and keywords; choosing a result navigates; completeness contract — every config key reachable in the UI has an entry and every route is at most one sidebar click plus at most one in-section control from any section (SC-005)
- [ ] T230 [US3-app] Implement `apps/mouser-app/ui/search_index.js` and the search results list in `apps/mouser-app/ui/nav.js`
- [ ] T231 [P] [US3-app] Write UI tests `apps/mouser-app/ui/format.test.js`: durations in whole milliseconds or seconds (`8.707351000000001` → "9 ms"); sizes in whole or one-decimal units; display scale as a percentage (FR-009)
- [ ] T232 [US3-app] Implement `apps/mouser-app/ui/format.js`
- [ ] T233 [P] [US3-app] Write UI tests `apps/mouser-app/ui/labels.test.js`: one table of connection texts and permission labels used by every section
- [ ] T234 [US3-app] Implement `apps/mouser-app/ui/labels.js`, moving the duplicated tables out of `health.js:6-21`, `desk_settings.js:7-13`, `first_run.js:6-31` (debt G24)
- [ ] T235 [P] [US3-app] Write UI tests `apps/mouser-app/ui/fixes.test.js`: a problem's fix renders as one action — `open_system_settings` calls `open_permission_settings`, `keep_placement:<id>` calls `layout.keep_placement`, `update_computer:<id>` calls `update.apply`
- [ ] T236 [US3-app] Implement `apps/mouser-app/ui/fixes.js`
- [ ] T237 [P] [US3-app] Write UI tests `apps/mouser-app/ui/sections/desk.test.js`: Desk shows the desk name and, per computer, friendly name, OS, version (both versions next to a computer whose version differs from this one), connected or not, whether it has the pointer (from `active_computer`), and a count of its open problems linking to `#health/<id>` (Story 3 sc. 4, 8); members with Remove, Leave, Move to another desk
- [ ] T238 [US3-app] Implement the Desk overview in `apps/mouser-app/ui/sections/desk.js`
- [ ] T239 [P] [US3-app] Write UI tests `apps/mouser-app/ui/sections/desk_join.test.js`: a `join_request` shows an approval card "Allow <computer> to join <desk>?" with Allow and Decline calling `join.decide` (001 FR-013, now in Desk)
- [ ] T240 [US3-app] Implement the approval card in `apps/mouser-app/ui/sections/desk_join.js`
- [ ] T241 [P] [US3-app] Write UI tests `apps/mouser-app/ui/sections/desk_transfer.test.js`: transfer progress (current item, bytes, rate, ETA rounded per `format.js`) and the overwrite question render as a panel in Desk (001 Story 7)
- [ ] T242 [US3-app] Implement the transfer panel in `apps/mouser-app/ui/sections/desk_transfer.js`
- [ ] T243 [P] [US3-app] Write UI tests `apps/mouser-app/ui/sections/displays.test.js`: every display tile shows its computer name and display name in full, wrapping; a tile too small for its name gets an external label with a leader line; the full names are always in the tile's accessible name and tooltip; desk-wide `block_fullscreen` and `equalize_dimensions` and the edge options are here (Story 3 sc. 10)
- [ ] T244 [US3-app] Implement the Displays section in `apps/mouser-app/ui/sections/displays.js`, reusing `apps/mouser-app/ui/arranger.js` and `apps/mouser-app/ui/arranger_edges.js`
- [ ] T245 [P] [US3-app] Write UI tests `apps/mouser-app/ui/sections/displays_review.test.js`: when a computer's displays have `needs_review`, a message names the computer, says its displays were placed automatically and should be checked against the real desk, and offers "Keep this placement" (calls `layout.keep_placement {computer}`); the message goes away when kept or moved (Story 3 sc. 11)
- [ ] T246 [US3-app] Implement the review message in `apps/mouser-app/ui/sections/displays_review.js`
- [ ] T247 [P] [US3-app] Write UI tests `apps/mouser-app/ui/sections/health.test.js`: one entry per computer with connection, responsiveness as "Responds quickly (9 ms)" (whole ms), this computer's permissions each granted or not with "Open settings", its problems once each with its fix (keyed by `(problem_id, computer)`, never under another computer), and "What doesn't work here" listing `computers[].limitations` of that computer only (Story 3 sc. 5–7, 9; FR-010)
- [ ] T248 [US3-app] Implement the Health section in `apps/mouser-app/ui/sections/health.js`
- [ ] T249 [P] [US3-app] Write UI tests `apps/mouser-app/ui/sections/welcome.test.js`: first run runs the 001 permission step and desk step inside the window at `#welcome` and then goes to `#desk` (Story 3 sc. 9)
- [ ] T250 [US3-app] Implement `apps/mouser-app/ui/sections/welcome.js` from the content of `apps/mouser-app/ui/first_run.js`
- [ ] T251 [P] [US3-app] Write UI tests `apps/mouser-app/ui/sections/diagnostics.test.js`: the logging switch calls `logging.set`; "Open log location" reveals the log folder; "Collect logs from the desk" calls `logs.collect`, shows one line per computer (included, or not included with offline / logging off / older version), and reveals the bundle with `reveal_in_file_manager` (Story 3 sc. 12)
- [ ] T252 [US3-app] Implement `apps/mouser-app/ui/sections/diagnostics.js`
- [ ] T253 [P] [US3-app] Write UI tests `apps/mouser-app/ui/markdown.test.js`: the renderer handles headings, paragraphs, lists and code (blocks and inline) and escapes HTML
- [ ] T254 [US3-app] Implement the Markdown subset renderer `apps/mouser-app/ui/markdown.js`
- [ ] T255 [P] [US3-app] Write UI tests `apps/mouser-app/ui/sections/help.test.js`: Help shows this computer's Mouser version and renders `ui/help/troubleshooting.md` (Story 3 sc. 17)
- [ ] T256 [US3-app] Implement `apps/mouser-app/ui/sections/help.js` and make `apps/mouser-app/scripts/stage-ui.sh` copy the repository's `docs/troubleshooting.md` into the staged `ui/help/`
- [ ] T257 [P] [US3-app] Write UI tests in `apps/mouser-app/ui/notify.test.js`: banners key by `(problem_id, computer)`, carry the computer's name, and appear only in Health and as counts on Desk
- [ ] T258 [US3-app] Re-key and attribute banners in `apps/mouser-app/ui/notify.js` (lines 19–50)
- [ ] T259 [US3-app] Delete the removed pages and their tests now that their content lives in sections (uniform mechanical change): `apps/mouser-app/ui/first_run.html`, `first_run.js`, `first_run.test.js`, `desk_settings.html`, `desk_settings.js`, `desk_settings.test.js`, `health.html`, `health.js`, `health.test.js`, `arranger.html`, `join_prompt.html`, `join_prompt.js`, `join_prompt.test.js`, `transfer.html`, `transfer.js`, `transfer.test.js` (all under `apps/mouser-app/ui/`); the per-page 2–3 s `status` polling goes with them (one subscription in `nav.js` replaces it). `input_settings.*` are deleted in T271
- [ ] T260 [US3-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [ ] T261 [US3-app] Commit: phase boundary "US3-app" (5.3)

#### Phase 12: US4-app — As a desk owner, I want to tailor hotkeys, names and pointer behavior from the Mouser window (P2)

**Independent test**: `node --test` passes `acc_v2_us4.test.js`; Rust tests show hotkeys re-registered on every `config` notification and an OS refusal reported, not panicking.

- [ ] T262 [US4-app] Write UI acceptance tests `apps/mouser-app/ui/acc_v2_us4.test.js`, one test per Story 4 scenario 1–6 as seen in the window, named with its Given/When/Then text verbatim from spec.md
- [ ] T263 [P] [US4-app] Write UI tests `apps/mouser-app/ui/key_recorder.test.js`: recording a chord captures the pressed modifiers + key as an accelerator string; recording the panic key captures one key plus a press count 1–5 as `<Key>*<count>`; Esc cancels recording
- [ ] T264 [US4-app] Implement `apps/mouser-app/ui/key_recorder.js`
- [ ] T265 [P] [US4-app] Write UI tests `apps/mouser-app/ui/sections/settings_names.test.js`: every desk computer's friendly name is editable from this window and saving calls `desk.rename {computer, name}` (1–64 characters, trimmed; an invalid name shows the error next to the field) (Story 4 sc. 3)
- [ ] T266 [US4-app] Implement friendly names in `apps/mouser-app/ui/sections/settings_names.js`
- [ ] T267 [P] [US4-app] Write UI tests `apps/mouser-app/ui/sections/settings_hotkeys.test.js`: panic key, QuickJump, enable/disable and targeted-paste recorders call `config.set` with `origin: "app"`; a hotkey the OS refused shows the refusal next to that hotkey (Story 4 sc. 1–2)
- [ ] T268 [US4-app] Implement `apps/mouser-app/ui/sections/settings_hotkeys.js`
- [ ] T269 [P] [US4-app] Write UI tests `apps/mouser-app/ui/sections/settings.test.js`: Settings shows the landing highlight toggle, "Use Command as Control on Windows and Linux", mouse buttons 4/5, start at login, notifications and Advanced (port, allowlist), each writing its config key with `origin: "app"`; `#settings/<key>` focuses that control (Story 4 sc. 4–6)
- [ ] T270 [US4-app] Implement `apps/mouser-app/ui/sections/settings.js` composing `settings_names.js` and `settings_hotkeys.js`
- [ ] T271 [US4-app] Delete `apps/mouser-app/ui/input_settings.html`, `apps/mouser-app/ui/input_settings.js` and `apps/mouser-app/ui/input_settings.test.js` (content moved to Settings)
- [ ] T272 [P] [US4-app] Write UI tests in `apps/mouser-app/ui/api.test.js`: every `config.set` the UI sends carries `origin: "app"`
- [ ] T273 [US4-app] Add `origin: "app"` in `apps/mouser-app/ui/api.js`
- [ ] T274 [P] [US4-app] Write unit tests in `apps/mouser-app/src/hotkeys_tests.rs`: a `config` topic notification for any `hotkeys.*` key from any origin (app, cli, file, desk_mate) re-registers OS hotkeys (`commands.rs:47-58` re-registers only after a UI `config.set` today)
- [ ] T275 [US4-app] Re-register hotkeys from the `config` topic in `apps/mouser-app/src/hotkeys.rs` and remove the re-registration from `apps/mouser-app/src/commands.rs`
- [ ] T276 [P] [US4-app] Write unit tests in `apps/mouser-app/src/hotkeys_tests.rs`: a hotkey the OS refuses to register is reported to the UI for display next to that hotkey and nothing panics (`shell.rs:91`; FR-002)
- [ ] T277 [US4-app] Report registration refusals in `apps/mouser-app/src/hotkeys.rs` and remove the panic in `apps/mouser-app/src/shell.rs`
- [ ] T278 [P] [US4-app] Write unit tests in `apps/mouser-app/src/hotkeys_tests.rs`: only `hotkeys.enable_disable` is registered with the OS; QuickJump and targeted paste are not (core detects both in the captured stream)
- [ ] T279 [US4-app] Delete the dead QuickJump and targeted-paste OS registrations (`shell.rs:262-266`) from `apps/mouser-app/src/hotkeys.rs` and `apps/mouser-app/src/shell.rs`
- [ ] T280 [US4-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [ ] T281 [US4-app] Commit: phase boundary "US4-app" (5.3)

#### Phase 13: US5-app — As a desk user, I want to control a Windows computer at its login screen, elevation prompts and administrator applications (P2)

**Depends on**: probe P3 (T010), Phase 8 (US5-core), Phase 22 (US5-platform-windows, for `service_client`).

**Independent test**: app tests pass for the elevated restart sequence behind a seam, `--wait-for`, UI mode when `MouserService` runs, and `--headless --machine` on Windows.

- [ ] T282 [P] [US5-app] Write unit tests in `apps/mouser-app/src/elevation_tests.rs` (behind a `Shell` seam, never elevating in tests): the restart runs `ShellExecuteExW(verb="runas", exe, "--wait-for <pid>")`; `ERROR_CANCELLED` changes nothing, the problem stays, and `elevation.restart {outcome: declined}` is logged; acceptance logs `prompted` then `handed_over`, runs `Host::shutdown()` and exits; the data folder stays `MouserData/` (FR-005; states `idle → prompting → handing_over → elevated_running`, declined → `idle`)
- [ ] T283 [US5-app] Implement the restart sequence in `apps/mouser-app/src/elevation.rs` and run it on `EngineEvent::RestartElevated` from `apps/mouser-app/src/host.rs`
- [ ] T284 [P] [US5-app] Write unit tests in `apps/mouser-app/src/args_tests.rs`: `--wait-for <pid>` waits for that process to exit before any Tauri initialisation, so the single-instance plugin's mutex is free (Windows only)
- [ ] T285 [US5-app] Implement `--wait-for` in `apps/mouser-app/src/args.rs` and `apps/mouser-app/src/main.rs`
- [ ] T286 [P] [US5-app] Write UI tests in `apps/mouser-app/ui/fixes.test.js`: the `restart_elevated:<id>` fix runs the restart locally when `<id>` is this computer and calls `elevation.restart {computer: id}` otherwise; the message says "Approve the Windows prompt on <computer>" (OD-1)
- [ ] T287 [US5-app] Add `restart_elevated` to `apps/mouser-app/ui/fixes.js`
- [ ] T288 [P] [US5-app] Write unit tests in `apps/mouser-app/src/ui_mode_tests.rs`: when `MouserService` is installed and running, the tray app starts in UI mode with no engine of its own and talks to the session engine over `\\.\pipe\mouser-service` (through a service-query seam)
- [ ] T289 [US5-app] Implement UI-mode detection in `apps/mouser-app/src/ui_mode.rs` and the remote control client in `apps/mouser-app/src/host.rs`
- [ ] T290 [P] [US5-app] Write unit tests in `apps/mouser-app/src/headless_tests.rs`: `--headless --machine` on Windows runs the engine with the machine folder `C:\ProgramData\Mouser`, machine-scope secrets and mode `service`; it connects the service IPC client, runs `Engine::shutdown` and exits on `Shutdown`, and sends `Stopping {reason: Quit | Fault}` before a voluntary exit
- [ ] T291 [US5-app] Implement Windows `--headless --machine` in `apps/mouser-app/src/headless.rs` using `mouser_platform_windows::service_client`
- [ ] T292 [US5-app] Gate: `cargo xtask gate --packages mouser-app --lane app` (including the Windows runner)
- [ ] T293 [US5-app] Commit: phase boundary "US5-app" (5.3)

#### Phase 14: US6-app — As a desk owner, I want Mouser to run without its window or tray on Windows and Linux (P3)

**Independent test**: `cargo test -p mouser-app --test acc_v2_us6` passes on the Windows and Ubuntu runners.

- [ ] T294 [US6-app] Write the acceptance runner `apps/mouser-app/tests/acc_v2_us6.rs` (cucumber 0.23 dev-dependency in `apps/mouser-app/Cargo.toml`, `harness = false`, `fail_on_skipped()`) reading the feature file at `../../crates/mouser-core/tests/features/v2_us6.feature`, with steps in `apps/mouser-app/tests/steps/v2_us6.rs` that start the headless host over `VirtualPlatform` alongside a second in-process engine
- [ ] T295 [P] [US6-app] Write unit tests in `apps/mouser-app/src/headless_tests.rs`: `--headless` on Windows and Linux no longer refuses (supersedes 001 T516); it uses the current user's folder, or `MouserData/` when portable; no window and no tray are created; on Linux `--headless --machine` uses `/var/lib/mouser`
- [ ] T296 [US6-app] Remove the refusal (`headless.rs:22-27`) and implement folder selection in `apps/mouser-app/src/headless.rs`
- [ ] T297 [P] [US6-app] Write unit tests in `apps/mouser-app/src/headless_tests.rs`: SIGTERM and Ctrl-C run `Engine::shutdown` before exit on all three OSes
- [ ] T298 [US6-app] Implement signal handling in `apps/mouser-app/src/headless.rs`
- [ ] T299 [US6-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [ ] T300 [US6-app] Commit: phase boundary "US6-app" (5.3)

### Lane: cli

Package `apps/mouser-cli`. Gate per phase: `cargo xtask gate --packages mouser-cli --lane cli`. Tests run the binary against an in-process engine from `mouser-core` serving the control socket in a temp dir (consumer side of the control/status v2 and log-bundle contracts). Each cli phase starts after the same-numbered core phase is merged.

#### Phase 15: US2-cli — As a troubleshooting agent, I want the diagnostic log to show what Mouser did and why (P1)

- [ ] T301 [P] [US2-cli] Write tests `apps/mouser-cli/tests/contract_cli_logs_collect.rs`: `mouser logs collect [--out DIR] [--json]` calls `logs.collect`; text output is the bundle path, then one line per computer `<name>: included` or `<name>: not included (<offline|logging off|older version>)`; `--json` prints the method's result; exit 2 when the engine is not running (Story 2 sc. 10)
- [ ] T302 [US2-cli] Implement `logs collect` in `apps/mouser-cli/src/logs.rs`
- [ ] T303 [P] [US2-cli] Write tests in `apps/mouser-cli/tests/contract_cli_config.rs`: `mouser config set` sends `origin: "cli"`, and the change appears as `setting.changed origin=cli` (Story 2 sc. 8)
- [ ] T304 [US2-cli] Send `origin: "cli"` in `apps/mouser-cli/src/config.rs`
- [ ] T305 [P] [US2-cli] Write tests in `apps/mouser-cli/tests/contract_cli_config.rs`: offline `mouser config set hotkeys.enable_disable <bad chord>` exits 3 with `hotkeys.enable_disable: <chord parse reason>`
- [ ] T306 [US2-cli] Validate offline `config set` with `Config::validate` in `apps/mouser-cli/src/config.rs`
- [ ] T307 [US2-cli] Gate: `cargo xtask gate --packages mouser-cli --lane cli`
- [ ] T308 [US2-cli] Commit: phase boundary "US2-cli" (5.3)

#### Phase 16: US3-cli — As a desk owner, I want one Mouser window where I can find everything (P2)

- [ ] T309 [P] [US3-cli] Write tests in `apps/mouser-cli/tests/contract_cli_status.rs`: `mouser status` text shows each computer's friendly name, version and mode and the tray connection state; `mouser status --json` validates against the `mouser-proto` schema `schemas/status.schema.json` (v2) (Story 3 sc. 4, 8)
- [ ] T310 [US3-cli] Update the text output in `apps/mouser-cli/src/status.rs`
- [ ] T311 [US3-cli] Gate: `cargo xtask gate --packages mouser-cli --lane cli`
- [ ] T312 [US3-cli] Commit: phase boundary "US3-cli" (5.3)

#### Phase 17: US4-cli — As a desk owner, I want to tailor hotkeys, names and pointer behavior from the Mouser window (P2)

- [ ] T313 [P] [US4-cli] Write tests in `apps/mouser-cli/tests/contract_cli_desk.rs`: `mouser desk rename <computer> <name>` calls `desk.rename`, where `<computer>` is a name or an id; an unknown computer exits 3; afterwards `mouser status` shows the new name (Story 4 sc. 3)
- [ ] T314 [US4-cli] Implement `desk rename` in `apps/mouser-cli/src/desk.rs`
- [ ] T315 [US4-cli] Gate: `cargo xtask gate --packages mouser-cli --lane cli`
- [ ] T316 [US4-cli] Commit: phase boundary "US4-cli" (5.3)

#### Phase 18: US5-cli — As a desk user, I want to control a Windows computer at its login screen, elevation prompts and administrator applications (P2)

- [ ] T317 [P] [US5-cli] Write tests `apps/mouser-cli/tests/contract_cli_endpoint.rs`: the CLI connects to `\\.\pipe\mouser-service` when it exists and otherwise to the per-user pipe `\\.\pipe\mouser-<USERNAME>`, using `mouser_core::control::default_path()` (Windows-only cases behind `cfg(windows)`)
- [ ] T318 [US5-cli] Use `mouser_core::control::default_path()` in `apps/mouser-cli/src/paths.rs`
- [ ] T319 [US5-cli] Gate: `cargo xtask gate --packages mouser-cli --lane cli` (including the Windows runner)
- [ ] T320 [US5-cli] Commit: phase boundary "US5-cli" (5.3)

### Lane: platform-windows

Package `crates/mouser-platform-windows` (`windows` crate 0.62.2). Gate per phase: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` **on the Windows runner** (driven by the Windows Claude session). Reference: research V5, V7, V9, V10; probe findings in `specs/002-mouser-v2/spikes/FINDINGS.md` (Phase 2). A refuted probe stops the lane and returns to the plan.

#### Phase 19: US1-platform-windows — As a desk user, I want my pointer and keyboard to keep working whatever goes wrong inside Mouser (P1)

**Depends on**: probe P1 (T008).

- [ ] T321 [P] [US1-platform-windows] Write `crates/mouser-platform-windows/tests/contract_platform_windows_v2.rs` running conformance rules 10 and 11 against `WinPlatform`, with the backend's own "pointer hidden" query (`cursor.rs` blank-arrow check) for rule 10
- [ ] T322 [P] [US1-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/uipi_tests.rs`: `window_elevated` returns a value instead of `.expect` when the process or token cannot be opened (`uipi.rs:54-62`); the module comment states what the code does (`uipi.rs:1-3` is false today, G15)
- [ ] T323 [US1-platform-windows] Fix `crates/mouser-platform-windows/src/uipi.rs`
- [ ] T324 [P] [US1-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/foreground_tests.rs`: `foreground_app()` returns `ForegroundApp { name }` with the executable file name only from `GetForegroundWindow` → process id → `QueryFullProcessImageNameW`; `admin` is true for an elevated or SYSTEM token; `outranks_mouser` is true only when the app's integrity level is above Mouser's own (so false when Mouser runs as SYSTEM or elevated); `None` when there is no foreground window (contracts/platform-api-delta.md)
- [ ] T325 [US1-platform-windows] Implement `foreground_app()` in `crates/mouser-platform-windows/src/foreground.rs` and wire it into `crates/mouser-platform-windows/src/lib.rs`, replacing the T040 placeholder
- [ ] T326 [P] [US1-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/capture_tests.rs`: the hook thread counts every event its hooks see, not only probe-tagged ones (`capture.rs:347-352`)
- [ ] T327 [US1-platform-windows] Count all hook events in `crates/mouser-platform-windows/src/capture.rs`
- [ ] T328 [P] [US1-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/watchdog_tests.rs` through the `HookApi` seam: a cycle is healthy if a probe *or* any real event was seen since the last cycle; a miss is counted only when the system's last-input tick (`GetLastInputInfo`) advanced and the hook saw nothing; a swallowed probe with real input arriving is healthy (research V7; no new timer, the existing 5 s cycle)
- [ ] T329 [US1-platform-windows] Implement real-input judgement in `crates/mouser-platform-windows/src/watchdog.rs` (lines 186–251)
- [ ] T330 [P] [US1-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/foreground_hook_tests.rs`: an out-of-context `EVENT_SYSTEM_FOREGROUND` WinEvent hook on the hook thread re-evaluates `foreground_app()` on each foreground change
- [ ] T331 [US1-platform-windows] Implement the WinEvent hook in `crates/mouser-platform-windows/src/foreground_hook.rs`
- [ ] T332 [P] [US1-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/capture_lost_tests.rs`: while grabbed, the backend ungrabs and shows the pointer itself *first*, then sends `LocalInput::CaptureLost(reason)`, when (a) the foreground outranks Mouser (each watchdog cycle and on the WinEvent) → `ElevatedForeground`, (b) `OpenInputDesktop` is not `Default` → `OtherDesktop`, (c) one real miss → `HooksSilent`; the ungrab does not wait for core (Story 1 sc. 3; rule 10)
- [ ] T333 [US1-platform-windows] Implement capture-loss detection in `crates/mouser-platform-windows/src/capture_lost.rs`
- [ ] T334 [P] [US1-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/cursor_guard_tests.rs`: `cursor_guard::run(parent_pid)` waits on the parent's process handle; when the parent ends for any reason and the arrow is the blank cursor (the `recover_at_startup` test, `cursor.rs:121-126`) it restores with `SPI_SETCURSORS` and writes `pointer.shown {reason: guard}` through `crate::diag` when logging is on, then exits; if the arrow is not blank it only exits (research V5; Story 1 sc. 6)
- [ ] T335 [US1-platform-windows] Implement `cursor_guard::run` in `crates/mouser-platform-windows/src/cursor_guard.rs` and export it from `crates/mouser-platform-windows/src/lib.rs`
- [ ] T336 [P] [US1-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/capture_tests.rs`: capture start spawns `<current exe> --cursor-guard <own pid>` in the same session with no window; a normal capture stop ends the guard
- [ ] T337 [US1-platform-windows] Spawn and stop the guard in `crates/mouser-platform-windows/src/capture.rs`
- [ ] T338 [US1-platform-windows] Rename the `#[allow(dead_code)]` field on `LiveInput` (`crates/mouser-platform-windows/src/lib.rs:384`) to `_guard` and delete the allow (debt G16); verified by clippy with `-D warnings`
- [ ] T339 [US1-platform-windows] Wait for the Explorer child in test teardown in `crates/mouser-platform-windows/src/drag_tests.rs` and delete `#[allow(clippy::zombie_processes)]` (line 699) (debt Tier 1); verified by clippy
- [ ] T340 [US1-platform-windows] Gate: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` on the Windows runner
- [ ] T341 [US1-platform-windows] Commit: phase boundary "US1-platform-windows" (5.3)

#### Phase 20: US2-platform-windows — As a troubleshooting agent, I want the diagnostic log to show what Mouser did and why (P1)

- [ ] T342 [P] [US2-platform-windows] Write the contract test `crates/mouser-platform-windows/tests/contract_log_events_windows.rs` (consumer side of the log-events catalog): every event name and field the `diag` sink can write — `pointer.hidden`, `pointer.shown`, `capture.lost {reason: elevated_foreground|other_desktop|hooks_silent, foreground?}`, `capture.desktop {desktop: default|winlogon|other}`, `capture.probe` (with outcome `live`), `capture.reinstall`, `capture.hook_thread`, `capture.start` — validates against the `mouser-proto` schema `schemas/log-events.schema.json`
- [ ] T343 [US2-platform-windows] Constrain `crates/mouser-platform-windows/src/diag.rs` to catalog names and fields (closed per event)
- [ ] T344 [P] [US2-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/capture_tests.rs`: grabbing writes `pointer.hidden` and ungrabbing writes `pointer.shown` with the catalog reasons; a capture loss writes `capture.lost` with the foreground program name only; a watchdog cycle judged healthy on real input writes `capture.probe` outcome `live` (Story 2 sc. 3)
- [ ] T345 [US2-platform-windows] Write those records from `crates/mouser-platform-windows/src/capture.rs` and `crates/mouser-platform-windows/src/capture_lost.rs`
- [ ] T346 [US2-platform-windows] Gate: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` on the Windows runner
- [ ] T347 [US2-platform-windows] Commit: phase boundary "US2-platform-windows" (5.3)

#### Phase 21: US3-platform-windows — As a desk owner, I want one Mouser window where I can find everything (P2)

- [ ] T348 [P] [US3-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/watchdog_tests.rs`: with probes swallowed by another LL hook installed after Mouser's (the P1 `swallow` mode modelled through `HookApi`) while real input keeps arriving, no "lost its hold" (`LOST_HOLD`) problem is reported through `watch_problems`; it is reported only after real-input misses (Story 3 sc. 16)
- [ ] T349 [US3-platform-windows] Report `LOST_HOLD` only after real-input misses in `crates/mouser-platform-windows/src/watchdog.rs`
- [ ] T350 [US3-platform-windows] Gate: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` on the Windows runner
- [ ] T351 [US3-platform-windows] Commit: phase boundary "US3-platform-windows" (5.3)

#### Phase 22: US5-platform-windows — As a desk user, I want to control a Windows computer at its login screen, elevation prompts and administrator applications (P2)

**Depends on**: probes P1 (T008) and P2 (T009).

- [ ] T352 [US5-platform-windows] Check probe P2 item (4) in `specs/002-mouser-v2/spikes/FINDINGS.md`: if an Explorer file drop from the SYSTEM process failed, stop this lane and escalate to the owner (research V9 risk: the installed engine would state the 001 limitation 3 text for drag on Windows); otherwise record "drag as SYSTEM: confirmed" (verification only)
- [ ] T353 [P] [US5-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/desktop_tests.rs` through a seam: the hook thread polls `OpenInputDesktop(DF_ALLOWOTHERACCOUNTHOOK, …)` each existing watchdog cycle; on a change (`Default` ↔ `Winlogon` ↔ other) it calls `SetThreadDesktop`, reinstalls the hooks, re-attaches injection, and writes `capture.desktop {desktop}` (research V9; Story 5 sc. 1–2)
- [ ] T354 [US5-platform-windows] Implement desktop following in `crates/mouser-platform-windows/src/desktop.rs` and call it from the hook thread in `crates/mouser-platform-windows/src/capture.rs`
- [ ] T355 [P] [US5-platform-windows] Write `crates/mouser-platform-windows/tests/contract_service_ipc.rs` (consumer side): the client's `Hello`, `SendSas`, `Stopping` frames and the service's `SasResult`, `Shutdown` frames round-trip against `mouser_proto::service_ipc` with the 4-byte BE + postcard framing
- [ ] T356 [P] [US5-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/service_client_tests.rs`: the client connects to `engine_pipe_name(<console session id>)` and sends `Hello {version}` first; `Shutdown` from the service invokes the registered shutdown callback; `stopping(reason)` sends `Stopping {reason}`; a dropped pipe makes later requests return `Err(Os)` and nothing else
- [ ] T357 [US5-platform-windows] Implement `crates/mouser-platform-windows/src/service_client.rs` and export it from `crates/mouser-platform-windows/src/lib.rs`
- [ ] T358 [P] [US5-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/inject_tests.rs`: with a service client present, `secure_attention()` sends `SendSas` and maps `SasResult::Sent` → `Ok(())`, `PolicyBlocked` and `Failed(_)` → `Err(Os(..))`; without a service client it keeps the 001 behaviour (Story 5 sc. 3)
- [ ] T359 [US5-platform-windows] Route `secure_attention()` through the service client in `crates/mouser-platform-windows/src/inject.rs`
- [ ] T360 [P] [US5-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/limitations_tests.rs`: `PlatformInfo.limitations` carry, verbatim — portable and not elevated: "Portable Mouser can't control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator." (`SecureAttention`); portable and elevated: "Portable Mouser can't control the login screen, permission prompts or Ctrl-Alt-Del." (`SecureAttention`); when `SoftwareSASGeneration` reads as neither 1 nor 3: "Ctrl-Alt-Del is blocked by this PC's policy." (`SecureAttention`); every build: "Windows warns that Mouser comes from an unknown publisher when it is installed or first run." (`Install`) (Story 5 sc. 3, 8; FR-004)
- [ ] T361 [US5-platform-windows] Implement the mode-dependent limitations in `crates/mouser-platform-windows/src/limitations.rs` and use them in `crates/mouser-platform-windows/src/lib.rs`
- [ ] T362 [US5-platform-windows] Gate: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` on the Windows runner
- [ ] T363 [US5-platform-windows] Commit: phase boundary "US5-platform-windows" (5.3)

### Lane: platform-macos

Package `crates/mouser-platform-macos`. Gate per phase: `cargo xtask gate --packages mouser-platform-macos --lane platform-macos` **on the `macos-14` CI runner only** — its suites drive real input and never run from an agent session on the owner's Mac.

#### Phase 23: US1-platform-macos — As a desk user, I want my pointer and keyboard to keep working whatever goes wrong inside Mouser (P1)

- [ ] T364 [P] [US1-platform-macos] Write `crates/mouser-platform-macos/tests/contract_platform_macos_v2.rs` running conformance rules 10 and 11 against `MacPlatform` (rule 10 query: the backend's cursor-hidden state)
- [ ] T365 [P] [US1-platform-macos] Write unit tests in `crates/mouser-platform-macos/src/foreground_tests.rs`: `foreground_app()` returns `NSWorkspace.frontmostApplication`'s localized name with `admin = false` and `outranks_mouser = false`; `None` with no frontmost application; `CaptureLost` is never emitted (contracts/platform-api-delta.md)
- [ ] T366 [US1-platform-macos] Implement `foreground_app()` in `crates/mouser-platform-macos/src/foreground.rs` and wire it into `crates/mouser-platform-macos/src/lib.rs`, replacing the T040 placeholder
- [ ] T367 [US1-platform-macos] Gate: `cargo xtask gate --packages mouser-platform-macos --lane platform-macos` on the `macos-14` CI runner
- [ ] T368 [US1-platform-macos] Commit: phase boundary "US1-platform-macos" (5.3)

### Lane: platform-linux

Package `crates/mouser-platform-linux` (`x11` and `wayland` modules). Gate per phase: `cargo xtask gate --packages mouser-platform-linux --lane platform-linux` **on the Ubuntu runner** (X11 under Xvfb).

#### Phase 24: US1-platform-linux — As a desk user, I want my pointer and keyboard to keep working whatever goes wrong inside Mouser (P1)

- [ ] T369 [P] [US1-platform-linux] Write `crates/mouser-platform-linux/tests/contract_platform_linux_v2.rs` running conformance rules 10 and 11 against the X11 backend under Xvfb (rule 10 query: XFixes cursor-hidden state)
- [ ] T370 [P] [US1-platform-linux] Write unit tests in `crates/mouser-platform-linux/src/x11/foreground_tests.rs`: `foreground_app()` follows `_NET_ACTIVE_WINDOW` → `_NET_WM_PID` → `/proc/<pid>/comm` for `name`; `admin` is true when the process euid is 0; `outranks_mouser = false`; `None` without an active window
- [ ] T371 [US1-platform-linux] Implement X11 `foreground_app()` in `crates/mouser-platform-linux/src/x11/foreground.rs` and wire it into `crates/mouser-platform-linux/src/lib.rs`, replacing the T040 placeholder
- [ ] T372 [P] [US1-platform-linux] Write unit tests in `crates/mouser-platform-linux/src/lib_tests.rs`: on Wayland `foreground_app()` returns `None`; neither backend emits `CaptureLost`
- [ ] T373 [US1-platform-linux] Return `None` for Wayland in `crates/mouser-platform-linux/src/wayland/mod.rs`
- [ ] T374 [US1-platform-linux] Gate: `cargo xtask gate --packages mouser-platform-linux --lane platform-linux` on the Ubuntu runner
- [ ] T375 [US1-platform-linux] Commit: phase boundary "US1-platform-linux" (5.3)

#### Phase 25: US3-platform-linux — As a desk owner, I want one Mouser window where I can find everything (P2)

- [ ] T376 [P] [US3-platform-linux] Write unit tests in `crates/mouser-platform-linux/src/tray_tests.rs`: with a tray that emits no click events (libappindicator) the limitation "Clicking the tray icon doesn't pause Mouser here; use Pause in its menu." is listed under `Capability::Tray`; the 001 no-tray limitation keeps its text and moves from `Capability::Capture` to `Capability::Tray` (`tray.rs:24`) (spec Assumptions; Story 3 sc. 13)
- [ ] T377 [US3-platform-linux] Implement the tray limitations in `crates/mouser-platform-linux/src/tray.rs`
- [ ] T378 [US3-platform-linux] Gate: `cargo xtask gate --packages mouser-platform-linux --lane platform-linux` on the Ubuntu runner
- [ ] T379 [US3-platform-linux] Commit: phase boundary "US3-platform-linux" (5.3)

#### Phase 26: US6-platform-linux — As a desk owner, I want Mouser to run without its window or tray on Windows and Linux (P3)

- [ ] T380 [P] [US6-platform-linux] Write unit tests in `crates/mouser-platform-linux/src/lib_tests.rs`: every Linux session (X11 and Wayland) lists the limitation "Mouser can't control this computer's login screen." under `Capability::LoginScreen` (Story 6 sc. 2; FR-006)
- [ ] T381 [US6-platform-linux] Add the login-screen limitation in `crates/mouser-platform-linux/src/lib.rs`
- [ ] T382 [US6-platform-linux] Gate: `cargo xtask gate --packages mouser-platform-linux --lane platform-linux` on the Ubuntu runner
- [ ] T383 [US6-platform-linux] Commit: phase boundary "US6-platform-linux" (5.3)

### Lane: service-win

Package `apps/mouser-service-win` (built as `mouser-service.exe`; `MouserService`, LocalSystem; plan Complexity Tracking). Gate per phase: `cargo xtask gate --packages mouser-service-win --lane service-win` **on the Windows runner**. Every Windows API call goes through the `WinApi` seam (fake session ids, fake process handles, fake registry), following the 001 `HookApi` seam pattern in the Windows watchdog; the real implementation is exercised by probe P2 and Story 7 rows R8/R9 (plan 4.1 row).

#### Phase 27: US5-service-win — As a desk user, I want to control a Windows computer at its login screen, elevation prompts and administrator applications (P2)

**Depends on**: probe P2 (T009).

- [ ] T384 [US5-service-win] Add dependencies to `apps/mouser-service-win/Cargo.toml`: `windows-service = "=0.8.1"`, `windows = "=0.62.2"` with features `Win32_System_Services`, `Win32_System_RemoteDesktop`, `Win32_Security`, `Win32_System_Pipes`, `Win32_System_Threading`, plus `mouser-proto`, `mouser-core`, and cucumber 0.23 as a dev-dependency; set the binary name to `mouser-service`
- [ ] T385 [US5-service-win] Declare the `WinApi` trait (console session id, winlogon token duplication, `CreateProcessAsUserW`, process wait/exit code, named pipe create/connect/client PID, `SendSAS`, registry read/write, SCM create/delete/start/stop, firewall rule add/remove) and its real implementation in `apps/mouser-service-win/src/winapi.rs` (declarations and pass-through only)
- [ ] T386 [US5-service-win] Write the acceptance runner `apps/mouser-service-win/tests/acc_v2_us5_service.rs` (`harness = false`, `fail_on_skipped()`) and steps `apps/mouser-service-win/tests/steps/v2_us5_service.rs` for every scenario in `tests/features/v2_us5_service.feature`, over a fake `WinApi`
- [ ] T387 [P] [US5-service-win] Write `apps/mouser-service-win/tests/contract_service_ipc.rs` (producer side): the service's frames (`SasResult`, `Shutdown`) and the engine's frames (`Hello`, `SendSas`, `Stopping`) round-trip against `mouser_proto::service_ipc`
- [ ] T388 [P] [US5-service-win] Write unit tests in `apps/mouser-service-win/src/ipc_tests.rs`: the pipe is `engine_pipe_name(session)` created with security descriptor exactly `D:P(A;;GA;;;SY)`, `PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_TYPE_BYTE | PIPE_REJECT_REMOTE_CLIENTS`; after `ConnectNamedPipe` a client whose `GetNamedPipeClientProcessId` differs from the launched PID is disconnected; a `Hello` with a different version is logged and `SendSas` is still served
- [ ] T389 [US5-service-win] Implement the pipe server in `apps/mouser-service-win/src/ipc.rs`
- [ ] T390 [P] [US5-service-win] Write unit tests in `apps/mouser-service-win/src/session_tests.rs` for `ServiceState`: "`Starting → WaitingForSession (session id 0xFFFFFFFF) → Running(session, pid)`"; on a console session change "`Running → Relaunching → Running`" (send `Shutdown`, wait for exit, new pipe, launch); "If the engine exits: `Running → Relaunching`" with no back-off and no timer; on stop "`Stopping` (it sends `Shutdown` and ends the engine)"; each transition logs `service.session` / `service.engine_started` / `service.engine_exited` (Story 5 sc. 2)
- [ ] T391 [US5-service-win] Implement the state machine in `apps/mouser-service-win/src/session.rs`
- [ ] T392 [P] [US5-service-win] Write unit tests in `apps/mouser-service-win/src/launch_tests.rs`: the engine is launched in the console session as `mouser-app.exe --headless --machine` (the executable beside `mouser-service.exe`) with `CreateProcessAsUserW`, a duplicate of that session's `winlogon.exe` token, and `lpDesktop = "winsta0\\default"`, after the pipe exists (Story 5 sc. 1)
- [ ] T393 [US5-service-win] Implement the launch in `apps/mouser-service-win/src/launch.rs`
- [ ] T394 [P] [US5-service-win] Write unit tests in `apps/mouser-service-win/src/sas_tests.rs`: `SendSas` reads `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System\SoftwareSASGeneration`; a value other than 1 or 3 answers `SasResult::PolicyBlocked`; otherwise it calls `SendSAS(FALSE)` and answers `Sent`, or `Failed(reason)`; it logs `service.sas {result}` (Story 5 sc. 3)
- [ ] T395 [US5-service-win] Implement `SendSas` handling in `apps/mouser-service-win/src/sas.rs`
- [ ] T396 [P] [US5-service-win] Write unit tests in `apps/mouser-service-win/src/log_tests.rs`: the service writes `service.log` in the machine folder `C:\ProgramData\Mouser` starting with `log.header` (mode `service`) and rotating under the same two-file `max_log_mb/2` rule as the engine; only `service.*` catalog events are written
- [ ] T397 [US5-service-win] Implement the service log in `apps/mouser-service-win/src/log.rs`
- [ ] T398 [P] [US5-service-win] Write unit tests in `apps/mouser-service-win/src/service_tests.rs`: the SCM entry dispatches `SERVICE_CONTROL_SESSIONCHANGE`, `SERVICE_CONTROL_STOP` and `SERVICE_CONTROL_PRESHUTDOWN` to the state machine and reports `SERVICE_RUNNING` / `SERVICE_STOPPED`
- [ ] T399 [US5-service-win] Implement the SCM entry point with `windows-service` in `apps/mouser-service-win/src/service.rs`
- [ ] T400 [P] [US5-service-win] Write unit tests in `apps/mouser-service-win/src/install_tests.rs`: `--install` creates `MouserService` (LocalSystem, automatic start, display name "Mouser"), sets `SoftwareSASGeneration=1`, adds the firewall rule for UDP 47801 and starts the service; it exits 0, or 1 with the reason on stderr (FR-013)
- [ ] T401 [US5-service-win] Implement `--install` in `apps/mouser-service-win/src/install.rs`
- [ ] T402 [P] [US5-service-win] Write unit tests in `apps/mouser-service-win/src/migrate_tests.rs`: during `--install`, a readable user-scope identity (device key, desk key, `state.json`) of the installing user is copied to machine scope with `Engine::export_machine_scope`; when it is not readable the session engine starts without a desk and the computer rejoins with one approval (research V12a; OD-2)
- [ ] T403 [US5-service-win] Implement identity migration in `apps/mouser-service-win/src/migrate.rs`
- [ ] T404 [P] [US5-service-win] Write unit tests in `apps/mouser-service-win/src/install_tests.rs`: `--uninstall` stops and deletes the service, ends the session engine, removes the firewall rule and the `SoftwareSASGeneration` value Mouser set, and exits 0 even if the service was absent (FR-013)
- [ ] T405 [US5-service-win] Implement `--uninstall` in `apps/mouser-service-win/src/install.rs`
- [ ] T406 [P] [US5-service-win] Write unit tests in `apps/mouser-service-win/src/args_tests.rs`: `--install`, `--uninstall`, and no arguments (run under the SCM) select the matching entry; any other argument exits 3
- [ ] T407 [US5-service-win] Replace the stub `fn main() {}` in `apps/mouser-service-win/src/main.rs` with argument dispatch in `apps/mouser-service-win/src/args.rs`
- [ ] T408 [US5-service-win] Gate: `cargo xtask gate --packages mouser-service-win --lane service-win` on the Windows runner
- [ ] T409 [US5-service-win] Commit: phase boundary "US5-service-win" (5.3)

---

## Phase 28: Convergence

**Purpose**: Cross-package work, packaging, and the feature's E2E obligation (J1 re-run, plan "E2E": no new journey), after every lane has passed its gates (5.2). Runs in the main tree.

### Cross-package tasks

- [ ] T410 Verify every lane's last phase gate passed: each `specs/002-mouser-v2/evidence/<lane>.jsonl` (core, app, cli, platform-windows, platform-macos, platform-linux, service-win) has a passing record newer than the lane's last commit (verification only)
- [ ] T411 Remove every `// v2: replaced in <task id>` placeholder left by T040/T041 that a lane did not already replace; verified by `grep -rn 'v2: replaced in\|v2: real foreground_app' crates apps` printing nothing and `cargo test --workspace --no-run` compiling on all three runners
- [ ] T412 Configure the Windows installer in `apps/mouser-app/tauri.conf.json`: NSIS `installMode: "perMachine"` (one UAC approval), `installerHooks: "windows/hooks.nsh"`, and `mouser-service` as an `externalBin` sidecar (the existing `binaries/` mechanism) (research V12; FR-013)
- [ ] T413 Write `apps/mouser-app/windows/hooks.nsh`: `NSIS_HOOK_POSTINSTALL` runs `"$INSTDIR\mouser-service.exe" --install` and aborts the install with its stderr on a non-zero exit; `NSIS_HOOK_PREUNINSTALL` runs `"$INSTDIR\mouser-service.exe" --uninstall` (research V12; P2 item 5)
- [ ] T414 Extend `apps/mouser-app/scripts/build-sidecar.sh` to build `mouser-service-win` for the Windows target and place it as `apps/mouser-app/binaries/mouser-service-<target triple>.exe`
- [ ] T415 Update `.github/workflows/release.yml`: the Windows job builds the `mouser-service` sidecar before bundling, keeps the build unsigned (FR-004), still produces the portable zip with an empty `MouserData/` (without the service), and includes the SmartScreen paragraph ("Windows protected your PC" → "More info" → "Run anyway") in the release notes body
- [ ] T416 Set `version = "0.3.0"` in the root `Cargo.toml` `[workspace.package]` and in `apps/mouser-app/tauri.conf.json` (`V2_SINCE`, research V20), and update `Cargo.lock`
- [ ] T417 Run `cargo xtask gate --all --lane convergence` on the three CI runners after the packaging changes (verification only; must stay green before E2E)

### E2E — J1 "two real computers on a LAN" (re-run; Convergence touches packaging)

- [ ] T418 Run J1 (`apps/mouser-cli/tests/e2e/features/e2e_j1_two_computers.feature`, unchanged) on the two real machines with the 0.3.0 build, fix any failure at its source (no timeouts widened, no retries added — 4.3), and record it with `cargo xtask gate --e2e j1 --lane convergence`

- [ ] T419 Gate: `cargo xtask gate --all --lane convergence` green on all three CI runners and the J1 evidence record passing (local `--all` is not run on the owner's Mac because the macOS suite drives real input)
- [ ] T420 Commit: phase boundary "Convergence" (5.3)

---

## Phase 29: Polish & Cross-Cutting Concerns

**Purpose**: Keep `docs/` matching what was delivered (constitution Development Workflow; gap G29), run the Story 7 real-hardware checks, and final validation.

### Documentation

- [ ] T421 [P] Update `docs/troubleshooting.md`: remove the stale "Windows backend is not implemented" and "crossings are not logged" statements (lines 5, 132–134); document each Problem kind including `elevated_app` and its `restart_elevated` fix, every v2 limitation text, the log header, the event catalog, rotation within `max_log_mb`, and `mouser logs collect` / "Collect logs from the desk" (this file is also shown in Help, Story 3 sc. 17)
- [ ] T422 [P] Update `docs/configuration.md`: `crossing.landing_highlight`, `keyboard.command_as_control` (desk-shared), the desk-wide meaning of `general.computer_name`, validation of `hotkeys.enable_disable` and `diagnostics.max_log_mb`, the Windows machine folder `C:\ProgramData\Mouser`, and fix the stale lines 11 and 26
- [ ] T423 [P] Update `docs/cli.md`: `mouser logs collect`, `mouser desk rename`, `config set` origin, the v2 `status` output, Windows endpoint selection, the `mouser-app` flags `--headless [--machine]`, `--cursor-guard`, `--wait-for`, and the `mouser-service.exe --install|--uninstall` command line; fix line 162
- [ ] T424 [P] Update `docs/releasing.md`: the `mouser-service` sidecar, the per-machine NSIS installer and its hooks, the unsigned Windows build and the exact install instructions past "Windows protected your PC" (FR-004, SC-010); fix line 63
- [ ] T425 [P] Update `docs/technical-decisions.md` with the as-built V1–V20 decisions, the `zip`/`windows-service` dependencies, the cursor guard, the service + session-engine model, and the probe P1–P4 outcomes from `specs/002-mouser-v2/spikes/FINDINGS.md`
- [ ] T426 [P] Update `docs/product-requirements.md` so every item states what v2 delivers, including the two new accepted limitations (unknown-publisher warning; Linux login screen) and 001 limitation 4 superseded for installed Windows
- [ ] T427 Review all files under `docs/` against the delivered behaviour and record "docs/ reviewed, no drift" or the corrections made in `specs/002-mouser-v2/evidence/docs-review.md` (verification only)
- [ ] T428 Build the 0.3.0 release candidate with `.github/workflows/release.yml` (the owner confirms and pushes the tag) producing the Windows NSIS installer, the Windows portable zip, the macOS dmg and the Linux AppImage/deb used by the checks below; record the run id in `specs/002-mouser-v2/evidence/quickstart.md` (verification only)

### Story 7 — real-hardware verification (quickstart.md; SC-011)

Each task below is verification only: run the check on the named hardware with diagnostic logging on, collect a bundle (`mouser logs collect`), and record outcome, hardware and OS, Mouser version, date, who ran it and the bundle name in `specs/002-mouser-v2/evidence/quickstart.md`. A failure is recorded as a defect against the story it breaks and added as a follow-up task under that story's lane. The Parallels Windows 11 VM counts for every Windows check (OD-3); Steve's PCs are optional, extra confirmation and gate nothing.

- [ ] T429 [US7] Run quickstart §3 (logging and desk bundle) on two computers: every bundle line validates against the log-events schema, each file starts with `log.header`, `grep -r MOUSER-MARKER-7f3a` finds 0 matches (SC-004), and the incident sequence is reconstructable from the bundle alone (SC-003)
- [ ] T430 [US7] Run quickstart §4 (one window) on a desk of two Macs, the Parallels VM and the Linux computer: every notice and limitation once under its computer, zero unrounded numbers (SC-006); tray left-click pause, right-click menu, badge appears/disappears/absent after removal and states distinguishable in a greyscale screenshot (SC-008); `spike_uipi swallow` for 10 minutes in the VM shows no "lost its hold" problem (Story 3 sc. 16); every section and setting reached in ≤ 2 clicks or by search (SC-005)
- [ ] T431 [US7] Run quickstart §5 (settings) with a desk owner new to v2 and no documentation: rename a computer (checked on every computer, including one offline during the rename), change the panic key to F12×3, record a QuickJump chord, turn off the landing highlight, turn off "Use Command as Control" (Cmd+R opens Run, Ctrl+C copies on Windows), and collect the desk's logs (SC-007)
- [ ] T432 [US7] Run quickstart §7 (headless) on the Parallels VM (portable `mouser-app.exe --headless`) and on the Linux computer in a signed-in session: no window or tray, listed connected, crossable; Linux Health lists "Mouser can't control this computer's login screen."
- [ ] T433 [US7] R1: Wi-Fi to Ethernet switch and Wi-Fi toggle while controlling a computer → crossing works within 5 s (Mac pair; Windows)
- [ ] T434 [US7] R2: VPN connect and disconnect (Mac)
- [ ] T435 [US7] R3: unplug the network while the computer has the pointer → control returns, no key is stuck (Mac ↔ Windows)
- [ ] T436 [US7] R4: kill Mouser (Activity Monitor / Task Manager → End task, `kill -9`) while the pointer is remote → pointer visible on that computer (SC-002; research V5) on Mac, Windows, Linux X11 and Linux Wayland
- [ ] T437 [US7] R5: portable Mouser from a USB drive on Windows: join, eject, nothing left behind
- [ ] T438 [US7] R6: real Linux on X11, GNOME Wayland and KDE Wayland: join, cross, clipboard
- [ ] T439 [US7] R7: lock and unlock a Mac, and the Mac login window, from the shared keyboard
- [ ] T440 [US7] R8: quickstart §6 rows 5a–5h on the Parallels VM (gating): install past "Windows protected your PC" with one UAC approval and `MouserService` running (5a); sign in after cold boot (5b, Story 5 sc. 1); UAC prompt, user switch and lock screen operable (5c, sc. 2); Send Ctrl-Alt-Del, and the policy limitation with `SoftwareSASGeneration=0` (5d, sc. 3); type/click/scroll/move into an elevated `cmd.exe` or Task Manager and, at the PC, edge/panic key/hotkey work (5e, sc. 4–5); portable hand-back message with "Restart Mouser as administrator" (5f, sc. 6); restart approved at the VM's own keyboard (OD-1), input reaches the elevated app, portable limitations listed (5g, sc. 7–8); uninstall leaves no Mouser process and no `MouserService` (5h, FR-013). Together these are SC-009. Optional, non-gating: repeat 5e with VMware Workstation as administrator on Steve's PC
- [ ] T441 [US7] R9 trigger case (FR-011, SC-001), gating form on the owner's Mac + Parallels VM: with Mouser installed on Windows, the VM receives the pointer from the Mac, an elevated `cmd.exe` is brought to the front with a click, then type, Esc×2, cross back, cross again — 20 repetitions; then 20 with a non-elevated application; then both again with portable Mouser. Expected in 20 of 20 per variant: pointer visible and working on a computer the user can use, crossing both ways afterwards without restarting Mouser within 5 s of reachability, and the bundle names the computer, time, cause and application (SC-003). Optional, non-gating: the two-PC VMware Workstation form on Steve's PCs
- [ ] T442 [US7] R10 (optional, non-gating; OD-3): on Steve's PC, click into a running VMware VM → Ctrl+Alt releases VMware's capture and Mouser never leaves the pointer hidden (spec edge case); record "not run" if Steve is unavailable
- [ ] T443 [US7] R11: someone new to Mouser, if possible, installs on the Parallels VM past the unknown-publisher warning using only the release notes / `docs/releasing.md` instructions (SC-010)
- [ ] T444 [US7] R12: mixed desk 0.2.8 + 0.3.0 on the Mac pair: crossing, pause, and `mouser logs collect` lists the 0.2.8 computer as "older version"
- [ ] T445 [US7] Verify `specs/002-mouser-v2/evidence/quickstart.md` has a dated real-hardware outcome for every gating row (§3–§7, R1–R9, R11, R12) and an outcome or "not run (optional)" for R10 and the Steve variants, and that every recorded failure has a follow-up task (SC-011; verification only)

### Final validation

- [ ] T446 Gate: `cargo xtask gate --all --lane polish` green on all three CI runners (local `--all` is not run on the owner's Mac because the macOS suite drives real input)
- [ ] T447 Commit: phase boundary "Polish" (5.3)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)** → **Probes (Phase 2)** ∥ **Foundational (Phase 3)** → **PARALLEL GROUP (Phases 4–27)** → **Convergence (28)** → **Polish (29)**
- No lane phase starts before the Foundational gate (T050) passes and lane disjointness (T049) is verified.
- Probes run on the Parallels VM / by the owner and may overlap Foundational; each gates only the lane phases it names (T008 → Phases 19, 21, 22; T009 → Phases 22, 27 and T412–T415; T010 → Phase 13; T011 → T215–T218).

### Lane order (within each lane, sequential in story order)

| Lane | Phases | Runs on |
|---|---|---|
| core | 4–8 (US1, US2, US3, US4, US5) | any; CI all three |
| app | 9–14 (US1, US2, US3, US4, US5, US6) | macOS locally (UI and Rust unit tests only); CI all three |
| cli | 15–18 (US2, US3, US4, US5) | any |
| platform-windows | 19–22 (US1, US2, US3, US5) | Windows runner (Windows Claude session) |
| platform-macos | 23 (US1) | `macos-14` CI runner only (drives real input) |
| platform-linux | 24–26 (US1, US3, US6) | Ubuntu runner |
| service-win | 27 (US5) | Windows runner |

### Cross-lane dependencies

- Lanes compile against the frozen `mouser-proto`, `mouser-platform` and `mouser-testkit` only. `app` and `cli` tests use `mouser-core`, so each app/cli phase starts after the core phase of the same story is merged (US1-app after US1-core, US2-cli after US2-core, …). US6-app needs US5-core and US2-core only.
- US1-app T200 calls `mouser_platform_windows::cursor_guard::run` (Phase 19, T335).
- US5-app needs US5-platform-windows (`service_client`, T357) and probe P3.
- US5-cli needs US5-core (`default_path()`, T190).
- service-win needs probe P2; it uses only the existing `Engine::export_machine_scope` from core.
- A refuted probe halts the lanes it gates and returns to the plan (contracts are frozen; changes are plan-level, 3.3).

### Story completion

A story is complete when all its `USn-*` phases and Convergence have passed (5.2): US1 = Phases 4, 9, 19, 23, 24; US2 = 5, 10, 15, 20; US3 = 6, 11, 16, 21, 25; US4 = 7, 12, 17; US5 = 8, 13, 18, 22, 27; US6 = 14, 26. US7 completes with T445 in Polish.

## Parallel Example: after Foundational

```bash
# Seven lanes (separate worktrees, each bootstrapped with
#   cargo fetch --locked && cargo xtask bootstrap):
lane core              → Phase 4  (US1-core)
lane app               → Phase 9  (US1-app)              # after US1-core merged; T200 after T335
lane cli               → Phase 15 (US2-cli)              # after US2-core merged
lane platform-windows  → Phase 19 (US1-platform-windows) # after probe P1 (T008)
lane platform-macos    → Phase 23 (US1-platform-macos)   # gate on macos-14 CI only
lane platform-linux    → Phase 24 (US1-platform-linux)
lane service-win       → Phase 27 (US5-service-win)      # after probe P2 (T009)
```

Within a phase, `[P]` test tasks for different files can be written together; each implementation task follows its own test task. Tasks that touch the same file within a phase (e.g. T058/T060 in `crossing/controller_tests.rs`) run in listed order.

## Implementation Strategy

### MVP (P1 stories US1–US2)

1. Setup → Probes P1 → Foundational.
2. core Phases 4–5, app Phases 9–10, cli Phase 15, platform-windows Phases 19–20, platform-macos Phase 23, platform-linux Phase 24.
3. Convergence (version 0.3.0, J1) → a release in which no refused input or fault leaves the pointer hidden or the desk wedged, and the desk's logs can be collected in one step — the trigger case's reliability fix plus the evidence to confirm it.

### Incremental delivery

Add P2 (US3–US5) then P3 (US6) phases lane by lane; each lane phase is gated and committed, so any lane can pause without blocking the others. Story 7 runs once a 0.3.0 release candidate exists (T428).

## Notes

- Every implementation task is preceded by its own test task (5.1 rule 3); verification-only tasks (gates, commits, probes, reviews, Story 7 checks) have size 0; T014, T040, T041, T055, T259 are uniform mechanical changes (size 1).
- Field constraints from data-model.md and the contracts are quoted in the tasks that implement them.
- Do not add behaviour, error handling or defensive branches beyond spec.md (5.5, 5.6); unspecified failures crash loudly. FR-002 is the specified exception for refused OS requests and faults.
- No new timers or retry policies (constitution 2.7; plan "Performance Goals"): V7 uses the existing 5 s watchdog cycle, V15 waits on existing link liveness, the service relaunches the engine without back-off.
- Touched-file ownership (5.7): fix any defect found in a file you modify, with a test, in the same task.
- Line numbers cited are at HEAD bd97abe; if they have moved, locate the cited code by the quoted text.

## Deferred to a future feature

None at generation. Work deferred by the owner during implementation is moved here with its reason and date (constitution 1.4), as in 001.

---

## Requirement and success-criterion coverage

| FR / SC | Tasks |
|---|---|
| FR-001 (001 behaviour holds) | T040, T041, T088 (all 001 acceptance suites in every core gate), T142, T159, T178, T191, T417, T418 (J1), T419, T426, T446 |
| FR-002 (no refusal/fault ends crossing, input, reconnect, panic, pause) | T053–T087, T036 (rule 11), T037–T039, T197–T198, T213–T214, T276–T277, T321–T323, T332–T333 |
| FR-003 (privacy of logs and bundles) | T091, T093–T094, T107–T108, T125–T128, T136–T137, T140, T141, T342–T343, T429 |
| FR-004 (unsigned Windows; unknown-publisher limitation and instructions) | T360–T361, T415, T424, T440, T443 |
| FR-005 (portable "Restart Mouser as administrator", stays portable) | T010, T183–T188, T282–T287, T440 |
| FR-006 (Linux headless, login-screen limitation) | T047, T294–T298, T380–T381, T432 |
| FR-007 (bundle contents, local only, no secrets) | T019–T020, T131–T140, T251–T252, T301–T302, T429 |
| FR-008 (rotation within `max_log_mb`) | T031–T032, T097–T098, T396–T397 |
| FR-009 (rounded numbers) | T231–T232, T241–T242, T247–T248, T430 |
| FR-010 (actionable notices once; limitations only under their computer) | T145–T150, T221–T222, T247–T248, T257–T258, T430 |
| FR-011 (reference artifacts; trigger case) | T008, T042, T227–T228, T441 |
| FR-012 (desk-shared names/highlight/mapping; per-computer hotkeys) | T015–T016, T031–T032, T162–T177, T265–T270 |
| FR-013 (one approval install; uninstall leaves nothing) | T009, T400–T405, T412–T415, T440 |
| SC-001 (trigger case 20/20, cross again within 5 s) | T042, T052, T181–T182, T441 |
| SC-002 (zero runs end with pointer hidden) | T052, T066–T067, T084–T087, T105–T106, T332–T337, T436 |
| SC-003 (agent names computer, time, cause, app from bundle) | T090, T103–T108, T134–T137, T429, T441 |
| SC-004 (zero marker matches) | T141, T429 |
| SC-005 (≤ 2 clicks or search) | T229–T230, T269–T270, T430 |
| SC-006 (each notice/limitation once, no misattribution, no raw numbers) | T145–T150, T207, T247–T248, T430 |
| SC-007 (new owner tailors settings and collects logs unaided) | T262–T270, T251–T252, T431 |
| SC-008 (tray disconnected state exact, greyscale distinguishable) | T151–T152, T215–T220, T430 |
| SC-009 (installed Windows: sign-in, UAC, user switch, Ctrl-Alt-Del, admin app) | T009, T048, T352–T361, T384–T409, T440 |
| SC-010 (install past unknown-publisher warning from instructions) | T415, T424, T443 |
| SC-011 (every Story 7 check recorded) | T005, T428–T445 |
