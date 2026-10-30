//! Story 3 scenario 5 and the edge case "no key or button stays stuck down
//! on any computer": the computer that has the pointer is lost.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use mouser_platform::LocalInput;
use mouser_proto::control::ConnectionState;
use mouser_testkit::Injected;

use crate::testing::*;

const A_KEY: u32 = 0x0007_0004;
const C_KEY: u32 = 0x0007_0006;

/// What is still held down after `injected`.
fn held(injected: &[Injected]) -> (BTreeSet<u32>, BTreeSet<u8>) {
    let (mut keys, mut buttons) = (BTreeSet::new(), BTreeSet::new());
    for i in injected {
        match i {
            Injected::Key { usage, down: true } => keys.insert(*usage),
            Injected::Key { usage, down: false } => keys.remove(usage),
            Injected::Button { button, down: true } => buttons.insert(*button),
            Injected::Button {
                button,
                down: false,
            } => buttons.remove(button),
            _ => false,
        };
    }
    (keys, buttons)
}

#[tokio::test]
async fn losing_the_active_computer_returns_the_mouse_and_keyboard_and_releases_everything() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    // Alpha's mouse and keyboard now drive Beta, with a key and a button held.
    push_right(a);
    injected_where(&b.platform, "the pointer on Beta", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    assert!(a.platform.is_grabbed());
    a.platform.send_input(LocalInput::Key {
        usage: A_KEY,
        down: true,
    });
    a.platform.send_input(LocalInput::Button {
        button: 1,
        down: true,
    });
    injected_where(&b.platform, "the held button", |i| {
        *i == Injected::Button {
            button: 1,
            down: true,
        }
    })
    .await;
    assert_eq!(held(&b.platform.injected()).0, BTreeSet::from([A_KEY]));
    assert!(
        !a.platform
            .local_delivered()
            .iter()
            .any(|i| matches!(i, LocalInput::Key { .. } | LocalInput::Button { .. })),
        "Alpha saw none of the typing"
    );

    // Beta becomes unreachable while it has the pointer.
    let lost = Instant::now();
    b.sim.as_ref().unwrap().set_blackhole(true);
    wait_connection(a, b, ConnectionState::Reconnecting).await;
    assert!(
        lost.elapsed() < Duration::from_secs(4),
        "{:?}",
        lost.elapsed()
    );

    // The pointer and keyboard are Alpha's again.
    assert!(!a.platform.is_grabbed());
    assert_eq!(a.platform.warps(), [(1919.0, 540.0)], "where it left");
    assert_eq!(status(a).await.active_computer, a.engine.computer_id());
    a.platform.send_input(LocalInput::Key {
        usage: C_KEY,
        down: true,
    });
    a.platform.send_input(LocalInput::Key {
        usage: C_KEY,
        down: false,
    });
    let delivered = a.platform.local_delivered();
    assert_eq!(
        delivered[delivered.len() - 2..],
        [
            LocalInput::Key {
                usage: C_KEY,
                down: true
            },
            LocalInput::Key {
                usage: C_KEY,
                down: false
            }
        ],
        "keys typed afterwards go to Alpha"
    );
    // Nothing was injected on Alpha, so nothing of Beta's session is stuck there.
    assert!(a.platform.injected().is_empty());

    // Beta notices the loss too and lets go of what Alpha left pressed.
    eventually("Beta to release the key and button", || async {
        held(&b.platform.injected()) == (BTreeSet::new(), BTreeSet::new())
    })
    .await;
    // Nor does Beta accept input from a controller it has lost.
    assert!(b.engine.ctx.crossing.lock().unwrap().receiving.is_none());
}

#[tokio::test]
async fn a_lost_controller_leaves_nothing_pressed_on_the_receiver() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "the pointer on Beta", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    a.platform.send_input(LocalInput::Key {
        usage: A_KEY,
        down: true,
    });
    injected_where(&b.platform, "the key", |i| {
        *i == Injected::Key {
            usage: A_KEY,
            down: true,
        }
    })
    .await;
    // The controller is the one that goes away.
    a.sim.as_ref().unwrap().set_blackhole(true);
    wait_connection(b, a, ConnectionState::Reconnecting).await;
    assert_eq!(
        held(&b.platform.injected()),
        (BTreeSet::new(), BTreeSet::new())
    );
}

