//! Step definitions for `features/v2_us4.feature`: hotkeys, names and
//! pointer behavior changed while Mouser runs.

use std::fmt;
use std::time::Duration;

use cucumber::{World, given, then, when};
use mouser_core::EngineEvent;
use mouser_platform::{ClipRep, LocalInput, Os, PlatformInfo, SecureInputHolder, SessionType};
use mouser_proto::control::{Empty, Method, Topic};
use mouser_testkit::{Injected, VirtualPlatform};
use serde_json::{Value, json};

use crate::support::desk::{Lab, Setup};

const WAIT: Duration = Duration::from_secs(20);

const C: u32 = 0x0007_0006;
const V: u32 = 0x0007_0019;
const P: u32 = 0x0007_0013;
const ESC: u32 = 0x0007_0029;
const F5: u32 = 0x0007_003E;
const CTRL: u32 = 0x0007_00E0;
const SHIFT: u32 = 0x0007_00E1;
const ALT: u32 = 0x0007_00E2;
const META: u32 = 0x0007_00E3;
const J: u32 = 0x0007_000D;
const K: u32 = 0x0007_000E;

const NEW_NAME: &str = "Studio PC";

#[derive(World)]
#[world(init = Self::new)]
pub struct TailorWorld {
    pub lab: Lab,
    /// The combination the owner recorded last.
    recorded: Option<Recorded>,
}

/// Which hotkey the owner re-recorded, for the step that tries both.
#[derive(Clone, Copy)]
enum Recorded {
    QuickJump,
    TargetedPaste,
}

impl fmt::Debug for TailorWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TailorWorld({:?})", self.lab.names())
    }
}

/// A computer running Mouser 0.3.0 on `os`.
fn setup(os: Os) -> Setup {
    let platform = VirtualPlatform::new();
    platform.set_info(PlatformInfo {
        os,
        version: "test".into(),
        session: SessionType::Native,
        limitations: Vec::new(),
    });
    Setup {
        platform,
        mouser_version: "0.3.0".into(),
        ..Setup::default()
    }
}

fn key(usage: u32, down: bool) -> LocalInput {
    LocalInput::Key { usage, down }
}

/// `keys` held while `then` is tapped, then all let go.
fn press(platform: &VirtualPlatform, keys: &[u32], then: u32) {
    for usage in keys {
        platform.send_input(key(*usage, true));
    }
    platform.send_input(key(then, true));
    platform.send_input(key(then, false));
    for usage in keys.iter().rev() {
        platform.send_input(key(*usage, false));
    }
}

