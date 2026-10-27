//! The Wayland backend: the InputCapture and RemoteDesktop portals, with
//! libei (EIS) carrying the events.

pub(crate) mod capture;
pub(crate) mod clipboard;
pub(crate) mod datacontrol;
pub(crate) mod desktop;
pub(crate) mod displays;
pub(crate) mod inject;
pub(crate) mod portal;
pub(crate) mod portal_clip;

/// Wayland does not let a client name the application in front.
pub(crate) fn foreground_app() -> Option<mouser_platform::ForegroundApp> {
    None
}

#[cfg(test)]
mod capture_tests;
#[cfg(test)]
mod clipboard_tests;
#[cfg(test)]
mod displays_tests;
#[cfg(test)]
mod inject_tests;
