//! Mouse buttons 4 and 5 (Story 2 scenario 11): passed through unchanged, or
//! acting as Back and Forward on the computer that receives them.

use mouser_proto::model::Os;

use super::modifiers::{LEFT_ALT, LEFT_META};

/// What to inject for one button event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Button(u8, bool),
    Key(u32, bool),
}

/// The `mouse.buttons_4_5` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Buttons45 {
    Passthrough,
    BackForward,
}

impl Buttons45 {
    /// Parse the config value; `Config::validate` has already checked it.
    pub fn from_config(value: &str) -> Self {
        match value {
            "passthrough" => Self::Passthrough,
            "back_forward" => Self::BackForward,
            other => panic!("mouse.buttons_4_5 is {other:?}"),
        }
    }
}

const LEFT_ARROW: u32 = 0x0007_0050;
const RIGHT_ARROW: u32 = 0x0007_004F;
const BRACKET_LEFT: u32 = 0x0007_002F;
const BRACKET_RIGHT: u32 = 0x0007_0030;

/// The injections for `button` going `down` on a computer running `os`.
/// Back is Command-[ on a Mac and Alt-Left elsewhere; Forward is Command-]
/// and Alt-Right. The shortcut is typed when the button goes down.
pub fn actions(setting: Buttons45, os: Os, button: u8, down: bool) -> Vec<Action> {
    if setting == Buttons45::Passthrough || !matches!(button, 4 | 5) {
        return vec![Action::Button(button, down)];
    }
    if !down {
        return Vec::new();
    }
    let (modifier, back, forward) = match os {
        Os::Macos => (LEFT_META, BRACKET_LEFT, BRACKET_RIGHT),
        Os::Windows | Os::Linux => (LEFT_ALT, LEFT_ARROW, RIGHT_ARROW),
    };
    let key = if button == 4 { back } else { forward };
    vec![
        Action::Key(modifier, true),
        Action::Key(key, true),
        Action::Key(key, false),
        Action::Key(modifier, false),
    ]
}
