use crate::MacPlatform;
use crate::permissions::{Probe, SystemProbe, report, settings_url};
use mouser_platform::{Error, PermissionKind, PermissionState, Platform};
use std::path::Path;

struct Fake([bool; 4]);

impl Probe for Fake {
    fn accessibility(&self) -> bool {
        self.0[0]
    }
    fn input_monitoring(&self) -> bool {
        self.0[1]
    }
    fn screen_recording(&self) -> bool {
        self.0[2]
    }
    fn full_disk_access(&self) -> bool {
        self.0[3]
    }
}

#[test]
fn report_maps_each_probe_to_its_kind() {
    use PermissionKind::*;
    use PermissionState::*;
    assert_eq!(
        report(&Fake([true, false, true, false])),
        [
            (Accessibility, Granted),
            (InputMonitoring, Missing),
            (ScreenRecording, Granted),
            (FullDiskAccess, Missing),
        ]
    );
}

#[test]
fn system_probe_answers_without_prompting() {
    let kinds: Vec<_> = report(&SystemProbe).iter().map(|(k, _)| *k).collect();
    assert_eq!(
        kinds,
        [
            PermissionKind::Accessibility,
            PermissionKind::InputMonitoring,
            PermissionKind::ScreenRecording,
            PermissionKind::FullDiskAccess,
        ]
    );
    for (_, state) in MacPlatform::new().permissions() {
        assert_ne!(state, PermissionState::NotApplicable);
    }
}

#[test]
fn each_permission_has_its_settings_pane() {
    let pane = |k| settings_url(k).unwrap();
    let base = "x-apple.systempreferences:com.apple.preference.security?";
    assert_eq!(
        pane(PermissionKind::Accessibility),
        format!("{base}Privacy_Accessibility")
    );
    assert_eq!(
        pane(PermissionKind::InputMonitoring),
        format!("{base}Privacy_ListenEvent")
    );
    assert_eq!(
        pane(PermissionKind::ScreenRecording),
        format!("{base}Privacy_ScreenCapture")
    );
    assert_eq!(
        pane(PermissionKind::FullDiskAccess),
        format!("{base}Privacy_AllFiles")
    );
    assert_eq!(
        settings_url(PermissionKind::InputCapturePortal),
        Err(Error::Unsupported)
    );
    assert_eq!(
        settings_url(PermissionKind::RemoteDesktopPortal),
        Err(Error::Unsupported)
    );
}

#[test]
fn open_passes_the_pane_url_to_the_opener() {
    let dir = std::env::temp_dir().join(format!("mouser-open-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("arg.txt");
    let script = dir.join("fake-open.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n", out.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    let p = MacPlatform::with_open_program(&script);
    p.open_permission_settings(PermissionKind::ScreenRecording)
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(&out).unwrap(),
        "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn open_reports_opener_failures() {
    let missing = MacPlatform::with_open_program(Path::new("/nonexistent/open"));
    assert!(matches!(
        missing.open_permission_settings(PermissionKind::Accessibility),
        Err(Error::Os(m)) if m.contains("/nonexistent/open")
    ));
    let failing = MacPlatform::with_open_program("/usr/bin/false");
    assert!(matches!(
        failing.open_permission_settings(PermissionKind::Accessibility),
        Err(Error::Os(m)) if m.contains("/usr/bin/false failed")
    ));
    assert_eq!(
        failing.open_permission_settings(PermissionKind::InputCapturePortal),
        Err(Error::Unsupported)
    );
}
