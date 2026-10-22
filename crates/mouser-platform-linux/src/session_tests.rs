use crate::session::*;
use mouser_platform::{Capability, SessionType};
use std::collections::HashMap;

fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn xdg_session_type_selects_x11_or_wayland() {
    assert_eq!(
        session_type(&env(&[("XDG_SESSION_TYPE", "wayland")])),
        SessionType::Wayland
    );
    assert_eq!(
        session_type(&env(&[("XDG_SESSION_TYPE", "x11")])),
        SessionType::X11
    );
}

#[test]
fn xdg_session_type_wins_over_wayland_display() {
    assert_eq!(
        session_type(&env(&[
            ("XDG_SESSION_TYPE", "x11"),
            ("WAYLAND_DISPLAY", "wayland-0")
        ])),
        SessionType::X11
    );
}

#[test]
fn wayland_display_selects_wayland_when_the_session_type_is_unhelpful() {
    assert_eq!(
        session_type(&env(&[("WAYLAND_DISPLAY", "wayland-0")])),
        SessionType::Wayland
    );
    assert_eq!(
        session_type(&env(&[
            ("XDG_SESSION_TYPE", "tty"),
            ("WAYLAND_DISPLAY", "wayland-0")
        ])),
        SessionType::Wayland
    );
}

#[test]
fn no_session_hints_means_x11() {
    assert_eq!(session_type(&env(&[])), SessionType::X11);
    assert_eq!(session_type(&env(&[("DISPLAY", ":0")])), SessionType::X11);
}

fn texts(facts: &PortalFacts) -> Vec<(Capability, &'static str)> {
    wayland_limitations(facts)
        .into_iter()
        .map(|l| {
            let text = [
                NO_CAPTURE,
                NO_KEYBOARD_CAPTURE,
                NO_POINTER_CAPTURE,
                APPROVE_EACH_LOGIN,
                NO_INJECT,
            ]
            .into_iter()
            .find(|t| *t == l.text)
            .unwrap();
            (l.capability, text)
        })
        .collect()
}

#[test]
fn full_support_still_states_the_approval_at_every_login() {
    // GNOME sets bit 8 as well; only keyboard and pointer matter.
    let facts = PortalFacts {
        input_capture: Some(15),
        remote_desktop: Some(3),
    };
    assert_eq!(texts(&facts), [(Capability::Capture, APPROVE_EACH_LOGIN)]);
}

#[test]
fn a_missing_input_capture_portal_is_one_capture_limitation() {
    let facts = PortalFacts {
        input_capture: None,
        remote_desktop: Some(3),
    };
    assert_eq!(texts(&facts), [(Capability::Capture, NO_CAPTURE)]);
}

#[test]
fn a_missing_remote_desktop_portal_is_an_inject_limitation() {
    let facts = PortalFacts {
        input_capture: Some(3),
        remote_desktop: None,
    };
    assert_eq!(
        texts(&facts),
        [
            (Capability::Capture, APPROVE_EACH_LOGIN),
            (Capability::Inject, NO_INJECT)
        ]
    );
}

#[test]
fn each_missing_capture_capability_is_stated_separately() {
    let no_pointer = PortalFacts {
        input_capture: Some(1),
        remote_desktop: Some(3),
    };
    assert_eq!(
        texts(&no_pointer),
        [
            (Capability::Capture, NO_POINTER_CAPTURE),
            (Capability::Capture, APPROVE_EACH_LOGIN)
        ]
    );
    let no_keyboard = PortalFacts {
        input_capture: Some(2),
        remote_desktop: Some(3),
    };
    assert_eq!(
        texts(&no_keyboard),
        [
            (Capability::Capture, NO_KEYBOARD_CAPTURE),
            (Capability::Capture, APPROVE_EACH_LOGIN)
        ]
    );
    let neither = PortalFacts {
        input_capture: Some(4),
        remote_desktop: Some(3),
    };
    assert_eq!(
        texts(&neither),
        [
            (Capability::Capture, NO_KEYBOARD_CAPTURE),
            (Capability::Capture, NO_POINTER_CAPTURE),
            (Capability::Capture, APPROVE_EACH_LOGIN)
        ]
    );
}

#[test]
fn limitation_texts_are_plain_language() {
    for text in [
        NO_CAPTURE,
        NO_KEYBOARD_CAPTURE,
        NO_POINTER_CAPTURE,
        APPROVE_EACH_LOGIN,
        NO_INJECT,
    ] {
        assert!(
            !text.contains("portal") && !text.contains("libei"),
            "{text}"
        );
    }
}

#[test]
fn os_version_joins_name_and_version_id() {
    let text =
        "PRETTY_NAME=\"Ubuntu 24.04.4 LTS\"\nNAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nID=ubuntu\n";
    assert_eq!(os_version(text), "Ubuntu 24.04");
    assert_eq!(os_version("NAME=Arch\nVERSION_ID=rolling"), "Arch rolling");
}
