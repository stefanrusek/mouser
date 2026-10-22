use super::clipboard::{GNOME_SHARING, Strategy, choose, is_gnome, limitations, major};
use mouser_platform::Capability;
use std::collections::HashMap;

fn env(desktop: &str) -> HashMap<String, String> {
    HashMap::from([("XDG_CURRENT_DESKTOP".to_string(), desktop.to_string())])
}

#[test]
fn the_portal_clipboard_is_used_first_on_any_desktop() {
    assert_eq!(choose(true, true), Strategy::Portal);
    assert_eq!(choose(true, false), Strategy::Portal);
}

#[test]
fn without_the_portal_clipboard_data_control_is_the_fallback_except_on_gnome() {
    assert_eq!(choose(false, false), Strategy::DataControl);
    assert_eq!(choose(false, true), Strategy::Unavailable);
}

#[test]
fn gnome_is_found_in_the_desktop_list() {
    assert!(is_gnome(&env("ubuntu:GNOME")));
    assert!(is_gnome(&env("gnome")));
    assert!(!is_gnome(&env("KDE")));
    assert!(!is_gnome(&HashMap::new()));
}

#[test]
fn the_shell_major_version_is_parsed() {
    assert_eq!(major("46.2"), Some(46));
    assert_eq!(major("51"), Some(51));
    assert_eq!(major("beta"), None);
}

#[test]
fn gnome_before_51_or_of_unknown_version_lists_the_sharing_limitation() {
    for version in [Some(46), Some(50), None] {
        let l = limitations(true, version);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].capability, Capability::Clipboard);
        assert_eq!(l[0].text, GNOME_SHARING);
        assert!(
            l[0].text.contains(
                "cannot share the clipboard from this computer while it shares its mouse"
            )
        );
    }
    assert!(limitations(true, Some(51)).is_empty());
    assert!(limitations(false, Some(46)).is_empty());
}
