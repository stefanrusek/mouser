use crate::{LinuxPlatform, Settings};
use mouser_platform::{Capability, Os, Platform, SessionType};
use std::collections::HashMap;

fn platform(session: &str) -> (LinuxPlatform, tempfile::TempDir) {
    platform_on(session, HashMap::new())
}

fn platform_on(
    session: &str,
    display: HashMap<String, String>,
) -> (LinuxPlatform, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("os-release"),
        "NAME=\"Testix\"\nVERSION_ID=\"3\"\n",
    )
    .unwrap();
    let settings = Settings {
        env: HashMap::from([
            ("XDG_SESSION_TYPE".to_string(), session.to_string()),
            (
                "XDG_CONFIG_HOME".to_string(),
                dir.path().join("config").display().to_string(),
            ),
        ])
        .into_iter()
        .chain(display)
        .collect(),
        sys_class_net: "/sys/class/net".into(),
        os_release: dir.path().join("os-release"),
        exe: "/opt/mouser/mouser".into(),
        device_wait: std::time::Duration::from_millis(300),
    };
    (LinuxPlatform::with_settings(settings), dir)
}

#[test]
fn info_reports_linux_and_declares_what_is_unavailable() {
    let (p, _dir) = platform("x11");
    let info = p.info();
    assert_eq!(info.os, Os::Linux);
    assert_eq!(info.session, SessionType::X11);
    assert_eq!(info.version, "Testix 3");
    let declared: Vec<Capability> = info.limitations.iter().map(|l| l.capability).collect();
    for capability in [Capability::FileDrag, Capability::SecureAttention] {
        assert!(declared.contains(&capability), "{capability:?}");
    }
}

#[test]
fn x11_supports_capture_and_injection_so_does_not_list_them() {
    let (p, _dir) = platform("x11");
    let limitations = p.info().limitations;
    assert!(
        !limitations
            .iter()
            .any(|l| l.capability == Capability::Inject)
    );
    assert!(
        !limitations
            .iter()
            .any(|l| l.capability == Capability::Capture && l.text.contains("mouse and keyboard"))
    );
    assert!(p.capture().is_ok());
    assert!(p.inject().is_ok());
}

#[test]
fn wayland_capture_and_injection_start_on_first_use() {
    let (p, _dir) = platform("wayland");
    assert!(p.capture().is_ok());
    // Nothing changes on Wayland, but the engine still needs a watch to hold.
    drop(p.watch_displays(std::sync::mpsc::channel().0).unwrap());
}

#[test]
fn wayland_lists_that_quickjump_cannot_start_here_and_x11_does_not() {
    let text = "QuickJump and hotkey crossings can't start from this computer on Wayland; push the pointer off the screen edge instead.";
    let has = |p: &LinuxPlatform| {
        p.info()
            .limitations
            .iter()
            .any(|l| l.capability == Capability::Capture && l.text == text)
    };
    let (wayland, _a) = platform("wayland");
    let (x11, _b) = platform("x11");
    assert!(has(&wayland));
    assert!(!has(&x11));
}

#[test]
fn x11_has_a_clipboard() {
    use crate::x11::testserver::Xvfb;
    let server = Xvfb::start();
    let (p, _dir) = platform_on("x11", server.env());
    assert!(p.clipboard().is_ok());
}

#[test]
fn wayland_has_a_clipboard_that_starts_on_first_use() {
    let (p, _dir) = platform("wayland");
    assert!(p.clipboard().is_ok());
}

#[test]
fn windows_and_macos_only_features_are_absent() {
    let (p, _dir) = platform("x11");
    assert!(p.secure_input().is_none());
    assert!(p.foreground_app().is_none());
    assert!(p.file_drag().is_none());
}

#[test]
fn autostart_writes_into_the_config_dir() {
    let (p, dir) = platform("x11");
    let mut autostart = p.autostart();
    assert!(!autostart.enabled());
    autostart.set(true).unwrap();
    assert!(autostart.enabled());
    let entry = dir.path().join("config/autostart/com.mouser.app.desktop");
    assert!(
        std::fs::read_to_string(entry)
            .unwrap()
            .contains("Exec=\"/opt/mouser/mouser\"")
    );
}

#[test]
fn interfaces_come_from_this_machine() {
    let (p, _dir) = platform("x11");
    assert!(p.interfaces().iter().any(|i| i.name == "lo"));
}

#[test]
fn the_system_settings_describe_this_machine() {
    let settings = Settings::system();
    assert_eq!(settings.os_release, std::path::Path::new("/etc/os-release"));
    assert!(settings.exe.is_absolute());
    assert!(settings.env.contains_key("PATH"));
    let _default = LinuxPlatform::default();
}

#[test]
fn x11_displays_and_their_changes_come_from_randr() {
    use crate::x11::{connect, testserver::Xvfb};
    use x11rb::protocol::randr::ConnectionExt as _;
    let server = Xvfb::start();
    let (p, _dir) = platform_on("x11", server.env());
    let displays = p.displays().unwrap();
    assert_eq!(displays[0].pixel_size, (1920, 1080));
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = p.watch_displays(tx).unwrap();
    let (conn, root) = connect(&server.env());
    conn.randr_set_screen_size(root, 1920, 1080, 600, 340)
        .unwrap()
        .check()
        .unwrap();
    assert!(
        !rx.recv_timeout(std::time::Duration::from_secs(10))
            .unwrap()
            .is_empty()
    );
    drop(handle);
}

#[test]
fn the_platform_watches_the_network_until_the_handle_is_dropped() {
    let (p, _dir) = platform("x11");
    drop(p.watch_network(std::sync::mpsc::channel().0).unwrap());
}
