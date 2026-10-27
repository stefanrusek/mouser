//! The backend against a private session bus with fake portals, Secret
//! Service and StatusNotifier watcher. Runs wherever `dbus-daemon` does, so
//! the CI runner (no desktop) exercises the same code as a real session.

#![cfg(target_os = "linux")]

mod clipboard;
mod compositor;
mod daemon;
mod datacontrol;
mod eis;
mod netwatch;
mod portals;
mod secret_service;
mod wayland;

use daemon::{Daemon, FAKES, wait_until};
use mouser_platform::{Capability, Error, PermissionKind, PermissionState, Platform, SessionType};
use mouser_platform_linux::{LinuxPlatform, Settings};
use portals::{InputCaptureConfig, PortalConfig, Portals};
use secret_service::{SecretService, SecretServiceConfig};
use std::collections::HashMap;

/// A platform on `bus` (`None`: no session bus at all) with a scratch home.
pub struct Machine {
    pub platform: LinuxPlatform,
    pub home: tempfile::TempDir,
}

pub fn machine(bus: Option<&Daemon>, session: &str) -> Machine {
    machine_waiting(bus, session, std::time::Duration::from_secs(5))
}

/// Like [`machine`], with Wayland input waiting `wait` for a device.
pub fn machine_waiting(bus: Option<&Daemon>, session: &str, wait: std::time::Duration) -> Machine {
    machine_with(bus, session, wait, &[])
}

