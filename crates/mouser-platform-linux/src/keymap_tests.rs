use crate::keymap::*;

#[test]
fn letters_digits_and_enter_map_to_their_evdev_codes() {
    assert_eq!(evdev_for_usage(0x0007_0004), Some(30)); // A
    assert_eq!(evdev_for_usage(0x0007_001D), Some(44)); // Z
    assert_eq!(evdev_for_usage(0x0007_001E), Some(2)); // 1
    assert_eq!(evdev_for_usage(0x0007_0027), Some(11)); // 0
    assert_eq!(evdev_for_usage(0x0007_0028), Some(28)); // Enter
    assert_eq!(evdev_for_usage(0x0007_0029), Some(1)); // Escape
}

#[test]
fn modifiers_and_the_consumer_page_are_mapped() {
    assert_eq!(evdev_for_usage(0x0007_00E0), Some(29)); // left Ctrl
    assert_eq!(evdev_for_usage(0x0007_00E3), Some(125)); // left GUI
    assert_eq!(evdev_for_usage(0x0007_00E7), Some(126)); // right GUI
    assert_eq!(evdev_for_usage(0x000C_00E9), Some(115)); // Volume Up
    assert_eq!(evdev_for_usage(0x0007_0073), Some(194)); // F24
}

#[test]
fn x11_keycodes_are_evdev_plus_eight() {
    assert_eq!(x11_keycode_for_usage(0x0007_0004), Some(38)); // A is 38 on X11
    assert_eq!(usage_for_x11_keycode(38), Some(0x0007_0004));
    assert_eq!(x11_keycode_for_usage(0x000C_00E9), Some(123));
}

#[test]
fn unknown_keys_map_to_nothing() {
    assert_eq!(evdev_for_usage(0x0007_0032), None);
    assert_eq!(evdev_for_usage(0x0001_0004), None);
    assert_eq!(usage_for_evdev(0), None);
    assert_eq!(x11_keycode_for_usage(0x0001_0004), None);
    assert_eq!(usage_for_x11_keycode(3), None);
    assert_eq!(usage_for_x11_keycode(250), None);
}

#[test]
fn every_entry_round_trips_and_none_is_ambiguous() {
    let mut usages = std::collections::HashSet::new();
    let mut codes = std::collections::HashSet::new();
    for usage in (0..0x0010_0000u32).filter(|u| evdev_for_usage(*u).is_some()) {
        let code = evdev_for_usage(usage).unwrap();
        assert_eq!(usage_for_evdev(code), Some(usage), "{usage:#x}");
        assert!(usages.insert(usage));
        assert!(codes.insert(code), "evdev {code} mapped twice");
        let x11 = x11_keycode_for_usage(usage).unwrap();
        assert_eq!(usage_for_x11_keycode(x11), Some(usage));
    }
    assert!(usages.len() > 120);
}
