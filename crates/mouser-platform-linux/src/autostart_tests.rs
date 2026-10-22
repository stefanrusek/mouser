use crate::autostart::XdgAutostart;
use mouser_platform::Autostart;
use std::path::Path;

#[test]
fn enabling_writes_the_autostart_entry_and_disabling_removes_it() {
    let config = tempfile::tempdir().unwrap();
    let entry = config.path().join("autostart/com.mouser.app.desktop");
    let mut autostart =
        XdgAutostart::new(config.path().to_path_buf(), Path::new("/opt/mouser/mouser"));
    assert!(!autostart.enabled());
    autostart.set(true).unwrap();
    assert!(autostart.enabled());
    let text = std::fs::read_to_string(&entry).unwrap();
    assert!(text.starts_with("[Desktop Entry]\n"), "{text}");
    assert!(text.contains("Type=Application\n"));
    assert!(text.contains("Exec=\"/opt/mouser/mouser\"\n"));
    autostart.set(true).unwrap();
    autostart.set(false).unwrap();
    assert!(!autostart.enabled());
    assert!(!entry.exists());
    autostart.set(false).unwrap();
}
