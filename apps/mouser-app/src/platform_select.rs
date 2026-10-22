//! Which platform backend this build of Mouser uses.

use std::sync::Arc;

use mouser_platform::{Os, Platform};

/// A platform backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    MacOs,
    Windows,
    LinuxX11,
    LinuxWayland,
}

/// The Linux backend for a session: `XDG_SESSION_TYPE` decides when it says
/// `wayland` or `x11`; otherwise a set `WAYLAND_DISPLAY` means Wayland, and
/// a session that names neither is X11.
pub fn linux_backend(session_type: Option<&str>, wayland_display: Option<&str>) -> Backend {
    match session_type {
        Some("wayland") => Backend::LinuxWayland,
        Some("x11") => Backend::LinuxX11,
        _ if wayland_display.is_some_and(|d| !d.is_empty()) => Backend::LinuxWayland,
        _ => Backend::LinuxX11,
    }
}

/// The backend for `os`, reading the Linux session from `env`.
pub fn select(os: Os, env: impl Fn(&str) -> Option<String>) -> Backend {
    match os {
        Os::MacOs => Backend::MacOs,
        Os::Windows => Backend::Windows,
        Os::Linux => linux_backend(
            env("XDG_SESSION_TYPE").as_deref(),
            env("WAYLAND_DISPLAY").as_deref(),
        ),
    }
}

/// The operating system this binary was built for.
#[cfg(target_os = "macos")]
pub fn current_os() -> Os {
    Os::MacOs
}

/// The operating system this binary was built for.
#[cfg(target_os = "windows")]
pub fn current_os() -> Os {
    Os::Windows
}

/// The operating system this binary was built for.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn current_os() -> Os {
    Os::Linux
}

/// An environment variable of this process, if set.
pub fn process_env(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

/// The backend for this computer.
pub fn current() -> Backend {
    select(current_os(), process_env)
}

/// The platform for this computer.
#[cfg(target_os = "macos")]
pub fn create() -> Arc<dyn Platform> {
    Arc::new(mouser_platform_macos::MacPlatform::new())
}

/// The platform for this computer.
#[cfg(target_os = "linux")]
pub fn create() -> Arc<dyn Platform> {
    Arc::new(mouser_platform_linux::LinuxPlatform::new())
}

/// The Windows backend crate does not define a platform type yet.
#[cfg(target_os = "windows")]
pub fn create() -> Arc<dyn Platform> {
    panic!(
        "the {:?} platform backend is not implemented yet",
        current()
    )
}
