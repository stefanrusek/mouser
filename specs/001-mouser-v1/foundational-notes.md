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
