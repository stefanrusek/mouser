# Foundational decisions (frozen with the contract crates)

Interpretations made while implementing `mouser-proto`, `mouser-platform` and `mouser-testkit`, where the contracts left a choice open. The crate source is authoritative; this file is the index.

## Contract clarifications
- **HID usage**: a key usage is `u32 = (usage_page << 16) | usage_id` (keyboard A = `0x0007_0004`, consumer Volume Up = `0x000C_00E9`). Written into contracts/platform-api.md and wire-protocol.md.
- **DeskState** carries `author: ComputerId`; `DeskState::supersedes` compares `(epoch, author)` lexicographically (wire-protocol.md "highest (epoch, computer_id) wins").
- **Motion datagram** `display_id_hash` = `mouser_proto::wire::display_id_hash(&DisplayId)`: first 4 bytes big-endian of SHA-256(computer id bytes ‖ UTF-8 display key).

## mouser-proto
- `ComputerId`, `DeskId`, `DevicePublicKey` serialize as lowercase hex strings in every format (postcard included).
- Framing: 4-byte big-endian length + postcard body; `decode_frame` returns `Incomplete` or `Malformed`.
- `ReleaseAvailable` carries `manifest_json: String`, the signed `latest.json` text byte-for-byte.
- `FixAction` serializes as a string: `open_system_settings:<permission_kind>`, `approve_join`, `open_update`, `cross_anyway_hint`, `open_arranger_edge`.
- `Layout.placements` and `Layout.edge_options` are `Vec`s of entries (not maps); `block_fullscreen` and `equalize_dimensions` live in `Layout`.
- `corner_size` ("fixed default", data-model.md) is a `mouser-core` constant, not a wire field.
- Timestamps are RFC 3339 strings; `Modifiers` is four bools; `TransferId` is 16 raw bytes; `LeaveControl.position` is `f32` along the edge.
- Control API: `Method` enum with `name()`/`from_name()`; `error.data.kind` is a `ProblemKind`.
- `Config::validate` returns the first violation as `key: reason`; `DESK_SHARED_KEYS` = `crossing.block_fullscreen`, `crossing.equalize_dimensions`, `advanced.allowlist`.
- `LogRecord::to_line()` errors on an invalid event name.

## mouser-platform
- Channels are `std::sync::mpsc::Sender<T>`; `Result<T> = std::result::Result<T, mouser_platform::Error>`, `enum Error { Unsupported, Os(String) }`.
- `Limitation { capability: Capability, text }`; every capability a backend does not support (including `SecureAttention` off Windows) is listed (rule 9).
- Conformance checks: `mouser_platform::conformance::ALL` (feature `conformance`), one `rule_N_*` fn per behavioural rule, each taking `&dyn Platform`. Rule 5 requires Text, Html, Rtf and Png to round-trip.
- `WatchHandle::new(cancel)`; dropping it cancels the watch.

## mouser-testkit
- `VirtualPlatform` (clones share one virtual computer) and `SimNet`/`SimNode` (tokio; `route_to(&peer)` gives the address to dial; `set_delay`, `set_blackhole`, `change_address`, `sleep`/`wake`, `add_interface`/`remove_interface`, `bind_platform`). See `crates/mouser-testkit/src/`.
- Double-usage banner printed at process exit (D2).

## mouser-core (US1-core decisions, accepted by orchestrator)
- Desk-key rotation after leave/remove reuses `JoinDecision{approved:true, sealed}` sent to remaining members; the lowest remaining `ComputerId` rotates (`desk/membership.rs::rotate`). The sealed payload rides inside the pinned-TLS QUIC connection.
- Discovery is behind a `Discovery` trait (`announce`, `found`, `stop`); `MdnsDiscovery` is production, the acceptance harness uses an in-process registry over SimNet addresses.
- Missing permissions surface as a `missing_permission` Problem with `fix = open_system_settings:<kind>`; the app calls `Platform::open_permission_settings` itself (no control method).
- Windows control pipe: `\\.\pipe\mouser-<USERNAME>` with `reject_remote_clients`.

