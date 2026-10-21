//! The frozen platform seam between `mouser-core` and each OS backend.
//!
//! Declarations only (contracts/platform-api.md). Channels are
//! `std::sync::mpsc`: backends deliver from OS threads, the core adapts to its
//! own runtime.

use std::fmt;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

#[cfg(feature = "conformance")]
pub mod conformance;

/// Failure of a platform operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The capability does not exist on this platform; it is also listed in
    /// [`PlatformInfo::limitations`] (rule 9).
    Unsupported,
    /// The OS refused or failed the operation.
    Os(String),
}

pub use Error::Unsupported;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unsupported => write!(f, "unsupported on this platform"),
            Error::Os(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    MacOs,
    Windows,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    /// macOS and Windows.
    Native,
    X11,
    Wayland,
}

/// A capability a platform may lack (rule 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Capture,
    Inject,
    Clipboard,
    FileDrag,
    SecureAttention,
}

/// A platform limitation with plain-language text shown in the app (FR-001).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limitation {
    pub capability: Capability,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformInfo {
    pub os: Os,
    pub version: String,
    pub session: SessionType,
    pub limitations: Vec<Limitation>,
}

/// One display, in this computer's own desktop logical coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalDisplay {
    /// Stable across reboots where the OS provides a key, else the index.
    pub key: String,
    pub name: String,
    pub pixel_size: (u32, u32),
    pub logical_size: (f64, f64),
    /// At least 1.0.
    pub scale: f32,
    pub physical_size_mm: Option<(f64, f64)>,
    /// Top-left corner in the computer's desktop coordinates.
    pub origin: (f64, f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PermissionKind {
    Accessibility,
    InputMonitoring,
    ScreenRecording,
    FullDiskAccess,
    InputCapturePortal,
    RemoteDesktopPortal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionState {
    Granted,
    Missing,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterfaceKind {
    PhysicalLan,
    Virtual,
    Loopback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetInterface {
    pub name: String,
    pub kind: InterfaceKind,
    pub addrs: Vec<IpAddr>,
}

/// The application holding macOS Secure Input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecureInputHolder {
    pub pid: u32,
    pub app_name: String,
}

/// Where a drag begun with [`FileDrag::begin_incoming`] was dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropTarget(pub PathBuf);

/// Keeps a watch alive; dropping it stops the watch.
pub struct WatchHandle(Option<Box<dyn FnOnce() + Send>>);

impl WatchHandle {
    pub fn new(cancel: impl FnOnce() + Send + 'static) -> Self {
        WatchHandle(Some(Box::new(cancel)))
    }
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        if let Some(cancel) = self.0.take() {
            cancel();
        }
    }
}

pub trait Platform: Send + Sync + 'static {
    /// os, version, session type (x11/wayland), limitations
    fn info(&self) -> PlatformInfo;
    fn displays(&self) -> Result<Vec<LocalDisplay>>;
    fn watch_displays(&self, tx: Sender<Vec<LocalDisplay>>) -> Result<WatchHandle>;

    fn permissions(&self) -> Vec<(PermissionKind, PermissionState)>;
    fn open_permission_settings(&self, kind: PermissionKind) -> Result<()>;

    fn capture(&self) -> Result<Box<dyn InputCapture>>;
    fn inject(&self) -> Result<Box<dyn InputInject>>;
    fn clipboard(&self) -> Result<Box<dyn Clipboard>>;
    /// macOS only; `None` elsewhere.
    fn secure_input(&self) -> Option<SecureInputHolder>;
    fn foreground_is_fullscreen(&self) -> bool;
    /// With kind: PhysicalLan | Virtual | Loopback (research R3).
    fn interfaces(&self) -> Vec<NetInterface>;
    /// Fires on any address/interface change.
    fn watch_network(&self, tx: Sender<()>) -> Result<WatchHandle>;
    /// Windows UIPI detection; `false` elsewhere.
    fn foreground_elevated(&self) -> bool;
    /// Device key + desk keys (data-model.md).
    fn secrets(&self) -> Box<dyn SecretStore>;
    fn autostart(&self) -> Box<dyn Autostart>;
    /// `None` => limitation shown.
    fn file_drag(&self) -> Option<Box<dyn FileDrag>>;
}

pub trait InputCapture: Send {
    /// Start delivering local input events. While `grabbed` is true, local events are
    /// swallowed (not delivered to local apps) and the pointer is held still and hidden.
    fn start(&mut self, tx: Sender<LocalInput>) -> Result<()>;
    fn set_grabbed(&mut self, grabbed: bool) -> Result<()>;
    fn warp(&mut self, x: f64, y: f64) -> Result<()>;
    fn stop(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LocalInput {
    /// Relative deltas + absolute position.
    Motion {
        dx: f64,
        dy: f64,
        abs: (f64, f64),
    },
    Button {
        button: u8,
        down: bool,
    },
    Scroll {
        dx: f64,
        dy: f64,
    },
    /// `usage` = (usage_page << 16) | usage_id, e.g. keyboard A = 0x0007_0004.
    Key {
        usage: u32,
        down: bool,
    },
}

pub trait InputInject: Send {
    fn motion_abs(&mut self, x: f64, y: f64) -> Result<()>;
    fn button(&mut self, button: u8, down: bool) -> Result<()>;
    fn scroll(&mut self, dx: f64, dy: f64) -> Result<()>;
    /// `usage` = (usage_page << 16) | usage_id; keyboard A = 0x0007_0004, consumer Volume Up = 0x000C_00E9.
    fn key(&mut self, usage: u32, down: bool) -> Result<()>;
    /// Ctrl-Alt-Del; `Err(Unsupported)` off Windows.
    fn secure_attention(&mut self) -> Result<()>;
}

pub trait Clipboard: Send {
    /// Every representation present.
    fn read(&mut self) -> Result<Vec<ClipRep>>;
    fn write(&mut self, reps: Vec<ClipRep>) -> Result<()>;
    /// Polled by core; increments on any change.
    fn change_count(&mut self) -> u64;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipRep {
    Text(String),
    Html(String),
    Rtf(Vec<u8>),
    Png(Vec<u8>),
    Files(Vec<PathBuf>),
}

pub trait FileDrag: Send {
    /// Report files being dragged when the pointer leaves the display edge.
    fn outgoing(&mut self) -> Option<Vec<PathBuf>>;
    /// Begin a drag on this computer whose drop location is reported back.
    fn begin_incoming(&mut self, names: Vec<String>) -> Result<DropTarget>;
}

pub trait Autostart: Send {
    fn enabled(&self) -> bool;
    fn set(&mut self, on: bool) -> Result<()>;
}

/// macOS Keychain; Windows DPAPI machine scope (readable by mouser-service before login);
/// Linux Secret Service, falling back to a 0600 file in the config dir; portable: file in MouserData.
pub trait SecretStore: Send {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>>;
    fn set(&mut self, name: &str, value: &[u8]) -> Result<()>;
    fn delete(&mut self, name: &str) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn error_display_and_unsupported_alias() {
        assert_eq!(Unsupported.to_string(), "unsupported on this platform");
        assert_eq!(Error::Os("denied".into()).to_string(), "denied");
        let e: &dyn std::error::Error = &Unsupported;
        assert!(e.source().is_none());
    }

    #[test]
    fn dropping_watch_handle_cancels_once() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let handle = WatchHandle::new(move || flag.store(true, Ordering::SeqCst));
        assert!(!cancelled.load(Ordering::SeqCst));
        drop(handle);
        assert!(cancelled.load(Ordering::SeqCst));
    }
}
