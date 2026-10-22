use std::time::Duration;

use mouser_platform::LocalInput;
use mouser_proto::control::{Empty, Method};
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, DisplayId};
use mouser_testkit::Injected;
use serde_json::json;
use tokio::sync::broadcast::Receiver;

use super::assign;
use crate::EngineEvent;
use crate::layout::edges::{Placed, Rect};
use crate::testing::*;

const CTRL: u32 = 0x0007_00E0;
const SHIFT: u32 = 0x0007_00E1;
const ALT: u32 = 0x0007_00E2;
const ESCAPE: u32 = 0x0007_0029;

/// The HID usage of the key lettered `letter`.
fn usage(letter: char) -> u32 {
    crate::crossing::panic::usage_for_name(&letter.to_string()).unwrap()
}

fn tap(node: &TestNode, usage: u32) {
    for down in [true, false] {
        node.platform.send_input(LocalInput::Key { usage, down });
    }
}

/// Ctrl+Alt+Shift+`key`, the way a hand does it.
fn chord(node: &TestNode, key: char) {
    let modifiers = [CTRL, ALT, SHIFT];
    for usage in modifiers {
        node.platform
            .send_input(LocalInput::Key { usage, down: true });
    }
    tap(node, self::usage(key));
    for usage in modifiers.into_iter().rev() {
        node.platform
            .send_input(LocalInput::Key { usage, down: false });
    }
}

fn hotkey(node: &TestNode) {
    chord(node, 'J');
}

type Letters = Vec<(DisplayId, char)>;

/// The next event satisfying `pick`, within 10 s.
async fn next<T>(events: &mut Receiver<EngineEvent>, pick: impl Fn(EngineEvent) -> Option<T>) -> T {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(found) = pick(events.recv().await.unwrap()) {
                return found;
            }
        }
    })
    .await
    .expect("the event arrives in time")
}

async fn shown(events: &mut Receiver<EngineEvent>) -> Letters {
    next(events, |e| match e {
        EngineEvent::QuickJumpShow { letters } => Some(letters),
        _ => None,
    })
    .await
}

async fn hidden(events: &mut Receiver<EngineEvent>) {
    next(events, |e| (e == EngineEvent::QuickJumpHide).then_some(())).await;
}

/// The letter shown on the display `key` of `node`.
fn letter_of(letters: &Letters, node: &TestNode, key: &str) -> char {
    letters
        .iter()
        .find(|(d, _)| d.computer == node.engine.computer_id() && d.key == key)
        .unwrap_or_else(|| panic!("a letter for {key}"))
        .1
}

/// The letter of Alpha's only display.
fn alpha_letter(letters: &Letters, alpha: &TestNode) -> char {
    letters
        .iter()
        .find(|(d, _)| d.computer == alpha.engine.computer_id())
        .unwrap()
        .1
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1.0
}

async fn a_moment() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

fn warps(node: &TestNode) -> Vec<(f64, f64)> {
    node.platform.warps()
}

async fn active(node: &TestNode) -> Option<ComputerId> {
    status(node).await.active_computer
}

fn entered_at(node: &TestNode, x: f64, y: f64) -> impl std::future::Future<Output = ()> + '_ {
    injected_where(
        &node.platform,
        "the pointer at the centre of the display",
        move |i| matches!(i, Injected::MotionAbs { x: ix, y: iy } if near(*ix, x) && near(*iy, y)),
    )
}

#[tokio::test]
async fn the_hotkey_shows_a_distinct_letter_on_every_display_of_the_desk() {
    let lab = Lab::new();
    let nodes = docked_desk(&lab).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let (mut mine, mut theirs) = (a.engine.events(), b.engine.events());
    hotkey(a);
    let letters = shown(&mut mine).await;
    // Every computer is told, and told the same.
    assert_eq!(shown(&mut theirs).await, letters);
    assert_eq!(letters.len(), 3);
    let mut chars: Vec<char> = letters.iter().map(|(_, c)| *c).collect();
    chars.sort();
    chars.dedup();
    assert_eq!(chars.len(), 3, "the letters are distinct");
    let mut displays: Vec<_> = letters.iter().map(|(d, _)| d.key.clone()).collect();
    displays.sort();
    assert_eq!(displays, ["b1", "b2", "virtual-0"]);
    // Nothing moved: the hotkey is not typed anywhere.
    assert!(!a.platform.is_grabbed());
    assert!(b.platform.injected().is_empty());
}

