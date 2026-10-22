//! USB HID usage <-> Windows set-1 scan code table (keyboard page 0x07,
//! consumer page 0x0C). Keys are addressed by physical position (scan code plus
//! the E0 "extended" prefix), so the table is independent of the keyboard
//! layout. Pause (an E1 sequence `SendInput` cannot express as a scan code) is
//! not forwarded.

const KEYBOARD: u32 = 0x0007_0000;
const CONSUMER: u32 = 0x000C_0000;

/// `(HID usage, scan code, E0-extended)` for every key Mouser forwards.
pub(crate) const TABLE: &[(u32, u16, bool)] = &[
    // Letters A-Z.
    (KEYBOARD | 0x04, 0x1E, false),
    (KEYBOARD | 0x05, 0x30, false),
    (KEYBOARD | 0x06, 0x2E, false),
    (KEYBOARD | 0x07, 0x20, false),
    (KEYBOARD | 0x08, 0x12, false),
    (KEYBOARD | 0x09, 0x21, false),
    (KEYBOARD | 0x0A, 0x22, false),
    (KEYBOARD | 0x0B, 0x23, false),
    (KEYBOARD | 0x0C, 0x17, false),
    (KEYBOARD | 0x0D, 0x24, false),
    (KEYBOARD | 0x0E, 0x25, false),
    (KEYBOARD | 0x0F, 0x26, false),
    (KEYBOARD | 0x10, 0x32, false),
    (KEYBOARD | 0x11, 0x31, false),
    (KEYBOARD | 0x12, 0x18, false),
    (KEYBOARD | 0x13, 0x19, false),
    (KEYBOARD | 0x14, 0x10, false),
    (KEYBOARD | 0x15, 0x13, false),
    (KEYBOARD | 0x16, 0x1F, false),
    (KEYBOARD | 0x17, 0x14, false),
    (KEYBOARD | 0x18, 0x16, false),
    (KEYBOARD | 0x19, 0x2F, false),
    (KEYBOARD | 0x1A, 0x11, false),
    (KEYBOARD | 0x1B, 0x2D, false),
    (KEYBOARD | 0x1C, 0x15, false),
    (KEYBOARD | 0x1D, 0x2C, false),
    // Digits 1-9, 0.
    (KEYBOARD | 0x1E, 0x02, false),
    (KEYBOARD | 0x1F, 0x03, false),
    (KEYBOARD | 0x20, 0x04, false),
    (KEYBOARD | 0x21, 0x05, false),
    (KEYBOARD | 0x22, 0x06, false),
    (KEYBOARD | 0x23, 0x07, false),
    (KEYBOARD | 0x24, 0x08, false),
    (KEYBOARD | 0x25, 0x09, false),
    (KEYBOARD | 0x26, 0x0A, false),
    (KEYBOARD | 0x27, 0x0B, false),
    // Enter, Escape, Backspace, Tab, Space.
    (KEYBOARD | 0x28, 0x1C, false),
    (KEYBOARD | 0x29, 0x01, false),
    (KEYBOARD | 0x2A, 0x0E, false),
    (KEYBOARD | 0x2B, 0x0F, false),
    (KEYBOARD | 0x2C, 0x39, false),
    // Punctuation: - = [ ] \ ; ' ` , . /
    (KEYBOARD | 0x2D, 0x0C, false),
    (KEYBOARD | 0x2E, 0x0D, false),
    (KEYBOARD | 0x2F, 0x1A, false),
    (KEYBOARD | 0x30, 0x1B, false),
    (KEYBOARD | 0x31, 0x2B, false),
    (KEYBOARD | 0x33, 0x27, false),
    (KEYBOARD | 0x34, 0x28, false),
    (KEYBOARD | 0x35, 0x29, false),
    (KEYBOARD | 0x36, 0x33, false),
    (KEYBOARD | 0x37, 0x34, false),
    (KEYBOARD | 0x38, 0x35, false),
    // Caps Lock, F1-F12.
    (KEYBOARD | 0x39, 0x3A, false),
    (KEYBOARD | 0x3A, 0x3B, false),
    (KEYBOARD | 0x3B, 0x3C, false),
    (KEYBOARD | 0x3C, 0x3D, false),
    (KEYBOARD | 0x3D, 0x3E, false),
    (KEYBOARD | 0x3E, 0x3F, false),
    (KEYBOARD | 0x3F, 0x40, false),
    (KEYBOARD | 0x40, 0x41, false),
    (KEYBOARD | 0x41, 0x42, false),
    (KEYBOARD | 0x42, 0x43, false),
    (KEYBOARD | 0x43, 0x44, false),
    (KEYBOARD | 0x44, 0x57, false),
    (KEYBOARD | 0x45, 0x58, false),
    // Print Screen, Scroll Lock.
    (KEYBOARD | 0x46, 0x37, true),
    (KEYBOARD | 0x47, 0x46, false),
    // Insert, Home, Page Up, Delete, End, Page Down, Right, Left, Down, Up.
    (KEYBOARD | 0x49, 0x52, true),
    (KEYBOARD | 0x4A, 0x47, true),
    (KEYBOARD | 0x4B, 0x49, true),
    (KEYBOARD | 0x4C, 0x53, true),
    (KEYBOARD | 0x4D, 0x4F, true),
    (KEYBOARD | 0x4E, 0x51, true),
    (KEYBOARD | 0x4F, 0x4D, true),
    (KEYBOARD | 0x50, 0x4B, true),
    (KEYBOARD | 0x51, 0x50, true),
    (KEYBOARD | 0x52, 0x48, true),
    // Num Lock (reported with the extended flag), keypad / * - + Enter 1-9 0 .
    (KEYBOARD | 0x53, 0x45, true),
    (KEYBOARD | 0x54, 0x35, true),
    (KEYBOARD | 0x55, 0x37, false),
    (KEYBOARD | 0x56, 0x4A, false),
    (KEYBOARD | 0x57, 0x4E, false),
    (KEYBOARD | 0x58, 0x1C, true),
    (KEYBOARD | 0x59, 0x4F, false),
    (KEYBOARD | 0x5A, 0x50, false),
    (KEYBOARD | 0x5B, 0x51, false),
    (KEYBOARD | 0x5C, 0x4B, false),
    (KEYBOARD | 0x5D, 0x4C, false),
    (KEYBOARD | 0x5E, 0x4D, false),
    (KEYBOARD | 0x5F, 0x47, false),
    (KEYBOARD | 0x60, 0x48, false),
    (KEYBOARD | 0x61, 0x49, false),
    (KEYBOARD | 0x62, 0x52, false),
    (KEYBOARD | 0x63, 0x53, false),
    // ISO key between Left Shift and Z, Menu (Application), keypad =.
    (KEYBOARD | 0x64, 0x56, false),
    (KEYBOARD | 0x65, 0x5D, true),
    (KEYBOARD | 0x67, 0x59, false),
    // F13-F24.
    (KEYBOARD | 0x68, 0x64, false),
    (KEYBOARD | 0x69, 0x65, false),
    (KEYBOARD | 0x6A, 0x66, false),
    (KEYBOARD | 0x6B, 0x67, false),
    (KEYBOARD | 0x6C, 0x68, false),
    (KEYBOARD | 0x6D, 0x69, false),
    (KEYBOARD | 0x6E, 0x6A, false),
    (KEYBOARD | 0x6F, 0x6B, false),
    (KEYBOARD | 0x70, 0x6C, false),
    (KEYBOARD | 0x71, 0x6D, false),
    (KEYBOARD | 0x72, 0x6E, false),
    (KEYBOARD | 0x73, 0x76, false),
    // Japanese keys: Ro, Katakana/Hiragana, Yen, Henkan, Muhenkan.
    (KEYBOARD | 0x87, 0x73, false),
    (KEYBOARD | 0x88, 0x70, false),
    (KEYBOARD | 0x89, 0x7D, false),
    (KEYBOARD | 0x8A, 0x79, false),
    (KEYBOARD | 0x8B, 0x7B, false),
    // Modifiers: Left Ctrl, Shift, Alt, Windows; Right Ctrl, Shift, Alt, Windows.
    (KEYBOARD | 0xE0, 0x1D, false),
    (KEYBOARD | 0xE1, 0x2A, false),
    (KEYBOARD | 0xE2, 0x38, false),
    (KEYBOARD | 0xE3, 0x5B, true),
    (KEYBOARD | 0xE4, 0x1D, true),
    (KEYBOARD | 0xE5, 0x36, false),
    (KEYBOARD | 0xE6, 0x38, true),
    (KEYBOARD | 0xE7, 0x5C, true),
    // Consumer page: Next, Previous, Stop, Play/Pause, Mute, Volume Up, Volume Down.
    (CONSUMER | 0xB5, 0x19, true),
    (CONSUMER | 0xB6, 0x10, true),
    (CONSUMER | 0xB7, 0x24, true),
    (CONSUMER | 0xCD, 0x22, true),
    (CONSUMER | 0xE2, 0x20, true),
    (CONSUMER | 0xE9, 0x30, true),
    (CONSUMER | 0xEA, 0x2E, true),
];

/// Scan code and E0 flag for a HID usage.
pub(crate) fn scan_for_usage(usage: u32) -> Option<(u16, bool)> {
    TABLE
        .iter()
        .find(|(u, _, _)| *u == usage)
        .map(|&(_, scan, ext)| (scan, ext))
}

/// HID usage for a scan code and E0 flag as a keyboard hook reports them.
pub(crate) fn usage_for_scan(scan: u16, extended: bool) -> Option<u32> {
    TABLE
        .iter()
        .find(|(_, s, e)| *s == scan && *e == extended)
        .map(|&(usage, _, _)| usage)
}
