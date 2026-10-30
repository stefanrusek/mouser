# Releasing Mouser

A release is produced entirely by `.github/workflows/release.yml` when a tag matching `v*` is pushed. It builds on macOS, Windows and Ubuntu, then creates a GitHub **pre-release** with every installer, the signed updater bundles and `latest.json`.

## Cutting a release

1. Set the same version in the workspace `Cargo.toml` (`version`) and `apps/mouser-app/tauri.conf.json` (`version`). The workflow fails if the tag is not `v` + the `tauri.conf.json` version.
2. Merge to `main`, make sure CI is green.
3. `git tag v<version> && git push origin v<version>` (for example `v0.2.0`).
4. Watch the `release` workflow. When it finishes, the pre-release `v<version>` has the assets below. Published so far: v0.1.0, v0.1.1, v0.2.0, v0.2.1, v0.2.2, v0.2.3, v0.2.4, v0.2.5, v0.2.6, v0.2.7 and v0.2.8.
5. To promote to a normal release, untick "pre-release" on GitHub. Note that `releases/latest/download/latest.json` only resolves for non-pre-releases. The app's default `updates.release_url` reads the GitHub releases list instead (pre-releases included), so installed apps still find pre-releases.

## Repository secrets (Settings > Secrets and variables > Actions)

| Secret | What it is |
|---|---|
| `APPLE_CERTIFICATE` | Developer ID Application certificate + private key exported from Keychain Access as `.p12`, then `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | Password chosen when exporting the `.p12` |
| `KEYCHAIN_PASSWORD` | Any random string; protects the temporary keychain the job creates and deletes |
| `APPLE_SIGNING_IDENTITY` | Exact certificate name, e.g. `Developer ID Application: Stefan Rusek (M4RA6A4VX9)` |
| `APPLE_API_KEY_ID` | App Store Connect API key ID (passed to Tauri as `APPLE_API_KEY`) |
| `APPLE_API_ISSUER` | App Store Connect issuer ID (a UUID) |
| `APPLE_API_KEY_P8` | Full text of the `AuthKey_XXXX.p8` file. The job writes it to a temp file and sets `APPLE_API_KEY_PATH` |
| `TAURI_SIGNING_PRIVATE_KEY` | Contents of the minisign updater private key file (see below) |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password of that key |

Tauri 2's environment variable names for notarization with an API key are `APPLE_API_ISSUER`, `APPLE_API_KEY` (the key ID) and `APPLE_API_KEY_PATH` (path to the `.p8`). Windows is unsigned (Authenticode is deferred, decision D4, and v2 ships unsigned by owner decision), so SmartScreen warns on first run; see "Installing the unsigned Windows build".

Updater key: generated with `npx @tauri-apps/cli@2.12.4 signer generate`. The public half is `plugins.updater.pubkey` in `apps/mouser-app/tauri.conf.json`. If the private key or its password is lost, installed apps can never accept another update, so keep a backup outside GitHub. `crates/mouser-core/src/release/pubkey.rs` carries the same public key (compiled in); if you ever rotate the key, change both.

## Assets in the release

- `Mouser_<v>_universal.dmg` - macOS (arm64 + x86_64), signed, notarized, stapled
- `Mouser_<v>_x64-setup.exe` - Windows NSIS installer (unsigned). Today it is per-user (`installMode: currentUser`); the per-machine installer described under "Windows installer and service" is not built yet
- `Mouser_<v>_x64-portable.zip` - Windows portable copy: `mouser-app.exe`, `mouser.exe` (the CLI) and an empty `MouserData/` folder. Unsigned, no `.sig`, not in `latest.json`
- `Mouser_<v>_amd64.AppImage` and `Mouser_<v>_amd64.deb` - Linux
- `Mouser_<v>_universal.app.tar.gz`, `*-setup.exe`, `*.AppImage` each with a `.sig` - updater bundles
- `latest.json` - updater manifest, validated in CI against `specs/001-mouser-v1/contracts/release-manifest.schema.json`

## The `mouser` command-line tool

The CLI ships inside every bundle (a Tauri `externalBin` sidecar, signed with the app on macOS):

- macOS: `/Applications/Mouser.app/Contents/MacOS/mouser`. Put it on your PATH with `sudo ln -s /Applications/Mouser.app/Contents/MacOS/mouser /usr/local/bin/mouser`.
- Windows: `mouser.exe` next to `mouser-app.exe` in `%LOCALAPPDATA%\Mouser`. Add that folder to PATH (Settings > System > About > Advanced system settings > Environment Variables).
- Linux: the deb installs it as `/usr/bin/mouser`. From the AppImage, run `./Mouser_*.AppImage` with the extracted `usr/bin/mouser` (`--appimage-extract`).

## Windows installer and service (v2)

Status: the service program and its install and uninstall logic are built (`apps/mouser-service-win`, shipped as `mouser-service.exe`, command line in [cli.md](cli.md)). The packaging is **not done yet**: the installer is still per-user, `tauri.conf.json` has no `installerHooks`, the release workflow and `build-sidecar.sh` do not build `mouser-service`, and the version is still 0.2.x. The description below is the plan the code was written to; do not tell users it exists until it is released.

- `mouser-service` is a Tauri `externalBin` sidecar beside `mouser-app.exe` and `mouser.exe`.
- The NSIS installer becomes per-machine (`installMode: perMachine`), which needs one administrator approval. Its hooks run `mouser-service.exe --install` after copying files (a non-zero exit aborts the install, showing the reason) and `mouser-service.exe --uninstall` before removing them. `--install` creates `MouserService` (LocalSystem, automatic start), sets `SoftwareSASGeneration=1`, adds the Windows Firewall rule for UDP 47801, protects `C:\ProgramData\Mouser\secrets` and starts the service. `--uninstall` stops and deletes the service, ends the session engine and removes the rule and the policy value, so nothing of Mouser keeps running.
- **Uninstalling keeps `C:\ProgramData\Mouser`** (keys and desk membership), so upgrades and reinstalls keep the computer on its desk. Deleting that folder after uninstalling makes the PC forget its desk.
- An upgrade from a per-user 0.2.x install by an administrator copies that user's desk membership to the machine folder; if it cannot (for example a different administrator approved the install), the computer rejoins with one approval.
- The portable zip stays as it is: no service, an empty `MouserData/`.

## Installing the unsigned Windows build

Mouser for Windows is not code-signed (a certificate is future work), so Windows shows an unknown-publisher warning the first time. Put these steps in the release notes:

1. Download `Mouser_<version>_x64-setup.exe` (or the portable zip) from the GitHub release page.
2. Run it. If Windows shows a blue "Windows protected your PC" box saying Microsoft Defender SmartScreen prevented an unrecognized app from starting, click **More info**, then click **Run anyway**.
3. For the per-machine installer, approve Windows' administrator prompt (it will name the publisher as unknown). That is the only approval needed.
4. For the portable zip, extract it anywhere and run `mouser-app.exe`; the same SmartScreen box can appear on first run.

These click-through steps apply to today's per-user installer as well. The verification that someone new to Mouser can install from these instructions (SC-010) has not been done on real hardware.

## Building a bundle locally (macOS, signed, not notarized)

```
apps/mouser-app/scripts/build-sidecar.sh aarch64-apple-darwin
apps/mouser-app/scripts/stage-ui.sh
cd apps/mouser-app
APPLE_SIGNING_IDENTITY="Developer ID Application: ..." \
  npx --yes @tauri-apps/cli@2.12.4 build --target aarch64-apple-darwin --bundles app,dmg \
  --features gui --config tauri.release.conf.json
