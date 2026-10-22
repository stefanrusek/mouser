use crate::permissions::{KINDS, open_settings, report};
use mouser_platform::{Error, PermissionKind, PermissionState};

#[test]
fn every_kind_is_not_applicable() {
    let all = report();
    assert_eq!(all.len(), 6);
    for kind in [
        PermissionKind::Accessibility,
        PermissionKind::InputMonitoring,
        PermissionKind::ScreenRecording,
        PermissionKind::FullDiskAccess,
        PermissionKind::InputCapturePortal,
        PermissionKind::RemoteDesktopPortal,
    ] {
        assert!(
            all.contains(&(kind, PermissionState::NotApplicable)),
            "{kind:?}"
        );
    }
}

#[test]
fn no_settings_pane_opens_for_any_kind() {
    for kind in KINDS {
        assert_eq!(open_settings(kind), Err(Error::Unsupported), "{kind:?}");
    }
}
