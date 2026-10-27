use mouser_platform::LocalInput;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::DisplayId;
use mouser_proto::wire::{ControlMessage, Modifiers, MotionDatagram, display_id_hash};
use mouser_testkit::Injected;

use crate::EngineEvent;
use crate::lock::LockExt;
use crate::session::auth::{export, proof};
use crate::state::desk_key_name;
use crate::testing::*;
use mouser_platform::Platform;
use mouser_proto::control::{Desk, Method};
use serde_json::json;

fn display_of(node: &TestNode) -> DisplayId {
    DisplayId {
        computer: node.engine.computer_id(),
        key: "virtual-0".into(),
    }
}

async fn fault_at(events: &mut tokio::sync::broadcast::Receiver<EngineEvent>) -> String {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let EngineEvent::Fault { r#where } = events.recv().await.unwrap() {
                return r#where;
            }
        }
    })
    .await
    .expect("the fault is reported")
}

/// Both computers are connected again, with a new link, and the desk user
/// can cross from `a` to `b`.
async fn healed_and_crossing(nodes: &[TestNode], old_link: u64) {
    let (a, b) = (&nodes[0], &nodes[1]);
    eventually("the link to be re-established", || async {
        link_of(a, b).is_some_and(|l| l.0 != old_link && l.2 == crate::engine::Phase::Authed)
            && link_of(b, a).is_some_and(|l| l.2 == crate::engine::Phase::Authed)
            && connected(a).await == 1
            && connected(b).await == 1
    })
    .await;
    eventually("both input maps to know the desk", || async {
        a.engine.ctx.crossing.guard().map.wires.len() == 1
            && b.engine.ctx.crossing.guard().map.wires.len() == 1
    })
    .await;
    b.platform.take_injected();
    push_right(a);
    injected_where(&b.platform, "entry on Beta after the heal", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

#[tokio::test]
async fn a_panic_handling_an_inbound_message_closes_the_link_with_fault_and_it_heals() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let old = link_of(b, a).unwrap().0;
    let mut events = b.engine.events();

    b.platform.panic_on_next_inject();
    send_to(
        a,
        b,
        ControlMessage::Enter {
            display_id: display_of(b),
            x: 1.0,
            y: 2.0,
            seq_base: 1,
            modifiers_down: Modifiers::default(),
        },
    );

    assert_eq!(fault_at(&mut events).await, "read_loop");
    // The faulting computer is Local with nothing held.
    assert_eq!(
        b.engine.ctx.crossing.guard().session.control,
        crate::crossing::state::Control::Local
    );
    healed_and_crossing(&nodes, old).await;
}

#[tokio::test]
async fn a_panic_handling_a_datagram_closes_the_link_with_fault_and_it_heals() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    a.platform.send_input(LocalInput::Motion {
        dx: 0.0,
        dy: 0.0,
        abs: (0.0, 0.0),
    });
    let old = link_of(b, a).unwrap().0;
    let mut events = b.engine.events();

    b.platform.panic_on_next_inject();
    datagram_to(
        a,
        b,
        &MotionDatagram {
            seq: 1000,
            display_id_hash: display_id_hash(&display_of(b)),
            x: 5.0,
            y: 6.0,
        },
    );

    assert_eq!(fault_at(&mut events).await, "motion_loop");
    assert!(b.engine.ctx.crossing.guard().receiving.is_none());
    // Alpha lost Beta and has the pointer back.
    eventually("Alpha to be local", || async {
        a.engine.ctx.crossing.guard().session.remote().is_none()
    })
    .await;
    healed_and_crossing(&nodes, old).await;
}

#[tokio::test]
async fn a_message_that_violates_the_protocol_closes_the_link_with_reason_protocol() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let old = link_of(b, a).unwrap().0;

    // A second Hello on an established link is not part of the protocol.
    send_to(
        a,
        b,
        ControlMessage::Hello {
            protocol_version: mouser_proto::wire::PROTOCOL_VERSION,
            mouser_version: "0.1.0".into(),
            computer_id: ComputerId([9; 16]),
            display_name: "Again".into(),
            os: mouser_proto::model::Os::Linux,
            desk_id: None,
        },
    );

    healed_and_crossing(&nodes, old).await;
}