## mouser-core (US2-core decisions, accepted by orchestrator)
- Landing highlight and layout-review offers are in-process `EngineEvent`s (`Engine::events()` / `ControlHandle::events()`), not JSON-RPC notifications: `Topic` has no topic for them. The app hosts the engine, so it consumes them directly.
- Panic = Escape twice within 500 ms. Buttons 4/5 as Back/Forward = Alt-Left/Right (Windows/Linux), Cmd-[ / Cmd-] (macOS), on press.
- Modifiers held at a crossing are released locally, not carried; `LeaveControl` is unused (the controller tracks the remote pointer).
- Input capture/injection start only once no input permission is Missing (re-checked every 250 ms).
- **Deferred (contract change needed):** pause is per-computer. Story 2 sc. 12 says pausing applies on every computer; that needs a wire message (e.g. `Pause{paused}`) in mouser-proto. acc_us2 asserts it only on the computer where the hotkey fired.

## platform-macos US12/US13 decisions (for core/app US12, US13)

- macOS pre-login agent: `mouser_platform_macos::login_agent::LoginAgent::system(exe).install()` writes `/Library/LaunchAgents/com.mouser.login-window.plist` (LimitLoadToSessionType=LoginWindow, RunAtLoad, KeepAlive) with `ProgramArguments = [exe, "--headless"]`. The app binary must accept `--headless` (run the engine with no window/tray); the installer/app must call `install()` as root (owner-approved privileged step). It never calls launchctl.
- At the login window there is no user Keychain: core's headless mode needs a machine-scope secret source (open item for US12-core).
- Portable mode: `MacPlatform::portable(data_dir)` (data_dir = `MouserData/` beside the executable) stores secrets as 0600 files under `<data_dir>/secrets/` and adds the limitation "macOS permissions must be granted again on each computer you run Mouser from this drive on." (filed under `Capability::Capture`). Detection of `MouserData/` and skipping `Autostart::set` are core/app (T226-T229).

## core US4 decisions (for app US4, cli US9)

- `layout.get` returns the `Layout` JSON plus an extra `displays` array: each `Display` field plus `computer_name`, `online`, `last_seen`. `layout.set` takes a `Layout` plus optional `displays: [{display_id, included}]`; unchanged entries are ignored, so `layout.get` output can be edited and sent back. (Contract gap: `LayoutResult` = `Layout`; formalize in proto later.)
- Only the owning computer can remove/restore its own displays; a remote member's display is refused with "<display> belongs to another computer; remove it on that computer". Open item: replicate `included` (a `removed` list in `Layout` or a wire message) so any member can remove any display.
- `layout.set` also writes `crossing.equalize_dimensions` and `crossing.block_fullscreen` into each member's config. Equalize maps one edge's full length onto the adjoining edge's.
- A removal changes placed keys, so peers get `ReviewOffered` (same as docking).
- Open item for US6: the "all displays removed still receives clipboard" edge case is only half-tested (membership + no crossing).

## app US4 decisions

- The arranger reads `display.needs_review` per display from `layout.get`; core does not emit it yet and has no way to clear it (open item for core: add the boolean, clear on any `layout.set` touching that display).
- Equalize is set through `layout.set` (`equalize_dimensions`), not `config.set`.
- A ReviewOffered event opens the arranger window (notifications have no click callback). Revisit if the owner finds it intrusive.

## core US5 decisions (for app US5)

- Corner size 8 logical px (`crossing/corners.rs`); double-bump window 600 ms (`crossing/override.rs`); a bump is one continuous contact, re-armed by backing off.
- Corner blocking applies to every crossing (also on remote displays); full-screen blocking only when leaving this computer, from replicated `Layout.block_fullscreen`.
- A block raises a `crossing_blocked` Problem with `fix = open_arranger_edge`; it stays open for the session (no clear method yet).
- app US5: `open_arranger_edge` carries no edge identity, so the arranger opens the first shared edge's options; open item for core to include display+side in the fix. Edge options are written for both sides of an edge.

## core US6 decisions (for US7, app/cli)