fn push_right(platform: &VirtualPlatform) {
    platform.send_input(LocalInput::Motion {
        dx: 1.0,
        dy: 0.0,
        abs: (1919.0, 540.0),
    });
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = std::time::Instant::now() + WAIT;
    while !done() {
        assert!(
            std::time::Instant::now() < end,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn injected(platform: &VirtualPlatform, what: &str, pred: impl Fn(&Injected) -> bool) {
    until(what, || platform.injected().iter().any(&pred)).await;
}

fn keys_of(platform: &VirtualPlatform) -> Vec<(u32, bool)> {
    platform
        .injected()
        .into_iter()
        .filter_map(|i| match i {
            Injected::Key { usage, down } => Some((usage, down)),
            _ => None,
        })
        .collect()
}

fn count_of(platform: &VirtualPlatform, usage: u32) -> usize {
    keys_of(platform)
        .iter()
        .filter(|(u, down)| *u == usage && *down)
        .count()
}

fn text(s: &str) -> Vec<ClipRep> {
    vec![ClipRep::Text(s.into())]
}

impl TailorWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            recorded: None,
        }
    }

    fn platform(&mut self, name: &str) -> VirtualPlatform {
        self.lab.node(name).platform.clone()
    }

    async fn desk(&mut self, computers: Vec<(&'static str, Setup)>) {
        let names: Vec<&str> = computers.iter().map(|(n, _)| *n).collect();
        self.lab
            .form_desk("Home office", names[0], &names[1..], computers)
            .await;
    }

    /// Two ordinary computers, Alpha and Beta.
    async fn pair(&mut self) {
        self.desk(vec![
            ("Alpha", setup(Os::Linux)),
            ("Beta", setup(Os::Linux)),
        ])
        .await;
    }

    async fn set(&mut self, name: &str, key: &str, value: Value) {
        let _: Empty = self
            .lab
            .node(name)
            .call(
                Method::ConfigSet,
                json!({"key": key, "value": value, "origin": "app"}),
            )
            .await;
    }

    /// Wait until `name` holds `value` at `key`.
    async fn holds(&mut self, name: &str, key: &str, value: Value) {
        let node = self.lab.node(name);
        let end = std::time::Instant::now() + WAIT;
        loop {
            let held: Value = node.call(Method::ConfigGet, json!({"key": key})).await;
            if held == value {
                return;
            }
            assert!(
                std::time::Instant::now() < end,
                "{name}: timed out waiting for {key} = {value}"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// The QuickJump showings Alpha has seen.
    fn showings(&mut self) -> usize {
        self.lab
            .node("Alpha")
            .events
            .engine_events()
            .iter()
            .filter(|e| matches!(e, EngineEvent::QuickJumpShow { .. }))
            .count()
    }
}

#[given("Settings is shown")]
async fn settings_is_shown(w: &mut TailorWorld) {
    w.pair().await;
}

// ---- scenario 1: the panic key ----------------------------------------------------------------

#[when("the desk owner records a new key and number of presses for the panic key")]
async fn records_panic_key(w: &mut TailorWorld) {
    w.set("Alpha", "hotkeys.panic", json!("F5*3")).await;
}

#[then(
    "pressing that key that many times returns the pointer and keyboard to the computer the desk user is at, and the previous panic key no longer does"
)]
async fn new_panic_key_works(w: &mut TailorWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    push_right(&alpha);
    injected(&beta, "Beta entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    // The previous gesture is only two keys for Beta now.
    for _ in 0..2 {
        press(&alpha, &[], ESC);
    }
    until("both escapes on Beta", || count_of(&beta, ESC) == 2).await;
    assert!(alpha.is_grabbed(), "the previous panic key does nothing");
    for _ in 0..3 {
        press(&alpha, &[], F5);
    }
    until("Alpha to get control back", || !alpha.is_grabbed()).await;
    let own = w.lab.node("Alpha").id();
    w.lab
        .node("Alpha")
        .wait_status("Alpha active again", move |s| s.active_computer == own)
        .await;
}

// ---- scenario 2: QuickJump and targeted paste --------------------------------------------------

#[when("the desk owner records a new key combination for QuickJump by pressing it")]
async fn records_quickjump(w: &mut TailorWorld) {
    w.set("Alpha", "hotkeys.quickjump", json!("Ctrl+Alt+Shift+K"))
        .await;
    w.recorded = Some(Recorded::QuickJump);
}

#[when("the desk owner records a new key combination for targeted paste by pressing it")]
async fn records_targeted_paste(w: &mut TailorWorld) {
    // Alpha copies, then Beta does: each holds what the other copied last.
    w.platform("Alpha").set_local_clipboard(text("one"));
    until("Beta to hold it", || {
        w.platform("Beta").clipboard_contents() == text("one")
    })
    .await;
    w.platform("Beta").set_local_clipboard(text("two"));
    until("Alpha to hold it", || {
        w.platform("Alpha").clipboard_contents() == text("two")
    })
    .await;
    w.set("Beta", "hotkeys.targeted_paste", json!("Ctrl+Alt+P"))
        .await;
    w.recorded = Some(Recorded::TargetedPaste);
}

#[then("that combination performs the action on this computer and the previous one no longer does")]
async fn new_combination_works(w: &mut TailorWorld) {
    match w.recorded.expect("a combination was recorded") {
        Recorded::QuickJump => {
            let alpha = w.platform("Alpha");
            let before = w.showings();
            // The previous combination shows no letters.
            press(&alpha, &[CTRL, ALT, SHIFT], J);
            tokio::time::sleep(Duration::from_millis(500)).await;
            assert_eq!(w.showings(), before, "the previous combination");
            // The new one does.
            press(&alpha, &[CTRL, ALT, SHIFT], K);
            until("the letters", || w.showings() == before + 1).await;
        }
        Recorded::TargetedPaste => {
            let beta = w.platform("Beta");
            // The previous combination pastes nothing.
            press(&beta, &[CTRL, ALT, SHIFT], V);
            tokio::time::sleep(Duration::from_millis(500)).await;
            assert_eq!(beta.clipboard_contents(), text("two"));
            // The new one pastes what was copied on Alpha.
            press(&beta, &[CTRL, ALT], P);
            until("the paste", || beta.clipboard_contents() == text("one")).await;
        }
    }
}

// ---- scenario 3: friendly names ----------------------------------------------------------------

#[given("a desk computer")]
async fn a_desk_computer(w: &mut TailorWorld) {
    w.desk(vec![
        ("Alpha", setup(Os::Linux)),
        ("Beta", setup(Os::Linux)),
        ("Gamma", setup(Os::Linux)),
    ])
    .await;
}

#[when("the desk owner gives it a friendly name from any desk computer's window")]
async fn gives_a_friendly_name(w: &mut TailorWorld) {
    let beta = w.lab.node("Beta").id();
    // From Gamma's window, a third computer.
    let _: Empty = w
        .lab
        .node("Gamma")
        .call(
            Method::DeskRename,
            json!({"computer": beta, "name": format!("  {NEW_NAME} ")}),
        )
        .await;
}

#[then(
    "every desk computer shows that name wherever it names that computer, including the arranger, Health, notifications and the command-line tool's status"
)]
async fn every_computer_shows_the_name(w: &mut TailorWorld) {
    let beta = w.lab.node("Beta").id();
    // Health and the notices: a problem on Beta names it.
    w.platform("Beta").set_secure_input(Some(SecureInputHolder {
        pid: 42,
        app_name: "1Password".into(),
    }));
    for name in ["Alpha", "Beta", "Gamma"] {
        let node = w.lab.node(name);
        // Status, which the Desk section and the command-line tool show.
        node.wait_status("the new name", move |s| {
            s.computers
                .iter()
                .any(|c| c.id == beta && c.name == NEW_NAME)
        })
        .await;
        // The arranger.
        let layout: Value = node.call(Method::LayoutGet, json!({})).await;
        let names: Vec<&str> = layout["displays"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["display_id"]["computer"] == beta.to_string())
            .map(|d| d["computer_name"].as_str().unwrap())
            .collect();
        assert!(!names.is_empty());
        assert!(names.iter().all(|n| *n == NEW_NAME), "{name}: {names:?}");
        // Health, and the notification that told of it.
        let notice = node
            .notification("a notice about Beta", Topic::Problem, |p| {
                p["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("1Password"))
            })
            .await;
        assert!(
            notice["message"].as_str().unwrap().contains(NEW_NAME),
            "{name}: {notice}"
        );
        let problems = node
            .wait_status("the problem in Health", |s| !s.problems.is_empty())
            .await
            .problems;
        assert!(
            problems.iter().all(|p| p.message.contains(NEW_NAME)),
            "{name}: {problems:?}"
        );
    }
    // The computer itself also keeps it as its own name.
    w.holds("Beta", "general.computer_name", json!(NEW_NAME))
        .await;
}

// ---- scenario 4: the landing highlight ---------------------------------------------------------

#[given("the landing highlight is turned off")]
async fn highlight_turned_off(w: &mut TailorWorld) {
    w.pair().await;
    w.set("Alpha", "crossing.landing_highlight", json!(false))
        .await;
    // It is a desk-shared setting: Beta, where the pointer will land, has it.
    w.holds("Beta", "crossing.landing_highlight", json!(false))
        .await;
}

#[when("the pointer crosses to another computer")]
async fn pointer_crosses(w: &mut TailorWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    push_right(&alpha);
    injected(&beta, "Beta entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

#[then("no highlight is shown where it lands")]
async fn no_highlight(w: &mut TailorWorld) {
    // Long enough for an announcement to have been made.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let landings = w
        .lab
        .node("Beta")
        .events
        .engine_events()
        .into_iter()
        .filter(|e| matches!(e, EngineEvent::Landing { .. }))
        .count();
    assert_eq!(landings, 0);
}

// ---- scenarios 5 and 6: Command as Control -----------------------------------------------------

async fn mac_and_windows(w: &mut TailorWorld) {
    w.desk(vec![("Mac", setup(Os::MacOs)), ("Win", setup(Os::Windows))])
        .await;
}

#[given(regex = r#"^"Use Command as Control on Windows and Linux" is on$"#)]
async fn command_as_control_on(w: &mut TailorWorld) {
    mac_and_windows(w).await;
    w.set("Mac", "keyboard.command_as_control", json!(true))
        .await;
}

#[given(regex = r#"^"Use Command as Control on Windows and Linux" is off$"#)]
async fn command_as_control_off(w: &mut TailorWorld) {
    mac_and_windows(w).await;
    w.set("Mac", "keyboard.command_as_control", json!(false))
        .await;
    w.holds("Win", "keyboard.command_as_control", json!(false))
        .await;
}

/// The Mac's keyboard drives Windows; `modifier` and C are typed.
async fn type_on_windows(w: &mut TailorWorld, modifiers: &[u32]) {
    let (mac, win) = (w.platform("Mac"), w.platform("Win"));
    push_right(&mac);
    injected(&win, "Windows entered", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    win.take_injected();
    for modifier in modifiers {
        for (usage, down) in [(*modifier, true), (C, true), (C, false), (*modifier, false)] {
            mac.send_input(key(usage, down));
        }
    }
    until("the keys", || keys_of(&win).len() == 4 * modifiers.len()).await;
}

#[when("the desk user at a Mac presses Command with another key on a Windows or Linux computer")]
async fn presses_command(w: &mut TailorWorld) {
    type_on_windows(w, &[META]).await;
}

#[then("it acts as Control with that key")]
async fn acts_as_control(w: &mut TailorWorld) {
    let win = w.platform("Win");
    assert_eq!(
        keys_of(&win),
        [(CTRL, true), (C, true), (C, false), (CTRL, false)]
    );
}

#[when(
    "the desk user at a Mac presses Command or Control with another key on a Windows or Linux computer"
)]
async fn presses_command_or_control(w: &mut TailorWorld) {
    type_on_windows(w, &[META, CTRL]).await;
}

#[then("Command acts as the Windows key and Control acts as Control")]
async fn command_is_the_windows_key(w: &mut TailorWorld) {
    let win = w.platform("Win");
    assert_eq!(
        keys_of(&win),
        [
            (META, true),
            (C, true),
            (C, false),
            (META, false),
            (CTRL, true),
            (C, true),
            (C, false),
            (CTRL, false),
        ]
    );
}

// ---- edge case: renamed while another computer is offline --------------------------------------

#[given("a computer is renamed while another desk computer is offline")]
async fn renamed_while_offline(w: &mut TailorWorld) {
    a_desk_computer(w).await;
    w.lab.node("Gamma").stop().await;
    let beta = w.lab.node("Beta").id();
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(
            Method::DeskRename,
            json!({"computer": beta, "name": NEW_NAME}),
        )
        .await;
}

#[when("that desk computer reconnects")]
async fn reconnects(w: &mut TailorWorld) {
    w.lab.restart("Gamma").await;
}

#[then("that computer shows the new name once it reconnects")]
async fn shows_the_new_name(w: &mut TailorWorld) {
    let beta = w.lab.node("Beta").id();
    w.lab
        .node("Gamma")
        .wait_status("the new name after reconnecting", move |s| {
            s.computers
                .iter()
                .any(|c| c.id == beta && c.name == NEW_NAME)
        })
        .await;
}
