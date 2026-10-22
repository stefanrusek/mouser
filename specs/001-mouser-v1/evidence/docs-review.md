# docs/ review (T497-T502)

Hand-written review record, not gate evidence. Reviewed 2026-10-22 against the code in `apps/` and `crates/`, the contracts, `foundational-notes.md` and the FINDINGS files. Where code and contract differ the code won.

Files reviewed: `docs/product-requirements.md`, `docs/technical-decisions.md`, `docs/releasing.md`, and the new `docs/cli.md`, `docs/configuration.md`, `docs/troubleshooting.md`. `README.md` was read but not edited (outside scope).

## Result

docs/ had drift, now corrected. Corrections made:

1. `docs/product-requirements.md` (T497): added a "What v1 delivers" section (per-OS state, the five accepted limitations) and a `Delivered:` note under every Must and Nice item and under the Desks list and the principle on agent-friendly operations. The owner's original wording is unchanged.
2. `docs/technical-decisions.md` (T498): kept the original decision table; added as-built crates and pinned versions, CI layout (ci.yml, release.yml), signing state (macOS signed and notarized; Windows and Linux unsigned; minisign update key), behaviour that changed, and probe outcomes.
3. `docs/releasing.md`: it said `pubkey.rs` (T489) "is not written yet" (it exists and holds the key); its "Not included yet" list called the macOS login-window agent and portable mode missing (both are built in source; the portable zip and the Windows backend are not); the tag example was `v0.1.0` and is now `v<version>`; added the note that the default `updates.release_url` reads the releases list so pre-releases are found; the "unverified until the first CI run" line is now "not verified on real machines".
4. New: `docs/cli.md` (T499), `docs/configuration.md` (T500), `docs/troubleshooting.md` (T501). Help text in cli.md was taken from `cargo run -q -p mouser-cli -- ... --help`; offline `config` and `logs` behaviour and exit codes were exercised with a throwaway `HOME`.

## Code differs from contract (code is documented)

| Contract | Code |
|---|---|
| cli.md: `mouser pause` / `resume` "Story 2 sc. 12" (whole desk) | Per computer. Help text says so. Needs a wire message to change (foundational-notes, core US2). |
| control-api.md: Windows pipe `\\.\pipe\mouser-<user SID>` | `mouser-<USERNAME>` (`control/server.rs`). |
| config.md: Linux path `$XDG_CONFIG_HOME/mouser/config.toml` | Falls back to `~/.config/mouser` when the variable is unset. |
| config.md: "every GUI setting has exactly one key" | `general.notifications` exists, is type-checked, and is read by nothing (v1 has no code path that consults it). |
| config.md: invalid values rejected with `key: reason` | True for `config.set` and the running app. Offline `mouser config set` checks `Config::validate`, `hotkeys.panic` and `hotkeys.targeted_paste` but **not `hotkeys.quickjump`**, which the running check and engine start both require. Verified: `HOME=<tmp> mouser config set hotkeys.quickjump Hyper+J` exits 0 and writes it; the engine start would then fail on `expect("hotkeys.quickjump is valid")`. Documented in configuration.md; this looks like a bug to fix in `apps/mouser-cli/src/config.rs`. |
| config.md: "Invalid values are rejected ... previous value stays" for a hand edit | True while running (one `config.rejected` log record, no in-app notice, only when logging is on). At start an invalid or incomplete file is **not** refused: `config::load_or_create` panics. A file missing any key is invalid (no serde defaults). Documented. |
| config.md: portable file `<exe dir>/MouserData/config.toml` | `mouser config path` and the offline `config`/`logs path|tail` always use the installed per-OS directory, never `MouserData/` (`config::default_dir()`). Running commands go through the app and are right. Documented in cli.md. |
| cli.md: `mouser desk create/join/leave/remove` | All present. There is no `desk move`, although `Method::DeskMove` exists in mouser-proto. |
| cli.md: exit code 3 "invalid argument/value" | Also used for every clap usage error (unknown command, missing argument) and for engine "invalid params". `--help`/`--version` exit 0. Errors are plain text on stderr even with `--json`. |
| cli.md: `logs tail` "raw JSON Lines" | Matches; it ignores `--json`. |
| status.schema.json problems have `kind, computer, message, fix` | Code adds `problem_id`, `raised_at`, `cleared_at` (additive). |
| task text: probe results in `crates/mouser-platform-*/probes/FINDINGS.md` | Only `crates/mouser-platform-linux/probes/FINDINGS.md` exists. The macOS, network, app-tooling and Windows/Linux research is in `specs/001-mouser-v1/spikes/FINDINGS-*.md`; technical-decisions.md uses both. |
| Cargo.toml / tauri.conf.json | Both say 0.1.1; the repo has no v0.2.0 bump yet. |

## README.md corrections needed (not applied)

1. Status table and "What is built / not done": Linux input capture and injection (X11 and Wayland), displays, clipboard, network watcher and portable mode are now built (commit 2af8676), tested in aarch64 VMs only. Update the Linux row and move those items out of "not done".
2. "Not done" lists Health panel, QuickJump, Send Ctrl-Alt-Del, auto-update across the LAN, portable mode and the macOS login-window agent: all are built in source. Remaining gaps: Windows backend (the crate is empty; the Windows app panics with "the Windows platform backend is not implemented yet"), `mouser-service-win` (a stub, not a "skeleton"), portable zip packaging, the login-window agent never run on a real login screen.
3. "CLI commands `config`, `logs`, `update` and `send-cad` are not done" and "only the commands listed under Status are implemented so far": every command in contracts/cli.md is implemented. Link to `docs/cli.md`.
4. "Windows: The Windows backend beyond a skeleton" and "not functional": say there is no backend and the app stops on start.
5. Install > Linux: "As of v0.1.1 the Linux backend is not functional" stays true for v0.1.1; reword when v0.2.0 ships. Add that file drag is unavailable on Linux and GNOME without AppIndicator shows no tray icon.
6. "Other: docs/ (requirements, technical decisions, releasing)": add cli.md, configuration.md, troubleshooting.md.
7. "Known limitations": add that clipboard files are never cleaned up and that Windows is not implemented; "Pause ... applies to the computer where you press the hotkey" is accurate.

## Could not verify

- Nothing was run on Windows. No Windows behaviour is documented beyond "not implemented".
- Linux behaviour comes from FINDINGS.md and the code; nothing was run on Linux or Wayland here.
- Portable mode on macOS: the executable lives in `Mouser.app/Contents/MacOS/`, so `MouserData/` has to go there; not tried on a real Mac. Whether WKWebView honours `MouserData/webview` is unverified (foundational-notes).
- `hotkeys.enable_disable`: the app hands the string to the OS global-shortcut plugin; what happens with an unparseable accelerator was not tested.
- The login-window agent, the admin prompt, the root install and a real login-window session were never run (foundational-notes).
- Whether the app-US7 transfer-window race (a one-shot `rejected` lost before the window opens) and the other "open item" bullets in foundational-notes have since been fixed was not re-checked; troubleshooting.md lists only the ones a user is likely to meet.
- The installed Mouser on this Mac is v0.1.1, older than the working tree. I ran read-only commands against it (`mouser status`, `config get`, `logs path`). The old app's `status` has no `logging` block, so a current `mouser logs path` panicked against it (`status reports logging`), its `config get` still showed the `<owner>` placeholder `release_url`, and its limitation list includes "Clipboard sharing is not available yet." and "Dragging files between computers is not available yet." (not in the current code). That is a build mismatch, not documented as behaviour. No other process was touched.
- The tray icon variants, the arranger and the other app windows were described from code and foundational-notes, not driven.