```

(Add `TAURI_SIGNING_PRIVATE_KEY` and its password to also produce updater signatures.) The `gui` feature is required: without it the app has no window or tray. `tauri.release.conf.json` adds the sidecar, the updater artifacts and a clean UI folder, so plain `cargo build` and the gate do not need them.

## Not included yet

- Windows: the per-machine installer with the `MouserService` hooks and the `mouser-service` sidecar (see above) is not built, so a released installer does not install `MouserService` or set `SoftwareSASGeneration`. The login screen, permission prompts, user switching and Ctrl-Alt-Del of an installed Windows Mouser are therefore not available in a release yet, and are unverified on real hardware.
- A portable zip for macOS or Linux. Windows has one (see below); elsewhere create `MouserData/` yourself.
- Windows Authenticode signing (D4). SignPath Foundation's free signing for open-source projects is worth revisiting. Linux packages are not signed.
- The Mouser window cannot yet run fixes, answer file-replace questions, or show the landing flash, QuickJump or the review prompt when the tray app runs against `MouserService` (UI mode).
- The macOS login-window agent is built (the "Allow control at the login screen" option), but it has never been run on a real login screen.
- Not verified on real machines: the Windows NSIS installer and the Linux AppImage and `.deb` (CI builds them; nobody has installed them on real hardware), the Linux desktop template (`apps/mouser-app/linux/com.mouser.app.desktop`; Tauri names the installed file after the app binary), and a real GUI launch of a portable run.
