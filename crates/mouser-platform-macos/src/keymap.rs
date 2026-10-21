//! USB HID usage <-> macOS virtual keycode table (keyboard page 0x07,
//! consumer page 0x0C). Keys are addressed by physical position, so the table
//! is independent of the keyboard layout.

const KEYBOARD: u32 = 0x0007_0000;
const CONSUMER: u32 = 0x000C_0000;

/// `(HID usage, macOS virtual keycode)` for every key Mouser forwards.
pub(crate) const TABLE: &[(u32, u16)] = &[
    // Letters A-Z.
    (KEYBOARD | 0x04, 0x00),
    (KEYBOARD | 0x05, 0x0B),
    (KEYBOARD | 0x06, 0x08),
    (KEYBOARD | 0x07, 0x02),
    (KEYBOARD | 0x08, 0x0E),
    (KEYBOARD | 0x09, 0x03),
    (KEYBOARD | 0x0A, 0x05),
    (KEYBOARD | 0x0B, 0x04),
    (KEYBOARD | 0x0C, 0x22),
    (KEYBOARD | 0x0D, 0x26),
    (KEYBOARD | 0x0E, 0x28),
    (KEYBOARD | 0x0F, 0x25),
    (KEYBOARD | 0x10, 0x2E),
    (KEYBOARD | 0x11, 0x2D),
    (KEYBOARD | 0x12, 0x1F),
    (KEYBOARD | 0x13, 0x23),
    (KEYBOARD | 0x14, 0x0C),
    (KEYBOARD | 0x15, 0x0F),
    (KEYBOARD | 0x16, 0x01),
    (KEYBOARD | 0x17, 0x11),
    (KEYBOARD | 0x18, 0x20),
    (KEYBOARD | 0x19, 0x09),
    (KEYBOARD | 0x1A, 0x0D),
    (KEYBOARD | 0x1B, 0x07),
    (KEYBOARD | 0x1C, 0x10),
    (KEYBOARD | 0x1D, 0x06),
    // Digits 1-9, 0.
    (KEYBOARD | 0x1E, 0x12),
    (KEYBOARD | 0x1F, 0x13),
    (KEYBOARD | 0x20, 0x14),
    (KEYBOARD | 0x21, 0x15),
    (KEYBOARD | 0x22, 0x17),
    (KEYBOARD | 0x23, 0x16),
    (KEYBOARD | 0x24, 0x1A),
    (KEYBOARD | 0x25, 0x1C),
    (KEYBOARD | 0x26, 0x19),
    (KEYBOARD | 0x27, 0x1D),
    // Enter, Escape, Backspace, Tab, Space.
    (KEYBOARD | 0x28, 0x24),
    (KEYBOARD | 0x29, 0x35),
    (KEYBOARD | 0x2A, 0x33),
    (KEYBOARD | 0x2B, 0x30),
    (KEYBOARD | 0x2C, 0x31),
    // Punctuation: - = [ ] \ ; ' ` , . /
    (KEYBOARD | 0x2D, 0x1B),
    (KEYBOARD | 0x2E, 0x18),
    (KEYBOARD | 0x2F, 0x21),
    (KEYBOARD | 0x30, 0x1E),
    (KEYBOARD | 0x31, 0x2A),
    (KEYBOARD | 0x33, 0x29),
    (KEYBOARD | 0x34, 0x27),
    (KEYBOARD | 0x35, 0x32),
    (KEYBOARD | 0x36, 0x2B),
    (KEYBOARD | 0x37, 0x2F),
    (KEYBOARD | 0x38, 0x2C),
    (KEYBOARD | 0x39, 0x39),
    // F1-F12.
    (KEYBOARD | 0x3A, 0x7A),
    (KEYBOARD | 0x3B, 0x78),
    (KEYBOARD | 0x3C, 0x63),
    (KEYBOARD | 0x3D, 0x76),
    (KEYBOARD | 0x3E, 0x60),
    (KEYBOARD | 0x3F, 0x61),
    (KEYBOARD | 0x40, 0x62),
    (KEYBOARD | 0x41, 0x64),
    (KEYBOARD | 0x42, 0x65),
    (KEYBOARD | 0x43, 0x6D),
    (KEYBOARD | 0x44, 0x67),
    (KEYBOARD | 0x45, 0x6F),
    // Insert (Help), Home, Page Up, Delete, End, Page Down, arrows.
    (KEYBOARD | 0x49, 0x72),
    (KEYBOARD | 0x4A, 0x73),
    (KEYBOARD | 0x4B, 0x74),
    (KEYBOARD | 0x4C, 0x75),
    (KEYBOARD | 0x4D, 0x77),
    (KEYBOARD | 0x4E, 0x79),
    (KEYBOARD | 0x4F, 0x7C),
    (KEYBOARD | 0x50, 0x7B),
    (KEYBOARD | 0x51, 0x7D),
    (KEYBOARD | 0x52, 0x7E),
    // Keypad: Num Lock (Clear), / * - + Enter 1-9 0 .
    (KEYBOARD | 0x53, 0x47),
    (KEYBOARD | 0x54, 0x4B),
    (KEYBOARD | 0x55, 0x43),
    (KEYBOARD | 0x56, 0x4E),
    (KEYBOARD | 0x57, 0x45),
    (KEYBOARD | 0x58, 0x4C),
    (KEYBOARD | 0x59, 0x53),
    (KEYBOARD | 0x5A, 0x54),
    (KEYBOARD | 0x5B, 0x55),
    (KEYBOARD | 0x5C, 0x56),
    (KEYBOARD | 0x5D, 0x57),
    (KEYBOARD | 0x5E, 0x58),
    (KEYBOARD | 0x5F, 0x59),
    (KEYBOARD | 0x60, 0x5B),
    (KEYBOARD | 0x61, 0x5C),
    (KEYBOARD | 0x62, 0x52),
    (KEYBOARD | 0x63, 0x41),
    // ISO key left of Z, keypad =, F13-F20.
    (KEYBOARD | 0x64, 0x0A),
    (KEYBOARD | 0x67, 0x51),
    (KEYBOARD | 0x68, 0x69),
    (KEYBOARD | 0x69, 0x6B),
    (KEYBOARD | 0x6A, 0x71),
    (KEYBOARD | 0x6B, 0x6A),
    (KEYBOARD | 0x6C, 0x40),
    (KEYBOARD | 0x6D, 0x4F),
    (KEYBOARD | 0x6E, 0x50),
    (KEYBOARD | 0x6F, 0x5A),
    // JIS keys: keypad comma, underscore, yen, Eisu, Kana.
    (KEYBOARD | 0x85, 0x5F),
    (KEYBOARD | 0x87, 0x5E),
    (KEYBOARD | 0x89, 0x5D),
    (KEYBOARD | 0x90, 0x68),
    (KEYBOARD | 0x91, 0x66),
    // Modifiers: Left Ctrl/Shift/Alt/GUI, Right Ctrl/Shift/Alt/GUI.
    (KEYBOARD | 0xE0, 0x3B),
    (KEYBOARD | 0xE1, 0x38),
    (KEYBOARD | 0xE2, 0x3A),
    (KEYBOARD | 0xE3, 0x37),
    (KEYBOARD | 0xE4, 0x3E),
    (KEYBOARD | 0xE5, 0x3C),
    (KEYBOARD | 0xE6, 0x3D),
    (KEYBOARD | 0xE7, 0x36),
    // Consumer: Mute, Volume Up, Volume Down.
    (CONSUMER | 0xE2, 0x4A),
    (CONSUMER | 0xE9, 0x48),
    (CONSUMER | 0xEA, 0x49),
];

