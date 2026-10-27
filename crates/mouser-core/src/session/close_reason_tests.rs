//! Why a link ended (data-model "Link loss reason"): derived from the
//! connection's error and the close reason bytes, and written as
//! `peer.disconnected`.

use bytes::Bytes;
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ControlMessage;
use quinn::{ApplicationClose, ConnectionError};

use super::close_reason::{CloseReason, LinkLossReason};
use crate::testing::*;

fn app_closed(reason: &'static [u8]) -> ConnectionError {
    ConnectionError::ApplicationClosed(ApplicationClose {
        error_code: 0u32.into(),
        reason: Bytes::from_static(reason),
    })
}

#[test]
fn the_close_reason_bytes_of_the_peer_name_why_it_closed() {
    use LinkLossReason::*;
    for (bytes, reason) in [
        (&b"done"[..], PeerLeft),
        (b"quit", PeerQuit),
        // A 0.2.x computer says `shutdown` when it quits.
        (b"shutdown", PeerQuit),
        (b"not a member of this desk", Removed),
        (b"replaced", Replaced),
        (b"incompatible", Incompatible),
        (b"protocol", Protocol),
        // The peer's own link task faulted: it closed the link.
        (b"fault", PeerLeft),
        // Anything else is a peer that closed the link.
        (b"something newer", PeerLeft),
    ] {
        let error = ConnectionError::ApplicationClosed(ApplicationClose {
            error_code: 0u32.into(),
            reason: Bytes::copy_from_slice(bytes),
        });
        assert_eq!(LinkLossReason::of(&error), reason, "{bytes:?}");
    }
}

#[test]
fn a_silent_path_is_unreachable_and_a_version_mismatch_is_incompatible() {
    assert_eq!(
        LinkLossReason::of(&ConnectionError::TimedOut),
        LinkLossReason::Unreachable
    );
    assert_eq!(
        LinkLossReason::of(&ConnectionError::Reset),
        LinkLossReason::Unreachable
    );
    assert_eq!(
        LinkLossReason::of(&ConnectionError::VersionMismatch),
        LinkLossReason::Incompatible
    );
    // Closed on this side without a reason of its own: the link was closed
    // as one is when a computer leaves.
    assert_eq!(
        LinkLossReason::of(&ConnectionError::LocallyClosed),
        LinkLossReason::PeerLeft
    );
    assert_eq!(
        LinkLossReason::of(&app_closed(b"quit")),
        LinkLossReason::PeerQuit
    );
}

#[test]
fn this_computer_sends_done_quit_fault_protocol_replaced_and_not_a_member() {
    for (reason, bytes, lost) in [
        (CloseReason::Done, &b"done"[..], LinkLossReason::PeerLeft),
        (CloseReason::Quit, b"quit", LinkLossReason::QuitHere),
        (CloseReason::Fault, b"fault", LinkLossReason::FaultHere),
        (CloseReason::Protocol, b"protocol", LinkLossReason::Protocol),
        (CloseReason::Replaced, b"replaced", LinkLossReason::Replaced),
        (
            CloseReason::NotMember,
            b"not a member of this desk",
            LinkLossReason::Removed,
        ),
    ] {
        assert_eq!(reason.bytes(), bytes);
        assert_eq!(reason.loss(), lost);
    }
}

/// The reason `node` logged for losing `peer`, once it did.
async fn lost(node: &TestNode, peer: ComputerId) -> String {
    let id = peer.to_string();
    let record = logged(node, "peer.disconnected", |r| r["peer"] == id.as_str()).await;
    record["fields"]["reason"].as_str().unwrap().to_string()
}

fn enter(node: &TestNode) -> ControlMessage {
    ControlMessage::Enter {
        display_id: mouser_proto::model::DisplayId {
            computer: node.engine.computer_id(),
            key: "virtual-0".into(),
        },
        x: 1.0,
        y: 2.0,
        seq_base: 1,
        modifiers_down: mouser_proto::wire::Modifiers::default(),
    }
}

