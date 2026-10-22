use crate::keymap::*;

#[test]
fn every_forwarded_usage_round_trips_through_its_keycode() {
    assert!(TABLE.len() > 100);
    for (usage, keycode) in TABLE {
        assert_eq!(keycode_for_usage(*usage), Some(*keycode));
        assert_eq!(usage_for_keycode(*keycode), Some(*usage));
    }
}

#[test]
fn table_has_no_duplicate_usage_or_keycode() {
    for (i, (u, k)) in TABLE.iter().enumerate() {
        for (u2, k2) in &TABLE[i + 1..] {
            assert_ne!(u, u2, "duplicate usage {u:#x}");
            assert_ne!(k, k2, "duplicate keycode {k:#x}");
        }
    }
}

#[test]
fn known_keys_map_to_their_physical_positions() {
    // Keyboard page: A, Z, 1, Enter, Space, Left Ctrl, Left GUI, F13, Right Arrow.
    assert_eq!(keycode_for_usage(0x0007_0004), Some(0x00));
    assert_eq!(keycode_for_usage(0x0007_001D), Some(0x06));
    assert_eq!(keycode_for_usage(0x0007_001E), Some(0x12));
    assert_eq!(keycode_for_usage(0x0007_0028), Some(0x24));
    assert_eq!(keycode_for_usage(0x0007_002C), Some(0x31));
    assert_eq!(keycode_for_usage(0x0007_00E0), Some(0x3B));
    assert_eq!(keycode_for_usage(0x0007_00E3), Some(0x37));
    assert_eq!(keycode_for_usage(0x0007_0068), Some(0x69));
    assert_eq!(keycode_for_usage(0x0007_004F), Some(0x7C));
    // Consumer page: Volume Up, Volume Down, Mute.
    assert_eq!(keycode_for_usage(0x000C_00E9), Some(0x48));
    assert_eq!(keycode_for_usage(0x000C_00EA), Some(0x49));
    assert_eq!(keycode_for_usage(0x000C_00E2), Some(0x4A));
}

#[test]
fn unforwarded_keys_have_no_mapping() {
    assert_eq!(keycode_for_usage(0x0007_0046), None);
    assert_eq!(keycode_for_usage(0x0001_0004), None);
    assert_eq!(usage_for_keycode(0x3F), None);
}

#[test]
fn modifier_bits_and_flags() {
    assert_eq!(modifier_down_bit(0x38), Some(0x2));
    assert_eq!(modifier_flag(0x38), Some(0x0002_0000));
    assert_eq!(modifier_flag(0x3C), Some(0x0002_0000));
    assert_eq!(modifier_flag(0x37), Some(0x0010_0000));
    assert_eq!(modifier_down_bit(0x39), Some(0x0001_0000));
    assert_eq!(modifier_down_bit(0x00), None);
    assert_eq!(modifier_flag(0x00), None);
    assert_eq!(modifier_mask(0x37), Some(0x0010_0008));
    assert_eq!(modifier_mask(0x3D), Some(0x0008_0040));
    assert_eq!(modifier_mask(0x00), None);
}
