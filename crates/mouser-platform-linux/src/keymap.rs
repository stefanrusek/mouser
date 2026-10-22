//! The HID usage <-> evdev keycode table. A usage is `(page << 16) | id`;
//! X11 keycodes are the evdev code plus 8.

const KEYBOARD_PAGE: u32 = 0x0007_0000;
const CONSUMER_PAGE: u32 = 0x000C_0000;

/// `(usage, evdev code)`. Keyboard page (0x07) then consumer page (0x0C).
const TABLE: &[(u32, u16)] = &[
    // Letters A-Z.
    (KEYBOARD_PAGE | 0x04, 30),
    (KEYBOARD_PAGE | 0x05, 48),
    (KEYBOARD_PAGE | 0x06, 46),
    (KEYBOARD_PAGE | 0x07, 32),
    (KEYBOARD_PAGE | 0x08, 18),
    (KEYBOARD_PAGE | 0x09, 33),
    (KEYBOARD_PAGE | 0x0A, 34),
    (KEYBOARD_PAGE | 0x0B, 35),
    (KEYBOARD_PAGE | 0x0C, 23),
    (KEYBOARD_PAGE | 0x0D, 36),
    (KEYBOARD_PAGE | 0x0E, 37),
    (KEYBOARD_PAGE | 0x0F, 38),
    (KEYBOARD_PAGE | 0x10, 50),
    (KEYBOARD_PAGE | 0x11, 49),
    (KEYBOARD_PAGE | 0x12, 24),
    (KEYBOARD_PAGE | 0x13, 25),
    (KEYBOARD_PAGE | 0x14, 16),
    (KEYBOARD_PAGE | 0x15, 19),
    (KEYBOARD_PAGE | 0x16, 31),
    (KEYBOARD_PAGE | 0x17, 20),
    (KEYBOARD_PAGE | 0x18, 22),
    (KEYBOARD_PAGE | 0x19, 47),
    (KEYBOARD_PAGE | 0x1A, 17),
    (KEYBOARD_PAGE | 0x1B, 45),
    (KEYBOARD_PAGE | 0x1C, 21),
    (KEYBOARD_PAGE | 0x1D, 44),
    // Digits 1-9, 0.
    (KEYBOARD_PAGE | 0x1E, 2),
    (KEYBOARD_PAGE | 0x1F, 3),
    (KEYBOARD_PAGE | 0x20, 4),
    (KEYBOARD_PAGE | 0x21, 5),
    (KEYBOARD_PAGE | 0x22, 6),
    (KEYBOARD_PAGE | 0x23, 7),
    (KEYBOARD_PAGE | 0x24, 8),
    (KEYBOARD_PAGE | 0x25, 9),
    (KEYBOARD_PAGE | 0x26, 10),
    (KEYBOARD_PAGE | 0x27, 11),
    // Enter, Escape, Backspace, Tab, Space, punctuation.
    (KEYBOARD_PAGE | 0x28, 28),
    (KEYBOARD_PAGE | 0x29, 1),
    (KEYBOARD_PAGE | 0x2A, 14),
    (KEYBOARD_PAGE | 0x2B, 15),
    (KEYBOARD_PAGE | 0x2C, 57),
    (KEYBOARD_PAGE | 0x2D, 12),
    (KEYBOARD_PAGE | 0x2E, 13),
    (KEYBOARD_PAGE | 0x2F, 26),
    (KEYBOARD_PAGE | 0x30, 27),
    (KEYBOARD_PAGE | 0x31, 43),
    (KEYBOARD_PAGE | 0x33, 39),
    (KEYBOARD_PAGE | 0x34, 40),
    (KEYBOARD_PAGE | 0x35, 41),
    (KEYBOARD_PAGE | 0x36, 51),
    (KEYBOARD_PAGE | 0x37, 52),
    (KEYBOARD_PAGE | 0x38, 53),
    (KEYBOARD_PAGE | 0x39, 58),
    // F1-F12.
    (KEYBOARD_PAGE | 0x3A, 59),
    (KEYBOARD_PAGE | 0x3B, 60),
    (KEYBOARD_PAGE | 0x3C, 61),
    (KEYBOARD_PAGE | 0x3D, 62),
    (KEYBOARD_PAGE | 0x3E, 63),
    (KEYBOARD_PAGE | 0x3F, 64),
    (KEYBOARD_PAGE | 0x40, 65),
    (KEYBOARD_PAGE | 0x41, 66),
    (KEYBOARD_PAGE | 0x42, 67),
    (KEYBOARD_PAGE | 0x43, 68),
    (KEYBOARD_PAGE | 0x44, 87),
    (KEYBOARD_PAGE | 0x45, 88),
    // Print Screen, Scroll Lock, Pause, navigation block.
    (KEYBOARD_PAGE | 0x46, 99),
    (KEYBOARD_PAGE | 0x47, 70),
    (KEYBOARD_PAGE | 0x48, 119),
    (KEYBOARD_PAGE | 0x49, 110),
    (KEYBOARD_PAGE | 0x4A, 102),
    (KEYBOARD_PAGE | 0x4B, 104),
    (KEYBOARD_PAGE | 0x4C, 111),
    (KEYBOARD_PAGE | 0x4D, 107),
    (KEYBOARD_PAGE | 0x4E, 109),
    (KEYBOARD_PAGE | 0x4F, 106),
    (KEYBOARD_PAGE | 0x50, 105),
    (KEYBOARD_PAGE | 0x51, 108),
    (KEYBOARD_PAGE | 0x52, 103),
    // Keypad.
    (KEYBOARD_PAGE | 0x53, 69),
    (KEYBOARD_PAGE | 0x54, 98),
    (KEYBOARD_PAGE | 0x55, 55),
    (KEYBOARD_PAGE | 0x56, 74),
    (KEYBOARD_PAGE | 0x57, 78),
    (KEYBOARD_PAGE | 0x58, 96),
    (KEYBOARD_PAGE | 0x59, 79),
    (KEYBOARD_PAGE | 0x5A, 80),
    (KEYBOARD_PAGE | 0x5B, 81),
    (KEYBOARD_PAGE | 0x5C, 75),
    (KEYBOARD_PAGE | 0x5D, 76),
    (KEYBOARD_PAGE | 0x5E, 77),
    (KEYBOARD_PAGE | 0x5F, 71),
    (KEYBOARD_PAGE | 0x60, 72),
    (KEYBOARD_PAGE | 0x61, 73),
    (KEYBOARD_PAGE | 0x62, 82),
    (KEYBOARD_PAGE | 0x63, 83),
    // ISO extra key, Application (menu), keypad equals.
    (KEYBOARD_PAGE | 0x64, 86),
    (KEYBOARD_PAGE | 0x65, 127),
    (KEYBOARD_PAGE | 0x67, 117),
    // F13-F24.
    (KEYBOARD_PAGE | 0x68, 183),
    (KEYBOARD_PAGE | 0x69, 184),
    (KEYBOARD_PAGE | 0x6A, 185),
    (KEYBOARD_PAGE | 0x6B, 186),
    (KEYBOARD_PAGE | 0x6C, 187),
    (KEYBOARD_PAGE | 0x6D, 188),
    (KEYBOARD_PAGE | 0x6E, 189),
    (KEYBOARD_PAGE | 0x6F, 190),
    (KEYBOARD_PAGE | 0x70, 191),
    (KEYBOARD_PAGE | 0x71, 192),
    (KEYBOARD_PAGE | 0x72, 193),
    (KEYBOARD_PAGE | 0x73, 194),
    // International keys (ro, katakana/hiragana, yen, henkan, muhenkan) and Hangul.
    (KEYBOARD_PAGE | 0x87, 89),
    (KEYBOARD_PAGE | 0x88, 93),
    (KEYBOARD_PAGE | 0x89, 124),
    (KEYBOARD_PAGE | 0x8A, 92),
    (KEYBOARD_PAGE | 0x8B, 94),
    (KEYBOARD_PAGE | 0x90, 122),
    (KEYBOARD_PAGE | 0x91, 123),
    // Modifiers: left Ctrl, Shift, Alt, GUI then right.
    (KEYBOARD_PAGE | 0xE0, 29),
    (KEYBOARD_PAGE | 0xE1, 42),
    (KEYBOARD_PAGE | 0xE2, 56),
    (KEYBOARD_PAGE | 0xE3, 125),
    (KEYBOARD_PAGE | 0xE4, 97),
    (KEYBOARD_PAGE | 0xE5, 54),
    (KEYBOARD_PAGE | 0xE6, 100),
    (KEYBOARD_PAGE | 0xE7, 126),
    // Consumer page: media and brightness keys.
    (CONSUMER_PAGE | 0x6F, 225),
    (CONSUMER_PAGE | 0x70, 224),
    (CONSUMER_PAGE | 0xB5, 163),
    (CONSUMER_PAGE | 0xB6, 165),
    (CONSUMER_PAGE | 0xB7, 166),
    (CONSUMER_PAGE | 0xB8, 161),
    (CONSUMER_PAGE | 0xCD, 164),
    (CONSUMER_PAGE | 0xE2, 113),
    (CONSUMER_PAGE | 0xE9, 115),
    (CONSUMER_PAGE | 0xEA, 114),
];

/// The evdev code of a HID usage.
pub(crate) fn evdev_for_usage(usage: u32) -> Option<u16> {
    TABLE
        .iter()
        .find(|(hid, _)| *hid == usage)
        .map(|(_, code)| *code)
}

/// The HID usage of an evdev code.
pub(crate) fn usage_for_evdev(code: u16) -> Option<u32> {
    TABLE
        .iter()
        .find(|(_, evdev)| *evdev == code)
        .map(|(hid, _)| *hid)
}

/// The X11 keycode of a HID usage.
pub(crate) fn x11_keycode_for_usage(usage: u32) -> Option<u8> {
    evdev_for_usage(usage).map(|code| (code + 8) as u8)
}

/// The HID usage of an X11 keycode.
pub(crate) fn usage_for_x11_keycode(keycode: u8) -> Option<u32> {
    usage_for_evdev(u16::from(keycode).checked_sub(8)?)
}
