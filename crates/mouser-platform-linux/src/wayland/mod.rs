//! The Wayland backend: the InputCapture and RemoteDesktop portals, with
//! libei (EIS) carrying the events.

pub(crate) mod capture;
pub(crate) mod displays;
pub(crate) mod inject;
pub(crate) mod portal;

#[cfg(test)]
mod capture_tests;
#[cfg(test)]
mod displays_tests;
#[cfg(test)]
mod inject_tests;
