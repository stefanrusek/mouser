use super::*;

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
    assert_eq!(declared, [Capability::Inject, Capability::SecureAttention,]);
}

#[test]
fn a_portable_run_declares_no_more_capabilities_than_an_installed_one() {
    // Core adds the portable login-screen limitation itself; the platform's
    // own list for a portable run on a drive with permissions is unchanged.
    let root = tempfile::tempdir().unwrap();
    let info = WinPlatform::portable(root.path().join("MouserData")).info();
    let declared: Vec<Capability> = info.limitations.iter().map(|l| l.capability).collect();
    assert!(declared.starts_with(&[Capability::Inject, Capability::SecureAttention]));
    assert!(
        !info
            .limitations
            .iter()
            .any(|l| l.text.starts_with("Portable mode"))
    );
}

#[test]
fn games_and_remote_desktop_apps_are_a_declared_limitation_of_injection() {
    let info = WinPlatform::default().info();
    assert!(info.limitations.iter().any(|l| {
        l.capability == Capability::Inject
            && l.text
                .contains("Some games and remote-desktop apps ignore Mouser's input on this PC")
    }));
}
