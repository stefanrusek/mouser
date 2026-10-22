use super::*;
use mouser_platform::Error;

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
fn secure_input_is_not_offered_yet() {
    let p = WinPlatform::new();
    assert!(p.file_drag().is_some());
    assert!(p.secure_input().is_none());
    let _ = p.foreground_is_fullscreen();
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

#[test]
fn delegates_displays_and_their_watch() {
    let p = WinPlatform::new();
    assert_eq!(p.displays().unwrap(), crate::displays::displays().unwrap());
    let (tx, _rx) = std::sync::mpsc::channel();
    drop(p.watch_displays(tx).unwrap());
}

#[test]
fn capture_and_injection_are_provided_and_elevation_is_asked_of_uipi() {
    let p = WinPlatform::new();
    drop(p.capture().unwrap());
    drop(p.inject().unwrap());
    let _ = p.foreground_elevated();
}

#[test]
fn problems_reach_the_listener_when_there_is_one() {
    let p = WinPlatform::new();
    let sink = p.problem_sink();
    // Nobody listens yet.
    sink("unheard".into());
    let (tx, rx) = std::sync::mpsc::channel();
    p.watch_problems(tx);
    sink("heard".into());
    assert_eq!(rx.try_recv().unwrap(), "heard");
    // A listener that went away is not an error.
    drop(rx);
    sink("gone".into());
}

#[test]
fn starting_the_platform_undoes_a_pointer_hidden_by_an_earlier_run() {
    let _live = live_input();
    // What a crashed run leaves: the cursors replaced, with nothing in this
    // process recorded as having done it.
    crate::cursor::hide().unwrap();
    crate::cursor::forget_hidden();
    assert!(crate::cursor::arrow_is_blank());
    let _p = WinPlatform::new();
    assert!(!crate::cursor::arrow_is_blank());
}
