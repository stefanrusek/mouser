# Spike findings: app shell, clipboard, cross-compilation, test infra, releases, signing

Date 2026-10-21. Host macOS 26.5 arm64, rustc/cargo 1.99.0, Homebrew 7.0.2, Docker CLI installed (daemon not running). Probes under `specs/001-mouser-v1/spikes/spike_*`, each its own `[workspace]` with `=` pins and a Cargo.lock; `CARGO_TARGET_DIR` in the scratchpad. Saved by the plan orchestrator from the probe agent's report.

| Crate / tool | Version (latest stable 2026-10-21) |
|---|---|
| tauri | 2.12.5 |
| tauri-build | 2.7.4 |
| tauri-plugin-autostart | 2.7.0 (auto-launch 0.6.0) |
| tauri-plugin-single-instance | 2.5.4 |
| tauri-plugin-updater | 2.14.0 (source inspected, not built) |
| arboard | 3.7.0 |
| clipboard-rs | 0.3.5 |
| cucumber | 0.23.0 |
| tokio | 1.54.0 |
| tauri-cli (not installed) | 2.12.4 |
| cargo-llvm-cov (not installed) | 0.9.1 |
| cargo-nextest (not installed) | 0.9.149 |
| cargo-xwin (not installed) | 0.23.1 |

## A1. Tauri v2 tray app with hidden window builds and runs without tauri-cli
- **Probe:** `spikes/spike_tauri_tray/` (src/main.rs; tauri.conf.json with window `"visible": false`, `bundle.active: false`; dist/index.html; capabilities/default.json; generated icons).
- **Commands:** `cargo tauri --version`; `cargo build`; run 5 s in background, start a second copy, kill both.
- **Observed:** `error: no such command: tauri`. `cargo build` cold: `Finished dev profile ... in 22.61s`. First copy: `SPIKE: tray built; window visible=Some(Ok(false)); autostart enabled=Ok(false)`. Second copy exited immediately (single-instance works). `TrayIconBuilder::icon_as_template(true)` / `TrayIcon::set_icon_as_template` exist (tauri/src/tray/mod.rs:295,561); macOS tints template icons; Windows/Linux have no template concept. autostart 2.7.0 exposes only `MacosLauncher::{LaunchAgent, AppleScript}` though auto-launch 0.6.0 supports SMAppService (macOS 13+); Windows uses HKCU Run + honors StartupApproved. single-instance has mac/win/linux impls (Linux via D-Bus). Build writes `gen/schemas/` into the crate dir (deleted after).
- **Verdict:** Confirmed. tauri-cli is only needed for bundling (.app/.dmg/NSIS/MSI) and updater `.sig` artifacts.
- **Implication:** plain cargo for dev/CI; tauri-cli only for release. Ship monochrome template tray icon (macOS) and light/dark variants (Windows/Linux). For FR-011 on macOS 13+ consider auto-launch in SMAppService mode directly. Add `gen/` to .gitignore.

## A2. Clipboard crates on macOS
- **Probe:** `spikes/spike_clipboard/src/main.rs`. **Command:** `pbpaste > scratchpad/clip_backup.txt` (4735 bytes); `cargo run -q`; `pbpaste | cmp - clip_backup.txt`.
- **Observed:** all round-trips OK — arboard text, HTML(+alt), image 16x16, file_list; clipboard-rs text, HTML, RTF, image 32x32, files, multi-format `set()`, watcher. Formats after multi-format set: `public.utf8-plain-text`, `public.html`, `public.rtf`, … Watcher `callbacks=2` for 2 writes. `restored original text` / `CLIPBOARD_RESTORED_IDENTICAL`. arboard wraps HTML in a full document on read; clipboard-rs returns verbatim. File path `/private/var/...` (arboard) vs `/var/...` (clipboard-rs).

| Capability | arboard 3.7.0 | clipboard-rs 0.3.5 |
|---|---|---|
| Text | yes | yes |
| HTML | yes (wrapped on read) | yes |
| RTF | no | yes |
| Image | yes (RGBA) | yes |
| File list | yes | yes |
| Multi-format write | no | yes |
| List formats / raw types | no | yes |
| Change watching | none | polls NSPasteboard.changeCount (500 ms default, configurable) |
| `cargo check --target x86_64-pc-windows-msvc` | OK | OK |

- **Verdict:** Confirmed on macOS; Windows/Linux not run.
- **Implication:** use clipboard-rs; polling change detection (100–250 ms); origin-tag Mouser's own writes to avoid echo; normalize file paths; concealed-type markers (`org.nspasteboard.ConcealedType`) reachable via clipboard-rs `available_formats`/`get_buffer`.