/// A desk member that is hand-driven, so the way the node closes a link can
/// be read from the other end of it.
async fn member_rogue(node: &TestNode, desk: &Desk, seed: u8) -> crate::transport::Link {
    let key: [u8; 32] = node
        .platform
        .secrets()
        .get(&desk_key_name(&desk.desk_id))
        .unwrap()
        .unwrap()
        .try_into()
        .unwrap();
    let rogue = Rogue::new(seed);
    let (mut link, _) = rogue.dial(node, Some(desk.desk_id)).await;
    assert!(matches!(
        link.reader.recv::<ControlMessage>().await,
        Some(ControlMessage::DeskAuth { .. })
    ));
    let good = proof(
        &key,
        &export(&link.conn),
        &desk.desk_id,
        &rogue.identity.computer_id(),
    );
    link.writer
        .send(&ControlMessage::DeskAuth { hmac: good })
        .await
        .unwrap();
    for _ in 0..2 {
        link.reader.recv::<ControlMessage>().await.unwrap();
    }
    // The node knows the rogue's (non-existent) display; give it one.
    link
}

async fn closed_with(conn: &quinn::Connection) -> Vec<u8> {
    match conn.closed().await {
        quinn::ConnectionError::ApplicationClosed(close) => close.reason.to_vec(),
        other => panic!("closed by the node with a reason, not {other:?}"),
    }
}

async fn node_with_desk(lab: &Lab) -> (TestNode, Desk) {
    let node = lab.start("Alpha").await;
    let desk: Desk = call(&node, Method::DeskCreate, json!({"name": "Home office"})).await;
    (node, desk)
}

fn enter_alpha(node: &TestNode) -> ControlMessage {
    ControlMessage::Enter {
        display_id: display_of(node),
        x: 1.0,
        y: 2.0,
        seq_base: 1,
        modifiers_down: Modifiers::default(),
    }
}

#[tokio::test]
async fn a_faulted_inbound_message_closes_the_link_with_reason_fault() {
    let lab = Lab::new();
    let (node, desk) = node_with_desk(&lab).await;
    let mut link = member_rogue(&node, &desk, 0x61).await;
    node.platform.panic_on_next_inject();
    link.writer.send(&enter_alpha(&node)).await.unwrap();
    assert_eq!(closed_with(&link.conn).await, b"fault");
}

#[tokio::test]
async fn a_faulted_datagram_closes_the_link_with_reason_fault() {
    let lab = Lab::new();
    let (node, desk) = node_with_desk(&lab).await;
    let mut link = member_rogue(&node, &desk, 0x62).await;
    link.writer.send(&enter_alpha(&node)).await.unwrap();
    injected_where(&node.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    node.platform.panic_on_next_inject();
    let motion = MotionDatagram {
        seq: 50,
        display_id_hash: display_id_hash(&display_of(&node)),
        x: 3.0,
        y: 4.0,
    };
    link.conn
        .send_datagram(postcard::to_stdvec(&motion).unwrap().into())
        .unwrap();
    assert_eq!(closed_with(&link.conn).await, b"fault");
}

#[tokio::test]
async fn a_protocol_violation_closes_the_link_with_reason_protocol() {
    let lab = Lab::new();
    let (node, desk) = node_with_desk(&lab).await;
    let mut link = member_rogue(&node, &desk, 0x63).await;
    link.writer
        .send(&ControlMessage::Hello {
            protocol_version: mouser_proto::wire::PROTOCOL_VERSION,
            mouser_version: "0.1.0".into(),
            computer_id: ComputerId([9; 16]),
            display_name: "Again".into(),
            os: mouser_proto::model::Os::Linux,
            desk_id: None,
        })
        .await
        .unwrap();
    assert_eq!(closed_with(&link.conn).await, b"protocol");
}