#[tokio::test]
async fn quitting_is_peer_quit_to_the_other_computer_and_quit_here_to_this_one() {
    let lab = Lab::new();
    let mut nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let beta = nodes.pop().unwrap();
    let alpha = nodes.pop().unwrap();
    let (alpha_id, beta_id) = (alpha.engine.computer_id(), beta.engine.computer_id());
    let alpha_log = alpha.engine.log().path();
    alpha.engine.shutdown().await;
    assert_eq!(lost(&beta, alpha_id).await, "peer_quit");
    // Alpha wrote its own record before it was gone.
    let mine = read_log(&alpha_log)
        .into_iter()
        .find(|r| r["event"] == "peer.disconnected" && r["peer"] == beta_id.to_string())
        .expect("Alpha logged the end of the link");
    assert_eq!(mine["fields"]["reason"], "quit_here");
}

#[tokio::test]
async fn a_faulted_link_is_fault_here_where_it_faulted_and_a_peer_that_closed_to_the_other() {
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    b.platform.panic_on_next_inject();
    send_to(a, b, enter(b));
    assert_eq!(lost(b, a.engine.computer_id()).await, "fault_here");
    assert_eq!(lost(a, b.engine.computer_id()).await, "peer_left");
}

#[tokio::test]
async fn an_undecodable_message_is_a_protocol_loss_on_both_sides() {
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
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
    assert_eq!(lost(b, a.engine.computer_id()).await, "protocol");
    assert_eq!(lost(a, b.engine.computer_id()).await, "protocol");
}

#[tokio::test]
async fn a_malformed_frame_is_a_protocol_loss() {
    let lab = Lab::new();
    let node = lab.start_logged("Alpha").await;
    let rogue = Rogue::new(0x71);
    let (mut link, _) = rogue.dial(&node, None).await;
    // A frame whose body cannot be decoded as any message.
    link.writer.send_raw(&[0, 0, 0, 2, 0xff, 0xff]).await;
    assert_eq!(lost(&node, rogue.identity.computer_id()).await, "protocol");
}

#[tokio::test]
async fn a_newer_connection_replaces_the_older_one_which_is_logged_as_replaced() {
    let lab = Lab::new();
    let node = lab.start_logged("Alpha").await;
    let rogue = Rogue::new(0x72);
    let (_first, _) = rogue.dial(&node, None).await;
    let (_second, _) = rogue.dial(&node, None).await;
    assert_eq!(lost(&node, rogue.identity.computer_id()).await, "replaced");
}

#[tokio::test]
async fn a_peer_that_says_incompatible_is_an_incompatible_loss() {
    let lab = Lab::new();
    let node = lab.start_logged("Alpha").await;
    let rogue = Rogue::new(0x73);
    let (link, _) = rogue.dial(&node, None).await;
    link.conn.close(0u32.into(), b"incompatible");
    assert_eq!(
        lost(&node, rogue.identity.computer_id()).await,
        "incompatible"
    );
}

#[tokio::test]
async fn a_computer_that_fails_the_desk_check_is_removed() {
    use mouser_proto::control::{Desk, Method};
    let lab = Lab::new();
    let node = lab.start_logged("Alpha").await;
    let desk: Desk = call(&node, Method::DeskCreate, json_name()).await;
    let rogue = Rogue::new(0x74);
    let (mut link, _) = rogue.dial(&node, Some(desk.desk_id)).await;
    assert!(matches!(
        link.reader.recv::<ControlMessage>().await,
        Some(ControlMessage::DeskAuth { .. })
    ));
    link.writer
        .send(&ControlMessage::DeskAuth { hmac: [0; 32] })
        .await
        .unwrap();
    assert_eq!(lost(&node, rogue.identity.computer_id()).await, "removed");
}

fn json_name() -> serde_json::Value {
    serde_json::json!({"name": "Home office"})
}

#[tokio::test]
async fn a_silent_path_is_unreachable() {
    let lab = Lab::simulated();
    let mut nodes = Vec::new();
    for name in ["Alpha", "Beta"] {
        nodes.push(lab.start_logged(name).await);
    }
    let nodes = desk_from(nodes).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    b.sim.as_ref().unwrap().set_blackhole(true);
    assert_eq!(lost(a, b.engine.computer_id()).await, "unreachable");
    assert_eq!(lost(b, a.engine.computer_id()).await, "unreachable");
}
