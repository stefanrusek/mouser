//! TCC permission preflight and System Settings deep links. Never prompts.

use mouser_platform::{Error, PermissionKind, PermissionState, Result};
use std::path::Path;
use std::process::Command;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGPreflightScreenCaptureAccess() -> bool;
}

/// File only readable by a process holding Full Disk Access; opening it for
/// reading is the side-effect-free probe (there is no preflight API).
const FDA_PROBE_FILE: &str = "/Library/Application Support/com.apple.TCC/TCC.db";

/// The four answers the OS gives; a seam so the mapping is testable.
pub(crate) trait Probe {
    fn accessibility(&self) -> bool;
    fn input_monitoring(&self) -> bool;
    fn screen_recording(&self) -> bool;
    fn full_disk_access(&self) -> bool;
}

pub(crate) struct SystemProbe;

impl Probe for SystemProbe {
    fn accessibility(&self) -> bool {
        unsafe { AXIsProcessTrusted() }
    }
    fn input_monitoring(&self) -> bool {
        unsafe { CGPreflightListenEventAccess() }
    }
    fn screen_recording(&self) -> bool {
        unsafe { CGPreflightScreenCaptureAccess() }
    }
    fn full_disk_access(&self) -> bool {
        std::fs::File::open(FDA_PROBE_FILE).is_ok()
    }
}

fn state(granted: bool) -> PermissionState {
    if granted {
        PermissionState::Granted
    } else {
        PermissionState::Missing
    }
}

pub(crate) fn report(probe: &dyn Probe) -> Vec<(PermissionKind, PermissionState)> {
    vec![
        (PermissionKind::Accessibility, state(probe.accessibility())),
        (
            PermissionKind::InputMonitoring,
            state(probe.input_monitoring()),
        ),
        (
            PermissionKind::ScreenRecording,
            state(probe.screen_recording()),
        ),
        (
            PermissionKind::FullDiskAccess,
            state(probe.full_disk_access()),
        ),
    ]
}

/// The System Settings pane URL for a permission; the Linux portal kinds have
/// no macOS pane.
pub(crate) fn settings_url(kind: PermissionKind) -> Result<&'static str> {
    match kind {
        PermissionKind::Accessibility => {
            Ok("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        }
        PermissionKind::InputMonitoring => {
            Ok("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
        }
        PermissionKind::ScreenRecording => {
            Ok("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        }
        PermissionKind::FullDiskAccess => {
            Ok("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
        }
        PermissionKind::InputCapturePortal | PermissionKind::RemoteDesktopPortal => {
            Err(Error::Unsupported)
        }
    }
}

pub(crate) fn open_settings(program: &Path, kind: PermissionKind) -> Result<()> {
    let url = settings_url(kind)?;
    let status = Command::new(program)
        .arg(url)
        .status()
        .map_err(|e| Error::Os(format!("{}: {e}", program.display())))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Os(format!("{} failed: {status}", program.display())))
    }
}
