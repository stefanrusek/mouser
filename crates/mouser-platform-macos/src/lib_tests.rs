use super::*;

#[test]
fn info_reports_macos_with_declared_limitations() {
    let info = MacPlatform::default().info();
    assert_eq!(info.os, Os::MacOs);
    assert_eq!(info.session, SessionType::Native);
    assert!(info.version.chars().next().unwrap().is_ascii_digit());
    let declared: Vec<Capability> = info.limitations.iter().map(|l| l.capability).collect();
    assert_eq!(
        declared,
        [
            Capability::Capture,
            Capability::Inject,
            Capability::Clipboard,
            Capability::FileDrag,
            Capability::SecureAttention,
        ]
    );
}

#[test]
fn capabilities_of_later_phases_are_unsupported_not_panicking() {
    let p = MacPlatform::new();
    let (tx, _rx) = std::sync::mpsc::channel();
    let (txd, _rxd) = std::sync::mpsc::channel();
    assert_eq!(p.displays().unwrap_err(), Error::Unsupported);
    assert!(matches!(p.watch_displays(txd), Err(Error::Unsupported)));
    assert!(matches!(p.watch_network(tx), Err(Error::Unsupported)));
    assert!(matches!(p.capture(), Err(Error::Unsupported)));
    assert!(matches!(p.inject(), Err(Error::Unsupported)));
    assert!(matches!(p.clipboard(), Err(Error::Unsupported)));
    assert!(p.file_drag().is_none());
    assert!(p.secure_input().is_none());
    assert!(!p.foreground_is_fullscreen());
    assert!(!p.foreground_elevated());
}

#[test]
fn delegates_permissions_interfaces_and_autostart_status() {
    let p = MacPlatform::new();
    assert_eq!(p.permissions().len(), 4);
    assert!(
        p.interfaces()
            .iter()
            .any(|i| i.kind == mouser_platform::InterfaceKind::Loopback)
    );
    assert!(!p.autostart().enabled());
    assert!(
        p.secrets()
            .get("mouser-nonexistent-lib-test")
            .unwrap()
            .is_none()
    );
}
