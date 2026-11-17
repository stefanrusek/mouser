# Tasks: Coexist With Apps That Capture the Mouse

**Input**: `specs/003-foreign-cursor-capture/` (plan.md, spec.md, research.md, data-model.md, contracts/, quickstart.md, spikes/FINDINGS.md)

**Tests**: Required. FR-011 demands regression tests for capture-while-remote and move-onto-the-Mac; the repo gate is 100% line coverage (cargo-llvm-cov), so every new production line has a test task in the same phase.

**Execution mode**: Standard (plan.md). One lane of real work, so no [P] across packages is claimed beyond independent files.

**Format**: `- [ ] Txxx [P?] [Story?] Description with file path`

Paths: `M` = `crates/mouser-platform-macos/src`, `A` = `apps/mouser-app/src`, `S` = `specs/002-mouser-v2/contracts`.

**Evidence note**: FINDINGS S5 shows the real symptom is visibility flipping to visible with Mouser unaware (position did not move). The guard MUST therefore re-read visibility first; position and association follow the plan. S2 and S4 are unobserved: D2 has a fallback (T014) and D3 is a constant.

## Phase 1: Setup

- [X] T001 Confirm baseline is green before any change: run `cargo test -p mouser-platform-macos`, `cargo test -p mouser-app`, `cargo clippy --workspace --all-targets`, `cargo fmt --check`; record failures (if any) in `specs/003-foreign-cursor-capture/spikes/FINDINGS.md` as pre-existing.
- [X] T002 Add the `cursor.foreign_change` event to the log catalog: new row in `S/log-events.md` and a schema entry in `S/log-events.schema.json` (fields `aspect` enum `visible|associated|position`, `phase` enum `remote|arrived`, `corrected` bool, `count` integer; level `warn`; no peer; `additionalProperties: false`), exactly per `contracts/log-events-delta.md`.
- [X] T003 Extend `contract_log_events` and `contract_log_privacy` tests (locate with `grep -rn contract_log_events crates apps`) to cover the new row, including that no field can carry an app name or window title.

## Phase 2: Foundational (blocks all stories)

**Debt items from plan.md survey; behaviour-preserving except where noted.**

- [X] T004 Create `M/cursor.rs`: move the `Cursor` trait and `SystemCursor` (including the private SkyLight call) out of `M/capture.rs`; declare `mod cursor;` in `M/lib.rs`; no behaviour change. Existing tests must still pass unchanged.
- [X] T005 Change the `Cursor` trait in `M/cursor.rs` per `contracts/macos-cursor-guard.md`: `associate`, `hide`, `show` return `Result<()>` (remove their `expect`s), add `fn visible(&self) -> bool` backed by `CGCursorIsVisible`; make `allow_hiding_in_background` return an error instead of `assert_eq!`. Update every `Cursor` implementor and test double (`M/capture_tests.rs` and any other `impl Cursor`) and callers in `M/capture.rs`.
- [X] T006 In `M/capture.rs`, replace `stop()`'s `.unwrap()` on `set_grabbed(false)` with a logged ignore (cannot return an error from `Drop`); add a test in `M/capture_tests.rs` where the scripted cursor refuses and `stop()`/drop does not panic.
- [X] T007 In `M/capture.rs`, delete the duplicate `MacCapture.grabbed` field and use the single `AtomicBool` in `State`; keep `set_grabbed` with an unchanged value a no-op (add a test if none exists).
- [X] T008 Move the diagnostics sink types (`DiagLevel`, `DiagValue`, `Diagnostics` alias, sink registration) from `crates/mouser-platform-windows/src/diag.rs` to `crates/mouser-platform` as types only, re-export from the Windows crate so its call sites compile unchanged. Fallback per plan debt item 3: if this touches the frozen trait surface, instead add a small duplicate type in a new `M/diag.rs`, and record the choice in `specs/003-foreign-cursor-capture/research.md`. Decision criterion: if the Cursor/platform trait surface is unchanged (types only), do the move into `mouser-platform`; otherwise use the fallback. Ruling: moving sink types (types only, no trait or method signature change) into `mouser-platform` does NOT count as touching the frozen trait surface; if it would add or change any trait, take the fallback.
- [X] T009 Outcome A (T008 moved the types into `mouser-platform`): the sink holder reuses the shared type and this task reduces to the holder and its test. Outcome B (T008 took the fallback), add `M/diag.rs` (macOS sink holder: `set_diagnostics(Option<Diagnostics>)` and an internal `emit(level, name, fields)`), mirroring the Windows pattern; test in `M/diag_tests.rs` that a set sink receives events and an unset sink drops them silently.
- [X] T010 Wire the macOS sink in `A/diagnostics.rs` so it forwards any catalog name with typed fields to `log.write`, as for Windows; replace the Windows-typed coupling at line 8 with the shared type (outcome A) or the local `M/diag.rs` type (outcome B); update `A/diagnostics_tests.rs`.
- [X] T011 Verify the foundation: `cargo test -p mouser-platform-macos --test contract_platform_macos_v2` (rule 10) and `cargo test -p mouser-app` pass with no test loosened (SC-006).

