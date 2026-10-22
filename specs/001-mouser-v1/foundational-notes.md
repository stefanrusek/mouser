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
