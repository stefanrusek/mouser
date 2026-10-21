use super::*;

#[test]
fn capabilities_of_later_phases_are_unsupported_not_panicking() {
    let p = MacPlatform::new();
    assert!(matches!(p.clipboard(), Err(Error::Unsupported)));
    assert!(p.file_drag().is_none());
    assert!(!p.foreground_is_fullscreen());
    assert!(!p.foreground_elevated());
}

#[test]
fn delegates_network_watching() {
    let p = MacPlatform::new();
    let (tx, rx) = std::sync::mpsc::channel();
    drop(p.watch_network(tx).unwrap());
    assert!(rx.recv().is_err());
}

#[test]
fn delegates_displays_and_secure_input() {
    let p = MacPlatform::new();
    assert!(!p.displays().unwrap().is_empty());
    let (tx, rx) = std::sync::mpsc::channel();
    drop(p.watch_displays(tx).unwrap());
    assert!(rx.recv().is_err());
    // Process-wide state other tests toggle: only delegation is checked here.
    let _ = p.secure_input();
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