#[tokio::test]
async fn the_controlled_display_leaving_the_layout_releases_what_the_still_connected_computer_holds()
 {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "the pointer on Beta", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    // Command is held on Beta.
    const LEFT_CMD: u32 = 0x0007_00E3;
    a.platform.send_input(LocalInput::Key {
        usage: LEFT_CMD,
        down: true,
    });
    injected_where(&b.platform, "Command held", |i| {
        *i == Injected::Key {
            usage: LEFT_CMD,
            down: true,
        }
    })
    .await;
    // Beta's display drops out of the layout (replaced by another) while Beta stays connected.
    b.platform
        .set_displays(vec![screen("replacement", (0.0, 0.0))]);
    eventually("Alpha to take control back", || async {
        !a.platform.is_grabbed()
    })
    .await;
    assert_eq!(connected(a).await, 1, "Beta is still connected");
    eventually("Beta to release Command", || async {
        held(&b.platform.injected()).0.is_empty()
    })
    .await;
}

/// A desk with one hand-driven member (`seed`) in session, and a second
/// connection from that same computer, greeted and asked for its proof: the
/// session's connection, the new one and the computer's id.
async fn resuming(
    node: &TestNode,
    seed: u8,
) -> (
    crate::transport::Link,
    crate::transport::Link,
    mouser_proto::ids::ComputerId,
    [u8; 32],
) {
    let desk: mouser_proto::control::Desk = call(
        node,
        mouser_proto::control::Method::DeskCreate,
        serde_json::json!({"name": "Home office"}),
    )
    .await;
    let session = member_rogue(node, &desk, seed).await;
    let rogue = Rogue::new(seed);
    let (mut new, _) = rogue
        .dial_as(node, Some(desk.desk_id), mouser_proto::wire::V2_SINCE)
        .await;
    assert!(matches!(
        new.reader
            .recv::<mouser_proto::wire::ControlMessage>()
            .await,
        Some(mouser_proto::wire::ControlMessage::DeskAuth { .. })
    ));
    let key: [u8; 32] = mouser_platform::Platform::secrets(&node.platform)
        .get(&crate::state::desk_key_name(&desk.desk_id))
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();
    let id = rogue.identity.computer_id();
    let proof = crate::session::auth::proof(
        &key,
        &crate::session::auth::export(&new.conn),
        &desk.desk_id,
        &id,
    );
    (session, new, id, proof)
}

/// Why `conn` was closed by the node.
async fn closed_with(conn: &quinn::Connection) -> Vec<u8> {
    match conn.closed().await {
        quinn::ConnectionError::ApplicationClosed(close) => close.reason.to_vec(),
        other => panic!("closed by the node with a reason, not {other:?}"),
    }
}

/// T536: the computer that takes a new connection first keeps the session's
/// one open until the new one proves membership. Closed earlier, the close
/// could reach the other computer before it has the new connection, and that
/// computer would end the session (seen on the Linux and Windows CI runners).
#[tokio::test]
async fn the_session_connection_stays_open_until_the_new_one_proves_membership() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let (session, mut new, id, proof) = resuming(&node, 0x72).await;
    {
        let core = node.engine.ctx.core.lock().unwrap();
        let previous = core.peers[&id].previous.as_ref().expect("a resumption");
        assert!(previous.conn.close_reason().is_none(), "still open");
    }
    new.writer
        .send(&mouser_proto::wire::ControlMessage::DeskAuth { hmac: proof })
        .await
        .unwrap();
    assert_eq!(closed_with(&session.conn).await, b"replaced");
}

/// A new connection that never proves membership ends the session, and the
/// session's connection with it.
#[tokio::test]
async fn a_new_connection_that_fails_its_proof_closes_the_session_connection() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let (session, mut new, _, _) = resuming(&node, 0x73).await;
    new.writer
        .send(&mouser_proto::wire::ControlMessage::DeskAuth { hmac: [0; 32] })
        .await
        .unwrap();
    assert_eq!(closed_with(&session.conn).await, b"replaced");
}
