use crate::Settings;
use crate::permissions::{open_settings, report};
use crate::session::PortalFacts;
use mouser_platform::PermissionKind::*;
use mouser_platform::PermissionState::*;
use mouser_platform::{Error, PermissionKind, PermissionState, Result, SecretStore, SessionType};
use std::collections::HashMap;

#[derive(Default)]
struct Memory(HashMap<String, Vec<u8>>);

impl SecretStore for Memory {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.0.get(name).cloned())
    }
    fn set(&mut self, name: &str, value: &[u8]) -> Result<()> {
        self.0.insert(name.into(), value.to_vec());
        Ok(())
    }
    fn delete(&mut self, name: &str) -> Result<()> {
        self.0.remove(name);
        Ok(())
    }
}

fn state(r: &[(PermissionKind, PermissionState)], kind: PermissionKind) -> PermissionState {
    r.iter().find(|(k, _)| *k == kind).unwrap().1
}

fn facts(input_capture: bool, remote_desktop: bool) -> PortalFacts {
    PortalFacts {
        input_capture: input_capture.then_some(3),
        remote_desktop: remote_desktop.then_some(3),
    }
}

#[test]
fn on_wayland_an_offered_portal_is_granted_and_an_absent_one_is_missing() {
    let r = report(SessionType::Wayland, &facts(false, false));
    assert_eq!(state(&r, InputCapturePortal), Missing);
    assert_eq!(state(&r, RemoteDesktopPortal), Missing);
    let r = report(SessionType::Wayland, &facts(false, true));
    assert_eq!(state(&r, InputCapturePortal), Missing);
    assert_eq!(state(&r, RemoteDesktopPortal), Granted);
    let r = report(SessionType::Wayland, &facts(true, false));
    assert_eq!(state(&r, InputCapturePortal), Granted);
    assert_eq!(state(&r, RemoteDesktopPortal), Missing);
}

#[test]
fn macos_permission_kinds_do_not_apply_on_linux() {
    let r = report(SessionType::Wayland, &facts(true, true));
    for kind in [
        Accessibility,
        InputMonitoring,
        ScreenRecording,
        FullDiskAccess,
    ] {
        assert_eq!(state(&r, kind), NotApplicable, "{kind:?}");
    }
}

#[test]
fn on_x11_every_kind_is_not_applicable() {
    let r = report(SessionType::X11, &facts(true, true));
    assert_eq!(r.len(), 6);
    assert!(r.iter().all(|(_, s)| *s == NotApplicable));
}

fn settings(session: &str) -> Settings {
    Settings {
        env: HashMap::from([("XDG_SESSION_TYPE".to_string(), session.to_string())]),
        sys_class_net: "/sys/class/net".into(),
        os_release: "/etc/os-release".into(),
        exe: "/bin/mouser".into(),
        device_wait: std::time::Duration::from_millis(300),
    }
}

#[test]
fn settings_are_unsupported_on_x11_and_for_kinds_that_are_not_portals() {
    for kind in [InputCapturePortal, RemoteDesktopPortal] {
        assert_eq!(
            open_settings(&settings("x11"), Box::new(Memory::default()), kind),
            Err(Error::Unsupported)
        );
    }
    for kind in [
        Accessibility,
        InputMonitoring,
        ScreenRecording,
        FullDiskAccess,
    ] {
        assert_eq!(
            open_settings(&settings("wayland"), Box::new(Memory::default()), kind),
            Err(Error::Unsupported)
        );
    }
}
