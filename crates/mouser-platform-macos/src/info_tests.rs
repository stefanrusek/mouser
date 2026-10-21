use crate::MacPlatform;
use mouser_platform::{Capability, Os, Platform, SessionType};

#[test]
fn info_reports_macos_with_the_declared_limitations() {
    let info = MacPlatform::default().info();
    assert_eq!(info.os, Os::MacOs);
    assert_eq!(info.session, SessionType::Native);
    assert!(info.version.chars().next().unwrap().is_ascii_digit());
    let declared: Vec<Capability> = info.limitations.iter().map(|l| l.capability).collect();
    assert_eq!(declared, [Capability::Inject, Capability::SecureAttention,]);
}

#[test]
fn injection_limitation_names_games_and_remote_desktop_apps() {
    let info = MacPlatform::new().info();
    let inject = info
        .limitations
        .iter()
        .find(|l| l.capability == Capability::Inject)
        .unwrap();
    assert_eq!(
        inject.text,
        "Some games and remote-desktop apps ignore Mouser's input on this Mac."
    );
}

#[test]
fn capture_and_injection_are_supported_so_not_limitations() {
    let p = MacPlatform::new();
    assert!(
        !p.info()
            .limitations
            .iter()
            .any(|l| l.capability == Capability::Capture)
    );
    assert!(p.capture().is_ok());
    assert!(p.inject().is_ok());
}
