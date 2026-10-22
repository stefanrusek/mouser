# Quickstart Validation: Mouser v1

Runnable scenarios that prove the feature end to end. Commands reference `contracts/` and `data-model.md` instead of repeating them.

## Prerequisites

- Rust 1.99+, `rustup component add llvm-tools-preview`, `cargo install cargo-llvm-cov --version 0.9.1 --locked`
- For real-machine checks: two or three computers on one LAN (at least one macOS; Windows and Linux per decision D1 in plan.md), Mouser builds for each, and the OS permissions from Story 1 granted to the Mouser app (not to a terminal — see spikes/FINDINGS-macos-input "Permission context").
- Quit ShareMouse on every test computer (both install input taps; FINDINGS-macos-input §2).

## 1. Automated gates (any dev machine)

```bash
cargo xtask gate --all          # fmt, clippy -D warnings, all tests, 100% coverage
cargo test -p mouser-core --test acc_us1_desk   # one story's acceptance suite
cargo test -p mouser-core --test contract_wire  # wire protocol contract
```

Expected: all pass; coverage report shows 100% lines for production crates (contracts/: schemas validated by `contract_*` tests).

## 2. Two computers, one desk (Stories 1, 2) — real machines

1. Launch Mouser on computer A. First-run: grant permissions; choose **Create desk** "Home office".
2. Launch on computer B. First-run shows "Home office" with A's name; choose it.
3. On A, click **Allow** on "Allow B to join Home office?".
4. Expected: both trays show 2 computers; `mouser status --json` on either validates against `contracts/status.schema.json` and lists both as `connected`.
5. Move the pointer off A's shared edge → it appears on B with a landing highlight; type in a B text field; cross back.
6. Press Esc twice while on B → control returns to A.

## 3. Self-healing (Story 3, SC-001/SC-002/SC-008)

On B, while A is controlling B:
1. Type a long passage into a text editor on B for 60 s → every character present, in order.
2. Sleep B, wake it, wait 5 s, cross to B → works with no action in Mouser.
3. Switch B from Wi-Fi to Ethernet (or toggle Wi-Fi) → within 5 s crossing works.
4. Connect then disconnect a VPN on B → crossing works throughout or within 5 s.
5. Unplug B's network while it has the pointer → control returns to A, no stuck keys.

## 4. Two desks on one network (Story 1 sc. 4, SC-004)

Form desk "Lab bench" on C. Copy text on C; confirm A and B clipboards unchanged and neither desk lists the other's computers.

## 5. Data exchange (Stories 6, 7)

Copy formatted text, an image and a folder on A; paste each on B. Drag a folder from A to B's desktop; observe progress; repeat to trigger the overwrite prompt.

## 6. Arranger, crossing control, QuickJump (Stories 4, 5, 11)

Open the arranger; drag B to the other side of A → crossing follows. Push into a corner → stays; bump twice → crosses. Press the QuickJump hotkey → letters on every display; press one → pointer there.

## 7. Diagnostics and agent access (Stories 8, 9)

Revoke a permission on B → health panel names it with **Open settings**. Run `mouser logs enable`, reproduce a crossing, `mouser logs tail -n 20` → each line validates against `contracts/log-record.schema.json`. `mouser config set crossing.block_fullscreen false` → GUI shows the change.

## 8. Updates, secure screens, portable (Stories 10, 12, 13)

Publish a test release with `latest.json` (contracts/release-manifest.schema.json); confirm one computer downloads and the other updates from it over the LAN with permissions intact. On Windows: restart and log in with the shared keyboard; use **Send Ctrl-Alt-Del**. (Deferred to a future feature, owner decision 2026-10-22; skip these Windows steps. On a Mac, lock it and unlock it from the shared keyboard.) Run the portable build from a USB drive, join the desk, remove the drive, and confirm no Mouser files remain on the computer.
