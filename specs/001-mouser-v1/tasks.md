---
description: "Task list for Mouser v1 (001-mouser-v1)"
---

# Tasks: Mouser v1

**Input**: Design documents from `specs/001-mouser-v1/` — plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md

**Prerequisites**: plan.md (package-parallel mode, operator decisions D1–D5), spec.md (13 stories), constitution v1.4.0

**Tests**: Required. Constitution 4.1 mandates acceptance (`acc_`, Gherkin), contract (`contract_`) and implementation tests with 100% coverage; every test task precedes the implementation task it verifies (5.1 rule 3).

**Organization**: Package-parallel (plan.md "Execution Mode", constitution 5.2). Setup and Foundational run first. Then a PARALLEL GROUP of lanes, one per package; within a lane, phases follow story order (spec dependency order US1→US13); across lanes, execution is concurrent. Convergence and Polish follow.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel within its phase (different files, no dependency on an incomplete task)
- **[Story]**: `USn-<lane>` for lane phases (composite tag, 5.2); story completion = all `USn-*` phases + Convergence passed
- Gate commands write evidence via `cargo xtask gate`, the only sanctioned evidence source (5.4); never hand-write evidence
- Phase-boundary commits (5.3) are verification-only tasks; commit only after the phase `Gate:` task passes
- No behavior beyond spec.md acceptance criteria and edge cases (5.5); no error handling for unspecified conditions — fail loudly (5.6)
- Lanes MUST NOT write `contracts/`, `crates/mouser-proto/**`, `crates/mouser-platform/**`, `crates/mouser-testkit/**`, or any `.feature` file; a needed contract change returns to the plan

## Path Conventions

Cargo workspace at repo root (plan.md "Project Structure"): `crates/mouser-{proto,platform,testkit,core,platform-macos,platform-windows,platform-linux}`, `apps/mouser-{app,cli,service-win}`, `xtask/`. Acceptance features: `crates/mouser-core/tests/features/usN.feature` (authored in Foundational); acceptance runners `crates/mouser-core/tests/acc_usN.rs`; step definitions `crates/mouser-core/tests/steps/usN.rs`.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Repository, workspace, tooling, CI and evidence tooling (constitution 3.4: infrastructure precedes all feature tasks)

- [X] T001 Create the GitHub repository for Mouser (ask the owner to confirm the repository name and public/private visibility before running `gh repo create`), add it as `origin`, and push `main` (D1)
- [X] T002 Create root `Cargo.toml` workspace with members `crates/mouser-proto`, `crates/mouser-platform`, `crates/mouser-testkit`, `crates/mouser-core`, `crates/mouser-platform-macos`, `crates/mouser-platform-windows`, `crates/mouser-platform-linux`, `apps/mouser-app`, `apps/mouser-cli`, `apps/mouser-service-win`, `xtask`; `edition = "2024"`, `rust-version = "1.99"`; `[workspace.lints]` with `rust.warnings = "deny"` and `clippy.all = "deny"`; `exclude = ["specs"]`
- [X] T003 Create `rust-toolchain.toml` pinning channel `1.99.0` with components `rustfmt`, `clippy`, `llvm-tools-preview`
- [X] T004 Create root `.gitignore` covering `target/`, `**/gen/`, `apps/mouser-app/ui/node_modules/`, `specs/**/spikes/**/target/`, `crates/*/probes/**/target/`
- [X] T005 Create skeleton crates (each with `Cargo.toml` inheriting workspace lints and an empty `src/lib.rs` or `src/main.rs`) for every workspace member listed in the root `Cargo.toml` (uniform mechanical change)
- [X] T006 [P] Create `xtask/src/bootstrap.rs` implementing `cargo xtask bootstrap`: install `cargo-llvm-cov --version 0.9.1 --locked` when `cargo llvm-cov --version` fails, and `npm ci` in `apps/mouser-app/ui` when Node is present; no E2E infrastructure (constitution v1.4.0 workspace bootstrap)
- [X] T007 [P] Create `xtask/src/gate.rs` implementing `cargo xtask gate --packages <list> | --all | --e2e <journey> [--lane <name>]` (`--e2e j1` runs `cargo test -p mouser-cli --features e2e --test e2e_j1`): runs `cargo fmt --check`, `cargo clippy -p <pkgs> --all-targets -- -D warnings`, `cargo test -p <pkgs>`, `cargo llvm-cov -p <pkgs> --fail-under-lines 100 --ignore-filename-regex '(mouser-testkit|xtask|spikes|probes|/gen/|e2e)'`, and for `mouser-app` also `node --test --experimental-test-coverage --test-coverage-lines=100` in `apps/mouser-app/ui`; appends one JSON record per check (command, exit code, full stdout/stderr, UTC timestamp captured at run time) to `specs/001-mouser-v1/evidence/<lane>.jsonl`; exits non-zero on any failure
- [X] T008 [P] Create `xtask/src/main.rs` wiring the `bootstrap` and `gate` subcommands with clap, and `.cargo/config.toml` alias `xtask = "run --package xtask --"`
- [X] T009 [P] Create `apps/mouser-app/ui/package.json` (private, `"type": "module"`, devDependency `happy-dom` pinned to an exact version, script `test`: `node --test --experimental-test-coverage --test-coverage-lines=100`) and `package-lock.json` via `npm install`
- [X] T010 Create `.github/workflows/ci.yml`: jobs on `macos-14`, `windows-2025`, `ubuntu-24.04` (Ubuntu installs `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libxdo-dev libssl-dev libdbus-1-dev`); each runs `cargo fetch --locked && cargo xtask bootstrap` then `cargo xtask gate` for the packages that build on that OS (macOS: all except `mouser-platform-windows`, `mouser-platform-linux`, `mouser-service-win`; Windows: all except `mouser-platform-macos`, `mouser-platform-linux`; Linux: all except `mouser-platform-macos`, `mouser-platform-windows`, `mouser-service-win`); uploads `specs/001-mouser-v1/evidence/` as an artifact
- [X] T011 Verify the Apple Developer ID Application identity is installed: `security find-identity -v -p codesigning` lists a "Developer ID Application" identity; record the output (D4; verification only)
- [X] T012 Gate: `cargo fetch --locked && cargo xtask bootstrap && cargo xtask gate --all --lane setup` exits 0 locally and the `ci.yml` run on `origin/main` is green on all three runners
- [X] T013 Commit: phase boundary "Setup" (5.3)

---

## Phase 2: Foundational (Blocking Prerequisites — contracts surface only)

**Purpose**: Freeze every shared contract (constitution 3.3) and the cross-lane test infrastructure. No lane phase begins until this phase's gate passes. Lanes read these files; they never write them.

### Contract crate `mouser-proto` (contracts/wire-protocol.md, control-api.md, config.md, schemas)

- [X] T014 [P] Copy `specs/001-mouser-v1/contracts/{status,log-record,release-manifest}.schema.json` into `crates/mouser-proto/schemas/` unchanged (uniform mechanical change)
- [X] T015 [P] Write `crates/mouser-proto/tests/contract_ids.rs`: `ComputerId` equals the first 16 bytes of SHA-256 of the Ed25519 device public key; `DeskId` and `ComputerId` render as 32 lowercase hex characters and parse back
- [X] T016 Implement `ComputerId`, `DeskId`, `DevicePublicKey` in `crates/mouser-proto/src/ids.rs` per data-model.md "Identity" ("ComputerId = first 16 bytes of SHA-256(device public key)")
- [X] T017 [P] Write `crates/mouser-proto/tests/contract_model.rs`: serde/postcard round-trip of `Display` (fields `display_id`, `name`, `pixel_size`, `logical_size`, `scale` with "≥1.0", `physical_size_mm` optional, `origin_local`, `included`), `Layout` (`placements`, `edge_options` with `block_corners` default `true`, `equalize_dimensions` default `false`, `block_fullscreen` default `true`), `Member`, `DeskState` (`epoch` u64)
- [X] T018 Implement `Display`, `Layout`, `EdgeOptions`, `Member`, `DeskState` types in `crates/mouser-proto/src/model.rs` per data-model.md
- [X] T019 [P] Write `crates/mouser-proto/tests/contract_wire.rs`: every control message in contracts/wire-protocol.md "Control messages" (Hello, JoinRequest, JoinDecision, DeskAuth, DeskState, Leave, Remove, Displays, Enter, LeaveControl, Key, Button, Scroll, ReleaseAll, Panic, ClipboardOffer, ClipboardFetch, ClipboardData, TransferOffer, TransferAccept, TransferReject, TransferConflict, TransferResolve, TransferDone, QuickJumpShow, QuickJumpHide, ReleaseAvailable, ReleaseFetch, Status, SecureAttention) and the motion datagram round-trip through length-prefixed postcard framing; motion datagram encodes in under 1100 bytes; `PROTOCOL_VERSION == 1`; `is_compatible(a, b)` false when protocol versions differ
- [X] T020 Implement the wire message enums, motion datagram, framing (`encode_frame`/`decode_frame`), `PROTOCOL_VERSION: u16 = 1` and `is_compatible` in `crates/mouser-proto/src/wire.rs`
- [X] T021 [P] Write `crates/mouser-proto/tests/contract_control.rs`: every JSON-RPC method in contracts/control-api.md serializes params/results; a populated `Status` validates against `schemas/status.schema.json` with the `jsonschema` crate; `Problem.kind` enumerates exactly `missing_permission, incompatible_version, join_refused, secure_input, crossing_blocked, peer_offline, platform_limitation`
- [X] T022 Implement control-API request/response/notification types, `Status`, `Problem`, `ProblemKind`, `FixAction` in `crates/mouser-proto/src/control.rs` per contracts/control-api.md and data-model.md "Problem"
- [X] T023 [P] Write `crates/mouser-proto/tests/contract_log.rs`: a `LogRecord` serializes to one line that validates against `schemas/log-record.schema.json`; `event` must match `^[a-z0-9_]+(\.[a-z0-9_]+)*$`
- [X] T024 Implement `LogRecord` in `crates/mouser-proto/src/log.rs`
- [X] T025 [P] Write `crates/mouser-proto/tests/contract_config.rs`: the TOML block in contracts/config.md parses; `Config::default()` equals it (`start_at_login = true`, `notifications = true`, `panic = "Escape*2"`, `buttons_4_5 = "passthrough"`, `block_fullscreen = true`, `equalize_dimensions = false`, `logging = false`, `max_log_mb = 50`, `automatic = true`, `port = 47801`, `allowlist = []`); `Config::validate` rejects `buttons_4_5` values other than `"passthrough" | "back_forward"`, ports outside 1–65535 and non-CIDR allowlist entries with `key: reason`
- [X] T026 Implement `Config` (all sections and keys of contracts/config.md), defaults, `validate`, and the list of desk-shared keys in `crates/mouser-proto/src/config.rs`
- [X] T027 [P] Write `crates/mouser-proto/tests/contract_release.rs`: a sample `latest.json` (tauri-plugin-updater format) validates against `schemas/release-manifest.schema.json` and parses into `ReleaseManifest`
- [X] T028 Implement `ReleaseManifest` in `crates/mouser-proto/src/release.rs`
- [X] T029 Export all modules from `crates/mouser-proto/src/lib.rs`

### Contract crate `mouser-platform` (contracts/platform-api.md)

- [X] T030 Implement the `Platform`, `InputCapture`, `InputInject`, `Clipboard`, `FileDrag`, `Autostart`, `SecretStore` traits and types `PlatformInfo`, `LocalDisplay`, `LocalInput`, `ClipRep`, `NetInterface` (kind `PhysicalLan | Virtual | Loopback`), `PermissionKind`, `PermissionState`, `Limitation`, `SecureInputHolder`, `DropTarget`, `WatchHandle`, `Unsupported` error in `crates/mouser-platform/src/lib.rs` exactly as contracts/platform-api.md (declarations only)
- [X] T031 Implement the conformance suite (test infrastructure, feature `conformance`) in `crates/mouser-platform/src/conformance.rs`: one public check function per behavioural rule 1–9 of contracts/platform-api.md, each taking `&dyn Platform`, so every backend's `contract_platform_<os>.rs` calls the same checks

### Test infrastructure crate `mouser-testkit` (sanctioned doubles, D2)

- [X] T032 Implement `VirtualPlatform` in `crates/mouser-testkit/src/virtual_platform.rs`: scripted displays, local-input injection from tests, recorded injected output, in-memory clipboard with change counter, permission states, interfaces list and `watch_network` trigger, fullscreen flag, secure-input holder, in-memory `SecretStore`, `FileDrag` stub with scripted outgoing paths and drop targets
- [X] T033 Write `crates/mouser-testkit/tests/contract_platform_virtual.rs` running every `mouser_platform::conformance` check against `VirtualPlatform` (consumer side of the platform seam)
- [X] T034 Implement `SimNet` in `crates/mouser-testkit/src/simnet.rs`: in-process UDP relay per node that can delay, black-hole (both directions), change a node's address, add/remove virtual interfaces, and sleep/wake a node
- [X] T035 Implement the aggregated double-usage signal in `crates/mouser-testkit/src/report.rs`: at process exit print one banner listing every test that ran against `VirtualPlatform` or `SimNet` in place of real configuration (D2)

### Acceptance features (spec-derived; authored here because lanes may not write `.feature` files)

- [X] T036 [P] Write `crates/mouser-core/tests/features/us1.feature` with one Scenario per Story 1 acceptance scenario 1–11 and the Story 1 edge case (same desk names), Given/When/Then text taken verbatim from spec.md
- [X] T037 [P] Write `crates/mouser-core/tests/features/us2.feature` with one Scenario per Story 2 acceptance scenario 1–12 and the Story 2 edge cases (held key/button at crossing; local mouse on active computer; docked/undocked display change), verbatim from spec.md, plus the accepted-limitation scenario "the app states that applications ignoring simulated input are not reached" (spec Assumptions, D3 item 1)
- [X] T038 [P] Write `crates/mouser-core/tests/features/us3.feature` with one Scenario per Story 3 acceptance scenario 1–5, the edge cases (unreachable computer; held key when connection drops), and SC-008 ("within 5 seconds of waking or a network change the desk user can cross"), verbatim from spec.md
- [X] T039 [P] Write `crates/mouser-core/tests/features/us4.feature` (Story 4 scenarios 1–4; edge cases: all displays of a computer removed; docked/undocked review offer), verbatim
- [X] T040 [P] Write `crates/mouser-core/tests/features/us5.feature` (Story 5 scenarios 1–5; limitation scenario: full-screen blocking unavailable on Wayland is stated in the app), verbatim
- [X] T041 [P] Write `crates/mouser-core/tests/features/us6.feature` (Story 6 scenarios 1–3; edge case: unrepresentable format pastes best representation; Story 1 sc. 4 clipboard isolation between desks), verbatim
- [X] T042 [P] Write `crates/mouser-core/tests/features/us7.feature` (Story 7 scenarios 1–4; limitation scenario: file drag unavailable on Linux is stated), verbatim
- [X] T043 [P] Write `crates/mouser-core/tests/features/us8.feature` (Story 8 scenarios 1–4), verbatim
- [X] T044 [P] Write `crates/mouser-core/tests/features/us9.feature` (Story 9 scenarios 1–3; edge case: config file edited while running takes effect without restart), verbatim
- [X] T045 [P] Write `crates/mouser-core/tests/features/us10.feature` (Story 10 scenarios 1–4), verbatim
- [X] T046 [P] Write `crates/mouser-core/tests/features/us11.feature` (Story 11 scenarios 1–3), verbatim
- [X] T047 [P] Write `crates/mouser-core/tests/features/us12.feature` (Story 12 scenarios 1–4), verbatim
- [X] T048 [P] Write `crates/mouser-core/tests/features/us13.feature` (Story 13 scenarios 1–2; limitation scenario: Windows portable states it cannot control login, permission prompts, Ctrl-Alt-Del or administrator apps), verbatim

