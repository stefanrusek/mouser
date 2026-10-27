# Contract delta: platform API (v2)

These are changes to `specs/001-mouser-v1/contracts/platform-api.md` and `crates/mouser-platform/src/lib.rs`.

> **Frozen-crate change.** This changes `crates/mouser-platform` (trait and conformance) and `crates/mouser-testkit` (VirtualPlatform). Both are frozen and both need approval. Every backend (`mouser-platform-{macos,windows,linux}`), `mouser-testkit`, `mouser-core/src/testing.rs`, `crates/mouser-core/tests/support/picky.rs` and `apps/mouser-app/src/support_tests.rs` implement the trait and must follow these changes.

## Types and methods

```rust
/// Replaces `fn foreground_elevated(&self) -> bool` (removed).
fn foreground_app(&self) -> Option<ForegroundApp>;   // default: None

pub struct ForegroundApp {
    pub name: String,            // executable file name, no path (FR-003)
    pub admin: bool,             // runs elevated (high IL) or as SYSTEM
    pub outranks_mouser: bool,   // integrity above Mouser's own: UIPI blocks Mouser
}

pub enum LocalInput {
    /* …001 variants unchanged… */
    /// Capture can no longer see this computer's input while grabbed. The backend has
    /// ALREADY ungrabbed and shown the pointer before sending this (rule 10).
    CaptureLost(CaptureLossReason),
}
pub enum CaptureLossReason { ElevatedForeground, OtherDesktop, HooksSilent }

pub enum Capability { /* …001… */ Install, LoginScreen, Tray }
```

## Per-backend obligations

| Backend | `foreground_app` | `CaptureLost` |
|---|---|---|
| Windows | `GetForegroundWindow` → process id → image name (`QueryFullProcessImageNameW`, file name only) and token elevation/integrity compared with Mouser's (`uipi.rs`) | V7 conditions (a)–(c) |
| macOS | `NSWorkspace.frontmostApplication` localized name; `admin = false`, `outranks_mouser = false` | not emitted (the CGEventTap disable path is 001's re-enable) |
| Linux X11 | `_NET_ACTIVE_WINDOW` → `_NET_WM_PID` → `/proc/<pid>/comm`; `admin` = process euid is 0; `outranks_mouser = false` | not emitted |
| Linux Wayland | `None` | not emitted |
| VirtualPlatform | scripted (`set_foreground_app`) | scripted (`lose_capture(reason)`) |

## Behavioural rules (added)

- **Rule 10.** After `CaptureLost` is sent, or after `InputCapture::stop()`, or after the capture is dropped while grabbed, the pointer is visible and input acts locally. Conformance: on a real backend, grab, then stop or drop, then the backend's own "pointer hidden" query returns false. On VirtualPlatform, the recorded grab state is false.
- **Rule 11.** Every inject and capture method returns `Err`, never panics, when the OS refuses. Conformance: a button above 5 returns `Err(Os(_))` or `Ok(())`, never a panic. `warp` outside every display returns `Err`.

## Frozen cross-lane Rust signatures (owner-approved 2026-10-27)

These are called across lanes (app → platform-windows), so their signatures are fixed here. The platform-windows lane implements them; the app lane calls them and never changes them. Both are `#[cfg(windows)]`.

```rust
// crates/mouser-platform-windows/src/cursor_guard.rs
/// Runs the cursor guard for `parent_pid` (research V5). Returns when the parent has ended,
/// after restoring blank system cursors. Never creates a window.
pub fn run(parent_pid: u32) -> std::process::ExitCode;

// crates/mouser-platform-windows/src/service_client.rs
pub struct ServiceClient { /* private */ }
impl ServiceClient {
    /// Connects to `engine_pipe_name(console_session_id())` and sends `Hello { version }` first.
    pub fn connect(version: &str) -> mouser_platform::Result<ServiceClient>;
    /// Registers what to run when the service sends `Shutdown`; called at most once per client.
    pub fn on_shutdown(&self, f: Box<dyn FnOnce() + Send>);
    /// Sends `Stopping { reason }` before a voluntary exit.
    pub fn stopping(&self, reason: mouser_proto::service_ipc::StopReason) -> mouser_platform::Result<()>;
}
pub fn console_session_id() -> u32;

// crates/mouser-platform-windows/src/lib.rs
impl WinPlatform {
    /// A backend whose `secure_attention()` goes through the service (Story 5 sc. 3).
    pub fn with_service_client(settings: Settings, client: ServiceClient) -> Self;
}
```

## Linux Wayland limitation (added)

A Wayland session lists `Limitation { capability: Capture, text: "Mouser can't tell which application is in front here, so its diagnostic log doesn't name it." }` (spec Assumptions; `foreground_app()` is `None` there).

## VirtualPlatform knobs (mouser-testkit, added)

| Knob | Effect |
|---|---|
| `refuse_input(kinds: &[InputKind])` / `accept_input()` | inject methods of those kinds return `Err(Os("refused"))` |
| `panic_on_next_inject()` | the next inject call panics; used to make a fault "inside Mouser" (Story 1 sc. 2) |
| `set_foreground_app(Option<ForegroundApp>)` | replaces `set_foreground_elevated` |
| `lose_capture(reason)` | if grabbed: ungrab, then send `LocalInput::CaptureLost(reason)` |
| `pointer_hidden() -> bool` | true while grabbed and not lost |

The aggregated double-usage banner (001 D2) is unchanged.
