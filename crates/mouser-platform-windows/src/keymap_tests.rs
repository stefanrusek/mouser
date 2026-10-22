use crate::keymap::*;

#[test]
fn every_forwarded_usage_round_trips_through_its_scan_code() {
    assert!(TABLE.len() > 120);
    for &(usage, scan, ext) in TABLE {
        assert_eq!(scan_for_usage(usage), Some((scan, ext)));
        assert_eq!(usage_for_scan(scan, ext), Some(usage));
    }
}

#[test]
fn table_has_no_duplicate_usage_or_scan_code() {
    for (i, (u, s, e)) in TABLE.iter().enumerate() {
        for (u2, s2, e2) in &TABLE[i + 1..] {
            assert_ne!(u, u2, "duplicate usage {u:#x}");
            assert_ne!((s, e), (s2, e2), "duplicate scan code {s:#x} ext={e}");
        }
    }
}

#[test]
fn known_keys_map_to_their_physical_positions() {
    // A, Z, 1, Enter, Space, Left Ctrl, Left Windows, F13, Right Arrow.
    assert_eq!(scan_for_usage(0x0007_0004), Some((0x1E, false)));
    assert_eq!(scan_for_usage(0x0007_001D), Some((0x2C, false)));
    assert_eq!(scan_for_usage(0x0007_001E), Some((0x02, false)));
    assert_eq!(scan_for_usage(0x0007_0028), Some((0x1C, false)));
    assert_eq!(scan_for_usage(0x0007_002C), Some((0x39, false)));
    assert_eq!(scan_for_usage(0x0007_00E0), Some((0x1D, false)));
    assert_eq!(scan_for_usage(0x0007_00E3), Some((0x5B, true)));
    assert_eq!(scan_for_usage(0x0007_0068), Some((0x64, false)));
    assert_eq!(scan_for_usage(0x0007_004F), Some((0x4D, true)));
    // The E0 prefix tells keypad Enter from Enter and Right Ctrl from Left Ctrl.
    assert_eq!(usage_for_scan(0x1C, true), Some(0x0007_0058));
    assert_eq!(usage_for_scan(0x1D, true), Some(0x0007_00E4));
    // Consumer page: Volume Up, Volume Down, Mute.
    assert_eq!(scan_for_usage(0x000C_00E9), Some((0x30, true)));
    assert_eq!(scan_for_usage(0x000C_00EA), Some((0x2E, true)));
    assert_eq!(scan_for_usage(0x000C_00E2), Some((0x20, true)));
}

#[test]
fn unforwarded_keys_have_no_mapping() {
    // Pause, and a usage from another page.
    assert_eq!(scan_for_usage(0x0007_0048), None);
    assert_eq!(scan_for_usage(0x0001_0004), None);
    // The fake E0 Shift Windows sends around Num Lock navigation keys.
    assert_eq!(usage_for_scan(0x2A, true), None);
}