#[tokio::test]
async fn a_letter_jumps_to_the_centre_of_its_display_on_another_computer() {
    let lab = Lab::new();
    let nodes = docked_desk(&lab).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let (mut mine, mut theirs) = (a.engine.events(), b.engine.events());
    hotkey(a);
    let letters = shown(&mut mine).await;
    shown(&mut theirs).await;
    tap(a, usage(letter_of(&letters, b, "b2")));
    hidden(&mut mine).await;
    hidden(&mut theirs).await;
    // Beta's second display is 1920 px to the right of its first.
    entered_at(b, 1920.0 + 960.0, 540.0).await;
    assert!(
        a.platform.is_grabbed(),
        "Alpha's own input now goes to Beta"
    );
    eventually("Beta to be the active computer", || async {
        active(a).await == Some(b.engine.computer_id())
    })
    .await;
    // The letter was not typed on Beta.
    assert!(
        !b.platform
            .injected()
            .iter()
            .any(|i| matches!(i, Injected::Key { usage: u, .. } if *u == usage('A') || *u == usage('B') || *u == usage('C')))
    );
}

#[tokio::test]
async fn the_hotkey_and_a_letter_work_while_the_pointer_is_on_another_computer() {
    let lab = Lab::new();
    let nodes = docked_desk(&lab).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let (mut mine, mut theirs) = (a.engine.events(), b.engine.events());
    hotkey(a);
    let letters = shown(&mut mine).await;
    shown(&mut theirs).await;
    tap(a, usage(letter_of(&letters, b, "b1")));
    entered_at(b, 960.0, 540.0).await;
    hidden(&mut theirs).await;
    b.platform.take_injected();
    // The hotkey is Alpha's: Beta is not typed at.
    hotkey(a);
    let letters = shown(&mut mine).await;
    assert!(
        !b.platform
            .injected()
            .iter()
            .any(|i| matches!(i, Injected::Key { usage: u, down: true } if *u == usage('J')))
    );
    // Beta's other display: the pointer moves there and stays Beta's.
    tap(a, usage(letter_of(&letters, b, "b2")));
    entered_at(b, 1920.0 + 960.0, 540.0).await;
    assert_eq!(active(a).await, Some(b.engine.computer_id()));
    // And back to Alpha's own display.
    hotkey(a);
    let letters = shown(&mut mine).await;
    tap(a, usage(alpha_letter(&letters, a)));
    eventually("the pointer to be back on Alpha", || async {
        !a.platform.is_grabbed()
    })
    .await;
    let landed = *warps(a).last().expect("the pointer was placed");
    assert!(near(landed.0, 960.0) && near(landed.1, 540.0), "{landed:?}");
}

#[tokio::test]
async fn a_letter_on_this_computer_moves_the_pointer_there_without_a_crossing() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let mut mine = a.engine.events();
    hotkey(a);
    let letters = shown(&mut mine).await;
    tap(a, usage(alpha_letter(&letters, a)));
    hidden(&mut mine).await;
    eventually("the pointer to be placed", || async {
        !warps(a).is_empty()
    })
    .await;
    let landed = warps(a)[0];
    assert!(near(landed.0, 960.0) && near(landed.1, 540.0), "{landed:?}");
    assert!(!a.platform.is_grabbed());
    a_moment().await;
    assert!(b.platform.injected().is_empty());
}

