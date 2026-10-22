//! Portable mode (Story 13): secrets are files under the drive's
//! `MouserData/`, nothing is installed, so there is no autostart entry, and
//! on Wayland the desktop keeps the permission it was given.

use crate::{LinuxPlatform, Settings, limitation};
use mouser_platform::{Autostart, Capability, Error, Limitation, Result, SessionType};
use std::path::PathBuf;

pub(crate) const KEEPS_PERMISSION: &str = "This desktop keeps Mouser's permission after you remove the drive; remove it in the desktop's settings if you do not want it to stay.";

impl LinuxPlatform {
    /// A portable run: `data_dir` is the drive's `MouserData/`.
    pub fn portable(data_dir: impl Into<PathBuf>) -> Self {
        Self::portable_with_settings(Settings::system(), data_dir)
    }

    /// A portable run on the machine `settings` describe.
    pub fn portable_with_settings(settings: Settings, data_dir: impl Into<PathBuf>) -> Self {
        Self::build(settings, Some(data_dir.into()))
    }
}

pub(crate) fn limitations(portable: bool, session: SessionType) -> Vec<Limitation> {
    if portable && session == SessionType::Wayland {
        vec![limitation(Capability::Capture, KEEPS_PERMISSION)]
    } else {
        Vec::new()
    }
}

/// A portable run installs nothing, so it cannot start with the desktop.
pub(crate) struct NoAutostart;

impl Autostart for NoAutostart {
    fn enabled(&self) -> bool {
        false
    }

    fn set(&mut self, _on: bool) -> Result<()> {
        Err(Error::Unsupported)
    }
}
