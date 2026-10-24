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

Tauri 2's environment variable names for notarization with an API key are `APPLE_API_ISSUER`, `APPLE_API_KEY` (the key ID) and `APPLE_API_KEY_PATH` (path to the `.p8`). Windows is unsigned for now (Authenticode is deferred, decision D4), so SmartScreen will warn on first run.

Updater key: generated with `npx @tauri-apps/cli@2.12.4 signer generate`. The public half is `plugins.updater.pubkey` in `apps/mouser-app/tauri.conf.json`. If the private key or its password is lost, installed apps can never accept another update, so keep a backup outside GitHub. `crates/mouser-core/src/release/pubkey.rs` carries the same public key (compiled in); if you ever rotate the key, change both.

## Assets in the release

- `Mouser_<v>_universal.dmg` - macOS (arm64 + x86_64), signed, notarized, stapled
- `Mouser_<v>_x64-setup.exe` - Windows NSIS installer (per-user, unsigned)
- `Mouser_<v>_x64-portable.zip` - Windows portable copy: `mouser-app.exe`, `mouser.exe` (the CLI) and an empty `MouserData/` folder. Unsigned, no `.sig`, not in `latest.json`
- `Mouser_<v>_amd64.AppImage` and `Mouser_<v>_amd64.deb` - Linux
- `Mouser_<v>_universal.app.tar.gz`, `*-setup.exe`, `*.AppImage` each with a `.sig` - updater bundles
- `latest.json` - updater manifest, validated in CI against `specs/001-mouser-v1/contracts/release-manifest.schema.json`

## The `mouser` command-line tool

The CLI ships inside every bundle (a Tauri `externalBin` sidecar, signed with the app on macOS):

- macOS: `/Applications/Mouser.app/Contents/MacOS/mouser`. Put it on your PATH with `sudo ln -s /Applications/Mouser.app/Contents/MacOS/mouser /usr/local/bin/mouser`.
- Windows: `mouser.exe` next to `mouser-app.exe` in `%LOCALAPPDATA%\Mouser`. Add that folder to PATH (Settings > System > About > Advanced system settings > Environment Variables).
- Linux: the deb installs it as `/usr/bin/mouser`. From the AppImage, run `./Mouser_*.AppImage` with the extracted `usr/bin/mouser` (`--appimage-extract`).

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

- Windows: there is no backend (`crates/mouser-platform-windows` is empty), so the Windows installer starts an app that stops with "the Windows platform backend is not implemented yet". `mouser-service-win` is a stub (deferred to a future feature, owner decision 2026-10-22), so there is no `MouserService` install and no `SoftwareSASGeneration` registry value.
- A portable zip for macOS or Linux. Windows has one (see below); elsewhere create `MouserData/` yourself.
- Windows Authenticode signing (D4). Linux packages are not signed.
- The macOS login-window agent is built (the "Allow control at the login screen" option), but it has never been run on a real login screen.
- Not verified on real machines: the Windows NSIS installer and the Linux AppImage and `.deb` (CI builds them; nobody has installed them on real hardware), the Linux desktop template (`apps/mouser-app/linux/com.mouser.app.desktop`; Tauri names the installed file after the app binary), and a real GUI launch of a portable run.