- [X] T049 Verify lane disjointness: extract every file path named by tasks under each `### Lane:` heading of this file and confirm the lane file sets are pairwise disjoint and disjoint from `specs/001-mouser-v1/contracts/`, `crates/mouser-proto/`, `crates/mouser-platform/`, `crates/mouser-testkit/` and `crates/mouser-core/tests/features/`; record the script output (5.2; verification only)
- [X] T050 Gate: `cargo xtask gate --packages mouser-proto,mouser-platform,mouser-testkit --lane foundational` exits 0 on all three CI runners
- [X] T051 Commit: phase boundary "Foundational" (5.3)

**Checkpoint**: Contracts frozen. All lanes may start.

---

## PARALLEL GROUP — User story lanes

Disjointness (5.2): each lane writes only under its own package directory — core `crates/mouser-core/**` (except `tests/features/**`), app `apps/mouser-app/**`, cli `apps/mouser-cli/**`, platform-macos `crates/mouser-platform-macos/**`, platform-windows `crates/mouser-platform-windows/**`, platform-linux `crates/mouser-platform-linux/**`, service-win `apps/mouser-service-win/**` — plus its own `specs/001-mouser-v1/evidence/<lane>.jsonl`. Disjointness is verified mechanically at the end of Foundational, before any lane is dispatched.

### Lane: core

Package `crates/mouser-core`. Gate per phase: `cargo xtask gate --packages mouser-core --lane core` (runs on any OS; CI runs it on all three). Acceptance runners use cucumber 0.23 with `harness = false` and `fail_on_skipped()`; steps drive N in-process cores over `VirtualPlatform` + `SimNet` and assert outcomes on the control-API notifications the UI renders.

#### Phase 3: US1-core — As a desk owner, I want to put my computers on a desk (P1) 🎯 MVP

**Independent test**: `cargo test -p mouser-core --test acc_us1` passes: two virtual computers create/join a desk with one-click approval and list each other as members.

- [X] T052 [US1-core] Implement the multi-node desk harness (test infrastructure) in `crates/mouser-core/tests/support/desk.rs`: start N `Engine`s, each with its own `VirtualPlatform`, temp config dir and `SimNet` node; expose each node's control-API client and notification stream
- [X] T053 [US1-core] Write the acceptance runner `crates/mouser-core/tests/acc_us1.rs` and step definitions `crates/mouser-core/tests/steps/us1.rs` for every scenario in `tests/features/us1.feature`
- [X] T054 [P] [US1-core] Write unit tests for device identity in `crates/mouser-core/src/identity/tests.rs`: first launch generates an Ed25519 device key stored via `SecretStore` under `device_key`; later launches reuse it; `ComputerId` derives from it
- [X] T055 [US1-core] Implement device identity in `crates/mouser-core/src/identity/mod.rs`
- [X] T056 [P] [US1-core] Write unit tests for local state persistence in `crates/mouser-core/src/state/tests.rs`: `state.json` round-trips desk membership (`Unjoined | Pending(desk) | Member(desk)`), layout and epoch; desk key is stored only in `SecretStore` under `desk_key/<desk_id>`
- [X] T057 [US1-core] Implement local state persistence in `crates/mouser-core/src/state/mod.rs`
- [X] T058 [P] [US1-core] Write unit tests for config loading in `crates/mouser-core/src/config/tests.rs`: missing file yields `Config::default()` and is written; existing file is parsed with `mouser_proto::config::Config`
- [X] T059 [US1-core] Implement config loading and saving at the per-OS path from contracts/config.md in `crates/mouser-core/src/config/mod.rs`
- [X] T060 [P] [US1-core] Write unit tests for the interface filter in `crates/mouser-core/src/discovery/filter_tests.rs`: only `NetInterface` of kind `PhysicalLan` is enabled; IPv4 preferred; an fe80 address is advertised only on the interface it belongs to (research R3)
- [X] T061 [US1-core] Implement the physical-LAN interface predicate in `crates/mouser-core/src/discovery/filter.rs`
- [X] T062 [P] [US1-core] Write unit tests for discovery in `crates/mouser-core/src/discovery/tests.rs`: service `_mouser._udp.local.` advertises TXT `v`, `cid`, `desk`, `dn`, `port`; browse results group computers by desk; shutdown unregisters and waits for confirmation
- [X] T063 [US1-core] Implement discovery with mdns-sd 0.21.6 (`disable_interface(All)` + `enable_interface(Predicate(..))`) in `crates/mouser-core/src/discovery/mod.rs`
- [X] T064 [P] [US1-core] Write unit tests for the transport in `crates/mouser-core/src/transport/tests.rs`: endpoint binds `[::]:port` falling back to `0.0.0.0:port`, ALPN `mouser/1`; a peer whose certificate key does not hash to the expected `ComputerId` is rejected; datagrams and a bidirectional control stream carry framed messages
- [X] T065 [US1-core] Implement the QUIC transport (quinn 0.11.12, rcgen certificate from the device key, pinning verifier, keep-alive 750 ms, max idle timeout 3 s per research R2) in `crates/mouser-core/src/transport/mod.rs`
- [X] T066 [P] [US1-core] Write unit tests for the handshake in `crates/mouser-core/src/session/handshake_tests.rs`: `Hello` exchanged first; differing protocol versions mark the peer `Incompatible` and raise an `incompatible_version` Problem whose message names the other computer, both Mouser versions, and which one to update (Story 1 sc. 8)
- [X] T067 [US1-core] Implement the `Hello` handshake and version check in `crates/mouser-core/src/session/handshake.rs`
- [X] T068 [P] [US1-core] Write unit tests for the problems registry in `crates/mouser-core/src/problems/tests.rs`: raising a Problem with an existing `problem_id` (kind + subject) does not duplicate it; clearing sets `cleared_at`; subscribers receive `problem` notifications
- [X] T069 [US1-core] Implement the problems registry in `crates/mouser-core/src/problems/mod.rs`
- [X] T070 [P] [US1-core] Write unit tests for desk creation in `crates/mouser-core/src/desk/create_tests.rs`: `desk.create{name}` trims the name, requires 1–64 characters, generates `desk_id` and a 32-byte `desk_key`, makes this computer the only member, and starts advertising the desk
- [X] T071 [US1-core] Implement desk creation in `crates/mouser-core/src/desk/create.rs`
- [X] T072 [P] [US1-core] Write unit tests for join requests in `crates/mouser-core/src/desk/join_tests.rs`: `desk.join` sends `JoinRequest` and enters `Pending`; the member emits a `join_request` notification; `join.decide{approve:true}` sends `JoinDecision` with the sealed desk key and DeskState and the joiner becomes `Member`; `approve:false` returns the joiner to `Unjoined` with a `join_refused` Problem ("not admitted") (Story 1 sc. 2–3, FR-013)
- [X] T073 [US1-core] Implement the join request / one-click approval flow in `crates/mouser-core/src/desk/join.rs`
- [X] T074 [P] [US1-core] Write unit tests for `DeskAuth` in `crates/mouser-core/src/session/auth_tests.rs`: every new connection proves membership with HMAC(desk_key, handshake transcript); a computer without the desk key receives no desk traffic (Story 1 sc. 4)
- [X] T075 [US1-core] Implement `DeskAuth` and per-desk traffic isolation in `crates/mouser-core/src/session/auth.rs`
- [X] T076 [P] [US1-core] Write unit tests for DeskState replication in `crates/mouser-core/src/desk/replication_tests.rs`: highest `(epoch, computer_id)` wins; every member converges on the same member list
- [X] T077 [US1-core] Implement DeskState replication in `crates/mouser-core/src/desk/replication.rs`
- [X] T078 [P] [US1-core] Write unit tests for leave/remove/move in `crates/mouser-core/src/desk/membership_tests.rs`: `desk.leave` and `desk.remove_member` remove the computer from every member list and rotate `desk_key`; `desk.move` leaves the old desk only after approval by the new one (Story 1 sc. 5)
- [X] T079 [US1-core] Implement leave, remove and move with desk-key rotation in `crates/mouser-core/src/desk/membership.rs`
- [X] T080 [P] [US1-core] Write unit tests for joining by address in `crates/mouser-core/src/desk/manual_tests.rs`: `desk.join{address:"host:port"}` dials directly and then follows the same approval flow (Story 1 sc. 6)
- [X] T081 [US1-core] Implement joining by network address in `crates/mouser-core/src/desk/manual.rs`
- [X] T082 [P] [US1-core] Write unit tests for the allowlist in `crates/mouser-core/src/desk/allowlist_tests.rs`: with a non-empty allowlist a `JoinRequest` from an address outside every CIDR is refused, no prompt is shown, and the owner gets a `join_refused` Problem naming the refused computer (Story 1 sc. 7)
- [X] T083 [US1-core] Implement allowlist enforcement in `crates/mouser-core/src/desk/allowlist.rs`
- [X] T084 [P] [US1-core] Write unit tests for start-at-login in `crates/mouser-core/src/autostart/tests.rs`: completing first-run with `general.start_at_login = true` calls `Autostart::set(true)`; on start the engine reconnects to its desk from persisted state with no action (Story 1 sc. 9, FR-011)
- [X] T085 [US1-core] Implement start-at-login and resume-on-start in `crates/mouser-core/src/autostart/mod.rs`
- [X] T086 [P] [US1-core] Write unit tests for the local control server in `crates/mouser-core/src/control/server_tests.rs`: newline-delimited JSON-RPC 2.0 over the per-OS socket/pipe path from contracts/control-api.md; methods `status`, `desks.discovered`, `desk.create`, `desk.join`, `desk.leave`, `desk.move`, `desk.remove_member`, `join.pending`, `join.decide`, `subscribe`; results validate against the `mouser-proto` types; another OS user cannot connect
- [X] T087 [US1-core] Implement the local control server and the US1 methods in `crates/mouser-core/src/control/server.rs`
- [X] T088 [P] [US1-core] Write unit tests for the status builder in `crates/mouser-core/src/control/status_tests.rs`: `status` reports this computer, `state` (`active`/`paused`), desk, every member with connection, permissions, limitations and display count, and open problems; output validates against `status.schema.json`; tray summary counts computers and displays (Story 1 sc. 10)
- [X] T089 [US1-core] Implement the status builder in `crates/mouser-core/src/control/status.rs`
- [X] T090 [US1-core] Implement `Engine` startup/shutdown composing identity, state, config, discovery, transport, desk and control modules in `crates/mouser-core/src/engine.rs` and export it from `crates/mouser-core/src/lib.rs`
- [X] T091 [US1-core] Gate: `cargo xtask gate --packages mouser-core --lane core` (acc_us1 and all mouser-core tests pass, 100% coverage)
- [X] T092 [US1-core] Commit: phase boundary "US1-core" (5.3)

#### Phase 4: US2-core — As a desk user, I want one mouse and keyboard to drive every computer on my desk (P1)

**Independent test**: `cargo test -p mouser-core --test acc_us2` passes: pointer crosses between virtual computers, keys/buttons/scroll arrive on the active computer, panic returns control.

