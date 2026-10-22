//! Step definitions for `features/us11.feature`: one hotkey shows a letter
//! on every display, and a letter jumps the pointer there.

use std::fmt;
use std::time::Duration;

use cucumber::{World, given, then, when};
use mouser_core::EngineEvent;
use mouser_platform::LocalInput;
use mouser_proto::control::Method;
use mouser_proto::model::DisplayId;
use mouser_testkit::{Injected, VirtualPlatform};
use serde_json::json;

use crate::support::desk::Lab;

const NAMES: [&str; 3] = ["Alpha", "Beta", "Gamma"];

const CTRL: u32 = 0x0007_00E0;
const SHIFT: u32 = 0x0007_00E1;
const ALT: u32 = 0x0007_00E2;
const J: u32 = 0x0007_000D;
const ESC: u32 = 0x0007_0029;
const Z: u32 = 0x0007_001D;

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct JumpWorld {
    pub lab: Lab,
    /// The letters last shown, once they are.
    letters: Vec<(DisplayId, char)>,
}

impl fmt::Debug for JumpWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JumpWorld({:?})", self.lab.names())
    }
}

fn tap(platform: &VirtualPlatform, usage: u32) {
    for down in [true, false] {
        platform.send_input(LocalInput::Key { usage, down });
    }
}

fn usage_of(letter: char) -> u32 {
    // Letters are consecutive from A = 0x04.
    0x0007_0004 + (letter as u32 - 'A' as u32)
}

fn is_show(event: &EngineEvent) -> bool {
    matches!(event, EngineEvent::QuickJumpShow { .. })
}

fn is_hide(event: &EngineEvent) -> bool {
    matches!(event, EngineEvent::QuickJumpHide)
}

impl JumpWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            letters: Vec::new(),
        }
    }

    /// Three computers with one display each on one desk.
    async fn desk(&mut self) {
        let setups = NAMES
            .iter()
            .map(|name| (*name, crate::support::desk::Setup::default()))
            .collect();
        self.lab
            .form_desk("Home office", "Alpha", &["Beta", "Gamma"], setups)
            .await;
    }

    fn platform(&mut self, name: &str) -> VirtualPlatform {
        self.lab.node(name).platform.clone()
    }

    /// Ctrl+Alt+Shift+J on Alpha's keyboard.
    fn press_hotkey(&mut self) {
        let alpha = self.platform("Alpha");
        for usage in [CTRL, ALT, SHIFT] {
            alpha.send_input(LocalInput::Key { usage, down: true });
        }
        tap(&alpha, J);
        for usage in [SHIFT, ALT, CTRL] {
            alpha.send_input(LocalInput::Key { usage, down: false });
        }
    }

    /// Every computer shows the `nth` set of letters, and they are the same.
    async fn letters_shown(&mut self, nth: usize) -> Vec<(DisplayId, char)> {
        let mut seen = Vec::new();
        for name in NAMES {
            let event = self
                .lab
                .node(name)
                .engine_event("the letters", nth, is_show)
                .await;
            let EngineEvent::QuickJumpShow { letters } = event else {
                unreachable!("matched by is_show");
            };
            seen.push(letters);
        }
        assert!(
            seen.iter().all(|l| *l == seen[0]),
            "same letters everywhere"
        );
        seen.pop().unwrap()
    }

    async fn letters_hidden(&mut self, nth: usize) {
        for name in NAMES {
            self.lab
                .node(name)
                .engine_event("the letters to disappear", nth, is_hide)
                .await;
        }
    }

    fn letter_of(&mut self, name: &str) -> char {
        let id = self.lab.node(name).id();
        self.letters
            .iter()
            .find(|(d, _)| d.computer == id)
            .expect("a letter for every display")
            .1
    }
}

// ---- scenario 1: letters appear on every display ----------------------------------------

#[given("QuickJump has never been configured")]
async fn never_configured(w: &mut JumpWorld) {
    w.desk().await;
    let hotkey: String = w
        .lab
        .node("Alpha")
        .call(Method::ConfigGet, json!({"key": "hotkeys.quickjump"}))
        .await;
    assert_eq!(hotkey, "Ctrl+Alt+Shift+J", "the default hotkey is in force");
}

#[when("the desk user presses the QuickJump hotkey")]
async fn presses_the_hotkey(w: &mut JumpWorld) {
    w.press_hotkey();
}

#[then("a distinct letter appears on every display on the desk")]
async fn a_letter_on_every_display(w: &mut JumpWorld) {
    let letters = w.letters_shown(0).await;
    assert_eq!(letters.len(), NAMES.len());
    for name in NAMES {
        w.letters = letters.clone();
        w.letter_of(name);
    }
    let mut chars: Vec<char> = letters.iter().map(|(_, c)| *c).collect();
    chars.sort();
    chars.dedup();
    assert_eq!(chars.len(), NAMES.len(), "the letters are distinct");
}

// ---- scenario 2: pressing a letter jumps ------------------------------------------------

#[given("the letters are showing")]
async fn letters_are_showing(w: &mut JumpWorld) {
    w.desk().await;
    w.press_hotkey();
    w.letters = w.letters_shown(0).await;
}

#[when("the desk user presses one")]
async fn presses_one(w: &mut JumpWorld) {
    let letter = w.letter_of("Gamma");
    tap(&w.platform("Alpha"), usage_of(letter));
}

#[then(
    "the pointer moves to the centre of that display, that computer becomes the active computer, and the letters disappear"
)]
async fn the_pointer_jumped(w: &mut JumpWorld) {
    w.letters_hidden(0).await;
    let gamma = w.lab.node("Gamma").id();
    w.lab
        .node("Gamma")
        .wait_status("Gamma to be the active computer", |s| {
            s.active_computer == Some(gamma)
        })
        .await;
    let injected = w.platform("Gamma").injected();
    assert!(
        injected.iter().any(|i| matches!(i,
            Injected::MotionAbs { x, y } if (x - 960.0).abs() < 1.0 && (y - 540.0).abs() < 1.0)),
        "the pointer is at the centre of Gamma's display: {injected:?}"
    );
    assert!(w.platform("Alpha").is_grabbed());
}

// ---- scenario 3: Esc or another key cancels ---------------------------------------------

#[when("the desk user presses Esc or any key that is not a shown letter")]
async fn presses_a_cancelling_key(w: &mut JumpWorld) {
    tap(&w.platform("Alpha"), ESC);
    w.letters_hidden(0).await;
    // Three displays use three letters: Z is not one of them.
    w.press_hotkey();
    w.letters_shown(1).await;
    tap(&w.platform("Alpha"), Z);
}

#[then("the letters disappear and the pointer stays where it was")]
async fn the_letters_disappear(w: &mut JumpWorld) {
    w.letters_hidden(1).await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let alpha = w.platform("Alpha");
    assert!(!alpha.is_grabbed());
    assert!(alpha.warps().is_empty());
    for name in ["Beta", "Gamma"] {
        assert!(
            w.platform(name).injected().is_empty(),
            "{name} was left alone"
        );
    }
}