#[tokio::test]
async fn a_letter_can_jump_from_one_remote_computer_to_another() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
    let (a, b, c) = (&nodes[0], &nodes[1], &nodes[2]);
    let mut mine = a.engine.events();
    hotkey(a);
    let letters = shown(&mut mine).await;
    tap(a, usage(letter_of(&letters, b, "virtual-0")));
    entered_at(b, 960.0, 540.0).await;
    hidden(&mut mine).await;
    hotkey(a);
    let letters = shown(&mut mine).await;
    tap(a, usage(letter_of(&letters, c, "virtual-0")));
    entered_at(c, 960.0, 540.0).await;
    eventually("Gamma to be the active computer", || async {
        active(a).await == Some(c.engine.computer_id())
    })
    .await;
    assert!(a.platform.is_grabbed());
}

#[tokio::test]
async fn escape_or_another_key_hides_the_letters_and_leaves_the_pointer() {
    let lab = Lab::new();
    let nodes = docked_desk(&lab).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let (mut mine, mut theirs) = (a.engine.events(), b.engine.events());
    // 'Z' is not a letter of a three-display desk.
    for cancel in [ESCAPE, usage('Z')] {
        hotkey(a);
        let letters = shown(&mut mine).await;
        shown(&mut theirs).await;
        tap(a, cancel);
        hidden(&mut mine).await;
        hidden(&mut theirs).await;
        // The letters are gone: a letter key does nothing now.
        tap(a, usage(letter_of(&letters, b, "b1")));
    }
    a_moment().await;
    assert!(!a.platform.is_grabbed());
    assert!(warps(a).is_empty());
    assert!(
        !b.platform
            .injected()
            .iter()
            .any(|i| matches!(i, Injected::MotionAbs { .. }))
    );
}

#[tokio::test]
async fn nothing_starts_while_paused() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    let mut mine = a.engine.events();
    let _: Empty = call(a, Method::ActivePause, json!({})).await;
    hotkey(a);
    a_moment().await;
    assert!(mine.try_recv().is_err(), "no letters while paused");
}

#[tokio::test]
async fn the_hotkey_is_the_configured_one() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    let mut mine = a.engine.events();
    let refused = call_err(
        a,
        Method::ConfigSet,
        json!({"key": "hotkeys.quickjump", "value": "Nope"}),
    )
    .await;
    assert!(refused.message.contains("hotkeys.quickjump"));
    let _: Empty = call(
        a,
        Method::ConfigSet,
        json!({"key": "hotkeys.quickjump", "value": "Ctrl+Alt+Shift+Q"}),
    )
    .await;
    hotkey(a);
    a_moment().await;
    assert!(mine.try_recv().is_err(), "the old hotkey does nothing");
    chord(a, 'Q');
    assert_eq!(shown(&mut mine).await.len(), 2);
}

#[tokio::test]
async fn a_letter_whose_display_has_left_the_layout_cancels_the_jump() {
    let lab = Lab::new();
    let nodes = docked_desk(&lab).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let (mut mine, mut theirs) = (a.engine.events(), b.engine.events());
    hotkey(a);
    let letters = shown(&mut mine).await;
    shown(&mut theirs).await;
    // Beta's second display is unplugged while the letters show.
    b.platform.set_displays(vec![screen("b1", (0.0, 0.0))]);
    eventually("Alpha to forget the display", || async {
        mapped(a).len() == 2
    })
    .await;
    tap(a, usage(letter_of(&letters, b, "b2")));
    hidden(&mut mine).await;
    hidden(&mut theirs).await;
    a_moment().await;
    assert!(!a.platform.is_grabbed());
    assert!(warps(a).is_empty());
    assert!(
        !b.platform
            .injected()
            .iter()
            .any(|i| matches!(i, Injected::MotionAbs { .. }))
    );
    // QuickJump still works.
    hotkey(a);
    assert_eq!(shown(&mut mine).await.len(), 2);
}

