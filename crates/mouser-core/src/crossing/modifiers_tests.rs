use mouser_proto::model::Os;
use mouser_proto::wire::Modifiers;

use super::modifiers::*;

const C: u32 = 0x0007_0006;

#[test]
fn command_becomes_control_from_a_mac_to_windows_and_linux() {
    for to in [Os::Windows, Os::Linux] {
        assert_eq!(map_usage(Os::Macos, to, LEFT_META), LEFT_CTRL);
        assert_eq!(map_usage(Os::Macos, to, RIGHT_META), RIGHT_CTRL);
        assert_eq!(map_usage(Os::Macos, to, C), C);
        assert_eq!(map_usage(Os::Macos, to, LEFT_CTRL), LEFT_CTRL);
    }
}

#[test]
fn control_becomes_command_from_windows_and_linux_to_a_mac() {
    for from in [Os::Windows, Os::Linux] {
        assert_eq!(map_usage(from, Os::Macos, LEFT_CTRL), LEFT_META);
        assert_eq!(map_usage(from, Os::Macos, RIGHT_CTRL), RIGHT_META);
        assert_eq!(map_usage(from, Os::Macos, C), C);
        assert_eq!(map_usage(from, Os::Macos, LEFT_META), LEFT_META);
    }
}

#[test]
fn pairs_of_the_same_kind_pass_through() {
    for (from, to) in [
        (Os::Macos, Os::Macos),
        (Os::Windows, Os::Linux),
        (Os::Linux, Os::Windows),
        (Os::Windows, Os::Windows),
    ] {
        for usage in [LEFT_CTRL, LEFT_META, RIGHT_CTRL, RIGHT_META, C] {
            assert_eq!(map_usage(from, to, usage), usage);
        }
    }
}

#[test]
fn held_keys_give_the_platform_neutral_modifiers() {
    assert_eq!(held_modifiers(&[]), Modifiers::default());
    let all = held_modifiers(&[RIGHT_SHIFT, LEFT_CTRL, RIGHT_ALT, LEFT_META, C]);
    assert_eq!(
        all,
        Modifiers {
            shift: true,
            ctrl: true,
            alt: true,
            meta: true
        }
    );
    let some = held_modifiers(&[LEFT_SHIFT, RIGHT_CTRL, LEFT_ALT, RIGHT_META]);
    assert_eq!(some, all);
}
