use mouser_proto::model::Os;

use super::buttons::*;

#[test]
fn passthrough_injects_every_button_unchanged() {
    for button in 1..=5 {
        for down in [true, false] {
            assert_eq!(
                actions(Buttons45::Passthrough, Os::Linux, button, down),
                [Action::Button(button, down)]
            );
        }
    }
}

#[test]
fn back_forward_types_the_shortcut_of_the_receiving_system() {
    let alt_left = [
        Action::Key(0x0007_00E2, true),
        Action::Key(0x0007_0050, true),
        Action::Key(0x0007_0050, false),
        Action::Key(0x0007_00E2, false),
    ];
    assert_eq!(
        actions(Buttons45::BackForward, Os::Windows, 4, true),
        alt_left
    );
    assert_eq!(
        actions(Buttons45::BackForward, Os::Linux, 4, true),
        alt_left
    );
    let alt_right = actions(Buttons45::BackForward, Os::Windows, 5, true);
    assert_eq!(alt_right[1], Action::Key(0x0007_004F, true));
    let cmd_back = actions(Buttons45::BackForward, Os::Macos, 4, true);
    assert_eq!(cmd_back[0], Action::Key(0x0007_00E3, true));
    assert_eq!(cmd_back[1], Action::Key(0x0007_002F, true));
    let cmd_forward = actions(Buttons45::BackForward, Os::Macos, 5, true);
    assert_eq!(cmd_forward[1], Action::Key(0x0007_0030, true));
}

#[test]
fn back_forward_acts_on_the_press_only_and_leaves_other_buttons_alone() {
    assert!(actions(Buttons45::BackForward, Os::Macos, 4, false).is_empty());
    assert_eq!(
        actions(Buttons45::BackForward, Os::Macos, 1, true),
        [Action::Button(1, true)]
    );
}

#[test]
fn the_setting_is_read_from_the_config_value() {
    assert_eq!(
        Buttons45::from_config("passthrough"),
        Buttons45::Passthrough
    );
    assert_eq!(
        Buttons45::from_config("back_forward"),
        Buttons45::BackForward
    );
}

#[test]
#[should_panic(expected = "mouse.buttons_4_5")]
fn an_unvalidated_setting_is_fatal() {
    Buttons45::from_config("sideways");
}
