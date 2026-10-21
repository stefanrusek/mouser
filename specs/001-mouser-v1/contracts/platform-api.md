# Contract: Platform API (v1)

The seam between the platform-neutral core (`mouser-core`) and each OS backend (`mouser-platform-macos`, `-windows`, `-linux`). Defined as Rust traits in `crates/mouser-platform` (no implementation there). Both sides get `contract_` tests: the core against the in-process `VirtualPlatform` (a sanctioned test double — see plan.md, Test Doubles) and each backend against the trait's behavioural contract on its own OS.

All coordinates are in the computer's own desktop logical coordinates. Keys are USB HID usages (page 0x07 keyboard, 0x0C consumer) so the core never sees OS keycodes.

```rust
pub trait Platform: Send + Sync + 'static {
    fn info(&self) -> PlatformInfo;                 // os, version, session type (x11/wayland), limitations
    fn displays(&self) -> Result<Vec<LocalDisplay>>;
    fn watch_displays(&self, tx: Sender<Vec<LocalDisplay>>) -> Result<WatchHandle>;

    fn permissions(&self) -> Vec<(PermissionKind, PermissionState)>;
    fn open_permission_settings(&self, kind: PermissionKind) -> Result<()>;

    fn capture(&self) -> Result<Box<dyn InputCapture>>;
    fn inject(&self) -> Result<Box<dyn InputInject>>;
    fn clipboard(&self) -> Result<Box<dyn Clipboard>>;
    fn secure_input(&self) -> Option<SecureInputHolder>;         // macOS only; None elsewhere
    fn foreground_is_fullscreen(&self) -> bool;
    fn interfaces(&self) -> Vec<NetInterface>;                    // with kind: PhysicalLan | Virtual | Loopback (research R3)
    fn watch_network(&self, tx: Sender<()>) -> Result<WatchHandle>; // fires on any address/interface change
    fn foreground_elevated(&self) -> bool;                        // Windows UIPI detection; false elsewhere
    fn secrets(&self) -> Box<dyn SecretStore>;                    // device key + desk keys (data-model.md)
    fn autostart(&self) -> Box<dyn Autostart>;
    fn file_drag(&self) -> Option<Box<dyn FileDrag>>;            // None => limitation shown
}

pub trait InputCapture: Send {
    /// Start delivering local input events. While `grabbed` is true, local events are
    /// swallowed (not delivered to local apps) and the pointer is held still and hidden.
    fn start(&mut self, tx: Sender<LocalInput>) -> Result<()>;
    fn set_grabbed(&mut self, grabbed: bool) -> Result<()>;
    fn warp(&mut self, x: f64, y: f64) -> Result<()>;
    fn stop(&mut self);
}

pub enum LocalInput {
    Motion { dx: f64, dy: f64, abs: (f64, f64) },   // relative deltas + absolute position
    Button { button: u8, down: bool },
    Scroll { dx: f64, dy: f64 },
    Key { usage: u32, down: bool },
}

pub trait InputInject: Send {
    fn motion_abs(&mut self, x: f64, y: f64) -> Result<()>;
    fn button(&mut self, button: u8, down: bool) -> Result<()>;
    fn scroll(&mut self, dx: f64, dy: f64) -> Result<()>;
    fn key(&mut self, usage: u32, down: bool) -> Result<()>;
    fn secure_attention(&mut self) -> Result<()>;   // Ctrl-Alt-Del; Err(Unsupported) off Windows
}

pub trait Clipboard: Send {
    fn read(&mut self) -> Result<Vec<ClipRep>>;     // every representation present
    fn write(&mut self, reps: Vec<ClipRep>) -> Result<()>;
    fn change_count(&mut self) -> u64;              // polled by core; increments on any change
}

pub enum ClipRep { Text(String), Html(String), Rtf(Vec<u8>), Png(Vec<u8>), Files(Vec<PathBuf>) }

pub trait FileDrag: Send {
    /// Report files being dragged when the pointer leaves the display edge.
    fn outgoing(&mut self) -> Option<Vec<PathBuf>>;
    /// Begin a drag on this computer whose drop location is reported back.
    fn begin_incoming(&mut self, names: Vec<String>) -> Result<DropTarget>;
}

pub trait Autostart: Send { fn enabled(&self) -> bool; fn set(&mut self, on: bool) -> Result<()>; }

/// macOS Keychain; Windows DPAPI machine scope (readable by mouser-service before login);
/// Linux Secret Service, falling back to a 0600 file in the config dir; portable: file in MouserData.
pub trait SecretStore: Send {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>>;
    fn set(&mut self, name: &str, value: &[u8]) -> Result<()>;
    fn delete(&mut self, name: &str) -> Result<()>;
}
```

Behavioural rules each backend MUST satisfy (each is a `contract_` test case run on that OS):

1. `set_grabbed(true)` stops local pointer movement and local delivery of keys/buttons within one event; `set_grabbed(false)` restores both.
2. Every `key`/`button` down injected is releasable; dropping an `InputInject` releases anything still held.
3. `key(usage)` produces the OS key for that HID usage, independent of the current keyboard layout (layout translation happens in the receiving OS, as for a physical keyboard).
4. `displays()` reports `scale` and, where the OS knows it, `physical_size_mm`.
5. `Clipboard::write` followed by `read` returns every representation written that the OS can hold.
6. `interfaces()` never classifies a VPN, VM, container, bridge, AWDL or loopback interface as `PhysicalLan`.
7. Events injected by `InputInject` are never delivered back to this computer's own `InputCapture` as `LocalInput`.
8. `SecretStore::set` followed by `get` returns the same bytes after the store is re-opened.
9. Unsupported capabilities return `Err(Unsupported)` and are listed in `PlatformInfo.limitations` with plain-language text (FR-001).