- Representations up to 1 MiB (`clipboard::INLINE_LIMIT`) are pushed with the offer; larger ones and files are fetched when the paste shortcut (Cmd+V / Ctrl+V) or targeted-paste hotkey is seen, so the first paste of a large item can be stale (open item: needs lazy/delayed-render support in the `Clipboard` trait).
- Files: receiver sends `ClipboardFetch{Files}`, origin answers `TransferOffer`; `TransferAccept` → one uni stream per file (16-byte transfer id + 4-byte item index) → `TransferDone`. Files land in `<config_dir>/clipboard/<transfer id>/` (never cleaned up: open item); escaping paths refused. US7 must share the Transfer* dispatch now owned by clipboard.
- Large `ClipboardData` rides the control stream and can briefly delay input (open item).
- `hotkeys.targeted_paste` must parse as modifiers + key; `config.set` refuses bad values, applies live.

## core US7 decisions (for app US7, platform lanes)

- Shared transfer machinery lives in `transfer/streams.rs` (manifest, escape check, per-file uni streams); clipboard files and drags both use it.
- A drag offer is marked by `TransferOffer.destination_dir = "drop"` (`transfer::drag::DRAG_DESTINATION`); clipboard offers use `""`. Accept/Done route by transfer id; Reject/Conflict/Resolve are drag-only.
- Hand-off: `Crossing::begin` calls `FileDrag::outgoing()` after `Enter`; the receiver calls `begin_incoming(names)` on a dedicated thread (macOS blocks until the drop), checks free space, accepts, writes under the drop folder.
- Overwrite: destination publishes a `transfer` notification in state `awaiting_overwrite_decision` (`item`, `message`); the app answers via in-process `ControlHandle::resolve_transfer(transfer_id_hex, overwrite)` (no control method yet: open item). Declining skips that file.
- `transfer` notifications: `transfer_id` (hex), `state` (pending|checking|copying|awaiting_overwrite_decision|completed|rejected), `current_item`, `bytes_done`, `bytes_total`, `rate` (B/s), `eta_seconds`; throttled to 100 ms; conflict/rejected add `item`, `reason`, `message`.
- Open items: peer disconnect drops transfers silently (no notification), partial and completed files are never cleaned up; Linux backend must report a FileDrag limitation pointing to copy/paste.
- app US7: Overwrite/Skip go through the in-process Tauri command `transfer_resolve`. Open bug: the transfer window is created after the notification that opens it, so a one-shot `rejected` (no_space) can be lost; fix by having the app buffer the latest `transfer` notification per id and replay it to a newly opened window (app US8).

## core US8 decisions (for app US8, cli US9)

- Peers exchange `Status` on change; `status` fills each peer's `permissions`/`limitations`, and `status.problems` merges what peers report about themselves (dedup by problem_id+computer). Permission problems re-sync every 250 ms.
- In-process `ControlHandle::fix(problem_id)`: `open_system_settings:<kind>` opens settings; `open_arranger_edge` clears the notice. Errors for other fixes and for problems of other computers: the app shows fix buttons only for problems whose `computer` is this computer.
- `crossing_blocked` is per edge: `problem_id = crossing_blocked:<computer id>/<display key>/<side>`; it clears on fix, on a double-bump crossing, and when the edge stops blocking. The app parses the id to open that edge.
- Diagnostic log: `<config_dir>/logs/mouser.log` (JSONL per log-record.schema.json), rotates at `diagnostics.max_log_mb` to `mouser.log.1`; `logging.set{enabled}` persists `diagnostics.logging`; `status.logging{enabled,path}` always present. Events: logging.enabled/disabled, problem.raised/cleared, peer.connected/disconnected (crossing events not yet).
- Open: transfers are still dropped silently on peer disconnect (spec has no failed state; decided below).
- The test hang was a helper defect (join! on connect+accept waits forever when a handshake is lost); fixed by `testing::handshake` with 30 s timeouts.

## core US9 decisions (for cli US9)

- The config file is re-read on the 250 ms reconcile tick; a valid edit applies through the same `apply_config` path as `config.set` (shared keys get a new epoch and go to the desk); an invalid file is refused whole, previous values kept, one `config.rejected` warn record in the diagnostic log (not a problem).
- `transfer` notifications: state `failed`, `reason: "peer_offline"`, message "The copy stopped because <name> went offline. N of M files were copied." (drags only; clipboard-file fetches publish no transfer notifications).
- Open items: a computer rename reaches desk-mates only on reconnect (name travels in Hello/Member; needs a DeskState/wire update); `advanced.port` changes need a restart.

