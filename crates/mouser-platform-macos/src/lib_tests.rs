use super::*;

#[test]
fn clipboard_and_file_drag_are_supported() {
    let p = MacPlatform::new();
    // Building the clipboard only opens the pasteboard; nothing is read.
    assert!(p.clipboard().is_ok());
    // Building the drag handler reads nothing and starts nothing.
    assert!(p.file_drag().is_some());
    assert!(p.foreground_app().is_none());
}

#[test]
fn delegates_the_fullscreen_query() {
    // The answer depends on what is on screen; the call must just complete.
    let _ = MacPlatform::new().foreground_is_fullscreen();
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
