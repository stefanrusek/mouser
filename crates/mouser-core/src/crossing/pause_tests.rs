use mouser_platform::{ClipRep, LocalInput};
use mouser_proto::control::{Empty, EngineState, Method};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{ControlMessage, TransferId, TransferItem};
use mouser_testkit::Injected;
use serde_json::json;

use crate::EngineEvent;
use crate::testing::*;
use crate::transfer::drag::DRAG_DESTINATION;

/// A desk of computers running the version that introduced `Paused`.
async fn current_desk(lab: &Lab, names: &[&str]) -> Vec<TestNode> {
    let mut nodes = Vec::new();
    for name in names {
        nodes.push(
            lab.start_released(name, |_| {}, |p| p.mouser_version = "0.2.2".into())
                .await,
        );
    }
    desk_from(nodes).await
}

async fn pause(node: &TestNode) {
    let _: Empty = call(node, Method::ActivePause, json!({})).await;
}

async fn resume(node: &TestNode) {
    let _: Empty = call(node, Method::ActiveResume, json!({})).await;
}

/// Wait until `node`'s input path sees `other` paused (or not).
async fn sees_paused(node: &TestNode, other: &TestNode, paused: bool) {
    let id = other.engine.computer_id();
    eventually("the pause to be known", || async {
        let crossing = node.engine.ctx.crossing.lock().unwrap();
        crossing.map.paused.contains(&id) == paused
            && crossing.map.placed.len() == if paused { 1 } else { 2 }
    })
    .await;
}

fn moved(node: &TestNode) -> bool {
    node.platform
        .injected()
        .iter()
        .any(|i| matches!(i, Injected::MotionAbs { .. }))
}

fn text(node: &TestNode) -> Option<String> {
    node.platform
        .clipboard_contents()
        .into_iter()
        .find_map(|r| match r {
            ClipRep::Text(t) => Some(t),
            _ => None,
        })
}

fn copy(node: &TestNode, content: &str) {
    node.platform
        .set_local_clipboard(vec![ClipRep::Text(content.into())]);
}

#[tokio::test]
async fn pausing_stops_crossing_and_resuming_restores_it() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let _: Empty = call(a, Method::ActivePause, json!({})).await;
    assert_eq!(status(a).await.state, EngineState::Paused);
    push_right(a);
    settle().await;
    assert!(!a.platform.is_grabbed());
    let _: Empty = call(a, Method::ActiveResume, json!({})).await;
    assert_eq!(status(a).await.state, EngineState::Active);
    push_right(a);
    injected_where(&b.platform, "the crossing", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

#[tokio::test]
async fn pausing_while_the_pointer_is_elsewhere_brings_it_home() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    eventually("the grab", || async { a.platform.is_grabbed() }).await;
    a.platform.send_input(LocalInput::Key {
        usage: 4,
        down: true,
    });
    injected_where(&b.platform, "the key", |i| {
        matches!(i, Injected::Key { down: true, .. })
    })
    .await;
    let _: Empty = call(a, Method::ActivePause, json!({})).await;
    assert!(!a.platform.is_grabbed());
    injected_where(&b.platform, "the release", |i| {
        *i == Injected::Key {
            usage: 4,
            down: false,
        }
    })
    .await;
}

#[tokio::test]
async fn a_paused_computer_refuses_a_crossing_from_a_peer_that_cannot_be_told() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    pause(a).await;
    settle().await;
    // The older peer is never sent `Paused`: it still sees Alpha's display.
    assert!(b.engine.ctx.crossing.lock().unwrap().map.paused.is_empty());
    push_left(b);
    eventually("Beta's grab", || async { b.platform.is_grabbed() }).await;
    settle().await;
    assert!(!moved(a), "Alpha takes nothing from Beta");
}