#[tokio::test]
async fn desk_mates_hide_the_letters_when_the_computer_that_showed_them_goes_offline() {
    let lab = Lab::new();
    let mut nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
    let (mut beta, mut gamma) = (nodes[1].engine.events(), nodes[2].engine.events());
    hotkey(&nodes[0]);
    shown(&mut beta).await;
    shown(&mut gamma).await;
    nodes.remove(0).engine.shutdown().await;
    hidden(&mut beta).await;
    hidden(&mut gamma).await;
}

#[tokio::test]
async fn letters_already_put_away_are_not_hidden_again_when_their_computer_goes_offline() {
    let lab = Lab::new();
    let mut nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let mut beta = nodes[1].engine.events();
    hotkey(&nodes[0]);
    shown(&mut beta).await;
    tap(&nodes[0], ESCAPE);
    hidden(&mut beta).await;
    nodes.remove(0).engine.shutdown().await;
    eventually("Beta to see Alpha gone", || async {
        connected(&nodes[0]).await == 0
    })
    .await;
    a_moment().await;
    assert!(beta.try_recv().is_err(), "nothing more to hide");
}

#[tokio::test]
async fn a_computer_that_cannot_grab_input_does_not_start_a_jump_to_another_computer() {
    let lab = Lab::new();
    let nodes = vec![
        lab.start_ungrabbable("Alpha").await,
        lab.start("Beta").await,
    ];
    let nodes = desk_from(nodes).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let (mut mine, mut theirs) = (a.engine.events(), b.engine.events());
    hotkey(a);
    let letters = shown(&mut mine).await;
    shown(&mut theirs).await;
    tap(a, usage(letter_of(&letters, b, "virtual-0")));
    hidden(&mut mine).await;
    hidden(&mut theirs).await;
    a_moment().await;
    assert!(
        !b.platform
            .injected()
            .iter()
            .any(|i| matches!(i, Injected::MotionAbs { .. })),
        "Beta was never entered"
    );
    assert_eq!(active(a).await, None);
    // The jump to Alpha's own display still works.
    hotkey(a);
    let letters = shown(&mut mine).await;
    tap(a, usage(alpha_letter(&letters, a)));
    eventually("the pointer to be placed", || async {
        !warps(a).is_empty()
    })
    .await;
}

#[tokio::test]
async fn a_computer_that_cannot_grab_input_still_crosses_at_the_edge_and_comes_back() {
    let lab = Lab::new();
    let nodes = vec![
        lab.start_ungrabbable("Alpha").await,
        lab.start("Beta").await,
    ];
    let nodes = desk_from(nodes).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    entered_at(b, 0.0, 540.0).await;
    eventually("Beta to be the active computer", || async {
        active(a).await == Some(b.engine.computer_id())
    })
    .await;
    // The pointer returns through the shared mouse on Alpha.
    push_back(&nodes, 0);
    eventually("control to come home", || async {
        active(a).await != Some(b.engine.computer_id())
    })
    .await;
}

fn placed(n: i32) -> Vec<Placed> {
    (0..n)
        .rev()
        .map(|i| Placed {
            display: Display {
                display_id: DisplayId {
                    computer: ComputerId([1; 16]),
                    key: format!("d{i}"),
                },
                name: format!("d{i}"),
                pixel_size: (100, 100),
                logical_size: (100.0, 100.0),
                scale: 1.0,
                physical_size_mm: None,
                origin_local: (0, 0),
                included: true,
            },
            rect: Rect {
                x: i * 100,
                y: 0,
                w: 100,
                h: 100,
            },
        })
        .collect()
}

#[test]
fn letters_go_left_to_right_across_the_desk() {
    let letters = assign(&placed(36));
    assert_eq!(letters.len(), 36);
    assert_eq!(letters[0].0.key, "d0");
    assert_eq!(letters[0].1, 'A');
    assert_eq!(letters[1].1, 'B');
    assert_eq!(letters[35].1, '9');
    assert_eq!(letters[35].0.key, "d35");
}

#[test]
#[should_panic(expected = "QuickJump has 36 letters and the desk has 37 displays")]
fn a_desk_with_more_displays_than_letters_is_not_supported() {
    assign(&placed(37));
}
