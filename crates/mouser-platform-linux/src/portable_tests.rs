use crate::portable::{KEEPS_PERMISSION, limitations};
use crate::{LinuxPlatform, Settings};
use mouser_platform::{Capability, Error, Platform, SessionType};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;

fn settings(session: &str, home: &tempfile::TempDir) -> Settings {
    std::fs::write(
        home.path().join("os-release"),
        "NAME=\"Testix\"\nVERSION_ID=\"3\"\n",
    )
    .unwrap();
    Settings {
        env: HashMap::from([
            ("XDG_SESSION_TYPE".to_string(), session.to_string()),
            (
                "XDG_CONFIG_HOME".to_string(),
                home.path().join("config").display().to_string(),
            ),
        ]),
        sys_class_net: "/sys/class/net".into(),
        os_release: home.path().join("os-release"),
        exe: "/media/drive/mouser".into(),
        device_wait: std::time::Duration::from_millis(300),
    }
}

#[test]
fn secrets_are_files_under_the_drives_mouser_data() {
    let home = tempfile::tempdir().unwrap();
    let drive = tempfile::tempdir().unwrap();
    let data = drive.path().join("MouserData");
    let p = LinuxPlatform::portable_with_settings(settings("x11", &home), &data);
    let mut secrets = p.secrets();
    secrets.set("desk-key", b"abc").unwrap();
    let file = data.join("secrets/desk-key");
    assert_eq!(std::fs::read(&file).unwrap(), b"abc");
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(p.secrets().get("desk-key").unwrap(), Some(b"abc".to_vec()));
    assert!(
        !home.path().join("config").exists(),
        "nothing was written to the computer"
    );
}

#[test]
fn no_autostart_file_is_written() {
    let home = tempfile::tempdir().unwrap();
    let drive = tempfile::tempdir().unwrap();
    let p = LinuxPlatform::portable_with_settings(settings("x11", &home), drive.path());
    let mut autostart = p.autostart();
    assert!(!autostart.enabled());
    assert_eq!(autostart.set(true), Err(Error::Unsupported));
    assert!(!autostart.enabled());
    assert!(!home.path().join("config").exists());
}

#[test]
fn on_wayland_the_desktop_keeps_the_permission_after_the_drive_is_removed() {
    let home = tempfile::tempdir().unwrap();
    let drive = tempfile::tempdir().unwrap();
    let texts = |p: &LinuxPlatform| -> Vec<String> {
        p.info()
            .limitations
            .into_iter()
            .filter(|l| l.capability == Capability::Capture)
            .map(|l| l.text)
            .collect()
    };
    let portable = LinuxPlatform::portable_with_settings(settings("wayland", &home), drive.path());
    assert!(texts(&portable).contains(&KEEPS_PERMISSION.to_string()));
    assert!(KEEPS_PERMISSION.contains("keeps Mouser's permission after you remove the drive"));
    let installed = LinuxPlatform::with_settings(settings("wayland", &home));
    assert!(!texts(&installed).contains(&KEEPS_PERMISSION.to_string()));
}

#[test]
fn on_x11_there_is_no_permission_to_keep() {
    assert!(limitations(true, SessionType::X11).is_empty());
    assert!(limitations(false, SessionType::Wayland).is_empty());
    assert_eq!(limitations(true, SessionType::Wayland).len(), 1);
}

#[test]
fn the_system_portable_platform_is_built_from_this_machine() {
    let drive = tempfile::tempdir().unwrap();
    let p = LinuxPlatform::portable(drive.path().join("MouserData"));
    assert!(p.autostart().set(true).is_err());
}

#[test]
fn without_a_drive_secrets_stay_in_the_computers_config() {
    let home = tempfile::tempdir().unwrap();
    let p = LinuxPlatform::with_settings(settings("x11", &home));
    p.secrets().set("desk-key", b"abc").unwrap();
    assert_eq!(
        std::fs::read(home.path().join("config/mouser/secrets/desk-key")).unwrap(),
        b"abc"
    );
}