- [X] T093 [US2-core] Write the acceptance runner `crates/mouser-core/tests/acc_us2.rs` and steps `crates/mouser-core/tests/steps/us2.rs` for every scenario in `tests/features/us2.feature`
- [X] T094 [P] [US2-core] Write unit tests for display exchange in `crates/mouser-core/src/layout/displays_tests.rs`: on connect and on any `watch_displays` change a member sends `Displays`; a computer whose displays changed has them replaced in the layout and `needs_review` set (edge case: docked/undocked)
- [X] T095 [US2-core] Implement display exchange in `crates/mouser-core/src/layout/displays.rs`
- [X] T096 [P] [US2-core] Write unit tests for auto-placement in `crates/mouser-core/src/layout/autoplace_tests.rs`: a new computer's displays, in their local arrangement, are appended to the right of the current layout's bounding box, top-aligned, with `needs_review` set and a review offer notification (Story 2 sc. 1, data-model "Layout")
- [X] T097 [US2-core] Implement auto-placement in `crates/mouser-core/src/layout/autoplace.rs`
- [X] T098 [P] [US2-core] Write unit tests for edge mapping in `crates/mouser-core/src/layout/edges_tests.rs`: adjoining edges are derived from desk-space rectangles sharing a boundary segment of length > 0; a point leaving an edge maps to the matching position on the adjoining display; motion is scaled so physical speed is constant across scale/physical size (Story 2 sc. 2, 5, 6)
- [X] T099 [US2-core] Implement edge derivation and point mapping in `crates/mouser-core/src/layout/edges.rs`
- [X] T100 [P] [US2-core] Write unit tests for the control state machine in `crates/mouser-core/src/crossing/state_tests.rs`: `Local → Remote(c)` on crossing, `Remote(c) → Local` on crossing back or panic, `Remote(c) → Remote(d)` between remote displays; every transition releases all `pressed` keys and buttons on the computer being left; exactly one active computer per desk (data-model "ActiveSession"; edge case: held key at crossing)
- [X] T101 [US2-core] Implement the control state machine in `crates/mouser-core/src/crossing/state.rs`
- [X] T102 [P] [US2-core] Write unit tests for the controller side in `crates/mouser-core/src/crossing/controller_tests.rs`: crossing calls `set_grabbed(true)` and sends `Enter{display, x, y, seq_base, modifiers}`; while remote, motion goes as datagrams with increasing `seq` and keys/buttons/scroll on the control stream; returning calls `set_grabbed(false)`; any computer can be controller (Story 2 sc. 2–3)
- [X] T103 [US2-core] Implement the controller side of input forwarding in `crates/mouser-core/src/crossing/controller.rs`
- [X] T104 [P] [US2-core] Write unit tests for the receiver side in `crates/mouser-core/src/crossing/receiver_tests.rs`: `Enter` moves the pointer via `InputInject::motion_abs` and emits a `landing` notification (Story 2 sc. 9); stale motion datagrams (lower `seq`) are discarded; `Key`/`Button`/`Scroll` are injected in order; input from devices attached to the receiving computer itself is not blocked (edge case)
- [X] T105 [US2-core] Implement the receiver side in `crates/mouser-core/src/crossing/receiver.rs`
- [X] T106 [P] [US2-core] Write unit tests for shortcut mapping in `crates/mouser-core/src/crossing/modifiers_tests.rs`: when the controller is macOS and the active computer is Windows or Linux, Command maps to Control (⌘C copies); when the controller is Windows/Linux and the active computer is macOS, Control maps to Command; same-OS pairs pass through (Story 2 sc. 4)
- [X] T107 [US2-core] Implement shortcut modifier mapping in `crates/mouser-core/src/crossing/modifiers.rs`
- [X] T108 [P] [US2-core] Write unit tests for buttons 4/5 in `crates/mouser-core/src/crossing/buttons_tests.rs`: with `mouse.buttons_4_5 = "passthrough"` buttons 4/5 are injected unchanged; with `"back_forward"` they are injected as Back/Forward (Story 2 sc. 11)
- [X] T109 [US2-core] Implement the buttons 4/5 setting in `crates/mouser-core/src/crossing/buttons.rs`
- [X] T110 [P] [US2-core] Write unit tests for the panic key in `crates/mouser-core/src/crossing/panic_tests.rs`: two presses of the configured panic key (`"Escape*2"`) while remote send `Panic`, the receiver releases everything, and control returns locally (Story 2 sc. 8)
- [X] T111 [US2-core] Implement panic key detection in `crates/mouser-core/src/crossing/panic.rs`
- [X] T112 [P] [US2-core] Write unit tests for pause/resume in `crates/mouser-core/src/crossing/pause_tests.rs`: `active.pause` (and the enable/disable hotkey) stops crossing on every member and `status.state` becomes `paused`; `active.resume` restores it (Story 2 sc. 12)
- [X] T113 [US2-core] Implement pause/resume and the control-API methods `active.pause`/`active.resume` in `crates/mouser-core/src/crossing/pause.rs`
- [X] T114 [P] [US2-core] Write unit tests for Secure Input in `crates/mouser-core/src/problems/secure_input_tests.rs`: when `Platform::secure_input()` returns a holder, a `secure_input` Problem naming the application is raised and a notification emitted; it clears when the holder disappears; the connection stays up (Story 2 sc. 10)
- [X] T115 [US2-core] Implement Secure Input monitoring in `crates/mouser-core/src/problems/secure_input.rs`
- [X] T116 [P] [US2-core] Write unit tests for platform limitations in `crates/mouser-core/src/problems/limitations_tests.rs`: every `PlatformInfo.limitations` entry is raised as a `platform_limitation` Problem with its plain-language text and appears in `status.computers[].limitations` (FR-001; D3 item 1)
- [X] T117 [US2-core] Implement limitation reporting in `crates/mouser-core/src/problems/limitations.rs`
- [X] T118 [US2-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [X] T119 [US2-core] Commit: phase boundary "US2-core" (5.3)

#### Phase 5: US3-core — As a desk user, I want my desk to keep working through sleep, network changes, and VPNs (P1)

**Independent test**: `cargo test -p mouser-core --test acc_us3` passes under SimNet: continuous typing loses nothing; after sleep/wake, re-addressing and virtual-interface churn the next crossing works within 5 s; loss returns control locally.

- [X] T120 [US3-core] Write the acceptance runner `crates/mouser-core/tests/acc_us3.rs` and steps `crates/mouser-core/tests/steps/us3.rs` for every scenario in `tests/features/us3.feature`, using SimNet sleep/wake, re-address, virtual-interface and black-hole controls
- [X] T121 [P] [US3-core] Write unit tests for ordered delivery in `crates/mouser-core/src/crossing/ordering_tests.rs`: 10 000 keystrokes sent while SimNet delays and reorders datagrams arrive in `seq` order with none missing (Story 3 sc. 1, SC-002)
- [X] T122 [US3-core] Implement per-session input sequencing over the control stream in `crates/mouser-core/src/crossing/ordering.rs`
- [X] T123 [P] [US3-core] Write unit tests for network-change handling in `crates/mouser-core/src/transport/netchange_tests.rs`: a `watch_network` event reclassifies interfaces, re-announces discovery on physical LAN only, and calls `Endpoint::rebind` on the new local address when this computer dialled the connection (research R2)
- [X] T124 [US3-core] Implement network-change handling and rebind in `crates/mouser-core/src/transport/netchange.rs`
- [X] T125 [P] [US3-core] Write unit tests for re-dial in `crates/mouser-core/src/session/redial_tests.rs`: when this computer accepted the connection and its own address changed, it re-dials the peer, repeats `Hello` + `DeskAuth`, and resumes the session; the peer's `connection` returns to `connected` (Story 3 sc. 3)
- [X] T126 [US3-core] Implement re-dial and session resume in `crates/mouser-core/src/session/redial.rs`
- [X] T127 [P] [US3-core] Write unit tests for connection states in `crates/mouser-core/src/session/liveness_tests.rs`: no packets for the 3 s idle timeout moves a peer to `reconnecting`; rediscovery or reconnection returns it to `connected`; longer absence becomes `offline` with `last_seen` and a `peer_offline` Problem; crossing toward an unreachable computer leaves the pointer where it is (edge case)
- [X] T128 [US3-core] Implement peer liveness states in `crates/mouser-core/src/session/liveness.rs`
- [X] T129 [P] [US3-core] Write unit tests for loss of the active computer in `crates/mouser-core/src/crossing/loss_tests.rs`: when the active computer becomes `reconnecting`, the controller calls `set_grabbed(false)`, synthesizes releases for every pressed key/button, and control returns locally; keys typed afterwards go to the local computer (Story 3 sc. 5; edge case: no stuck keys)
- [X] T130 [US3-core] Implement control return on loss of the active computer in `crates/mouser-core/src/crossing/loss.rs`
- [X] T131 [P] [US3-core] Write regression tests in `crates/mouser-core/tests/regression_us3.rs` reproducing each hypothesis from research.md "Plan-Phase Handoff disposition": advertising over a virtual interface, a 2 s path flap while typing, and Secure Input on the receiver — each must not end the session (constitution VI)
- [X] T132 [US3-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [X] T133 [US3-core] Commit: phase boundary "US3-core" (5.3)

#### Phase 6: US4-core — As a desk owner, I want to arrange displays to match my physical desk (P2)

**Independent test**: `cargo test -p mouser-core --test acc_us4` passes: `layout.set` moves edges on every member; display details, removal with undo and equalized dimensions behave as specified.

- [X] T134 [US4-core] Write the acceptance runner `crates/mouser-core/tests/acc_us4.rs` and steps `crates/mouser-core/tests/steps/us4.rs` for every scenario in `tests/features/us4.feature`
- [X] T135 [P] [US4-core] Write unit tests for layout validation in `crates/mouser-core/src/layout/validate_tests.rs`: "placements of included displays MUST NOT overlap; every included display MUST be reachable (graph connected)"; an invalid `layout.set` is refused and the previous layout stays
- [X] T136 [US4-core] Implement layout validation in `crates/mouser-core/src/layout/validate.rs`
- [X] T137 [P] [US4-core] Write unit tests for `layout.get`/`layout.set` in `crates/mouser-core/src/control/layout_tests.rs`: `layout.set` bumps the desk epoch, replicates via DeskState, and every member derives the new edges (Story 4 sc. 1)
- [X] T138 [US4-core] Implement `layout.get`/`layout.set` in `crates/mouser-core/src/control/layout.rs`
- [X] T139 [P] [US4-core] Write unit tests for display details in `crates/mouser-core/src/layout/details_tests.rs`: each display in `layout.get` carries resolution, scale, owning computer name and online state or `last_seen` (Story 4 sc. 2)
- [X] T140 [US4-core] Implement display details in `crates/mouser-core/src/layout/details.rs`
- [X] T141 [P] [US4-core] Write unit tests for display removal in `crates/mouser-core/src/layout/remove_tests.rs`: setting `included = false` stops all crossing onto it and restoring `included = true` undoes it; a computer with every display removed stays a member and still receives clipboard offers (Story 4 sc. 3; edge case)
- [X] T142 [US4-core] Implement display removal and undo in `crates/mouser-core/src/layout/remove.rs`
- [X] T143 [P] [US4-core] Write unit tests for equalized dimensions in `crates/mouser-core/src/layout/equalize_tests.rs`: with `crossing.equalize_dimensions = true` the full length of one edge maps onto the full length of the adjoining edge (Story 4 sc. 4)
- [X] T144 [US4-core] Implement equalized-dimension mapping in `crates/mouser-core/src/layout/equalize.rs`
- [X] T145 [US4-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [X] T146 [US4-core] Commit: phase boundary "US4-core" (5.3)

#### Phase 7: US5-core — As a desk user, I want the pointer to stay put when I don't mean to cross (P2)

**Independent test**: `cargo test -p mouser-core --test acc_us5` passes: corner and full-screen blocking hold the pointer; a double bump crosses; the first block shows an explanation.

- [X] T147 [US5-core] Write the acceptance runner `crates/mouser-core/tests/acc_us5.rs` and steps `crates/mouser-core/tests/steps/us5.rs` for every scenario in `tests/features/us5.feature`
- [X] T148 [P] [US5-core] Write unit tests for corner blocking in `crates/mouser-core/src/crossing/corners_tests.rs`: with `block_corners = true` (default) on an edge, motion past that edge within the fixed corner size of either end does not cross (Story 5 sc. 1)
- [X] T149 [US5-core] Implement corner blocking in `crates/mouser-core/src/crossing/corners.rs`
- [X] T150 [P] [US5-core] Write unit tests for full-screen blocking in `crates/mouser-core/src/crossing/fullscreen_tests.rs`: with `crossing.block_fullscreen = true` and `Platform::foreground_is_fullscreen()` true, no crossing occurs (Story 5 sc. 2)
- [X] T151 [US5-core] Implement full-screen blocking in `crates/mouser-core/src/crossing/fullscreen.rs`
- [X] T152 [P] [US5-core] Write unit tests for the double-bump override in `crates/mouser-core/src/crossing/override_tests.rs`: a second blocked hit on the same edge within the override window crosses; a single hit does not (Story 5 sc. 3)
- [X] T153 [US5-core] Implement the double-bump override in `crates/mouser-core/src/crossing/override.rs`
- [X] T154 [P] [US5-core] Write unit tests for the block explanation in `crates/mouser-core/src/problems/blocked_tests.rs`: the first blocked crossing since the explanation was last shown raises a `crossing_blocked` notification saying what blocked it and how to cross anyway or open the arranger edge; later blocks do not repeat it (Story 5 sc. 4)
- [X] T155 [US5-core] Implement the block explanation in `crates/mouser-core/src/problems/blocked.rs`
- [X] T156 [P] [US5-core] Write unit tests for per-edge options in `crates/mouser-core/src/layout/edge_options_tests.rs`: changing `edge_options[(display, side)].block_corners` through `layout.set` applies to that edge only on every member (Story 5 sc. 5)
- [X] T157 [US5-core] Implement per-edge option updates in `crates/mouser-core/src/layout/edge_options.rs`
- [X] T158 [US5-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [X] T159 [US5-core] Commit: phase boundary "US5-core" (5.3)

#### Phase 8: US6-core — As a desk user, I want copy and paste to work between computers (P2)

**Independent test**: `cargo test -p mouser-core --test acc_us6` passes: text, HTML, RTF, PNG and files copied on one virtual computer paste on another; targeted paste changes only one clipboard; other desks are unaffected.

- [X] T160 [US6-core] Write the acceptance runner `crates/mouser-core/tests/acc_us6.rs` and steps `crates/mouser-core/tests/steps/us6.rs` for every scenario in `tests/features/us6.feature`
- [X] T161 [P] [US6-core] Write unit tests for clipboard watching in `crates/mouser-core/src/clipboard/watch_tests.rs`: a `change_count` increase triggers `read()` and a `ClipboardOffer` with an increasing `sequence`; writes Mouser makes itself are tagged and never re-offered (no echo)
- [X] T162 [US6-core] Implement clipboard watching and offers in `crates/mouser-core/src/clipboard/watch.rs`
- [X] T163 [P] [US6-core] Write unit tests for applying offers in `crates/mouser-core/src/clipboard/apply_tests.rs`: representations up to 1 MiB arrive inline and are written with every representation; larger ones are fetched with `ClipboardFetch` on paste; a representation the receiver cannot hold is dropped and the best remaining one pasted (Story 6 sc. 1; edge case)
- [X] T164 [US6-core] Implement offer application and lazy fetch in `crates/mouser-core/src/clipboard/apply.rs`
- [X] T165 [P] [US6-core] Write unit tests for copied files in `crates/mouser-core/src/clipboard/files_tests.rs`: a file/folder copy sends only a manifest (names, sizes, relative paths); pasting fetches the bytes through a transfer and writes local copies (Story 6 sc. 2)
- [X] T166 [US6-core] Implement file clipboard manifests and paste-time fetch in `crates/mouser-core/src/clipboard/files.rs`
- [X] T167 [P] [US6-core] Write unit tests for targeted paste in `crates/mouser-core/src/clipboard/targeted_tests.rs`: the targeted-paste hotkey fetches the latest remote content and writes it only on this computer; no other member's clipboard changes (Story 6 sc. 3)
- [X] T168 [US6-core] Implement targeted paste in `crates/mouser-core/src/clipboard/targeted.rs`
- [X] T169 [US6-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [X] T170 [US6-core] Commit: phase boundary "US6-core" (5.3)

#### Phase 9: US7-core — As a desk user, I want to drag files from one computer to another (P2)

**Independent test**: `cargo test -p mouser-core --test acc_us7` passes: a scripted drag copies files with progress, prompts on conflict and refuses when space is short.

- [X] T171 [US7-core] Write the acceptance runner `crates/mouser-core/tests/acc_us7.rs` and steps `crates/mouser-core/tests/steps/us7.rs` for every scenario in `tests/features/us7.feature`
- [X] T172 [P] [US7-core] Write unit tests for the transfer engine in `crates/mouser-core/src/transfer/engine_tests.rs`: states `Pending → Checking (space) → Copying → Completed`; one unidirectional stream per item; copies appear in `destination_dir` with relative paths preserved (Story 7 sc. 1)
- [X] T173 [US7-core] Implement the transfer engine in `crates/mouser-core/src/transfer/engine.rs`
- [X] T174 [P] [US7-core] Write unit tests for transfer progress in `crates/mouser-core/src/transfer/progress_tests.rs`: `transfer` notifications carry current item, bytes done/total, rate and ETA (Story 7 sc. 2)
- [X] T175 [US7-core] Implement transfer progress reporting in `crates/mouser-core/src/transfer/progress.rs`
- [X] T176 [P] [US7-core] Write unit tests for name conflicts in `crates/mouser-core/src/transfer/conflict_tests.rs`: an existing name moves the transfer to `AwaitingOverwriteDecision`, emits a conflict notification, and `TransferResolve{overwrite}` continues or skips that item (Story 7 sc. 3)
- [X] T177 [US7-core] Implement name-conflict handling in `crates/mouser-core/src/transfer/conflict.rs`
- [X] T178 [P] [US7-core] Write unit tests for the space check in `crates/mouser-core/src/transfer/space_tests.rs`: total size is compared with free space before any byte is written; insufficient space sends `TransferReject(no_space)`, writes nothing, and notifies the user (Story 7 sc. 4)
- [X] T179 [US7-core] Implement the free-space check in `crates/mouser-core/src/transfer/space.rs`
- [X] T180 [P] [US7-core] Write unit tests for drag hand-off in `crates/mouser-core/src/transfer/drag_tests.rs`: when the pointer crosses while `FileDrag::outgoing()` returns paths, `TransferOffer` follows `Enter`; the receiver calls `begin_incoming` and starts the transfer on the reported drop target; when `Platform::file_drag()` is `None` the limitation is reported instead (D3 item 3)
- [X] T181 [US7-core] Implement drag hand-off in `crates/mouser-core/src/transfer/drag.rs`
- [X] T182 [US7-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [X] T183 [US7-core] Commit: phase boundary "US7-core" (5.3)

#### Phase 10: US8-core — As a desk owner, I want Mouser to tell me what is wrong and how to fix it (P2)

**Independent test**: `cargo test -p mouser-core --test acc_us8` passes: status lists every computer's connection, responsiveness, missing permissions and latest problem; fixes and diagnostic logging work.

- [X] T184 [US8-core] Write the acceptance runner `crates/mouser-core/tests/acc_us8.rs` and steps `crates/mouser-core/tests/steps/us8.rs` for every scenario in `tests/features/us8.feature`
- [X] T185 [P] [US8-core] Write unit tests for peer status exchange in `crates/mouser-core/src/problems/peer_status_tests.rs`: each member sends `Status` (permissions, limitations, secure-input holder, problems) on change; `status` shows every member's state and `rtt_ms` from the transport (Story 8 sc. 1)
- [X] T186 [US8-core] Implement peer status exchange in `crates/mouser-core/src/problems/peer_status.rs`
- [X] T187 [P] [US8-core] Write unit tests for permission problems in `crates/mouser-core/src/problems/permissions_tests.rs`: a missing permission raises `missing_permission` with fix `open_system_settings(kind)`; invoking the fix calls `Platform::open_permission_settings(kind)` (Story 8 sc. 2)
- [X] T188 [US8-core] Implement permission problems and the fix action in `crates/mouser-core/src/problems/permissions.rs`
- [X] T189 [P] [US8-core] Write unit tests for problem notifications in `crates/mouser-core/src/problems/notify_tests.rs`: `join_refused`, `incompatible_version`, `missing_permission`, `secure_input`, `platform_limitation` emit a notification with the message and, when present, one fix action (Story 8 sc. 3)
- [X] T190 [US8-core] Implement problem notifications in `crates/mouser-core/src/problems/notify.rs`
- [X] T191 [P] [US8-core] Write unit tests for the diagnostic log in `crates/mouser-core/src/logging/tests.rs`: `logging.set{enabled:true}` returns the log file path and writes one JSON record per line validating against `log-record.schema.json`; nothing is written to the OS console or system log; files rotate at `diagnostics.max_log_mb` (Story 8 sc. 4, FR-010)
- [X] T192 [US8-core] Implement the JSONL diagnostic log writer and `logging.set` in `crates/mouser-core/src/logging/mod.rs`
- [X] T193 [US8-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [X] T194 [US8-core] Commit: phase boundary "US8-core" (5.3)

#### Phase 11: US9-core — As a troubleshooting agent, I want to inspect and change Mouser from the command line (P3)

**Independent test**: `cargo test -p mouser-core --test acc_us9` passes against the control socket: status in JSON, settings changed by API or file edit take effect, logs readable.

- [X] T195 [US9-core] Write the acceptance runner `crates/mouser-core/tests/acc_us9.rs` and steps `crates/mouser-core/tests/steps/us9.rs` for every scenario in `tests/features/us9.feature`, driving the control socket directly
- [X] T196 [P] [US9-core] Write unit tests for `config.get`/`config.set` in `crates/mouser-core/src/control/config_api_tests.rs`: every key of contracts/config.md is readable and writable; `set` validates with `Config::validate` and returns `key: reason` on invalid input leaving the previous value; desk-shared keys propagate with a new epoch; a `status` notification reports the change to the UI (Story 9 sc. 2, FR-005)
- [X] T197 [US9-core] Implement `config.get`/`config.set` in `crates/mouser-core/src/control/config_api.rs`
- [X] T198 [P] [US9-core] Write unit tests for config file watching in `crates/mouser-core/src/config/watch_tests.rs`: editing `config.toml` while running applies the new values without restart (edge case)
- [X] T199 [US9-core] Implement config file watching in `crates/mouser-core/src/config/watch.rs`
- [X] T200 [US9-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [X] T201 [US9-core] Commit: phase boundary "US9-core" (5.3)

#### Phase 12: US10-core — As a desk owner, I want every computer on my desk to stay up to date without visiting each one (P3)

**Independent test**: `cargo test -p mouser-core --test acc_us10` passes with a local HTTP fixture serving `latest.json`: one computer downloads and verifies, the others fetch over the desk connection.

- [X] T202 [US10-core] Write the acceptance runner `crates/mouser-core/tests/acc_us10.rs` and steps `crates/mouser-core/tests/steps/us10.rs` for every scenario in `tests/features/us10.feature`, serving release fixtures from a local HTTP server configured through `updates.release_url`
- [X] T203 [P] [US10-core] Write unit tests for fetcher election in `crates/mouser-core/src/release/elect_tests.rs`: the online member with the lowest `ComputerId` checks `updates.release_url`; no other member contacts the internet (FR-003)
- [X] T204 [US10-core] Implement fetcher election in `crates/mouser-core/src/release/elect.rs`
- [X] T205 [P] [US10-core] Write unit tests for release download in `crates/mouser-core/src/release/fetch_tests.rs`: a newer `latest.json` triggers download of every platform bundle; each is verified with minisign against the compiled-in Mouser updater public key; an unverified bundle is not offered (Story 10 sc. 3)
- [X] T206 [US10-core] Implement release check, download and minisign verification in `crates/mouser-core/src/release/fetch.rs`
- [X] T207 [P] [US10-core] Write unit tests for LAN distribution in `crates/mouser-core/src/release/serve_tests.rs`: the fetcher sends `ReleaseAvailable` with URLs rewritten to its own LAN endpoint and serves `ReleaseFetch` streams; peers never download from the internet (Story 10 sc. 1, 3)
- [X] T208 [US10-core] Implement LAN release distribution in `crates/mouser-core/src/release/serve.rs`
- [X] T209 [P] [US10-core] Write unit tests for `update.check`/`update.apply` in `crates/mouser-core/src/control/update_tests.rs`: `update.check` returns current and available versions (Story 10 sc. 4); `update.apply` hands the verified bundle bytes to the app's installer callback
- [X] T210 [US10-core] Implement `update.check`/`update.apply` in `crates/mouser-core/src/control/update.rs`
- [X] T211 [US10-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [X] T212 [US10-core] Commit: phase boundary "US10-core" (5.3)

#### Phase 13: US11-core — As a desk user, I want to jump the pointer straight to any display (P3)

**Independent test**: `cargo test -p mouser-core --test acc_us11` passes: the hotkey shows a distinct letter per display, a letter jumps there, other keys cancel.

- [X] T213 [US11-core] Write the acceptance runner `crates/mouser-core/tests/acc_us11.rs` and steps `crates/mouser-core/tests/steps/us11.rs` for every scenario in `tests/features/us11.feature`
- [X] T214 [P] [US11-core] Write unit tests for QuickJump in `crates/mouser-core/src/quickjump/tests.rs`: the hotkey assigns a distinct letter to every included display and sends `QuickJumpShow`; pressing a shown letter sends `QuickJumpHide` and moves control to that display's centre; Esc or any other key hides the letters and leaves the pointer where it was (Story 11 sc. 1–3)
- [X] T215 [US11-core] Implement QuickJump in `crates/mouser-core/src/quickjump/mod.rs`
- [X] T216 [US11-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [X] T217 [US11-core] Commit: phase boundary "US11-core" (5.3)

#### Phase 14: US12-core — As a desk user, I want to control a computer that is at its login or system security screen (P3)

**Independent test**: `cargo test -p mouser-core --test acc_us12` passes with a VirtualPlatform reporting a secure-attention-capable Windows computer.

- [X] T218 [US12-core] Write the acceptance runner `crates/mouser-core/tests/acc_us12.rs` and steps `crates/mouser-core/tests/steps/us12.rs` for every scenario in `tests/features/us12.feature`
- [X] T219 [P] [US12-core] Write unit tests for Send Ctrl-Alt-Del in `crates/mouser-core/src/crossing/sas_tests.rs`: `send_cad` while a Windows computer is active sends `SecureAttention` and its receiver calls `InputInject::secure_attention`; on a non-Windows active computer `send_cad` returns the unsupported error (Story 12 sc. 4)
- [X] T220 [US12-core] Implement `send_cad` and `SecureAttention` handling in `crates/mouser-core/src/crossing/sas.rs`
- [X] T221 [P] [US12-core] Write unit tests for headless start in `crates/mouser-core/src/engine_headless_tests.rs`: an `Engine` started without a logged-in user (login-window/service mode) loads the desk from machine-scope secrets, joins the desk, and accepts input (Story 12 sc. 1–3)
- [X] T222 [US12-core] Implement headless engine mode in `crates/mouser-core/src/engine_headless.rs`
- [X] T223 [US12-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [X] T224 [US12-core] Commit: phase boundary "US12-core" (5.3)

#### Phase 15: US13-core — As a desk owner, I want to run Mouser without installing it (P3)

**Independent test**: `cargo test -p mouser-core --test acc_us13` passes: with a portable marker next to the executable all state lives under `MouserData/` and autostart is never enabled.

- [X] T225 [US13-core] Write the acceptance runner `crates/mouser-core/tests/acc_us13.rs` and steps `crates/mouser-core/tests/steps/us13.rs` for every scenario in `tests/features/us13.feature`
- [X] T226 [P] [US13-core] Write unit tests for portable paths in `crates/mouser-core/src/paths_tests.rs`: when `MouserData/` exists beside the executable, config, state, logs and the file-backed `SecretStore` live there and nothing is written elsewhere; otherwise the per-OS paths from contracts/config.md are used (Story 13 sc. 1–2)
- [X] T227 [US13-core] Implement portable path resolution in `crates/mouser-core/src/paths.rs`
- [X] T228 [P] [US13-core] Write unit tests for portable behaviour in `crates/mouser-core/src/portable_tests.rs`: in portable mode `Autostart::set` is never called and the platform's portable limitations are reported (D3 item 4)
- [X] T229 [US13-core] Implement portable behaviour in `crates/mouser-core/src/portable.rs`
- [X] T230 [US13-core] Gate: `cargo xtask gate --packages mouser-core --lane core`
- [X] T231 [US13-core] Commit: phase boundary "US13-core" (5.3)

### Lane: app

Package `apps/mouser-app` (Tauri 2.12.5 host + `ui/` static ES modules). Gate per phase: `cargo xtask gate --packages mouser-app --lane app` (Rust tests + `node --test` UI tests, 100% coverage both). Each UI module exports pure render/update functions tested with happy-dom; the Tauri side keeps logic in testable modules and forwards UI calls to the in-process control API.

#### Phase 16: US1-app — As a desk owner, I want to put my computers on a desk (P1) 🎯 MVP

**Independent test**: `cargo xtask gate --packages mouser-app` passes; launching the app shows the tray icon and the first-run window drives permissions and desk choice through the control API.

- [X] T232 [US1-app] Create `apps/mouser-app/tauri.conf.json` (main window `"visible": false`, single-instance and autostart plugins, `bundle.active: false` for dev builds), `apps/mouser-app/build.rs`, `apps/mouser-app/capabilities/default.json`, and template/light/dark tray icons under `apps/mouser-app/icons/`
- [X] T233 [P] [US1-app] Write unit tests for platform selection in `apps/mouser-app/src/platform_select_tests.rs`: macOS builds use `mouser-platform-macos`, Windows `mouser-platform-windows`, Linux chooses the X11 or Wayland backend of `mouser-platform-linux` from `XDG_SESSION_TYPE`/`WAYLAND_DISPLAY`
- [X] T234 [US1-app] Implement platform selection in `apps/mouser-app/src/platform_select.rs`
- [X] T235 [P] [US1-app] Write unit tests for the engine host in `apps/mouser-app/src/host_tests.rs`: app start constructs `mouser_core::Engine` with the selected platform and exposes an in-process control-API client; a second app launch focuses the first (single instance)
- [X] T236 [US1-app] Implement the engine host and single-instance handling in `apps/mouser-app/src/host.rs`
- [X] T237 [P] [US1-app] Write unit tests for the tray model in `apps/mouser-app/src/tray_model_tests.rs`: from `status` the tray shows active/paused and "N computers, M displays"; on macOS the icon is a template image, elsewhere the light or dark variant follows the system appearance (Story 1 sc. 10)
- [X] T238 [US1-app] Implement the tray model and tray icon/menu wiring in `apps/mouser-app/src/tray.rs`
- [X] T239 [P] [US1-app] Write UI tests `apps/mouser-app/ui/first_run.test.js`: the permission step lists each required permission as granted or missing with an "Open settings" button calling `open_permission_settings`; the desk step lists discovered desks with member names and a "Create desk" form (name 1–64 characters); choosing a desk shows "Waiting for approval on <computer>" then "Member of <desk>" or "Not admitted" (Story 1 sc. 1–3)
- [X] T240 [US1-app] Implement the first-run window `apps/mouser-app/ui/first_run.html` and `apps/mouser-app/ui/first_run.js`
- [X] T241 [P] [US1-app] Write UI tests `apps/mouser-app/ui/join_prompt.test.js`: a `join_request` notification shows "Allow <computer> to join <desk>?" with Allow and Decline buttons calling `join.decide` (FR-013)
- [X] T242 [US1-app] Implement the join approval prompt `apps/mouser-app/ui/join_prompt.html` and `apps/mouser-app/ui/join_prompt.js`, opened by the host on each `join_request` notification
- [X] T243 [P] [US1-app] Write UI tests `apps/mouser-app/ui/desk_settings.test.js`: the desk section lists members with Remove, offers Leave and Move to another desk, "Add computer by address", and under Advanced the allowlist and port fields (Story 1 sc. 5–7, FR-007, FR-008)
- [X] T244 [US1-app] Implement desk settings `apps/mouser-app/ui/desk_settings.html` and `apps/mouser-app/ui/desk_settings.js`
- [X] T245 [P] [US1-app] Write UI tests `apps/mouser-app/ui/notify.test.js`: `problem` notifications render the plain-language message and at most one fix button (shared by all stories)
- [X] T246 [US1-app] Implement the notification presenter `apps/mouser-app/ui/notify.js` and its OS-notification bridge in `apps/mouser-app/src/notify.rs`
- [X] T247 [US1-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [X] T248 [US1-app] Commit: phase boundary "US1-app" (5.3)

#### Phase 17: US2-app — As a desk user, I want one mouse and keyboard to drive every computer on my desk (P1)

**Independent test**: UI and Rust tests pass for the landing highlight, global hotkeys and paused tray state.

- [X] T249 [P] [US2-app] Write UI tests `apps/mouser-app/ui/landing.test.js`: a `landing` notification shows a brief highlight at the given display position and removes it afterwards (Story 2 sc. 9)
- [X] T250 [US2-app] Implement the landing highlight overlay `apps/mouser-app/ui/landing.html`/`landing.js` and its transparent always-on-top window in `apps/mouser-app/src/overlay.rs`
- [X] T251 [P] [US2-app] Write unit tests for global hotkeys in `apps/mouser-app/src/hotkeys_tests.rs`: the `hotkeys.enable_disable`, `hotkeys.quickjump` and `hotkeys.targeted_paste` accelerators from config register with the OS and invoke `active.pause`/`active.resume`, QuickJump and targeted paste; changes in config re-register them (Story 2 sc. 12)
- [X] T252 [US2-app] Implement global hotkey registration in `apps/mouser-app/src/hotkeys.rs`
- [X] T253 [P] [US2-app] Write UI tests `apps/mouser-app/ui/input_settings.test.js`: the mouse section offers buttons 4/5 "Pass through unchanged" / "Back and Forward" writing `mouse.buttons_4_5` (Story 2 sc. 11)
- [X] T254 [US2-app] Implement input settings `apps/mouser-app/ui/input_settings.html` and `apps/mouser-app/ui/input_settings.js`
- [X] T255 [US2-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [X] T256 [US2-app] Commit: phase boundary "US2-app" (5.3)

#### Phase 18: US4-app — As a desk owner, I want to arrange displays to match my physical desk (P2)

**Independent test**: arranger UI tests pass: dragging, details, removal with undo and equalize call `layout.set` correctly.

- [X] T257 [P] [US4-app] Write UI tests `apps/mouser-app/ui/arranger.test.js`: every display renders to scale in desk space labelled with its computer; dragging snaps to adjoining edges and calls `layout.set`; an overlapping or disconnected drop is refused visibly; hovering shows resolution, scale, computer and online state or last seen; Remove hides a display with an Undo action; "Equalize display dimensions" toggles `crossing.equalize_dimensions`; displays flagged `needs_review` are highlighted with "Review placement" (Story 4 sc. 1–4; Story 2 sc. 1)
- [X] T258 [US4-app] Implement the arranger `apps/mouser-app/ui/arranger.html` and `apps/mouser-app/ui/arranger.js`, opened from the tray and from the review notification
- [X] T259 [US4-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [X] T260 [US4-app] Commit: phase boundary "US4-app" (5.3)

#### Phase 19: US5-app — As a desk user, I want the pointer to stay put when I don't mean to cross (P2)

**Independent test**: arranger edge-option UI tests pass.

- [X] T261 [P] [US5-app] Write UI tests `apps/mouser-app/ui/arranger_edges.test.js`: clicking an edge between two displays opens its options with "Block crossing in corners" (default on) writing `edge_options` via `layout.set`; the desk-wide "Don't cross while an app is full screen" toggles `crossing.block_fullscreen`; the `crossing_blocked` notification's fix opens this edge (Story 5 sc. 4–5)
- [X] T262 [US5-app] Implement edge options in `apps/mouser-app/ui/arranger_edges.js`
- [X] T263 [US5-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [X] T264 [US5-app] Commit: phase boundary "US5-app" (5.3)

#### Phase 20: US7-app — As a desk user, I want to drag files from one computer to another (P2)

**Independent test**: transfer window UI tests pass for progress, overwrite and no-space.

- [X] T265 [P] [US7-app] Write UI tests `apps/mouser-app/ui/transfer.test.js`: `transfer` notifications show file name, done/total, speed and time remaining; a conflict asks "Overwrite <name>?" with Overwrite/Skip calling `TransferResolve`; a `no_space` rejection says there is not enough space and nothing was copied (Story 7 sc. 2–4)
- [X] T266 [US7-app] Implement the transfer window `apps/mouser-app/ui/transfer.html` and `apps/mouser-app/ui/transfer.js`
- [X] T267 [US7-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [X] T268 [US7-app] Commit: phase boundary "US7-app" (5.3)

#### Phase 21: US8-app — As a desk owner, I want Mouser to tell me what is wrong and how to fix it (P2)

**Independent test**: health panel UI tests pass.

- [X] T269 [P] [US8-app] Write UI tests `apps/mouser-app/ui/health.test.js`: the panel lists each computer with connected/reconnecting/offline, responsiveness from `rtt_ms`, missing permissions with an "Open settings" button, limitations, and the most recent problem in plain language; a "Diagnostic logging" checkbox calls `logging.set` and shows "Open log folder" for the returned path (Story 8 sc. 1, 2, 4)
- [X] T270 [US8-app] Implement the health panel `apps/mouser-app/ui/health.html` and `apps/mouser-app/ui/health.js`, opened from the tray
- [X] T271 [US8-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [X] T272 [US8-app] Commit: phase boundary "US8-app" (5.3)

#### Phase 22: US10-app — As a desk owner, I want every computer on my desk to stay up to date without visiting each one (P3)

**Independent test**: updater tests pass: the tray shows versions and installs a verified LAN bundle.

- [X] T273 [P] [US10-app] Write unit tests for the updater bridge in `apps/mouser-app/src/updater_tests.rs`: the tray menu shows "Version X — Update to Y available" from `update.check`; applying installs the verified bundle with tauri-plugin-updater 2.14.0 pointed at the fetcher's LAN endpoint, then restarts; with `updates.automatic = true` it applies without asking (Story 10 sc. 1, 4)
- [X] T274 [US10-app] Implement the updater bridge in `apps/mouser-app/src/updater.rs`
- [X] T275 [US10-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [X] T276 [US10-app] Commit: phase boundary "US10-app" (5.3)

#### Phase 23: US11-app — As a desk user, I want to jump the pointer straight to any display (P3)

**Independent test**: QuickJump overlay UI tests pass.

- [X] T277 [P] [US11-app] Write UI tests `apps/mouser-app/ui/quickjump.test.js`: `QuickJumpShow` renders the assigned large letter centred on each local display; `QuickJumpHide` removes them (Story 11 sc. 1–3)
- [X] T278 [US11-app] Implement the QuickJump overlay `apps/mouser-app/ui/quickjump.html`/`quickjump.js` with one borderless overlay window per local display in `apps/mouser-app/src/quickjump_windows.rs`
- [X] T279 [US11-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [X] T280 [US11-app] Commit: phase boundary "US11-app" (5.3)

#### Phase 24: US12-app — As a desk user, I want to control a computer that is at its login or system security screen (P3)

**Independent test**: tray model test passes for the Send Ctrl-Alt-Del item.

- [X] T281 [P] [US12-app] Write unit tests in `apps/mouser-app/src/tray_sas_tests.rs`: the tray menu shows "Send Ctrl-Alt-Del" only while a Windows computer is active and invokes `send_cad`; when there is no tray (GNOME without AppIndicator) the same action is in the main window (Story 12 sc. 4; D3 item 5)
- [X] T282 [US12-app] Implement the Send Ctrl-Alt-Del menu item in `apps/mouser-app/src/tray_sas.rs`
- [X] T283 [US12-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [X] T284 [US12-app] Commit: phase boundary "US12-app" (5.3)

#### Phase 25: US13-app — As a desk owner, I want to run Mouser without installing it (P3)

**Independent test**: portable host test passes: WebView data and all app state resolve under `MouserData/`.

- [X] T285 [P] [US13-app] Write unit tests in `apps/mouser-app/src/portable_tests.rs`: in portable mode the WebView data directory and every app path resolve under `<exe dir>/MouserData` and autostart UI is hidden (Story 13 sc. 1–2)
- [X] T286 [US13-app] Implement portable host configuration in `apps/mouser-app/src/portable.rs`
- [X] T287 [US13-app] Gate: `cargo xtask gate --packages mouser-app --lane app`
- [X] T288 [US13-app] Commit: phase boundary "US13-app" (5.3)

### Lane: cli

Package `apps/mouser-cli`. Gate per phase: `cargo xtask gate --packages mouser-cli --lane cli`. Tests run the binary against an in-process engine from `mouser-core` serving the control socket in a temp dir.

#### Phase 26: US1-cli — As a desk owner, I want to put my computers on a desk (P1)

**Independent test**: `cargo test -p mouser-cli` passes for desk and approval commands.

- [X] T289 [P] [US1-cli] Write tests `apps/mouser-cli/tests/contract_cli_desk.rs`: `mouser desks [--json]`, `mouser desk create|join|leave|remove`, `mouser requests`, `mouser approve|decline` map to the control-API methods of contracts/cli.md; `join` waits and prints the decision; exit codes 0 success, 1 error, 2 not running, 3 invalid argument
- [X] T290 [US1-cli] Implement the desk and approval commands, socket client and exit-code mapping in `apps/mouser-cli/src/main.rs` and `apps/mouser-cli/src/desk.rs`
- [X] T291 [P] [US1-cli] Write J1 step definitions for Story 1 (install, create desk, join, approve, restart-and-rejoin) in `apps/mouser-cli/tests/e2e/steps/us1-cli.rs`, driving the installed `mouser` CLI over SSH; verified by `cargo test -p mouser-cli --no-run` (5.2: lanes author journey steps only)
- [X] T292 [US1-cli] Gate: `cargo xtask gate --packages mouser-cli --lane cli`
- [X] T293 [US1-cli] Commit: phase boundary "US1-cli" (5.3)

#### Phase 27: US2-cli — As a desk user, I want one mouse and keyboard to drive every computer on my desk (P1)

- [X] T294 [P] [US2-cli] Write tests `apps/mouser-cli/tests/contract_cli_pause.rs`: `mouser pause` / `mouser resume` call `active.pause` / `active.resume` (Story 2 sc. 12)
- [X] T295 [US2-cli] Implement `pause`/`resume` in `apps/mouser-cli/src/pause.rs`
- [X] T296 [P] [US2-cli] Write J1 step definitions for Story 2 (cross both ways by OS-level pointer movement on the controller, type text and read it back over SSH, panic key) in `apps/mouser-cli/tests/e2e/steps/us2-cli.rs`; verified by `cargo test -p mouser-cli --no-run`
- [X] T297 [P] [US2-cli] Write J1 step definitions for Story 3 (sleep and wake a computer over SSH, wait, cross again, continuous typing check) in `apps/mouser-cli/tests/e2e/steps/us3-cli.rs`; verified by `cargo test -p mouser-cli --no-run`
- [X] T298 [US2-cli] Gate: `cargo xtask gate --packages mouser-cli --lane cli`
- [X] T299 [US2-cli] Commit: phase boundary "US2-cli" (5.3)

#### Phase 28: US9-cli — As a troubleshooting agent, I want to inspect and change Mouser from the command line (P3)

**Independent test**: `cargo test -p mouser-cli` passes: status JSON validates against the schema; config and logs commands work with and without the app running.

- [X] T300 [P] [US9-cli] Write tests `apps/mouser-cli/tests/contract_cli_status.rs`: `mouser status --json` output validates against `status.schema.json`; plain `mouser status` prints the desk, each computer's connection, missing permissions and problems; with the app stopped it exits 2 with "Mouser is not running" (Story 9 sc. 1)
- [X] T301 [US9-cli] Implement `status` in `apps/mouser-cli/src/status.rs`
- [X] T302 [P] [US9-cli] Write tests `apps/mouser-cli/tests/contract_cli_config.rs`: `mouser config get [key]`, `config set key value` (invalid value prints `key: reason`, exit 3), `config path`; with the app stopped `get/set/path` operate directly on `config.toml` (Story 9 sc. 2)
- [X] T303 [US9-cli] Implement `config` in `apps/mouser-cli/src/config.rs`
- [X] T304 [P] [US9-cli] Write tests `apps/mouser-cli/tests/contract_cli_logs.rs`: `mouser logs enable|disable|path|tail [-n N]`; `tail` prints raw JSON Lines each validating against `log-record.schema.json`; `path`/`tail` work with the app stopped (Story 9 sc. 3, FR-010)
- [X] T305 [US9-cli] Implement `logs` in `apps/mouser-cli/src/logs.rs`
- [X] T306 [US9-cli] Gate: `cargo xtask gate --packages mouser-cli --lane cli`
- [X] T307 [US9-cli] Commit: phase boundary "US9-cli" (5.3)

#### Phase 29: US10-cli — As a desk owner, I want every computer on my desk to stay up to date without visiting each one (P3)

- [X] T308 [P] [US10-cli] Write tests `apps/mouser-cli/tests/contract_cli_update.rs`: `mouser update check` prints current/available versions; `mouser update apply` calls `update.apply` (Story 10 sc. 4)
- [X] T309 [US10-cli] Implement `update` in `apps/mouser-cli/src/update.rs`
- [X] T310 [US10-cli] Gate: `cargo xtask gate --packages mouser-cli --lane cli`
- [X] T311 [US10-cli] Commit: phase boundary "US10-cli" (5.3)

#### Phase 30: US12-cli — As a desk user, I want to control a computer that is at its login or system security screen (P3)

- [X] T312 [P] [US12-cli] Write tests `apps/mouser-cli/tests/contract_cli_sas.rs`: `mouser send-cad` calls `send_cad`; the unsupported error prints a plain message and exits 1 (Story 12 sc. 4)
- [X] T313 [US12-cli] Implement `send-cad` in `apps/mouser-cli/src/sas.rs`
- [X] T314 [US12-cli] Gate: `cargo xtask gate --packages mouser-cli --lane cli`
- [X] T315 [US12-cli] Commit: phase boundary "US12-cli" (5.3)

### Lane: platform-macos

Package `crates/mouser-platform-macos`. Gate per phase: `cargo xtask gate --packages mouser-platform-macos --lane platform-macos` **on the macOS runner** (and locally on macOS). Producer-side contract tests call `mouser_platform::conformance` checks against `MacPlatform`. Research R4 and spikes/FINDINGS-macos-input.md are the reference; tests that need granted permissions run from the signed test app bundle (D4).

#### Phase 31: US1-platform-macos — As a desk owner, I want to put my computers on a desk (P1)

- [X] T316 [P] [US1-platform-macos] Write `crates/mouser-platform-macos/tests/contract_platform_macos_us1.rs` running conformance rules 6 (interfaces), 8 (secrets) and 9 (unsupported) against `MacPlatform`
- [X] T317 [P] [US1-platform-macos] Write unit tests for permissions in `crates/mouser-platform-macos/src/permissions_tests.rs`: `permissions()` reports accessibility (AXIsProcessTrusted), input_monitoring (CGPreflightListenEventAccess), screen_recording (CGPreflightScreenCaptureAccess), full_disk_access without prompting; `open_permission_settings(kind)` opens the matching System Settings pane
- [X] T318 [US1-platform-macos] Implement permissions in `crates/mouser-platform-macos/src/permissions.rs`
- [X] T319 [P] [US1-platform-macos] Write unit tests for interface classification in `crates/mouser-platform-macos/src/net_tests.rs`: `SCNetworkInterfaceGetInterfaceType` Ethernet/IEEE80211 → `PhysicalLan`; utun, bridge (incl. Thunderbolt-bridge members), awdl, llw, anpi, ap, vmnet, docker → `Virtual`; lo0 incl. `fe80::1` → `Loopback` (research R3)
- [X] T320 [US1-platform-macos] Implement `interfaces()` in `crates/mouser-platform-macos/src/net.rs`
- [X] T321 [P] [US1-platform-macos] Write unit tests for the Keychain secret store in `crates/mouser-platform-macos/src/secrets_tests.rs`: set/get/delete round-trip under service `com.mouser.app`
- [X] T322 [US1-platform-macos] Implement `SecretStore` on the macOS Keychain in `crates/mouser-platform-macos/src/secrets.rs`
- [X] T323 [P] [US1-platform-macos] Write unit tests for autostart in `crates/mouser-platform-macos/src/autostart_tests.rs`: `set(true)` registers the app as a login item via SMAppService (auto-launch 0.6.0 SMAppService mode); `enabled()` reflects it (FR-011)
- [X] T324 [US1-platform-macos] Implement `Autostart` in `crates/mouser-platform-macos/src/autostart.rs`
- [X] T325 [US1-platform-macos] Gate: `cargo xtask gate --packages mouser-platform-macos --lane platform-macos` on the macOS runner
- [X] T326 [US1-platform-macos] Commit: phase boundary "US1-platform-macos" (5.3)

#### Phase 32: US2-platform-macos — As a desk user, I want one mouse and keyboard to drive every computer on my desk (P1)

- [X] T327 [P] [US2-platform-macos] Write `crates/mouser-platform-macos/tests/contract_platform_macos_us2.rs` running conformance rules 1 (grab), 2 (release on drop), 3 (HID usage independent of layout), 4 (displays), 7 (no feedback of injected events)
- [X] T328 [P] [US2-platform-macos] Write unit tests for displays in `crates/mouser-platform-macos/src/displays_tests.rs`: scale from `CGDisplayModeGetPixelWidth / GetWidth` (not `CGDisplayPixelsWide`); `physical_size_mm` from `CGDisplayScreenSize` omitted when 0; stable id from vendor/model/serial; reconfiguration callback fires `watch_displays`
- [X] T329 [US2-platform-macos] Implement `displays()` and `watch_displays()` in `crates/mouser-platform-macos/src/displays.rs`
- [X] T330 [P] [US2-platform-macos] Write unit tests for the HID keycode table in `crates/mouser-platform-macos/src/keymap_tests.rs`: every USB HID keyboard (0x07) and consumer (0x0C) usage Mouser forwards maps to a macOS virtual keycode and back
- [X] T331 [US2-platform-macos] Implement the HID ↔ macOS keycode table in `crates/mouser-platform-macos/src/keymap.rs`
- [X] T332 [P] [US2-platform-macos] Write unit tests for capture in `crates/mouser-platform-macos/src/capture_tests.rs`: one HID-level filtering CGEventTap delivers `LocalInput`; `set_grabbed(true)` drops keys/buttons/scroll/gestures/NX_SYSDEFINED, calls `CGAssociateMouseAndMouseCursorPosition(false)`, parks the cursor and reports `kCGMouseEventDeltaX/Y`; `set_grabbed(false)` restores association; the tap re-enables itself on `TapDisabledByTimeout`; events carrying Mouser's `kCGEventSourceUserData` tag are ignored (FINDINGS-macos-input §2)
- [X] T333 [US2-platform-macos] Implement `InputCapture` in `crates/mouser-platform-macos/src/capture.rs`
- [X] T334 [P] [US2-platform-macos] Write unit tests for injection in `crates/mouser-platform-macos/src/inject_tests.rs`: `CGEventPost` from an `HIDSystemState` source at `kCGHIDEventTap`, each event tagged with Mouser's `kCGEventSourceUserData`; held keys/buttons are released on drop; `secure_attention` returns `Unsupported`
- [X] T335 [US2-platform-macos] Implement `InputInject` in `crates/mouser-platform-macos/src/inject.rs`
- [X] T336 [P] [US2-platform-macos] Write unit tests for Secure Input in `crates/mouser-platform-macos/src/secure_input_tests.rs`: `IsSecureEventInputEnabled()` plus `kCGSSessionSecureInputPID` from `CGSessionCopyCurrentDictionary` yields `{pid, app_name}` via NSRunningApplication (fallback `proc_pidpath`); key absent → generic holder name
- [X] T337 [US2-platform-macos] Implement `secure_input()` in `crates/mouser-platform-macos/src/secure_input.rs`
- [X] T338 [P] [US2-platform-macos] Write unit tests for platform info in `crates/mouser-platform-macos/src/info_tests.rs`: `PlatformInfo.limitations` includes "Some games and remote-desktop apps ignore Mouser's input on this Mac" (D3 item 1)
- [X] T339 [US2-platform-macos] Implement `info()` and the `MacPlatform` facade in `crates/mouser-platform-macos/src/lib.rs`
- [X] T340 [US2-platform-macos] Gate: `cargo xtask gate --packages mouser-platform-macos --lane platform-macos` on the macOS runner
- [X] T341 [US2-platform-macos] Commit: phase boundary "US2-platform-macos" (5.3)

#### Phase 33: US3-platform-macos — As a desk user, I want my desk to keep working through sleep, network changes, and VPNs (P1)

- [X] T342 [P] [US3-platform-macos] Write unit tests for network watching in `crates/mouser-platform-macos/src/netwatch_tests.rs`: an SCDynamicStore notification on `State:/Network/Interface/.*/IPv4|IPv6` and on wake fires `watch_network`
- [X] T343 [US3-platform-macos] Implement `watch_network()` in `crates/mouser-platform-macos/src/netwatch.rs`
- [X] T344 [US3-platform-macos] Gate: `cargo xtask gate --packages mouser-platform-macos --lane platform-macos` on the macOS runner
- [X] T345 [US3-platform-macos] Commit: phase boundary "US3-platform-macos" (5.3)

#### Phase 34: US5-platform-macos — As a desk user, I want the pointer to stay put when I don't mean to cross (P2)

- [X] T346 [P] [US5-platform-macos] Write unit tests in `crates/mouser-platform-macos/src/fullscreen_tests.rs`: frontmost app's first layer-0 window on the cursor's display with bounds equal to the display bounds (±1 pt), or any window at or above the shielding level covering the display, means full-screen
- [X] T347 [US5-platform-macos] Implement `foreground_is_fullscreen()` in `crates/mouser-platform-macos/src/fullscreen.rs`
- [X] T348 [US5-platform-macos] Gate: `cargo xtask gate --packages mouser-platform-macos --lane platform-macos` on the macOS runner
- [X] T349 [US5-platform-macos] Commit: phase boundary "US5-platform-macos" (5.3)

#### Phase 35: US6-platform-macos — As a desk user, I want copy and paste to work between computers (P2)

- [X] T350 [P] [US6-platform-macos] Write `crates/mouser-platform-macos/tests/contract_platform_macos_us6.rs` running conformance rule 5 (clipboard write/read) for Text, Html, Rtf, Png and Files against `MacPlatform`, restoring the original clipboard afterwards
- [X] T351 [US6-platform-macos] Implement `Clipboard` on clipboard-rs 0.3.5 (multi-format `set`, `NSPasteboard.changeCount` for `change_count`, path normalisation) in `crates/mouser-platform-macos/src/clipboard.rs`
- [X] T352 [US6-platform-macos] Gate: `cargo xtask gate --packages mouser-platform-macos --lane platform-macos` on the macOS runner
- [X] T353 [US6-platform-macos] Commit: phase boundary "US6-platform-macos" (5.3)

#### Phase 36: US7-platform-macos — As a desk user, I want to drag files from one computer to another (P2)

- [X] T354 [P] [US7-platform-macos] Write unit tests in `crates/mouser-platform-macos/src/drag_tests.rs`: `outgoing()` returns the file URLs on the drag pasteboard while a drag is in progress; `begin_incoming(names)` starts a drag from a transparent window at the cursor whose drop reports the destination folder
- [X] T355 [US7-platform-macos] Implement `FileDrag` in `crates/mouser-platform-macos/src/drag.rs`
- [X] T356 [US7-platform-macos] Gate: `cargo xtask gate --packages mouser-platform-macos --lane platform-macos` on the macOS runner
- [X] T357 [US7-platform-macos] Commit: phase boundary "US7-platform-macos" (5.3)

#### Phase 37: US12-platform-macos — As a desk user, I want to control a computer that is at its login or system security screen (P3)

- [X] T358 [P] [US12-platform-macos] Write unit tests in `crates/mouser-platform-macos/src/login_agent_tests.rs`: installing the pre-login agent writes a LaunchAgent plist with `LimitLoadToSessionType = LoginWindow` that starts Mouser in headless mode; uninstall removes it (Story 12 sc. 1)
- [X] T359 [US12-platform-macos] Implement the login-window agent installer in `crates/mouser-platform-macos/src/login_agent.rs`
- [X] T360 [US12-platform-macos] Gate: `cargo xtask gate --packages mouser-platform-macos --lane platform-macos` on the macOS runner
- [X] T361 [US12-platform-macos] Commit: phase boundary "US12-platform-macos" (5.3)

#### Phase 38: US13-platform-macos — As a desk owner, I want to run Mouser without installing it (P3)

- [X] T362 [P] [US13-platform-macos] Write unit tests in `crates/mouser-platform-macos/src/portable_tests.rs`: in portable mode `secrets()` is file-backed under `MouserData/` and `limitations` states that macOS permissions must be granted again on each computer the drive is used on
- [X] T363 [US13-platform-macos] Implement portable mode in `crates/mouser-platform-macos/src/portable.rs`
- [X] T364 [US13-platform-macos] Gate: `cargo xtask gate --packages mouser-platform-macos --lane platform-macos` on the macOS runner
- [X] T365 [US13-platform-macos] Commit: phase boundary "US13-platform-macos" (5.3)

### Lane: platform-windows

Package `crates/mouser-platform-windows` (`windows` crate 0.62.2). Gate per phase: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` **on the Windows runner**. Reference: research R5, spikes/FINDINGS-windows-linux.md W1–W9. Hardware probes (D1) run first on the owner's Windows 11 machine; a refuted assumption stops the lane and returns to the plan.

#### Phase 39: US1-platform-windows — As a desk owner, I want to put my computers on a desk (P1)

- [ ] T366 [US1-platform-windows] Run hardware probe 1 from FINDINGS-windows-linux "Probes to schedule" on the owner's Windows 11 machine as `crates/mouser-platform-windows/probes/spike_hooks/` (LL hooks vs Raw Input for swallowed events; LL hooks with an elevated foreground window; ClipCursor across a desktop switch) and record observed output in `crates/mouser-platform-windows/probes/FINDINGS.md`
- [ ] T367 [US1-platform-windows] Run hardware probe 2 as `crates/mouser-platform-windows/probes/spike_service/` (winlogon-token agent at cold boot, on UAC and fast user switching; SendSAS with SoftwareSASGeneration 1 vs 3) and record observed output in `crates/mouser-platform-windows/probes/FINDINGS.md`
- [ ] T368 [US1-platform-windows] Run hardware probe 3 as `crates/mouser-platform-windows/probes/spike_drag/` (DoDragDrop started by an injected button-down; virtual-file drop into Explorer with IDataObjectAsyncCapability) and record observed output in `crates/mouser-platform-windows/probes/FINDINGS.md`
- [ ] T369 [P] [US1-platform-windows] Write `crates/mouser-platform-windows/tests/contract_platform_windows_us1.rs` running conformance rules 6, 8, 9 against `WinPlatform`
- [ ] T370 [P] [US1-platform-windows] Write unit tests for interface classification in `crates/mouser-platform-windows/src/net_tests.rs`: `GetAdaptersAddresses` IfType Ethernet/Wi-Fi → `PhysicalLan`; vEthernet, Hyper-V, VMware, VirtualBox, TAP, WireGuard, Tailscale, tunnel types → `Virtual`; loopback → `Loopback`
- [ ] T371 [US1-platform-windows] Implement `interfaces()` in `crates/mouser-platform-windows/src/net.rs`
- [ ] T372 [P] [US1-platform-windows] Write unit tests for the DPAPI secret store in `crates/mouser-platform-windows/src/secrets_tests.rs`: machine-scope DPAPI blobs under `%ProgramData%\Mouser\secrets\` round-trip (readable by the service before login, W4)
- [ ] T373 [US1-platform-windows] Implement `SecretStore` with machine-scope DPAPI in `crates/mouser-platform-windows/src/secrets.rs`
- [ ] T374 [P] [US1-platform-windows] Write unit tests for autostart in `crates/mouser-platform-windows/src/autostart_tests.rs`: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Mouser` set/removed; `enabled()` honours `StartupApproved` (FR-011)
- [ ] T375 [US1-platform-windows] Implement `Autostart` in `crates/mouser-platform-windows/src/autostart.rs`
- [ ] T376 [P] [US1-platform-windows] Write unit tests for permissions in `crates/mouser-platform-windows/src/permissions_tests.rs`: `permissions()` returns `not_applicable` for every kind; `open_permission_settings` returns `Unsupported`
- [ ] T377 [US1-platform-windows] Implement permissions in `crates/mouser-platform-windows/src/permissions.rs`
- [ ] T378 [US1-platform-windows] Gate: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` on the Windows runner
- [ ] T379 [US1-platform-windows] Commit: phase boundary "US1-platform-windows" (5.3)

#### Phase 40: US2-platform-windows — As a desk user, I want one mouse and keyboard to drive every computer on my desk (P1)

- [ ] T380 [P] [US2-platform-windows] Write `crates/mouser-platform-windows/tests/contract_platform_windows_us2.rs` running conformance rules 1, 2, 3, 4, 7 against `WinPlatform`
- [ ] T381 [P] [US2-platform-windows] Write unit tests for displays in `crates/mouser-platform-windows/src/displays_tests.rs`: Per-Monitor-V2 rectangles from `EnumDisplayMonitors`/`GetMonitorInfoW`; scale from `GetDpiForMonitor(MDT_EFFECTIVE_DPI)`; `physical_size_mm` from EDID when present; `WM_DISPLAYCHANGE`/`WM_DPICHANGED` fire `watch_displays`
- [ ] T382 [US2-platform-windows] Implement `displays()`/`watch_displays()` in `crates/mouser-platform-windows/src/displays.rs`
- [ ] T383 [P] [US2-platform-windows] Write unit tests for the HID ↔ scan-code table in `crates/mouser-platform-windows/src/keymap_tests.rs`
- [ ] T384 [US2-platform-windows] Implement the HID ↔ Windows scan-code/virtual-key table in `crates/mouser-platform-windows/src/keymap.rs`
- [ ] T385 [P] [US2-platform-windows] Write unit tests for capture in `crates/mouser-platform-windows/src/capture_tests.rs`: `WH_MOUSE_LL`/`WH_KEYBOARD_LL` on a dedicated thread with an I/O-free pump feeding a lock-free queue; while grabbed the hooks swallow input and the cursor is parked at the crossing edge with `SetCursorPos`, deltas computed from hook coordinates; events flagged as Mouser-injected are ignored; a watchdog reinstalls a hook that stopped delivering and raises a Problem (W1, W2)
- [ ] T386 [US2-platform-windows] Implement `InputCapture` in `crates/mouser-platform-windows/src/capture.rs`
- [ ] T387 [P] [US2-platform-windows] Write unit tests for injection in `crates/mouser-platform-windows/src/inject_tests.rs`: `SendInput` for absolute moves (via `SetCursorPos`), buttons 1–5, scroll and scan-code keys; held input released on drop; `secure_attention` forwards to the service when present, else `Unsupported`
- [ ] T388 [US2-platform-windows] Implement `InputInject` in `crates/mouser-platform-windows/src/inject.rs`
- [ ] T389 [P] [US2-platform-windows] Write unit tests for elevation detection in `crates/mouser-platform-windows/src/uipi_tests.rs`: `foreground_elevated()` is true when the foreground process integrity level is above Mouser's; the core reports "Mouser can't control <app> because it runs as administrator" (W3)
- [ ] T390 [US2-platform-windows] Implement `foreground_elevated()` in `crates/mouser-platform-windows/src/uipi.rs`
- [ ] T391 [P] [US2-platform-windows] Write unit tests for platform info in `crates/mouser-platform-windows/src/info_tests.rs`: `limitations` includes "Some games and remote-desktop apps ignore Mouser's input on this PC" (D3 item 1)
- [ ] T392 [US2-platform-windows] Implement `info()` and the `WinPlatform` facade in `crates/mouser-platform-windows/src/lib.rs`
- [ ] T393 [US2-platform-windows] Gate: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` on the Windows runner
- [ ] T394 [US2-platform-windows] Commit: phase boundary "US2-platform-windows" (5.3)

#### Phase 41: US3-platform-windows — As a desk user, I want my desk to keep working through sleep, network changes, and VPNs (P1)

- [ ] T395 [P] [US3-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/netwatch_tests.rs`: `NotifyIpInterfaceChange` and resume-from-sleep (`PBT_APMRESUMEAUTOMATIC`) fire `watch_network`
- [ ] T396 [US3-platform-windows] Implement `watch_network()` in `crates/mouser-platform-windows/src/netwatch.rs`
- [ ] T397 [US3-platform-windows] Gate: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` on the Windows runner
- [ ] T398 [US3-platform-windows] Commit: phase boundary "US3-platform-windows" (5.3)

#### Phase 42: US5-platform-windows — As a desk user, I want the pointer to stay put when I don't mean to cross (P2)

- [ ] T399 [P] [US5-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/fullscreen_tests.rs`: `SHQueryUserNotificationState` busy/D3D full-screen states, or a foreground window covering its monitor rectangle, mean full-screen
- [ ] T400 [US5-platform-windows] Implement `foreground_is_fullscreen()` in `crates/mouser-platform-windows/src/fullscreen.rs`
- [ ] T401 [US5-platform-windows] Gate: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` on the Windows runner
- [ ] T402 [US5-platform-windows] Commit: phase boundary "US5-platform-windows" (5.3)

#### Phase 43: US6-platform-windows — As a desk user, I want copy and paste to work between computers (P2)

- [ ] T403 [P] [US6-platform-windows] Write `crates/mouser-platform-windows/tests/contract_platform_windows_us6.rs` running conformance rule 5 for Text (CF_UNICODETEXT), Html ("HTML Format"), Rtf ("Rich Text Format"), Png ("PNG" + CF_DIBV5) and Files
- [ ] T404 [US6-platform-windows] Implement `Clipboard` with `AddClipboardFormatListener`, delayed rendering, and a virtual-file data object (`CFSTR_FILEDESCRIPTORW` + `CFSTR_FILECONTENTS`) for remote files in `crates/mouser-platform-windows/src/clipboard.rs`
- [ ] T405 [US6-platform-windows] Gate: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` on the Windows runner
- [ ] T406 [US6-platform-windows] Commit: phase boundary "US6-platform-windows" (5.3)

#### Phase 44: US7-platform-windows — As a desk user, I want to drag files from one computer to another (P2)

- [ ] T407 [P] [US7-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/drag_tests.rs`: `outgoing()` returns paths from an `IDropTarget` on a transparent edge window during an OLE drag; `begin_incoming(names)` starts `DoDragDrop` from a helper window with a virtual-file data object implementing `IDataObjectAsyncCapability` and reports the drop folder (as validated by hardware probe 3)
- [ ] T408 [US7-platform-windows] Implement `FileDrag` in `crates/mouser-platform-windows/src/drag.rs`
- [ ] T409 [US7-platform-windows] Gate: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` on the Windows runner
- [ ] T410 [US7-platform-windows] Commit: phase boundary "US7-platform-windows" (5.3)

#### Phase 45: US13-platform-windows — As a desk owner, I want to run Mouser without installing it (P3)

- [ ] T411 [P] [US13-platform-windows] Write unit tests in `crates/mouser-platform-windows/src/portable_tests.rs`: in portable mode `secrets()` is file-backed under `MouserData/`, no registry writes occur, and `limitations` states that portable Mouser cannot control the login screen, permission prompts, Ctrl-Alt-Del or apps running as administrator (D3 item 4)
- [ ] T412 [US13-platform-windows] Implement portable mode in `crates/mouser-platform-windows/src/portable.rs`
- [ ] T413 [US13-platform-windows] Gate: `cargo xtask gate --packages mouser-platform-windows --lane platform-windows` on the Windows runner
- [ ] T414 [US13-platform-windows] Commit: phase boundary "US13-platform-windows" (5.3)

### Lane: service-win

Package `apps/mouser-service-win` (Windows SYSTEM service + per-session agent; Complexity Tracking in plan.md). Gate per phase: `cargo xtask gate --packages mouser-service-win --lane service-win` **on the Windows runner**. Depends on the hardware probe 2 results recorded in the platform-windows lane.

#### Phase 46: US12-service-win — As a desk user, I want to control a computer that is at its login or system security screen (P3)

- [ ] T415 [P] [US12-service-win] Write unit tests for the service in `apps/mouser-service-win/src/service_tests.rs`: installs as `MouserService` (LocalSystem, automatic start); runs a headless `mouser_core::Engine` that loads machine-scope desk secrets and joins the desk before any user logs in (Story 12 sc. 2)
- [ ] T416 [US12-service-win] Implement the Windows service entry point in `apps/mouser-service-win/src/service.rs`
- [ ] T417 [P] [US12-service-win] Write unit tests for session tracking in `apps/mouser-service-win/src/session_tests.rs`: on console-session change the service duplicates the `winlogon.exe` token of the active session and starts the agent with `CreateProcessAsUserW`, restarting it on session switch (Story 12 sc. 3)
- [ ] T418 [US12-service-win] Implement session tracking and agent launch in `apps/mouser-service-win/src/session.rs`
- [ ] T419 [P] [US12-service-win] Write unit tests for the agent in `apps/mouser-service-win/src/agent_tests.rs`: the agent follows the input desktop with `OpenInputDesktop(DF_ALLOWOTHERACCOUNTHOOK)` + `SetThreadDesktop` and injects input received from the service on the login screen, UAC secure desktop and lock screen (Story 12 sc. 2–3)
- [ ] T420 [US12-service-win] Implement the per-session agent in `apps/mouser-service-win/src/agent.rs`
- [ ] T421 [P] [US12-service-win] Write unit tests for service ↔ agent IPC in `apps/mouser-service-win/src/ipc_tests.rs`: a named pipe restricted to SYSTEM and the agent's session carries input and secure-attention requests
- [ ] T422 [US12-service-win] Implement the service ↔ agent IPC in `apps/mouser-service-win/src/ipc.rs`
- [ ] T423 [P] [US12-service-win] Write unit tests for Ctrl-Alt-Del in `apps/mouser-service-win/src/sas_tests.rs`: a secure-attention request calls `SendSAS(FALSE)`; when the `SoftwareSASGeneration` policy is not 1 or 3 a `platform_limitation` Problem says Group Policy blocks it (Story 12 sc. 4)
- [ ] T424 [US12-service-win] Implement `SendSAS` handling in `apps/mouser-service-win/src/sas.rs`
- [ ] T425 [US12-service-win] Gate: `cargo xtask gate --packages mouser-service-win --lane service-win` on the Windows runner
- [ ] T426 [US12-service-win] Commit: phase boundary "US12-service-win" (5.3)

### Lane: platform-linux

Package `crates/mouser-platform-linux` (`x11` and `wayland` modules). Gate per phase: `cargo xtask gate --packages mouser-platform-linux --lane platform-linux` **on the Ubuntu runner** (X11 under Xvfb; Wayland unit tests against mocked D-Bus, real Wayland behaviour via the hardware probes). Reference: research R6, FINDINGS-windows-linux X1–X4, L1–L9.

#### Phase 47: US1-platform-linux — As a desk owner, I want to put my computers on a desk (P1)

- [X] T427 [US1-platform-linux] Run hardware probe 4 from FINDINGS-windows-linux on Ubuntu 24.04 (GNOME 46), Ubuntu 26.04 (GNOME 50), Fedora (GNOME 51) and Plasma 6.7/6.8 as `crates/mouser-platform-linux/probes/spike_portals/` (InputCapture + RemoteDesktop start; persistence across logout/reboot with a registered app ID; clipboard per session type) and record observed output in `crates/mouser-platform-linux/probes/FINDINGS.md`
- [X] T428 [US1-platform-linux] Run hardware probes 5 and 6 as `crates/mouser-platform-linux/probes/spike_lock_drag_x11/` (RemoteDesktop/EIS on the lock screen; InputCapture during a drag; X11 grab failures with menus open; XFixes barriers for corners) and record observed output in `crates/mouser-platform-linux/probes/FINDINGS.md`
- [X] T429 [P] [US1-platform-linux] Write `crates/mouser-platform-linux/tests/contract_platform_linux_us1.rs` running conformance rules 6, 8, 9 against `LinuxPlatform`
- [X] T430 [P] [US1-platform-linux] Write unit tests for session detection in `crates/mouser-platform-linux/src/session_tests.rs`: `XDG_SESSION_TYPE`/`WAYLAND_DISPLAY` select X11 or Wayland; on Wayland the InputCapture and RemoteDesktop portal versions and `SupportedCapabilities` are read and each missing capability becomes a `Limitation` with the plain-language text from spec Assumptions (D3 item 2)
- [X] T431 [US1-platform-linux] Implement session and capability detection in `crates/mouser-platform-linux/src/session.rs`
- [X] T432 [P] [US1-platform-linux] Write unit tests for permissions in `crates/mouser-platform-linux/src/permissions_tests.rs`: on Wayland `input_capture_portal` and `remote_desktop_portal` report granted when a stored restore token is valid, else missing; `open_permission_settings` starts the portal session so the desktop shows its dialog; on X11 all kinds are `not_applicable`
- [X] T433 [US1-platform-linux] Implement permissions in `crates/mouser-platform-linux/src/permissions.rs`
- [X] T434 [P] [US1-platform-linux] Write unit tests for interface classification in `crates/mouser-platform-linux/src/net_tests.rs`: `/sys/class/net/<if>/device` present and `type` = 1 (Ethernet) or a `wireless/` directory → `PhysicalLan`; tun/tap/wg/tailscale/docker/br-/virbr/veth/vmnet → `Virtual`; `lo` → `Loopback`
- [X] T435 [US1-platform-linux] Implement `interfaces()` in `crates/mouser-platform-linux/src/net.rs`
- [X] T436 [P] [US1-platform-linux] Write unit tests for the secret store in `crates/mouser-platform-linux/src/secrets_tests.rs`: Secret Service round-trip when available, else a 0600 file in the config dir
- [X] T437 [US1-platform-linux] Implement `SecretStore` in `crates/mouser-platform-linux/src/secrets.rs`
- [X] T438 [P] [US1-platform-linux] Write unit tests for autostart in `crates/mouser-platform-linux/src/autostart_tests.rs`: `set(true)` writes `~/.config/autostart/com.mouser.app.desktop`; `set(false)` removes it (FR-011)
- [X] T439 [US1-platform-linux] Implement `Autostart` in `crates/mouser-platform-linux/src/autostart.rs`
- [X] T440 [P] [US1-platform-linux] Write unit tests for tray availability in `crates/mouser-platform-linux/src/tray_tests.rs`: without `org.kde.StatusNotifierWatcher` on the session bus a `Limitation` says the tray icon needs the AppIndicator extension (D3 item 5)
- [X] T441 [US1-platform-linux] Implement tray availability detection in `crates/mouser-platform-linux/src/tray.rs`
- [X] T442 [US1-platform-linux] Gate: `cargo xtask gate --packages mouser-platform-linux --lane platform-linux` on the Ubuntu runner
- [X] T443 [US1-platform-linux] Commit: phase boundary "US1-platform-linux" (5.3)

#### Phase 48: US2-platform-linux — As a desk user, I want one mouse and keyboard to drive every computer on my desk (P1)

- [X] T444 [P] [US2-platform-linux] Write `crates/mouser-platform-linux/tests/contract_platform_linux_us2.rs` running conformance rules 1, 2, 3, 4, 7 against the X11 backend under Xvfb
- [X] T445 [P] [US2-platform-linux] Write unit tests for X11 displays in `crates/mouser-platform-linux/src/x11/displays_tests.rs`: RandR outputs give rectangles, scale and physical mm; RandR screen-change events fire `watch_displays`
- [X] T446 [US2-platform-linux] Implement X11 `displays()`/`watch_displays()` in `crates/mouser-platform-linux/src/x11/displays.rs`
- [X] T447 [P] [US2-platform-linux] Write unit tests for the HID ↔ evdev keycode table in `crates/mouser-platform-linux/src/keymap_tests.rs`
- [X] T448 [US2-platform-linux] Implement the HID ↔ evdev keycode table in `crates/mouser-platform-linux/src/keymap.rs`
- [X] T449 [P] [US2-platform-linux] Write unit tests for X11 capture in `crates/mouser-platform-linux/src/x11/capture_tests.rs`: XInput2 `XI_RawMotion` on the root window; grabbing uses `XGrabPointer`/`XGrabKeyboard` on Mouser's window with `XWarpPointer` parking, retried with backoff while another client holds a grab
- [X] T450 [US2-platform-linux] Implement X11 `InputCapture` with x11rb 0.14.0 in `crates/mouser-platform-linux/src/x11/capture.rs`
- [X] T451 [P] [US2-platform-linux] Write unit tests for X11 injection in `crates/mouser-platform-linux/src/x11/inject_tests.rs`: XTest fake motion/button/key with `XTestGrabControl(True)`; held input released on drop
- [X] T452 [US2-platform-linux] Implement X11 `InputInject` in `crates/mouser-platform-linux/src/x11/inject.rs`
- [X] T453 [P] [US2-platform-linux] Write unit tests for Wayland capture in `crates/mouser-platform-linux/src/wayland/capture_tests.rs`: ashpd 0.13.13 InputCapture v2 session with zones and edge barriers, `persist_mode = 2` and the stored single-use restore token replaced after every `Start`; app registered via `org.freedesktop.host.portal.Registry.Register` with app ID `com.mouser.app`; libei events via reis 0.7.1 become `LocalInput`; a pending dialog raises "Approve Mouser on <computer>" (L1–L3)
- [X] T454 [US2-platform-linux] Implement Wayland `InputCapture` in `crates/mouser-platform-linux/src/wayland/capture.rs`
- [X] T455 [P] [US2-platform-linux] Write unit tests for Wayland injection in `crates/mouser-platform-linux/src/wayland/inject_tests.rs`: RemoteDesktop portal session with EIS (persisted), emulated pointer/keyboard/scroll; held input released on drop
- [X] T456 [US2-platform-linux] Implement Wayland `InputInject` in `crates/mouser-platform-linux/src/wayland/inject.rs`
- [X] T457 [P] [US2-platform-linux] Write unit tests for Wayland displays in `crates/mouser-platform-linux/src/wayland/displays_tests.rs`: InputCapture zones give display rectangles; scale from wl_output where available
- [X] T458 [US2-platform-linux] Implement Wayland `displays()` in `crates/mouser-platform-linux/src/wayland/displays.rs`
- [X] T459 [US2-platform-linux] Implement the `LinuxPlatform` facade selecting the X11 or Wayland module and `info()` in `crates/mouser-platform-linux/src/lib.rs`
- [X] T460 [US2-platform-linux] Gate: `cargo xtask gate --packages mouser-platform-linux --lane platform-linux` on the Ubuntu runner
- [X] T461 [US2-platform-linux] Commit: phase boundary "US2-platform-linux" (5.3)

#### Phase 49: US3-platform-linux — As a desk user, I want my desk to keep working through sleep, network changes, and VPNs (P1)

- [X] T462 [P] [US3-platform-linux] Write unit tests in `crates/mouser-platform-linux/src/netwatch_tests.rs`: rtnetlink `RTMGRP_LINK | RTMGRP_IPV4_IFADDR | RTMGRP_IPV6_IFADDR` events and logind `PrepareForSleep(false)` fire `watch_network`
- [X] T463 [US3-platform-linux] Implement `watch_network()` in `crates/mouser-platform-linux/src/netwatch.rs`
- [X] T464 [US3-platform-linux] Gate: `cargo xtask gate --packages mouser-platform-linux --lane platform-linux` on the Ubuntu runner
- [X] T465 [US3-platform-linux] Commit: phase boundary "US3-platform-linux" (5.3)

#### Phase 50: US5-platform-linux — As a desk user, I want the pointer to stay put when I don't mean to cross (P2)

- [X] T466 [P] [US5-platform-linux] Write unit tests in `crates/mouser-platform-linux/src/x11/fullscreen_tests.rs`: on X11 `_NET_ACTIVE_WINDOW` with `_NET_WM_STATE_FULLSCREEN` means full-screen; on Wayland `foreground_is_fullscreen()` is always false and the "no full-screen blocking on Wayland" limitation is listed (D3 item 2)
- [X] T467 [US5-platform-linux] Implement `foreground_is_fullscreen()` in `crates/mouser-platform-linux/src/x11/fullscreen.rs`
- [X] T468 [US5-platform-linux] Gate: `cargo xtask gate --packages mouser-platform-linux --lane platform-linux` on the Ubuntu runner
- [X] T469 [US5-platform-linux] Commit: phase boundary "US5-platform-linux" (5.3)

#### Phase 51: US6-platform-linux — As a desk user, I want copy and paste to work between computers (P2)

- [X] T470 [P] [US6-platform-linux] Write `crates/mouser-platform-linux/tests/contract_platform_linux_us6.rs` running conformance rule 5 against the X11 clipboard under Xvfb (`UTF8_STRING`, `text/html`, `text/rtf`, `image/png`, `text/uri-list` + `x-special/gnome-copied-files`)
- [X] T471 [US6-platform-linux] Implement the X11 `Clipboard` (CLIPBOARD ownership, TARGETS, INCR, XFixes owner-change counter, remote files staged in a local cache dir before offering paths) in `crates/mouser-platform-linux/src/x11/clipboard.rs`
- [X] T472 [P] [US6-platform-linux] Write unit tests in `crates/mouser-platform-linux/src/wayland/clipboard_tests.rs`: the portal Clipboard on the RemoteDesktop/InputCapture session is used first; on KDE `ext-data-control` (wl-clipboard-rs 0.9.4) is the fallback; on GNOME before 51 the "cannot share the clipboard from this computer while it shares its mouse" limitation is listed (L4, D3 item 2)
- [X] T473 [US6-platform-linux] Implement the Wayland `Clipboard` in `crates/mouser-platform-linux/src/wayland/clipboard.rs`
- [X] T474 [US6-platform-linux] Gate: `cargo xtask gate --packages mouser-platform-linux --lane platform-linux` on the Ubuntu runner
- [X] T475 [US6-platform-linux] Commit: phase boundary "US6-platform-linux" (5.3)

#### Phase 52: US7-platform-linux — As a desk user, I want to drag files from one computer to another (P2)

- [X] T476 [P] [US7-platform-linux] Write unit tests in `crates/mouser-platform-linux/src/drag_tests.rs`: `file_drag()` returns `None` and `limitations` includes "Dragging files between computers isn't available on Linux; use copy and paste" (D3 item 3)
- [X] T477 [US7-platform-linux] Implement the file-drag limitation in `crates/mouser-platform-linux/src/drag.rs`
- [X] T478 [US7-platform-linux] Gate: `cargo xtask gate --packages mouser-platform-linux --lane platform-linux` on the Ubuntu runner
- [X] T479 [US7-platform-linux] Commit: phase boundary "US7-platform-linux" (5.3)

#### Phase 53: US13-platform-linux — As a desk owner, I want to run Mouser without installing it (P3)

- [X] T480 [P] [US13-platform-linux] Write unit tests in `crates/mouser-platform-linux/src/portable_tests.rs`: in portable mode secrets are file-backed under `MouserData/`, no autostart file is written, and on Wayland `limitations` states that the desktop keeps Mouser's permission after the drive is removed (L9)
- [X] T481 [US13-platform-linux] Implement portable mode in `crates/mouser-platform-linux/src/portable.rs`
- [X] T482 [US13-platform-linux] Gate: `cargo xtask gate --packages mouser-platform-linux --lane platform-linux` on the Ubuntu runner
- [X] T483 [US13-platform-linux] Commit: phase boundary "US13-platform-linux" (5.3)

---

## Phase 54: Convergence

**Purpose**: Cross-package work, packaging, and the feature's entire E2E obligation (J1, plan D5), after every lane has passed its gates (5.2). Runs in the main tree.

### Cross-package tasks

- [ ] T484 Verify every lane's last phase gate passed: each `specs/001-mouser-v1/evidence/<lane>.jsonl` has a passing record newer than the lane's last commit (verification only)
- [X] T485 Configure macOS bundling in `apps/mouser-app/tauri.conf.json` and `apps/mouser-app/Info.plist`: `NSLocalNetworkUsageDescription`, `NSBonjourServices = ["_mouser._udp"]`, hardened runtime, Developer ID signing identity from `APPLE_SIGNING_IDENTITY`, notarization via `APPLE_API_KEY`/`APPLE_API_ISSUER`, and the bundled login-window agent from `mouser-platform-macos`
- [X] T486 Configure the Windows NSIS installer in `apps/mouser-app/windows/installer.nsi` (referenced from `tauri.conf.json`): install `mouser-service-win` as `MouserService`, set `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System\SoftwareSASGeneration = 1`, Per-Monitor-V2 manifest, and a portable zip target containing an empty `MouserData/`
- [X] T487 Configure Linux packaging in `apps/mouser-app/linux/com.mouser.app.desktop` and the AppImage/deb targets in `tauri.conf.json` with app ID `com.mouser.app`
- [X] T488 Create `.github/workflows/release.yml`: on tag `v*`, build and bundle on macOS (signed + notarized + stapled, D4), Windows (unsigned until Authenticode is provided, D4) and Ubuntu; sign updater bundles with the minisign key from secret `TAURI_SIGNING_PRIVATE_KEY`; generate `latest.json` matching `contracts/release-manifest.schema.json`; upload all assets to the GitHub release
- [X] T489 Generate the minisign updater key pair with `cargo tauri signer generate`, store the private key as repository secret `TAURI_SIGNING_PRIVATE_KEY` (owner confirms), and set the public key in `crates/mouser-core/src/release/pubkey.rs` and `apps/mouser-app/tauri.conf.json`
- [X] T506 [P] Write tests for the app's `--headless` mode in `apps/mouser-app/src/headless_tests.rs`: argument parsing selects headless; headless starts the engine with `EngineParams.secrets = Some(FileSecrets)` and the machine config dir (macOS `/Library/Application Support/Mouser`), creates no window or tray, and runs until signalled (foundational-notes "core US12 decisions")
- [X] T507 Implement `--headless` in `apps/mouser-app/src/headless.rs` and the binary entry point (the macOS LaunchAgent runs `[exe, "--headless"]`)
- [X] T508 [P] Write tests for "Allow control at the login screen" (macOS): a settings action that runs `Engine::export_machine_scope` and `LoginAgent::system(exe).install()` through one OS administrator prompt; refusal leaves nothing changed and is explained; the action is offered only on macOS
- [X] T509 Implement "Allow control at the login screen" in `apps/mouser-app/src/login_screen.rs` and the settings UI (never run the privileged step in tests)
- [ ] T490 Run `cargo xtask gate --all --lane convergence` after the packaging changes (verification only; must stay green before E2E)

### E2E — J1 "two real computers on a LAN"

- [ ] T491 Wire the J1 environment: write `apps/mouser-cli/tests/e2e/README.md` naming the two real machines (one macOS, one Windows or Linux), how each installs the release-workflow build, and how the runner reaches both (SSH with the `mouser` CLI on each; OS-level input on the controller driven by `osascript` on macOS / `xdotool` on Linux / PowerShell `SendInput` on Windows)
- [ ] T492 Assemble `apps/mouser-cli/tests/e2e/features/e2e_j1_two_computers.feature` from spec.md Story 1 scenarios 2–4 and 9, Story 2 scenarios 2–3 and 8, and Story 3 scenarios 1–2, as one journey: install, form a desk with approval, cross both ways and type, panic, sleep/wake one computer, cross again
- [ ] T493 Implement the J1 runner `apps/mouser-cli/tests/e2e_j1.rs` (cucumber, `harness = false`, `fail_on_skipped()`, registered with `required-features = ["e2e"]` so it never runs in lane gates) binding the feature to the lane-authored steps in `apps/mouser-cli/tests/e2e/steps/`
- [ ] T494 Run J1 on the two real machines, fix any failure at its source (no timeouts widened, no retries added — 4.3), and record the run with `cargo xtask gate --e2e j1 --lane convergence`

- [ ] T495 Gate: `cargo xtask gate --all --lane convergence` green on all three CI runners and the J1 evidence record passing
- [ ] T496 Commit: phase boundary "Convergence" (5.3)

---

## Phase 55: Polish & Cross-Cutting Concerns

**Purpose**: Keep `docs/` matching what was delivered (constitution Development Workflow), and final validation.

- [ ] T497 Update `docs/product-requirements.md` so every Must/Nice item states what v1 delivers, including the five accepted platform limitations and any behaviour changed during implementation
- [ ] T498 Update `docs/technical-decisions.md` with the as-built crates, versions, CI layout, signing state and probe outcomes from `crates/mouser-platform-*/probes/FINDINGS.md`
- [ ] T499 Write `docs/cli.md`: the operator reference for every `mouser` command, flag, `--json` output and exit code as implemented (from `contracts/cli.md`)
- [ ] T500 Write `docs/configuration.md`: every `config.toml` key, default and valid values as implemented (from `contracts/config.md`), plus file locations per OS and portable mode
- [ ] T501 Write `docs/troubleshooting.md`: each Problem kind's message, cause and fix, the diagnostic log location and record format, and the platform limitations
- [ ] T502 Review all files under `docs/` against the delivered behaviour and record "docs/ reviewed, no drift" or the corrections made in `specs/001-mouser-v1/evidence/docs-review.md` (verification only)
- [ ] T503 Run the real-machine validation in `specs/001-mouser-v1/quickstart.md` sections 2–8 and record each section's outcome in `specs/001-mouser-v1/evidence/quickstart.md` (verification only)
- [ ] T504 Gate: `cargo xtask gate --all --lane polish` green on all three CI runners
- [ ] T505 Commit: phase boundary "Polish" (5.3)

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)** → **Foundational (Phase 2)** → **PARALLEL GROUP (Phases 3–53)** → **Convergence (54)** → **Polish (55)**
- No lane phase starts before the Foundational gate passes and lane disjointness is verified.

### Lane order (within each lane, sequential in story order)

| Lane | Phases | Runs on |
|---|---|---|
| core | 3–15 (US1…US13) | any |
| app | 16–25 (US1, US2, US4, US5, US7, US8, US10, US11, US12, US13) | macOS locally; CI all |
| cli | 26–30 (US1, US2, US9, US10, US12) | any |
| platform-macos | 31–38 (US1, US2, US3, US5, US6, US7, US12, US13) | macOS |
| platform-windows | 39–45 (US1, US2, US3, US5, US6, US7, US13) | Windows (+ owner's PC for probes) |
| service-win | 46 (US12) | Windows; after platform-windows probe 2 |
| platform-linux | 47–53 (US1, US2, US3, US5, US6, US7, US13) | Ubuntu (+ owner's Linux desktops for probes) |

### Cross-lane dependencies

- Lanes compile against the frozen `mouser-proto` and `mouser-platform` only; `app` and `cli` tests use `mouser-core` and `VirtualPlatform`, so they need the matching core phase merged first (e.g. US1-app after US1-core).
- `service-win` Phase 46 needs platform-windows Phase 39 probe 2 recorded.
- A refuted hardware probe (Phases 39, 47) halts that lane and returns to the plan (contracts are frozen; changes are plan-level, 3.3).

### Story completion

A story is complete when all its `USn-*` phases and Convergence have passed (5.2). US3, US6, US9 have no app phase; US4, US8, US11 have no platform phase on some lanes because their behaviour lives in core/app.

## Parallel Example: after Foundational

```bash
# Seven concurrent lanes (separate worktrees, each bootstrapped with
#   cargo fetch --locked && cargo xtask bootstrap):
lane core              → Phase 3  (US1-core)
lane app               → Phase 16 (US1-app)          # after US1-core merged
lane cli               → Phase 26 (US1-cli)          # after US1-core merged
lane platform-macos    → Phase 31 (US1-platform-macos)
lane platform-windows  → Phase 39 (probes, then US1-platform-windows)
lane platform-linux    → Phase 47 (probes, then US1-platform-linux)
lane service-win       → Phase 46 (after probe 2)
```

Within a phase, `[P]` test tasks for different files can be written together; each implementation task follows its own test task.

## Implementation Strategy

### MVP (P1 stories US1–US3)

1. Setup → Foundational.
2. core Phases 3–5, app Phases 16–17, cli Phases 26–27, platform-macos Phases 31–33 (plus Windows/Linux Phases 39–41 / 47–49 as runners and probes allow).
3. Convergence packaging for macOS + J1 on two machines → a usable ShareMouse replacement for the reliability problem that started this project.

### Incremental delivery

Add P2 (US4–US8) then P3 (US9–US13) phases lane by lane; each lane phase is gated and committed, so any lane can pause without blocking the others.

## Notes

- Every implementation task is preceded by its own test task (5.1 rule 3); verification-only tasks (gates, commits, probes, reviews) have size 0.
- Field constraints from data-model.md are quoted in the tasks that implement them.
- Do not add behaviour, error handling or defensive branches beyond spec.md (5.5, 5.6); unspecified failures crash loudly.
- Touched-file ownership (5.7): fix any defect found in a file you modify, with a test, in the same task.
