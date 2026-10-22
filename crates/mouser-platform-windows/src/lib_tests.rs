use super::*;

fn private_settings(dir: &std::path::Path) -> Settings {
    let root = format!(r"Software\MouserTest\lib-{}", std::process::id());
    Settings {
        secrets_dir: dir.join("secrets"),
        run_key: format!(r"{root}\Run"),
        approved_key: format!(r"{root}\StartupApproved\Run"),
        exe: r"C:\Mouser\Mouser.exe".into(),
    }
}

#[test]
fn info_reports_windows_with_its_version_and_declared_limitations() {
    let info = WinPlatform::default().info();
    assert_eq!(info.os, Os::Windows);
    assert_eq!(info.session, SessionType::Native);
    let parts: Vec<u32> = info
        .version
        .split('.')
        .map(|p| p.parse().unwrap())
        .collect();
    assert_eq!(parts.len(), 3, "{}", info.version);
    assert_eq!(
        parts[0], 10,
        "Windows 10 and 11 both report major version 10"
    );
    let declared: Vec<Capability> = info.limitations.iter().map(|l| l.capability).collect();
    assert_eq!(
        declared,
        [
            Capability::Capture,
            Capability::Inject,
            Capability::Clipboard,
            Capability::FileDrag
        ]
    );
}

#[test]
fn system_settings_point_at_program_data_and_the_real_run_keys() {
    let s = Settings::system();
    let expected = PathBuf::from(std::env::var("ProgramData").unwrap()).join(r"Mouser\secrets");
    assert_eq!(
        s.secrets_dir.to_string_lossy().to_lowercase(),
        expected.to_string_lossy().to_lowercase()
    );
    assert_eq!(s.run_key, r"Software\Microsoft\Windows\CurrentVersion\Run");
    assert_eq!(
        s.approved_key,
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
    );
    assert_eq!(s.exe, std::env::current_exe().unwrap());
}

#[test]
fn capabilities_of_later_phases_are_unsupported_not_panicking() {
    let p = WinPlatform::new();
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
fn delegates_permissions_interfaces_secrets_and_autostart() {
    let dir = tempfile::tempdir().unwrap();
    let p = WinPlatform::with_settings(private_settings(dir.path()));
    assert!(
        p.permissions()
            .iter()
            .all(|(_, s)| *s == PermissionState::NotApplicable)
    );
    assert_eq!(
        p.open_permission_settings(PermissionKind::Accessibility),
        Err(Error::Unsupported)
    );
    assert!(
        p.interfaces()
            .iter()
            .any(|i| i.kind == mouser_platform::InterfaceKind::Loopback)
    );
    p.secrets().set("lib-test", b"v").unwrap();
    assert!(dir.path().join(r"secrets\lib-test").exists());
    let mut autostart = p.autostart();
    assert!(!autostart.enabled());
    autostart.set(true).unwrap();
    assert!(p.autostart().enabled());
    autostart.set(false).unwrap();
    let r = unsafe {
        windows::Win32::System::Registry::RegDeleteTreeW(
            windows::Win32::System::Registry::HKEY_CURRENT_USER,
            &windows::core::HSTRING::from(format!(
                r"Software\MouserTest\lib-{}",
                std::process::id()
            )),
        )
    };
    assert_eq!(r, windows::Win32::Foundation::ERROR_SUCCESS);
}
