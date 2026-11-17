# Quickstart: validating 003

Prerequisites: macOS with the Mouser build from this branch, Accessibility and Input Monitoring granted, one other computer on the desk. Logging on (`[diagnostics] logging = true`). Unit and seam tests need nothing else.

## 1. Before the fix: run the owner probes (plan time)

1. `cd specs/003-foreign-cursor-capture/spikes && cc spike_foreign_capture.c -framework ApplicationServices -o spike_foreign_capture`
2. With the pointer on the other computer, run `./spike_foreign_capture capture-while-remote`, then `release-while-remote`, then move the pointer onto the Mac during `hold`.
3. Paste what was seen into `spikes/FINDINGS.md` (S1-S4). Expected before the fix: cursor reappears or moves; after the fix, step 2 of section 3 shows none of that.

## 2. Automated checks (all platforms that build the macOS crate)

- `cargo test -p mouser-platform-macos` : guard tests with a scripted foreign actor on the `Cursor` seam: capture-while-remote (Story 1), move-onto-the-Mac (Story 2), recovery keys (Story 3), echo is not foreign (FR-004).
- `cargo test -p mouser-platform-macos --test contract_platform_macos_v2` : rule 10 still true after stop and drop.
- `cargo test -p mouser-app` (diagnostics wiring) and the `contract_log_events` / `contract_log_privacy` tests with the new event.
- `cargo llvm-cov` gate stays at 100% lines for the changed crates.
- `cargo clippy --workspace --all-targets` and `cargo fmt --check`.

## 3. Real Mac, with the probe or the game (owner batch, after the unattended run)

1. Pointer on the other computer, run the probe `capture-while-remote`: the Mac screen shows no cursor throughout; `cursor.foreign_change` lines appear in `~/Library/Application Support/Mouser/logs/mouser.log` with `corrected: true`; keyboard and mouse still reach only the other computer (SC-001, FR-003).
2. After `release-while-remote`: cursor still hidden and still (Story 1 sc. 2); QuickJump (`Ctrl+Alt+Shift+J`) and panic (`Escape` twice) return the pointer to a visible state.
3. Run `hold`, cross onto the Mac: the cursor is visible and under control within 100 ms with no flicker (Story 2, SC-004).
4. Repeat step 1 a hundred times with the probe in a loop (SC-001).

Windows and Linux: nothing to run; see research D6 and `owner-confirmations.md`.