/// `(keycode, bit in the raw event flags that is set while the key is down,
/// device-independent flag it contributes)`. Caps Lock reports its lock state.
const MODIFIERS: &[(u16, u64, u64)] = &[
    (0x3B, 0x0000_0001, 0x0004_0000),
    (0x38, 0x0000_0002, 0x0002_0000),
    (0x3C, 0x0000_0004, 0x0002_0000),
    (0x37, 0x0000_0008, 0x0010_0000),
    (0x36, 0x0000_0010, 0x0010_0000),
    (0x3A, 0x0000_0020, 0x0008_0000),
    (0x3D, 0x0000_0040, 0x0008_0000),
    (0x3E, 0x0000_2000, 0x0004_0000),
    (0x39, 0x0001_0000, 0x0001_0000),
];

/// The macOS virtual keycode for a HID usage, if Mouser forwards it.
pub(crate) fn keycode_for_usage(usage: u32) -> Option<u16> {
    TABLE.iter().find(|(u, _)| *u == usage).map(|(_, k)| *k)
}

/// The HID usage for a macOS virtual keycode, if Mouser forwards it.
pub(crate) fn usage_for_keycode(keycode: u16) -> Option<u32> {
    TABLE.iter().find(|(_, k)| *k == keycode).map(|(u, _)| *u)
}

/// The raw-flags bit that is set while this modifier keycode is down.
pub(crate) fn modifier_down_bit(keycode: u16) -> Option<u64> {
    MODIFIERS
        .iter()
        .find(|(k, _, _)| *k == keycode)
        .map(|(_, bit, _)| *bit)
}

/// The device-independent event flag a held modifier keycode contributes.
pub(crate) fn modifier_flag(keycode: u16) -> Option<u64> {
    MODIFIERS
        .iter()
        .find(|(k, _, _)| *k == keycode)
        .map(|(_, _, flag)| *flag)
}