#[tokio::test]
async fn nobody_can_cross_onto_a_paused_computer_and_resuming_restores_both_ways() {
    let lab = Lab::new();
    let nodes = current_desk(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    pause(a).await;
    sees_paused(b, a, true).await;
    push_left(b);
    settle().await;
    assert!(!b.platform.is_grabbed(), "Alpha's display is unavailable");
    push_right(a);
    settle().await;
    assert!(!a.platform.is_grabbed(), "Alpha's pointer stays");
    assert!(!moved(a) && !moved(b));

    resume(a).await;
    sees_paused(b, a, false).await;
    push_left(b);
    injected_where(&a.platform, "Beta's crossing onto Alpha", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

#[tokio::test]
async fn after_a_resume_the_paused_computer_crosses_out_again() {
    let lab = Lab::new();
    let nodes = current_desk(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    pause(a).await;
    resume(a).await;
    push_right(a);
    injected_where(&b.platform, "Alpha's crossing onto Beta", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

#[tokio::test]
async fn pausing_while_controlled_returns_control_and_releases_what_was_held() {
    let lab = Lab::new();
    let nodes = current_desk(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_left(b);
    eventually("Beta's grab", || async { b.platform.is_grabbed() }).await;
    b.platform.send_input(LocalInput::Key {
        usage: 4,
        down: true,
    });
    injected_where(&a.platform, "the key", |i| {
        matches!(i, Injected::Key { down: true, .. })
    })
    .await;
    pause(a).await;
    injected_where(&a.platform, "the release", |i| {
        *i == Injected::Key {
            usage: 4,
            down: false,
        }
    })
    .await;
    eventually("control back on Beta", || async {
        !b.platform.is_grabbed()
    })
    .await;
    // Whatever Beta types now is not injected into Alpha.
    a.platform.take_injected();
    b.platform.send_input(LocalInput::Key {
        usage: 5,
        down: true,
    });
    settle().await;
    assert!(a.platform.injected().is_empty());
}

#[tokio::test]
async fn a_peer_that_pauses_while_controlling_us_stops_being_received() {
    let lab = Lab::new();
    let nodes = current_desk(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    eventually("Alpha's grab", || async { a.platform.is_grabbed() }).await;
    pause(a).await;
    eventually("Beta not receiving", || async {
        b.engine.ctx.crossing.lock().unwrap().receiving.is_none()
    })
    .await;
    assert_eq!(status(a).await.state, EngineState::Paused);
}

#[tokio::test]
async fn a_computer_that_paused_before_joining_is_known_paused_on_connect() {
    let lab = Lab::new();
    let a = lab
        .start_released("Alpha", |_| {}, |p| p.mouser_version = "0.2.2".into())
        .await;
    let b = lab
        .start_released("Beta", |_| {}, |p| p.mouser_version = "0.2.2".into())
        .await;
    pause(&a).await;
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    admit(&a, &b).await;
    let id = a.engine.computer_id();
    eventually("Beta to know", || async {
        b.engine
            .ctx
            .crossing
            .lock()
            .unwrap()
            .map
            .paused
            .contains(&id)
    })
    .await;
}

#[tokio::test]
async fn paused_computers_hear_nothing_unauthorised() {
    let lab = Lab::new();
    let nodes = current_desk(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    let stranger = ComputerId([9; 16]);
    assert!(a.engine.ctx.on_peer_paused(stranger, true).is_err());
}

#[tokio::test]
async fn quickjump_letters_are_hidden_by_a_pause_and_not_shown_while_paused() {
    let lab = Lab::new();
    let nodes = current_desk(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let mut events = a.engine.events();
    send_to(b, a, ControlMessage::QuickJumpShow { letters: vec![] });
    loop {
        if matches!(
            events.recv().await.unwrap(),
            EngineEvent::QuickJumpShow { .. }
        ) {
            break;
        }
    }
    pause(a).await;
    loop {
        if matches!(events.recv().await.unwrap(), EngineEvent::QuickJumpHide) {
            break;
        }
    }
    send_to(b, a, ControlMessage::QuickJumpShow { letters: vec![] });
    settle().await;
    assert!(
        events.try_recv().is_err(),
        "no letters on a paused computer"
    );
}

#[tokio::test]
async fn letters_waiting_for_an_answer_are_withdrawn_by_a_pause() {
    let lab = Lab::new();
    let nodes = current_desk(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    let mut events = a.engine.events();
    for usage in [0x0007_00E0, 0x0007_00E2, 0x0007_00E1, 0x0007_000D] {
        a.platform.send_input(LocalInput::Key { usage, down: true });
    }
    loop {
        if matches!(
            events.recv().await.unwrap(),
            EngineEvent::QuickJumpShow { .. }
        ) {
            break;
        }
    }
    pause(a).await;
    loop {
        if matches!(events.recv().await.unwrap(), EngineEvent::QuickJumpHide) {
            break;
        }
    }
}

#[tokio::test]
async fn a_paused_computer_shares_no_clipboard_and_offers_what_it_copied_on_resume() {
    let lab = Lab::new();
    let nodes = current_desk(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    pause(a).await;
    sees_paused(b, a, true).await;
    copy(a, "from Alpha");
    copy(b, "from Beta");
    settle().await;
    assert_eq!(text(b).as_deref(), Some("from Beta"), "nothing arrives");
    assert_eq!(text(a).as_deref(), Some("from Alpha"), "nothing leaves");
    resume(a).await;
    eventually("Alpha's copy to reach Beta", || async {
        text(b).as_deref() == Some("from Alpha")
    })
    .await;
}

#[tokio::test]
async fn a_peer_resuming_offers_what_it_missed() {
    let lab = Lab::new();
    let nodes = current_desk(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    pause(a).await;
    sees_paused(b, a, true).await;
    copy(b, "from Beta");
    settle().await;
    assert_ne!(text(a).as_deref(), Some("from Beta"));
    resume(a).await;
    eventually("Beta's copy to reach Alpha", || async {
        text(a).as_deref() == Some("from Beta")
    })
    .await;
}

#[tokio::test]
async fn what_came_from_elsewhere_is_not_offered_again_on_resume() {
    let lab = Lab::new();
    let nodes = current_desk(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    copy(a, "old");
    eventually("old on Beta", || async {
        text(b).as_deref() == Some("old")
    })
    .await;
    copy(b, "newer");
    eventually("newer on Alpha", || async {
        text(a).as_deref() == Some("newer")
    })
    .await;
    pause(a).await;
    resume(a).await;
    settle().await;
    assert_eq!(text(b).as_deref(), Some("newer"));
}

#[tokio::test]
async fn clipboard_traffic_and_hotkeys_do_nothing_on_a_paused_computer() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    pause(a).await;
    // An older peer still offers: a paused computer takes nothing.
    copy(b, "from Beta");
    settle().await;
    assert_ne!(text(a).as_deref(), Some("from Beta"));
    // The paste shortcut is not clipboard business now.
    for usage in [0x0007_00E0, 0x0007_0019] {
        a.platform.send_input(LocalInput::Key { usage, down: true });
    }
    settle().await;
}

#[tokio::test]
async fn dragged_files_are_not_taken_by_a_paused_computer() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    pause(a).await;
    send_to(
        b,
        a,
        ControlMessage::TransferOffer {
            transfer_id: TransferId([3; 16]),
            items: vec![TransferItem {
                relative_path: "notes.txt".into(),
                size: 3,
                is_dir: false,
            }],
            destination_dir: DRAG_DESTINATION.into(),
        },
    );
    settle().await;
    assert!(a.platform.incoming_drags().is_empty());
}
