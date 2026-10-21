//! Shortcut mapping across operating systems (Story 2 scenario 4): Command on
//! a Mac keyboard is Control elsewhere, and the other way round.

use mouser_proto::model::Os;
use mouser_proto::wire::Modifiers;

pub const LEFT_CTRL: u32 = 0x0007_00E0;
pub const LEFT_SHIFT: u32 = 0x0007_00E1;
pub const LEFT_ALT: u32 = 0x0007_00E2;
pub const LEFT_META: u32 = 0x0007_00E3;
pub const RIGHT_CTRL: u32 = 0x0007_00E4;
pub const RIGHT_SHIFT: u32 = 0x0007_00E5;
pub const RIGHT_ALT: u32 = 0x0007_00E6;
pub const RIGHT_META: u32 = 0x0007_00E7;

/// The key to inject on a `to` computer for `usage` typed on a `from`
/// computer's keyboard.
pub fn map_usage(from: Os, to: Os, usage: u32) -> u32 {
    match (from == Os::Macos, to == Os::Macos) {
        (true, false) => match usage {
            LEFT_META => LEFT_CTRL,
            RIGHT_META => RIGHT_CTRL,
            other => other,
        },
        (false, true) => match usage {
            LEFT_CTRL => LEFT_META,
            RIGHT_CTRL => RIGHT_META,
            other => other,
        },
        _ => usage,
    }
}

/// The modifiers among the held `keys`.
pub fn held_modifiers<'a>(keys: impl IntoIterator<Item = &'a u32>) -> Modifiers {
    let mut m = Modifiers::default();
    for key in keys {
        match *key {
            LEFT_SHIFT | RIGHT_SHIFT => m.shift = true,
            LEFT_CTRL | RIGHT_CTRL => m.ctrl = true,
            LEFT_ALT | RIGHT_ALT => m.alt = true,
            LEFT_META | RIGHT_META => m.meta = true,
            _ => {}
        }
    }
    m
}