**Checkpoint**: trait fallible and visible-aware, sink reachable, catalog row present.

## Phase 3: User Story 1 - Pointer on the other computer while a game captures the Mac mouse (P1)

**Goal**: while grabbed, Mac cursor stays hidden and still; foreign changes are corrected within 100 ms; foreign motion is not forwarded.

**Independent test**: scripted foreign actor on the `Cursor` seam captures and releases while grabbed; cursor ends hidden, parked, and no `LocalInput::Motion` is emitted for the foreign move.

- [X] T012 [US1] Create `M/cursor_guard.rs` with the timing constants `TICK = 20 ms`, `REASSOCIATE = 250 ms`, `SETTLE = 1000 ms`, `COALESCE = 1 s` in one place, and the types `Phase { Remote, Arrived, Idle }`, `Expectation { phase, park, hides, arrived_at }`, `Observation { visible, position, at }`, `Correction` per `data-model.md`; declare `mod cursor_guard;` in `M/lib.rs`.
- [X] T013 [US1] In `M/cursor_guard.rs`, implement pure `decide(&Expectation, &Observation, own_echo) -> Vec<Correction>` for the `Remote` rows of the data-model table: visible -> show `hides` times, then hide once, then associate(false); position differs from `park` by more than 1 point and not own echo -> warp to `park`, associate(false); no deviation -> empty. Unit tests in `M/cursor_guard_tests.rs` for each row, including FR-004 (own echo is not foreign) and visibility-first ordering.
- [X] T014 [US1] In `M/capture.rs` `handle()`, while grabbed, drop motion using the filter in force (default: drop motion whose `kCGEventSourceStateID` is not the HID system state, research D2; T035 may switch to the plausibility fallback after the S2 result) so foreign moves are not forwarded (FR-002, FR-003, contract rule 6). Add the plausibility fallback (drop motion whose absolute position equals the guard's last park or foreign correction, reusing `Echo` in `M/echo.rs`) behind one function so R2 is a one-line switch. Tests in `M/capture_tests.rs`/`M/tap_tests.rs`: foreign-source motion while grabbed yields no `LocalInput::Motion`; HID motion still does; ungrabbed behaviour unchanged. Label the foreign-source test explicitly "encodes UNVERIFIED assumption (probe S2 unobserved): the scripted foreign actor models the assumed source-state behaviour and cannot prove the real game behaves that way; the owner check in T035 is the real verification".
- [X] T015 [US1] In `M/cursor_guard.rs`, implement `tick(&mut Guard, &dyn Cursor, now)` applying `decide` corrections through the `Cursor` seam: record every applied warp in the `Echo` ring; re-assert `associate(false)` every `REASSOCIATE` and on any deviation; a refused OS call is logged and retried next tick, never panics, never unbalances `hides` (contract rule 4, D4). Tests with a scripted foreign actor: capture (hide+warp to centre), release (show), repeated re-capture, refused call then success; assert end state hidden, parked, `hides == 1`.
- [X] T016 [US1] In `M/cursor_guard.rs`, implement the guard thread wrapper (spawn on `Remote`, sleeps `TICK`, no lock shared with the tap callback, stoppable and joined by `stop()` and `Drop`; contract rules 1 and 2). Keep the wrapper to a few lines around `tick` so coverage stays at 100%; add start/stop/join tests including drop while running.
- [X] T017 [US1] In `M/capture.rs` `set_grabbed(true)`: after warp/associate(false)/hide, set `Expectation{Remote, park, hides: 1}` and start the guard; `set_grabbed(false)` and `stop()` end it. Test with the scripted cursor that a foreign visible flip right after grab is corrected within one tick (FR-001, SC-002) and that a later `set_grabbed(false)` leaves visible, associated, `hides == 0` (rule 10, contract rule 5, Story 1 sc.4).
- [X] T018 [US1] Regression test for Story 1 end to end on the seam in `crates/mouser-platform-macos/tests/contract_platform_macos_003.rs` (new; shared by T018, T022, T025): the S5 sequence (visible flips to 1 while grabbed, position unchanged) and the S1 sequence (position jumps to centre), both restored, then ungrab lands normally (FR-011). Also assert the controller state (which computer has the pointer) is unchanged after a foreign move, and cover the crash-while-capturing-in-Remote case (guard dropped while `Remote`: cursor shown, associated, `hides` balanced).
- [X] T032 [US1] Space switch and similar foreground changes: add a guard test where the scripted cursor's state is reset out from under the guard (as after a Space switch) and the next tick restores the expected state (spec edge case). `M/cursor_guard_tests.rs`.
- [X] T033 [US1] Two capturing apps: scripted cursor with two alternating foreign actors (one hides, one shows/warps); assert the end state is hidden and parked and the log record is coalesced. `M/cursor_guard_tests.rs`.
- [X] T034 [US1] Mac as the controlled computer: test that when the Mac is the guest (never grabbed, pointer owned by the other side), the guard is not started and a foreign capture causes no correction, no forwarded input and no log record. `M/capture_tests.rs`.

**Checkpoint**: Story 1 works and is independently testable (MVP).

## Phase 4: User Story 2 - Bringing the pointer onto the Mac while a game holds the mouse (P2)

**Goal**: on arrival, Mouser takes the cursor back completely and holds it through the 1 s settle window, then yields.

**Independent test**: scripted game re-hides/disassociates right after ungrab; within the window the cursor is shown and associated again, with no flicker; after `SETTLE` the app wins.

- [ ] T019 [US2] In `M/cursor_guard.rs`, extend `decide` for the `Arrived` rows: not visible -> show, associate(true); position differs from `park` -> no correction; `Idle` -> none. Unit tests for each row in `M/cursor_guard_tests.rs`.
- [ ] T020 [US2] In `M/cursor_guard.rs`, add the `Remote -> Arrived -> Idle` transitions: `set_grabbed(false)` sets `Arrived` with `arrived_at = now`; `tick` moves to `Idle` once `now - arrived_at >= SETTLE` and the thread then stops; `set_grabbed(true)` during `Arrived` returns to `Remote`. Tests with a fake clock: game re-hides at 100 ms (corrected), game re-hides at 1200 ms (not corrected, FR-006), crossing back and forth quickly, and a boundary test at exactly `now - arrived_at == SETTLE` (yield happens at `>=`, so the guard is Idle and does not correct).
- [ ] T021 [US2] In `M/capture.rs` `set_grabbed(false)`: land at the landing point, associate(true), show `hides` times, then hand `Arrived` to the guard (replacing today's partial take-back). Test: after ungrab with a game that re-captures on the focus change, cursor is visible and associated within one tick (SC-004, Story 2 sc.1).
- [ ] T022 [US2] Regression test for Story 2 in `crates/mouser-platform-macos/tests/contract_platform_macos_003.rs`: game holds capture, pointer arrives, capture broken; game releases afterwards and cursor behaves normally with no action (Story 2 sc.2); recovery key during the window leaves a usable state (sc.3). Re-run rule 10.

**Checkpoint**: Stories 1 and 2 both pass.

## Phase 5: User Story 3 - Recovery keys always work and Mouser never fights silently (P3)

**Goal**: every foreign change and correction is in the log; panic and QuickJump work with a capture active.

**Independent test**: cause a disagreement, assert one coalesced `cursor.foreign_change` record and that the recovery path ends with the cursor visible only on the pointer-owning computer.

- [x] T023 [US3] In `M/cursor_guard.rs`, record deviations per aspect (`visible`, `associated`, `position`) with phase and corrected flag, and emit one `cursor.foreign_change` (warn; fields `aspect`, `phase`, `corrected`, `count`) per aspect when the `COALESCE` window closes or on ungrab/stop, via the T009 sink (the `associated` aspect is inferred, per `contracts/log-events-delta.md`). `associated` is reported only when a re-assert follows a visible/position deviation in the same tick; Mouser's own corrections never produce a record; empty correction lists are not logged; no app or window names. Tests in `M/cursor_guard_tests.rs`: a game re-capturing every tick yields one record per second with the right `count`; own echo yields none; flush on stop.
- [x] T024 [US3] Pass the sink into the guard from `MacCapture` (via `M/diag.rs`) and confirm the app wiring from T010 reaches `log.write`: add an app-level test in `A/diagnostics_tests.rs` that a macOS-sink `cursor.foreign_change` lands in the log with the closed field set and passes the catalog schema.
- [x] T025 [US3] Recovery tests in `crates/mouser-platform-macos/tests/contract_platform_macos_003.rs`: with a foreign capture active, panic (`Escape` x2) and QuickJump (`Ctrl+Alt+Shift+J`) paths end with the cursor visible only where the pointer is (`set_grabbed(false)` or `(true)` per target) and `hides` balanced (FR-008, Story 3 sc.2).
- [x] T026 [US3] Run `contract_log_events` and `contract_log_privacy` (T003) against the real emitted record shape; fix any drift between emitter and catalog.

**Checkpoint**: all three stories independently functional and observable.

## Phase 6: Polish and cross-cutting

- [ ] T027 [P] Document the Windows/Linux limitation (research D6, FR-009; documentation only, no in-app note, owner decision) in `docs/troubleshooting.md`, and the guard design (watch not set, settle window, hide counting) in `docs/technical-decisions.md`.
- [ ] T028 [P] Record the open items in `specs/002-mouser-v2/owner-confirmations.md`: D3 settle window, D6 Windows/Linux, and the unrun probes S2 and S4 (R2, R4) with the quickstart section 3 steps for the owner batch. Record the owner's acceptance of proceeding with the diagnosis only partly confirmed (S5 showed the cursor becoming visible with Mouser unaware; S2 and S4 unrun). List the owner-batch acceptance checks explicitly: SC-001 (100 start/pause/quit cycles with the pointer remote: Mac cursor never visible, no input reaches the Mac), SC-003 (a full working session beside the game without pressing a recovery key), and the T035 filter choice.
- [ ] T029 Update `specs/003-foreign-cursor-capture/spikes/FINDINGS.md` and `research.md` Diagnosis status to reflect what was built and which risks (R1-R4) remain open.
- [ ] T030 Full gate: `cargo test --workspace`, `cargo llvm-cov` at 100% lines for `mouser-platform-macos` and `mouser-app` with no coverage-ignore, `cargo clippy --workspace --all-targets`, `cargo fmt --check`; confirm no existing test was loosened (SC-006).
- [ ] T031 Latency check: confirm the guard shares no lock with the tap callback (read `M/tap.rs` and `M/capture.rs` for any shared `Mutex`/`RwLock` the guard takes) and that forwarding benchmarks or tests are unchanged (FR-010).
- [ ] T035 [OWNER BATCH, not run by implement] Owner-batch step (after the unattended run, from the S2 result): pick T014's motion filter, source-state check or plausibility fallback, set the one-line switch accordingly, and record the choice in `research.md`. Also run the quickstart section 3 check on real hardware: with a real foreign app moving the Mac cursor while the pointer is remote, confirm the foreign move is NOT forwarded to the other computer (verifies the D2 source-state assumption behind T014). Referenced by T028.
- [ ] T036 Check `docs/product-requirements.md` and the rest of `docs/` against the delivered behaviour (bounded 1 s hold, no new settings, Windows/Linux limitation) and record the result in `research.md`, including "no change needed" where that is the case (constitution docs rule). Depends on T027.


## Dependencies and order

- Phase 1 -> Phase 2 -> Phases 3, 4, 5 in order -> Phase 6.
- T004 -> T005 -> T006/T007; T008 -> T009 (conditional on the T008 fallback) -> T010; T002 -> T003.
- T032, T033 (Phase 3) after T015; T034 (Phase 3) after T017; T035 (Phase 6) is an owner-batch step after the unattended run (feeds T014, T028); T036 (Phase 6) after T027 and T030.
- US2 (T019-T022) builds on the guard from US1 (T012-T017). US3 (T023-T026) builds on the guard and on T009/T010.
- Real-hardware validation (quickstart section 3, SC-001's 100 cycles) is the owner batch after the unattended run, not a task gate.

## Parallel opportunities

- T002/T003 (catalog) alongside T004-T007 (trait refactor): different files.
- T008-T010 (sink) alongside T004-T007 once T001 is done.
- T014 (foreign-motion filter) alongside T013 within US1 (different files: `capture.rs`/`tap.rs` vs `cursor_guard.rs`).
- T027 and T028 in Polish.

## Implementation strategy

MVP is Phase 1-3 (Story 1, including T032-T034): fixes the owner's daily symptom (cursor visible on the Mac while the pointer is remote) and the leaked input. Then Story 2 (arrival), then Story 3 (log and recovery proof), then Polish. Commit after each phase.

## Operator decisions (recorded by the orchestrator, 2026-11-17)

- **OD-1: the full gate is deferred to the owner's batch.** The baseline `cargo xtask gate --all` failed twice for environmental reasons (the owner's running Mouser.app interferes with the macOS input tests, and a Lima VM process collides with random test ports). The branch changes only `specs/`, so neither failure is a regression. The owner chose to run implement now and run the full gate (including the 100% line-coverage check) in their confirmation batch after the run, with Mouser.app quit. During implement:
  - Do NOT run `cargo xtask gate`, `cargo clean`, or anything that starts the Parallels VM or Mouser.app. Run only per-crate tests (`cargo test -p <crate> [--test <name>]`) plus `cargo fmt --check` and `cargo clippy` for the crates touched.
  - Known pre-existing environmental failures, NOT defects of this feature and NOT to be fixed here: `contract_platform_macos_us2::a_grab_swallows_local_keys_and_reports_pointer_motion_until_released`, intermittent `rule_7_no_self_capture`, and `UDP socket can be bound … Address already in use` flakes. New guard tests use a scripted actor and must not depend on live input events.
  - Watch `target/` (about 28 GB now): stop and report if it passes 40 GB.
  - Coverage cannot be confirmed without the gate; write the tests for every new line anyway (T016's thin thread wrapper is the one documented exception) and say in the phase report which new lines you believe are uncovered.
- **OD-1 addendum (after Phase 2):**
  - `contract_platform_macos_v2::rule_10_pointer_is_visible_after_stop_and_drop` also fails on an untouched HEAD while Mouser.app runs: add it to the known environmental failures.
  - The data volume was 99% full (16 GiB free) after Phase 2 and `target/` reached 45 GB; the orchestrator deleted `target/llvm-cov-default` and `target/debug/incremental`. For every remaining phase: run cargo with `CARGO_INCREMENTAL=0`, never build in a temporary worktree (it duplicates `target/`), and STOP and report if `target/` passes 40 GB or free space on `/System/Volumes/Data` drops below 12 GiB (`df -h /System/Volumes/Data`).
  - Phase 3 MUST remove the two temporary `expect(dead_code)` suppressions left by Phase 2 (`Cursor::visible` in `cursor.rs`, `diag::emit` in `diag.rs`); the plan allows no suppressions.
  - Owner batch: Phase 2 edited `crates/mouser-platform-windows/src/diag.rs` (re-exports of the shared diagnostics types) without being able to compile it on this Mac. Compile and test that crate on the Windows side in the confirmation batch.
- **OD-1 addendum (after Phase 3):**
  - Guard tests live in `crates/mouser-platform-macos/src/capture_guard_tests.rs` and `cursor_guard_tests.rs`, not in `tests/contract_platform_macos_003.rs` (the `Cursor` trait is `pub(crate)`, so an integration test cannot reach it). T022 and T025 use the same in-crate files.
  - Phase 3 left `Phase::Arrived`, `arrived_at` and `SETTLE` out so no `dead_code` suppression was needed; Phase 4 (T019-T022) adds them. The `cursor.foreign_change` coalesced logging (T023's core) was pulled forward into Phase 3; Phase 5 verifies and finishes T023-T026 rather than re-implementing it.
  - Owner batch, hardware checks the unit tests cannot make: (a) with the real cursor, `CGCursorIsVisible` may lag a just-issued hide, so the guard's first tick (20 ms after the grab) could briefly show then re-hide the cursor; (b) the foreign-motion filter (`FILTERS` / `FILTER_IN_FORCE`, one-line switch) rests on the unverified S2 assumption (T035).
- **OD-1 addendum (after Phase 4):**
  - The three new real-backend tests in `crates/mouser-platform-macos/tests/contract_platform_macos_003.rs` fail in this environment on `pointer_hidden()` assertions, the same way the existing `rule_10_pointer_is_visible_after_stop_and_drop` fails on untouched HEAD while Mouser.app runs. They have NEVER been seen passing. Owner batch: run them with Mouser.app quit; a failure there is a real defect (in the tests or the code), not the environment.
  - `set_grabbed(false)` does not warp; the caller's existing `warp` positions the pointer, so "the landing point" is the cursor's current position. Existing call order (`associate true`, then `show`) is unchanged.
- **OD-1 addendum (after Phase 5):** T023 was already complete from Phase 3 (coalescing, flush on stop, own-echo, per-aspect tests existed); Phase 5 added the T024 app-level catalog test (`apps/mouser-app/src/diagnostics_tests.rs`), the T025 recovery tests (in `capture_guard_tests.rs`, in-crate as in OD-1), and the T026 real-emitted-shape catalog test (`cursor_guard_tests.rs`; `jsonschema`/`serde_json` added as macOS dev-deps, `jsonschema` as a mouser-app dev-dep). `contract_log_events` and `contract_log_privacy` pass unchanged; no emitter/catalog drift found.
