//! Windows asks for no permission Mouser could check or open: every kind is
//! not applicable.

use mouser_platform::{Error, PermissionKind, PermissionState, Result};

pub(crate) const KINDS: [PermissionKind; 6] = [
    PermissionKind::Accessibility,
    PermissionKind::InputMonitoring,
    PermissionKind::ScreenRecording,
    PermissionKind::FullDiskAccess,
    PermissionKind::InputCapturePortal,
    PermissionKind::RemoteDesktopPortal,
];

pub(crate) fn report() -> Vec<(PermissionKind, PermissionState)> {
    KINDS
        .iter()
        .map(|&k| (k, PermissionState::NotApplicable))
        .collect()
}

pub(crate) fn open_settings(_kind: PermissionKind) -> Result<()> {
    Err(Error::Unsupported)
}