## A3. Cross-compiling from this Mac
- **Probe:** `spikes/spike_cross/` plus `cargo check` of other probes.
- **Observed:** windows-gnu build → `linker x86_64-w64-mingw32-gcc not found`; windows-msvc → `linker link.exe not found`; linux-gnu → `E0463 can't find crate for std` (target not installed). `cargo check` passes for windows-gnu (spike_cross) and windows-msvc (spike_clipboard). `cargo check --target x86_64-pc-windows-msvc` on spike_tauri_tray fails in build script (`icons/icon.ico not found`, then `tauri-winres NotAttempted("llvm-rc")`). No mingw, lld-link, xwin, cargo-xwin, zig; rustup llvm-tools has rust-lld but no llvm-rc. Docker daemon not running.
- **Verdict:** Refuted as-is: Windows type-checks for plain-Rust crates but nothing links; Tauri crate can't even `cargo check` for Windows without llvm-rc.
- **Implication:** release builds on native Windows and Linux CI runners (cargo-xwin is experimental; MSI can't be built off Windows; Linux needs gtk/webkit2gtk sysroot). Keep core crates (proto, config, CLI) free of Tauri so they check/test on every target.

## A4. Test infrastructure
- **Probe:** `spikes/spike_cucumber/` (tests/features/desk.feature; tests/bdd.rs `[[test]] harness=false`; `DeskWorld::cucumber().fail_on_skipped().run_and_exit("tests/features")`).
- **Observed:** `cargo test` → `1 feature, 1 scenario (1 passed), 3 steps (3 passed)`. Wrong expectation → `Step panicked ... 1 scenario (1 failed)`, exit 101. Undefined step → exit 101. `llvm-tools-aarch64-apple-darwin` installed; `cargo llvm-cov` and `cargo nextest` → `no such command`.
- **Verdict:** cucumber confirmed; coverage driver missing; nextest missing.
- **Implication:** one `harness=false` target per BDD suite, always `fail_on_skipped()`. Install cargo-llvm-cov (fresh machines also `rustup component add llvm-tools-preview`).

## A5. GitHub Releases + tauri-plugin-updater
- **Observed:** `curl -sI https://api.github.com/repos/tauri-apps/tauri/releases/latest` → HTTP/2 200 JSON, `cache-control: public, max-age=60`, `x-ratelimit-limit: 60` unauthenticated. Body has `tag_name`, `assets[].name/size/browser_download_url`, `published_at`, `prerelease`, `draft`. `…/releases/latest/download/latest.json` → 302 to the tagged download (CDN path, not the rate-limited API). updater 2.14.0 source: static manifest `{version, notes?, pub_date?, platforms: {"<os>-<arch>": {url, signature}}}`; minisign verification against configured `pubkey`; endpoint templates `{{current_version}} {{target}} {{arch}} {{bundle_type}}`; runtime override of endpoints/pubkey/headers/version_comparator; `Update::download()` returns verified bytes and `install(bytes)` is separate; plain-HTTP endpoints need `dangerous_insecure_transport_protocol`.
- **Verdict:** Confirmed.
- **Implication:** the designated desk computer polls `https://github.com/<owner>/mouser/releases/latest/download/latest.json`, downloads every platform entry, verifies minisign signatures, and re-serves manifest + bundles to peers over the desk connection. minisign provides integrity on the LAN hop. A minisign key pair must be generated and kept as a release secret.

## A6. Code signing and notarization
- **Observed:** `security find-identity -v -p codesigning` → `0 valid identities found`; notarytool 1.1.2 (41), stapler, codesign present; Command Line Tools only.
- **Verdict:** tools present, credentials absent — signing blocked on this machine.
- **Implication:** needs Developer ID Application certificate, hardened runtime, notary credentials (App Store Connect API key, or Apple ID + app password + Team ID). Until then dev builds are ad-hoc signed and every rebuild resets Accessibility/Input Monitoring grants. Windows Authenticode not probed.

## Missing tools
| Tool | Purpose | Install |
|---|---|---|
| tauri-cli 2.12.4 | bundling, updater sigs | `cargo install tauri-cli --version 2.12.4 --locked` |
| cargo-llvm-cov 0.9.1 | coverage | `cargo install cargo-llvm-cov --version 0.9.1 --locked` |
| cargo-nextest 0.9.149 (optional) | runner | `cargo install cargo-nextest --version 0.9.149 --locked` |
| cargo-xwin 0.23.1 + LLVM | Windows msvc cross-build | `cargo install cargo-xwin --version 0.23.1 --locked`; `brew install llvm` (+ `nsis`) |
| mingw-w64 (alt) | Windows gnu cross-build | `brew install mingw-w64` |
| Linux target + sysroot | Linux build | `rustup target add x86_64-unknown-linux-gnu` + container or native CI |
| Developer ID cert + notary creds | macOS signing | Apple Developer Program |
| Authenticode cert | Windows signing | CA / Azure Trusted Signing |
| minisign key pair | updater signatures | `cargo tauri signer generate -w ~/.tauri/mouser.key` |