## platform-linux US1/US2 decisions (for core, app, packaging)

- Wayland capture is barrier-driven: no local motion until the pointer pushes through an outer-edge barrier; on activation a unit `Motion` at the edge is synthesized; `set_grabbed(true)` is a no-op (hotkey/QuickJump crossings cannot start from Wayland); `warp()` only sets where the cursor lands on Release.
- Wayland displays come from InputCapture zones: scale 1.0, no physical size (T458 deviation: no wl_output scale); `watch_displays` Unsupported on Wayland.
- OPEN (blocks Wayland): `permissions()` reports InputCapturePortal Granted only with a stored restore token, which InputCapture never returns, and core starts capture only when no input permission is Missing, so Wayland capture never starts. Needs an orchestrator fix.
- The crate does not compile on macOS/Windows (reis/rustix): it must be cfg-gated so the workspace builds everywhere (OPEN).
- The tray (AppIndicator) limitation is filed under `Capability::Capture`; X11 scale fixed at 1.0; scroll 15 px per wheel click.
- CI's ubuntu runner needs `Xvfb` and `dbus-daemon` for the Linux contract tests.

## core US10 decisions (for app US10, cli US10)

- `release_url` may return `latest.json`, a GitHub release object, or a releases array (newest first; drafts and releases without `latest.json` skipped; pre-releases included).
- The app must opt in: set `EngineParams.installer` and `check_schedule = Some(CheckSchedule::DEFAULT)` (30 s, then every 6 h); defaults are `None` so tests never reach the network. Automatic checks/installs only when `updates.automatic`.
- The offered manifest rewrites platform URLs to `mouser-lan://<fetcher id>/<platform key>`, offering only verified bundles; each peer re-verifies before installing. The fetcher keeps all platforms under `<config_dir>/releases/` and offers them to older desk-mates on auth.
- LAN propagation: a computer that installs a release fetched from a desk-mate also stores it (only its own platform's bundle, after the same minisign check) under its `<config_dir>/releases/`, so after restarting on the new version it offers it to older desk-mates on auth, like the fetcher. An offer of the version already offered is a retry (accepted unless an install is in progress), so a fetch cut short or a failed install is retried when the fetcher re-offers on reconnect. Before its own install the fetcher waits for the desk-mates it offered to to finish their `ReleaseFetch` (or disconnect): up to `FETCH_START_GRACE` (5 s) for a desk-mate to start fetching, and at most `FETCH_SETTLE_LIMIT` (60 s) in total (`crates/mouser-core/src/release/serve.rs`). Desk-mates that do not update by themselves therefore delay the fetcher's install by the 5 s grace.
- `status` shows a connected peer's version from its Hello.
- Not done: `min_protocol` is parsed but ignored; no update-available notification topic (the tray polls `update.check`).

## Linux milestone decisions

- On Wayland, InputCapturePortal and RemoteDesktopPortal report Granted when the portal is present and usable (Missing only when absent or no session bus); the per-login approval dialog appears when capture/inject starts, described by the "approve each login" limitation. This breaks the deadlock where core never started capture.
- mouser-platform-linux is `#![cfg(target_os = "linux")]` (empty elsewhere); mouser-app depends on it only on Linux and `create()` returns `LinuxPlatform::new()`.
- Linux `watch_network` polls interface addresses every 2 s; Wayland `watch_displays` is a no-op handle; file-store secret names are percent-encoded.
- The landing overlay sets click-through only after it is first shown (GTK has no native window while hidden).
- Two-VM X11 e2e (Lima user-v2): desk create/join by address, crossing, keys and clicks observed. mDNS discovery between the VMs did NOT work (likely no multicast on user-v2; unverified): open item.

## core US11 decisions (for app US11)

- Core detects `hotkeys.quickjump` from captured input; letters A-Z then 0-9, left to right; `QuickJumpShow`/`QuickJumpHide` go to all desk-mates and every computer emits in-process `EngineEvent::QuickJumpShow { letters }` / `QuickJumpHide` for the app overlay.
- A letter on a local display warps without a grab; Esc or any unshown key cancels; the hotkey is ignored while paused; keys pressed while letters show are consumed only when the pointer is remote (local keys still reach the focused app). The Esc that cancels letters does not count toward the panic double-Esc.
- Orchestrator decisions (fix in core US12 phase): (a) a letter whose display has left the layout since the letters were shown cancels the jump (hides letters) instead of panicking; (b) when the computer that showed the letters goes offline, desk-mates hide their letters; (c) Wayland: the Linux backend lists the limitation "QuickJump and hotkey crossings can't start from this computer on Wayland; push the pointer off the screen edge instead." under Capability::Capture, and core does not send `Enter` for a QuickJump when the local platform cannot grab (`set_grabbed` unsupported).

## app US10 decisions

- The tray polls `update.check` every 15 min (`updater::POLL`) and shows "Version X — Update to Y available" (click = `update.apply`); `updates.automatic` is handled by the engine.
- The gui shell sets `installer` + `check_schedule = Some(CheckSchedule::DEFAULT)`; tests use `Updates::default()`.
- Installer: core's `Bundle {version, bytes}` has no signature, so the app re-signs the core-verified bundle with a one-time minisign key, serves `latest.json` + bundle on 127.0.0.1, and drives tauri-plugin-updater 2.14.0 (runtime `pubkey()`, `target("lan")`), then restarts. Needs `dangerousInsecureTransportProtocol: true` (loopback only). Open item: carry the original signature in `Bundle` instead.

## platform-macos gate (real hardware, 2026-10-21 evening, owner away)

- Full `gate --packages mouser-platform-macos` passed 3x on the Mac mini: real input (incl. rule 7), pasteboard (saved and restored), full-screen, cursor hiding, and a real Finder drop through the file promise (harness = false test that owns the AppKit main loop).
- Pasteboard failures in `SystemBoard::new`/`write` now panic (unspecified, unreachable on a real pasteboard).
- OPEN for incoming drags in the real app: an NSDraggingSession only advances if the mouse-down that started it went to a window of Mouser's own process; the transparent drag window must receive the press. Finder also needed ~50 drag events of hover before accepting a drop. Verify in the real app (J1-style test) before claiming file drag into a Mac works.
- US12 login-window agent: plist writing is gated; a real login-window run is unverified (needs sudo + a real login screen).

## core US12 decisions (for app US12, cli US12, service-win, platform lanes)

- Machine-scope secrets: `EngineParams.secrets: Option<Box<dyn SecretStore>>` (None = platform store). `mouser_core::engine_headless::FileSecrets::new(dir)` (0600 files, `/` → `%2F`). `Engine::export_machine_scope(dir, &mut store)` copies the device + desk keys and writes `state.json`; the privileged install step runs it from the user session and must re-run after join/leave/key rotation (core does not do this itself).
- `--headless` (app lane): start `Engine` with `secrets = Some(FileSecrets)` and `config_dir` = the machine directory (macOS: e.g. `/Library/Application Support/Mouser`).
- `send_cad` returns `{}`; non-Windows target or input not running → error -32001, `data.kind = platform_limitation`, "Ctrl-Alt-Del cannot be sent to <name>.". A received `SecureAttention` is honoured only from the computer currently controlling this one.
- QuickJump: a vanished display cancels; the showing computer going offline hides desk-mates' letters; a jump probes `set_grabbed(true)` and on `Error::Unsupported` sends nothing. Edge crossings tolerate `Unsupported` from `set_grabbed`. Wayland's `set_grabbed` should return `Err(Error::Unsupported)`.

## core US13 decisions (for app US13)

- `mouser_core::paths::{Paths, Env, PORTABLE_DIR}`: `Paths::resolve(current_exe, os, &Env::from_process())` → `{dir, portable}`; portable when `MouserData/` is a directory beside the executable, else the per-OS config dir.
- `EngineParams::with_paths(&paths)` sets `config_dir`, `portable`, and (portable) `FileSecrets` at `MouserData/secrets`; state and logs follow `config_dir`. On macOS also pass `MacPlatform::portable(paths.dir)` when portable.
- Portable never calls `Autostart::set`. Windows portable adds the limitation "Portable mode cannot control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator." (under `Capability::SecureAttention`). `Ctx::limitations()` is the single source for problems, status and peer status.

## app headless / login-screen decisions

- `--headless` runs the engine with no Tauri (machine dir: macOS `/Library/Application Support/Mouser`, Windows `C:\ProgramData\Mouser`, Linux `/var/lib/mouser`; secrets under `<dir>/secrets`), stops on SIGTERM/Ctrl-C.
- "Allow control at the login screen" (macOS): stages `config.toml` + `export_machine_scope` + `state.json` in a 0700 folder, then one administrator prompt runs `<exe> --install-login-screen <staging>` (root side: copy to the machine dir, `LoginAgent::system(exe).install()`); staging is always removed; "User canceled" = refusal.
- Open: `export_machine_scope` must be re-run after join/leave/key rotation; nothing triggers it yet. Not run for real: the osascript prompt, the root install, a real login-window session.
- The updater loopback server reads until `\r\n\r\n` (fixes the CI race).

## app US13 decisions

- The app resolves `Paths` at startup, builds the engine with `EngineParams::with_paths`, and on macOS uses `MacPlatform::portable` when portable; WebView data goes to `MouserData/webview` (may be ignored by WKWebView: unverified).
- The main window is created in `shell.rs` (`"create": false` in tauri.conf.json) so it can get the WebView data folder: a real GUI launch must be smoke-tested before release.
- "Allow control at the login screen" is hidden in portable runs. The control socket stays at the default path (socket path length). Open: Linux `create_platform` should use `LinuxPlatform::portable(dir)` now that the Linux lane adds it; portable zip packaging in release.yml is not done.

## platform-linux US3/US5/US6/US7/US13 decisions

- netwatch: rtnetlink (link + v4/v6 addr) plus logind PrepareForSleep(false); no system bus → logind skipped.
- Full screen: X11 via _NET_ACTIVE_WINDOW + _NET_WM_STATE_FULLSCREEN; Wayland always false with a limitation.
- Clipboard: X11 owner with TARGETS/INCR/XFixes; Wayland via the portal clipboard on the shared RemoteDesktop session (requested before Start), falling back to wl-clipboard-rs data-control on non-GNOME; GNOME < 51 lists "Mouser cannot share the clipboard from this computer while it shares its mouse. This needs GNOME 51 or later."
- File drag: `file_drag()` is None; limitation "Dragging files between computers isn't available on Linux; use copy and paste".
- Portable: `LinuxPlatform::portable(dir)` (secrets + restore token under `<dir>/secrets`, autostart Unsupported). Open: app `create_platform` should call it.
- Wayland `set_grabbed` returns Unsupported; QuickJump limitation listed.
- VM note: llvm-cov on the virtiofs-mounted repo loses profraw data; symlink `target/llvm-cov-<lane>` to local disk in the VM. CI is unaffected.
- Not verified: real GNOME/KDE clipboard, X11 INCR against real GTK/Qt clients.

## Orchestrator decisions from the docs review (2026-10-22)

- An invalid or incomplete `config.toml` at START no longer panics: Mouser starts with the defaults in memory (the file is left untouched), raises a Problem of kind `platform_limitation` with problem_id `config_invalid` and message "Mouser's settings file has a mistake and is being ignored until it is fixed: <reason>. File: <path>", and writes one `config.rejected` log record. Fixing the file clears it through the existing live reload. Missing keys take their defaults; unknown keys follow the existing validator.
- The CLI's offline `config set` validates exactly like the engine (including `hotkeys.quickjump` and `enable_disable`): one shared validator in core used by both.
- The CLI resolves its config/log paths with `mouser_core::paths::Paths::resolve(current_exe)`, so a CLI shipped beside a portable app uses `MouserData/`.
- The CLI tolerates a status without `logging` (older running Mouser) with the message "The running Mouser is older than this command; update it." and exit 1, never a panic.
- `general.notifications = false` suppresses Mouser's system notifications (problems, join requests stay visible in their windows).
- `desk move`: implement only if contracts/cli.md lists it.

## Open items found during the v0.2.0 release (2026-10-22)

- Open: a crossing started within moments of a local address change is handed straight back (`Ctx::adopt` → refresh_map → set_map → drop_lost → `return_local(Return::Lost)`) while the peer redials, although status already says connected; the user has to push again. The core test now re-pushes (6afb385); the product fix is open.
- Open: layout `details_tests` once saw only Beta's display listed at startup. The test now waits for both computers' displays by name; the root cause (startup ordering) is unconfirmed.

- Decision (2026-10-22, orchestrator): Windows secrets directory `%ProgramData%\Mouser\secrets` gets an explicit ACL granting access only to SYSTEM, Administrators and the installing user (inheritance off). Machine-scope DPAPI alone lets any local user who can read the file decrypt it; this matches the Linux 0600 file and the macOS keychain intent that only the owner and the system service can read desk secrets.

- Decided (owner, 2026-10-22): `--headless` is not supported on Linux in v1 (deferred to a future feature, which will need a spec line for who runs it, the root/system socket path with a matching CLI rule, and display needs). Windows `--headless` is likewise refused (its login-screen work is deferred). On both, `mouser-app --headless` prints "headless mode is only supported on macOS in this version" to stderr and exits with code 2, before any engine starts. Implemented as `headless::check_supported`.

- Decision (2026-10-22, owner): all Windows UAC, secure-desktop and login-screen features are deferred to a future feature run: T367 (probe 2, `spike_service`), the US12-service-win lane T415–T426 (`apps/mouser-service-win` stays a stub), the elevated/UAC parts of probe 1 (T366), Story 12 Windows scenarios 2–4 and the Windows part of quickstart §8. macOS login-window work is unaffected. Detection-only `foreground_elevated()` (T389–T390) stays in scope: it reads the foreground process integrity level and needs none of the deferred machinery. The core/app/cli Ctrl-Alt-Del plumbing (`send_cad`, `mouser send-cad`, tray item) is kept; it can only act once a Windows backend with `secure_attention` exists.

- Note (2026-10-22): the Windows installer does not install the deferred `MouserService` or set `SoftwareSASGeneration`. T486 is marked done but `apps/mouser-app/windows/installer.nsi` does not exist in the repo, and no tauri config, release workflow or xtask check references the service, so there was nothing to strip; whoever writes the NSIS installer must leave both out.

- Decision (2026-10-22, orchestrator): Windows portable `MouserData\` secrets get the same restriction where the drive's file system supports ACLs (NTFS/ReFS): inheritance off, access only for the current user and SYSTEM (no Administrators group, as the drive moves between machines). On FAT32/exFAT, which have no ACLs, no ACL is set and the platform declares the limitation "Secrets on this drive are not protected by file permissions; anyone with the drive can read them." (same spirit as the Linux portable 0600 file).

- Open (orchestrator, 2026-10-22): Windows `WinPlatform::watch_problems(Sender<String>)` is outside the platform seam. Its one problem: the capture watchdog (every 5 s while capturing; tagged 1-px probe not seen by the hooks within 500 ms → hooks reinstalled) reports "Mouser lost its hold on the keyboard and mouse and took it back; input may have briefly leaked to this PC." Nothing subscribes yet; needs a mouser-platform seam addition and a Problem in the app, done by the orchestrator. Scroll: Windows 1 notch = WHEEL_DELTA 120 = 15 px, matching Linux (15) and macOS pixel units.

- Decision (2026-10-22, owner feedback on v0.2.0): a computer's included displays are one rigid group in the layout. Their relative positions are the OS arrangement (`origin_local`, each display at its own density, via `layout::autoplace::arrange`); the owner drags a computer as a whole in the arranger (`snapGroup`), never one display away from its siblings. Before: `Layout.placements` was per display and free, `validate` only checked overlap and connectivity, so one display of a computer could be dragged away, and a changed OS arrangement re-placed the whole computer at the right end of the desk. Now: `validate::rigid` refuses a layout whose placements for a computer are not one common translation of the OS arrangement (wire shape and contracts unchanged); `sync_layout` also treats a computer as stale when its placements stop matching the OS arrangement, and `autoplace::reposition` re-arranges it anchored on the persisting display that is first in the OS arrangement (smallest `origin_local`), falling back to `place` when that would overlap another computer or touch none. Edge options of a re-arranged computer are still dropped.

- Owner (2026-10-22): the landing highlight on crossing stays as is in v1 ("I love the little indicator"). Future (v2): add a config key to toggle it, together with a config and onboarding update.

- Fixed (2026-10-22; was open): every platform-reported problem uses the one id `platform_limitation:platform_problem`, and problems de-duplicate by id, so while Windows's "lost its hold" problem is open a different text (e.g. "could not change the pointer") was not shown. Core now derives the id from the text: `platform_limitation:platform_problem:<16 hex digits of FNV-1a 64 of the text>`, so the same text is one problem and different texts are separate ones (no contract change; the schema does not constrain problem_id).

- Windows clipboard (T404; accepted by orchestrator): `ClipRep::Files` carries local paths only (the files themselves arrive through core's transfer); virtual-file data objects are used only for drag. Change detection polls `GetClipboardSequenceNumber` instead of `AddClipboardFormatListener`.

- Decided and fixed (owner, 2026-10-22; was open): core's incoming-drag thread (`crates/mouser-core/src/transfer/mod.rs`, `Out::Drag`) does `begin_incoming(names).expect("the drag begins")`, but the macOS and Windows backends return `Err` when the user cancels the drag or drops on something that is not a folder, which used to panic core. Now the thread sends `Command::Cancelled { id }`: the destination forgets the transfer (still `Pending`, no new state), publishes nothing, raises no Problem, and sends `TransferDone` so the source forgets it. Spec Story 7 scenario 5 and data-model Transfer "Cancelled drop". Future (v2, owner wish): put such files in a scratch folder instead.

## Pause isolation (2026-10-22, owner feedback on v0.2.1)

- Decided (orchestrator): a paused computer is isolated in both directions as if disconnected, links kept. Peers learn it with a new wire message `ControlMessage::Paused { paused: bool }`, appended last in the enum (postcard variant indexes of existing messages unchanged), documented in contracts/wire-protocol.md; `PROTOCOL_VERSION` is not bumped. **Version gating**: `Paused` is sent only to peers whose `Hello.mouser_version` is >= 0.2.2 (compared with the release code's `Version`), and again after each new connection while paused. Clipboard/transfer choice: a paused computer shares nothing, like a disconnected one (peers drop it with the same `PeerGone` path); on resume the clipboard copied meanwhile is offered again, both ways.
- Latent risk (found while designing this): `crates/mouser-core/src/transport/mod.rs` (`Link` reader, ~line 376) does `Err(FrameError::Malformed) => panic!("malformed frame from peer")`. An older peer therefore panics on any message variant it does not know. Every appended wire variant, and any new field on an existing message (a field missing from an older sender is also malformed for the new receiver), crashes older peers mid-update unless it is version-gated on `Hello.mouser_version`. Future wire additions MUST gate the same way; making unknown variants ignorable (a skip-unknown frame kind) is a v2 item.
- Mixed desks (peer older than 0.2.2): the paused computer refuses `Enter` and drops what it was receiving, but the older controller is not told, so its pointer can still land on the paused computer and has to be brought home (panic key).

- Platform problem seam (orchestrator, 2026-10-22): `Platform::watch_problems` now takes `Sender<PlatformProblem>` with `enum PlatformProblem { Raised(String), Cleared(String) }` (mouser-platform), keyed by the text. Core raises `platform_limitation:platform_problem:<fnv1a(text)>` on Raised (deduped by id, notified on first raise) and clears it via `Problems::clear` on Cleared (cleared_at set, announced; clearing an unknown/already-cleared text is a silent no-op, like every other problem). VirtualPlatform gained `clear_problem`. The Windows crate only wraps its sends as `Raised`; a Windows session adds the `Cleared` sends (e.g. when the capture watchdog sees the hooks healthy again).

- Owner (2026-10-22), future (v2): a left click on the menu-bar/tray icon toggles pause (the menu stays on right click). Reason: a laptop that is carried away from the desk can stay on the desk and just be paused, so its pointer never slides onto screens the owner can't see.