/// Like [`machine_waiting`], with more environment variables.
pub fn machine_with(
    bus: Option<&Daemon>,
    session: &str,
    wait: std::time::Duration,
    extra: &[(&str, &str)],
) -> Machine {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(
        home.path().join("os-release"),
        "NAME=\"Testix\"\nVERSION_ID=\"1.2\"\n",
    )
    .unwrap();
    let mut env = HashMap::from([
        ("XDG_SESSION_TYPE".to_string(), session.to_string()),
        (
            "XDG_CONFIG_HOME".to_string(),
            home.path().join("config").display().to_string(),
        ),
    ]);
    if let Some(bus) = bus {
        env.insert("DBUS_SESSION_BUS_ADDRESS".into(), bus.address.clone());
    }
    env.extend(extra.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    let platform = LinuxPlatform::with_settings(Settings {
        env,
        sys_class_net: "/sys/class/net".into(),
        os_release: home.path().join("os-release"),
        exe: "/bin/mouser".into(),
        device_wait: wait,
    });
    Machine { platform, home }
}

pub fn full_portals() -> PortalConfig {
    PortalConfig {
        remote_desktop: true,
        clipboard: true,
        registry: true,
        input_capture: Some(InputCaptureConfig::new(2, 15, None)),
        dialog_ms: 0,
        no_rd_token: false,
        no_keyboard: false,
        clipboard_enabled: true,
    }
}

fn texts(platform: &LinuxPlatform, capability: Capability) -> Vec<String> {
    platform
        .info()
        .limitations
        .into_iter()
        .filter(|l| l.capability == capability)
        .map(|l| l.text)
        .collect()
}

fn state(platform: &LinuxPlatform, kind: PermissionKind) -> PermissionState {
    platform
        .permissions()
        .into_iter()
        .find(|(k, _)| *k == kind)
        .unwrap()
        .1
}

fn start_watcher(bus: &Daemon) -> zbus::Connection {
    FAKES
        .block_on(
            zbus::connection::Builder::address(bus.address.as_str())
                .unwrap()
                .name("org.kde.StatusNotifierWatcher")
                .unwrap()
                .build(),
        )
        .unwrap()
}

// ---- info: session, portals, tray ----

#[test]
fn info_reports_the_os_version_and_session() {
    let bus = Daemon::start();
    let m = machine(Some(&bus), "x11");
    let info = m.platform.info();
    assert_eq!(info.version, "Testix 1.2");
    assert_eq!(info.session, SessionType::X11);
}

#[test]
fn wayland_with_full_portals_states_only_the_login_approval() {
    let bus = Daemon::start();
    let _portals = Portals::start(&bus.address, full_portals());
    let m = machine(Some(&bus), "wayland");
    assert_eq!(m.platform.info().session, SessionType::Wayland);
    let capture = texts(&m.platform, Capability::Capture);
    assert!(
        capture.iter().any(|t| t.contains("each time you sign in")),
        "{capture:?}"
    );
    assert!(!capture.iter().any(|t| t.contains("cannot share its")));
    assert!(
        !texts(&m.platform, Capability::Inject)
            .iter()
            .any(|t| t.contains("does not let Mouser"))
    );
}

#[test]
fn wayland_without_portals_lists_capture_and_inject_limitations() {
    let bus = Daemon::start();
    let _portals = Portals::start(
        &bus.address,
        PortalConfig {
            registry: true,
            ..PortalConfig::default()
        },
    );
    let m = machine(Some(&bus), "wayland");
    let capture = texts(&m.platform, Capability::Capture);
    assert!(
        capture
            .iter()
            .any(|t| t.contains("cannot share its mouse and keyboard"))
    );
    assert!(
        texts(&m.platform, Capability::Inject)
            .iter()
            .any(|t| t.contains("does not let Mouser move the mouse"))
    );
}

#[test]
fn wayland_without_keyboard_capture_says_so() {
    let bus = Daemon::start();
    let _portals = Portals::start(
        &bus.address,
        PortalConfig {
            input_capture: Some(InputCaptureConfig::new(1, 2, None)),
            ..full_portals()
        },
    );
    let m = machine(Some(&bus), "wayland");
    let capture = texts(&m.platform, Capability::Capture);
    assert!(
        capture
            .iter()
            .any(|t| t.contains("cannot share its keyboard"))
    );
}

#[test]
fn x11_does_not_ask_the_portals() {
    let bus = Daemon::start();
    let m = machine(Some(&bus), "x11");
    let capture = texts(&m.platform, Capability::Capture);
    assert!(!capture.iter().any(|t| t.contains("cannot share its")));
    assert!(!capture.iter().any(|t| t.contains("each time you sign in")));
}

#[test]
fn a_tray_watcher_on_the_bus_removes_the_tray_limitation() {
    let bus = Daemon::start();
    let m = machine(Some(&bus), "x11");
    let has_tray_limit = |m: &Machine| {
        texts(&m.platform, Capability::Tray)
            .iter()
            .any(|t| t.contains("AppIndicator"))
    };
    assert!(has_tray_limit(&m));
    let _watcher = start_watcher(&bus);
    assert!(!has_tray_limit(&m));
}

#[test]
fn without_a_session_bus_the_tray_and_portals_are_missing() {
    let m = machine(None, "wayland");
    let tray = texts(&m.platform, Capability::Tray);
    assert!(tray.iter().any(|t| t.contains("AppIndicator")));
    let capture = texts(&m.platform, Capability::Capture);
    assert!(
        capture
            .iter()
            .any(|t| t.contains("cannot share its mouse and keyboard"))
    );
}

// ---- secrets ----

#[test]
fn the_secret_service_holds_secrets_when_its_collection_is_unlocked() {
    let bus = Daemon::start();
    let _service = SecretService::start(
        &bus.address,
        SecretServiceConfig {
            locked: false,
            has_default: true,
            alias_fails: false,
        },
    );
    let m = machine(Some(&bus), "x11");
    let mut store = m.platform.secrets();
    assert_eq!(store.get("device-key").unwrap(), None);
    store.set("device-key", b"one").unwrap();
    store.set("other", b"x").unwrap();
    store.set("device-key", &[9, 8, 7]).unwrap();
    assert_eq!(
        m.platform.secrets().get("device-key").unwrap(),
        Some(vec![9, 8, 7])
    );
    store.delete("device-key").unwrap();
    assert_eq!(store.get("device-key").unwrap(), None);
    assert_eq!(store.get("other").unwrap(), Some(b"x".to_vec()));
    store.delete("device-key").unwrap();
    assert!(
        !m.home.path().join("config/mouser/secrets").exists(),
        "nothing was written to the file store"
    );
}

#[test]
fn a_locked_or_missing_default_collection_falls_back_to_the_file_store() {
    for config in [
        SecretServiceConfig {
            locked: true,
            has_default: true,
            alias_fails: false,
        },
        SecretServiceConfig {
            locked: false,
            has_default: false,
            alias_fails: false,
        },
        SecretServiceConfig {
            locked: false,
            has_default: true,
            alias_fails: true,
        },
    ] {
        let bus = Daemon::start();
        let _service = SecretService::start(&bus.address, config);
        let m = machine(Some(&bus), "x11");
        m.platform.secrets().set("desk-key", b"abc").unwrap();
        assert_eq!(
            std::fs::read(m.home.path().join("config/mouser/secrets/desk-key")).unwrap(),
            b"abc"
        );
    }
}

#[test]
fn a_bus_without_a_secret_service_falls_back_to_the_file_store() {
    let bus = Daemon::start();
    let m = machine(Some(&bus), "x11");
    m.platform.secrets().set("desk-key", b"abc").unwrap();
    assert!(
        m.home
            .path()
            .join("config/mouser/secrets/desk-key")
            .exists()
    );
}

// ---- portal permissions ----

#[test]
fn on_x11_opening_portal_settings_is_unsupported() {
    let bus = Daemon::start();
    let m = machine(Some(&bus), "x11");
    assert_eq!(
        m.platform
            .open_permission_settings(PermissionKind::RemoteDesktopPortal),
        Err(Error::Unsupported)
    );
    assert!(
        m.platform
            .permissions()
            .iter()
            .all(|(_, s)| *s == PermissionState::NotApplicable)
    );
}

#[test]
fn approving_remote_desktop_starts_the_session_and_keeps_the_single_use_token() {
    let bus = Daemon::start();
    let portals = Portals::start(
        &bus.address,
        PortalConfig {
            dialog_ms: 400,
            ..full_portals()
        },
    );
    let m = machine(Some(&bus), "wayland");
    // Offered portals count as granted before any dialog: the dialog appears
    // when capture or injection starts, which core only does once granted.
    assert_eq!(
        state(&m.platform, PermissionKind::RemoteDesktopPortal),
        PermissionState::Granted
    );
    m.platform
        .open_permission_settings(PermissionKind::RemoteDesktopPortal)
        .unwrap();
    // The call returned while the dialog is still waiting for the user.
    assert!(!portals.calls.contains("rd.start.done"));
    wait_until("the first token", || {
        m.platform
            .secrets()
            .get("portal-token-remote-desktop")
            .unwrap()
            .is_some()
    });
    assert_eq!(
        m.platform
            .secrets()
            .get("portal-token-remote-desktop")
            .unwrap(),
        Some(b"rd-token-1".to_vec())
    );
    m.platform
        .open_permission_settings(PermissionKind::RemoteDesktopPortal)
        .unwrap();
    wait_until("the replaced token", || {
        m.platform
            .secrets()
            .get("portal-token-remote-desktop")
            .unwrap()
            == Some(b"rd-token-2".to_vec())
    });
    let calls = portals.calls.all();
    assert!(calls.contains(&"registry.register com.mouser.app".to_string()));
    assert!(calls.contains(&"clipboard.request".to_string()));
    let selects: Vec<_> = calls
        .iter()
        .filter(|c| c.starts_with("rd.select_devices "))
        .collect();
    assert_eq!(
        selects,
        [
            "rd.select_devices persist_mode=Some(2) token=None",
            "rd.select_devices persist_mode=Some(2) token=Some(\"rd-token-1\")"
        ]
    );
    assert_eq!(
        state(&m.platform, PermissionKind::InputCapturePortal),
        PermissionState::Granted
    );
}

#[test]
fn a_host_without_the_registry_still_gets_its_dialog() {
    let bus = Daemon::start();
    let portals = Portals::start(
        &bus.address,
        PortalConfig {
            registry: false,
            ..full_portals()
        },
    );
    let m = machine(Some(&bus), "wayland");
    m.platform
        .open_permission_settings(PermissionKind::RemoteDesktopPortal)
        .unwrap();
    wait_until("the token", || {
        m.platform
            .secrets()
            .get("portal-token-remote-desktop")
            .unwrap()
            .is_some()
    });
    assert!(
        !portals
            .calls
            .all()
            .iter()
            .any(|c| c.starts_with("registry"))
    );
}

#[test]
fn approving_input_capture_v2_starts_a_persistent_session() {
    let bus = Daemon::start();
    let portals = Portals::start(
        &bus.address,
        PortalConfig {
            input_capture: Some(InputCaptureConfig::new(2, 7, Some("ic-token"))),
            ..full_portals()
        },
    );
    let m = machine(Some(&bus), "wayland");
    m.platform
        .open_permission_settings(PermissionKind::InputCapturePortal)
        .unwrap();
    wait_until("the token", || {
        m.platform
            .secrets()
            .get("portal-token-input-capture")
            .unwrap()
            .is_some()
    });
    let calls = portals.calls.all();
    assert!(calls.contains(&"ic.create_session2".to_string()));
    assert!(calls.contains(&"ic.start persist_mode=Some(2)".to_string()));
}

#[test]
fn approving_input_capture_v1_creates_the_session_and_keeps_no_token() {
    let bus = Daemon::start();
    let portals = Portals::start(
        &bus.address,
        PortalConfig {
            input_capture: Some(InputCaptureConfig::new(1, 15, None)),
            ..full_portals()
        },
    );
    let m = machine(Some(&bus), "wayland");
    m.platform
        .open_permission_settings(PermissionKind::InputCapturePortal)
        .unwrap();
    wait_until("the session", || {
        portals.calls.contains("ic.create_session.done")
    });
    assert_eq!(
        m.platform
            .secrets()
            .get("portal-token-input-capture")
            .unwrap(),
        None
    );
}

#[test]
fn a_portal_the_desktop_does_not_offer_is_missing() {
    let bus = Daemon::start();
    let _portals = Portals::start(
        &bus.address,
        PortalConfig {
            input_capture: None,
            ..full_portals()
        },
    );
    let m = machine(Some(&bus), "wayland");
    assert_eq!(
        state(&m.platform, PermissionKind::InputCapturePortal),
        PermissionState::Missing
    );
    assert_eq!(
        state(&m.platform, PermissionKind::RemoteDesktopPortal),
        PermissionState::Granted
    );
}

#[test]
fn without_a_session_bus_the_wayland_portals_are_missing() {
    let m = machine(None, "wayland");
    assert_eq!(
        state(&m.platform, PermissionKind::InputCapturePortal),
        PermissionState::Missing
    );
    assert_eq!(
        state(&m.platform, PermissionKind::RemoteDesktopPortal),
        PermissionState::Missing
    );
}

#[test]
fn a_portable_run_keeps_secrets_on_the_drive_even_with_a_secret_service() {
    let bus = Daemon::start();
    let _service = SecretService::start(
        &bus.address,
        SecretServiceConfig {
            locked: false,
            has_default: true,
            alias_fails: false,
        },
    );
    let drive = tempfile::tempdir().unwrap();
    let data = drive.path().join("MouserData");
    let platform = LinuxPlatform::portable_with_settings(
        Settings {
            env: HashMap::from([
                ("XDG_SESSION_TYPE".to_string(), "x11".to_string()),
                ("DBUS_SESSION_BUS_ADDRESS".to_string(), bus.address.clone()),
            ]),
            sys_class_net: "/sys/class/net".into(),
            os_release: "/etc/os-release".into(),
            exe: "/media/drive/mouser".into(),
            device_wait: std::time::Duration::from_secs(1),
        },
        &data,
    );
    platform.secrets().set("desk-key", b"abc").unwrap();
    assert_eq!(
        std::fs::read(data.join("secrets/desk-key")).unwrap(),
        b"abc"
    );
}
