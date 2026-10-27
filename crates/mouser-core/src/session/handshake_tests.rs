use mouser_proto::ids::{ComputerId, DeskId};
use mouser_proto::model::Os;
use mouser_proto::wire::{ControlMessage, PROTOCOL_VERSION};

use super::handshake::{PeerHello, exchange, incompatibility};
use crate::identity::Identity;
use crate::transport::Link;

fn hello(
    id: ComputerId,
    name: &str,
    version: &str,
    protocol: u16,
    desk: Option<DeskId>,
) -> ControlMessage {
    ControlMessage::Hello {
        protocol_version: protocol,
        mouser_version: version.into(),
        computer_id: id,
        display_name: name.into(),
        os: Os::Macos,
        desk_id: desk,
    }
}

async fn links() -> (Identity, Identity, Link, Link) {
    let (ia, ib) = (Identity::from_seed([1; 32]), Identity::from_seed([2; 32]));
    let (a, b) = (
        crate::transport::Transport::bind(&ia, 0),
        crate::transport::Transport::bind(&ib, 0),
    );
    let to = std::net::SocketAddr::from(([127, 0, 0, 1], b.local_addr().port()));
    let (dialed, accepted) = crate::testing::handshake(&a, &b, to, Some(ib.computer_id())).await;
    // Keep the endpoints alive for the whole test.
    Box::leak(Box::new((a, b)));
    let mut dialer = Link::open(dialed).await.unwrap();
    // The control stream becomes visible to the peer with its first frame.
    dialer.writer.send(&ControlMessage::Panic).await.unwrap();
    let mut acceptor = Link::accepted(accepted).await.unwrap();
    assert_eq!(
        acceptor.reader.recv::<ControlMessage>().await,
        Some(ControlMessage::Panic)
    );
    (ia, ib, dialer, acceptor)
}

#[tokio::test]
async fn hello_is_exchanged_first_in_both_directions() {
    let (ia, ib, mut dialer, mut acceptor) = links().await;
    let desk = DeskId([5; 16]);
    let (from_b, from_a) = tokio::join!(
        exchange(
            &mut dialer,
            hello(ia.computer_id(), "Alpha", "0.1.0", PROTOCOL_VERSION, None)
        ),
        exchange(
            &mut acceptor,
            hello(
                ib.computer_id(),
                "Beta",
                "0.1.0",
                PROTOCOL_VERSION,
                Some(desk)
            )
        ),
    );
    let (from_b, from_a) = (from_b.unwrap(), from_a.unwrap());
    assert_eq!(from_b.computer_id, ib.computer_id());
    assert_eq!(from_b.display_name, "Beta");
    assert_eq!(from_b.desk_id, Some(desk));
    assert_eq!(from_a.computer_id, ia.computer_id());
    assert_eq!(from_a.desk_id, None);
    assert!(from_a.compatible_with(PROTOCOL_VERSION));
}

#[tokio::test]
async fn a_peer_that_hangs_up_before_hello_yields_nothing() {
    let (ia, _ib, mut dialer, acceptor) = links().await;
    acceptor.conn.close(0u32.into(), b"bye");
    let hello = hello(ia.computer_id(), "Alpha", "0.1.0", PROTOCOL_VERSION, None);
    assert!(exchange(&mut dialer, hello).await.is_none());
}

#[tokio::test]
#[should_panic(expected = "first frame must be Hello")]
async fn anything_but_hello_first_is_a_protocol_violation() {
    let (ia, _ib, mut dialer, mut acceptor) = links().await;
    acceptor.writer.send(&ControlMessage::Panic).await.unwrap();
    let hello = hello(ia.computer_id(), "Alpha", "0.1.0", PROTOCOL_VERSION, None);
    exchange(&mut dialer, hello).await;
}

#[tokio::test]
#[should_panic(expected = "Hello does not match")]
async fn a_hello_claiming_another_identity_is_rejected() {
    let (ia, _ib, mut dialer, mut acceptor) = links().await;
    let liar = hello(
        ComputerId([0xcc; 16]),
        "Liar",
        "0.1.0",
        PROTOCOL_VERSION,
        None,
    );
    acceptor.writer.send(&liar).await.unwrap();
    let mine = hello(ia.computer_id(), "Alpha", "0.1.0", PROTOCOL_VERSION, None);
    exchange(&mut dialer, mine).await;
}

fn peer(protocol: u16) -> PeerHello {
    PeerHello {
        computer_id: ComputerId([2; 16]),
        mouser_version: "0.9.0".into(),
        protocol_version: protocol,
        display_name: "Studio iMac".into(),
        os: Os::Macos,
        desk_id: None,
    }
}

#[test]
fn differing_protocol_versions_are_incompatible() {
    assert!(peer(1).compatible_with(1));
    assert!(!peer(2).compatible_with(1));
}

#[test]
fn the_message_names_the_other_computer_both_versions_and_which_to_update() {
    let (text, mine_is_older) = incompatibility("Laptop", "0.1.0", 1, &peer(2));
    assert!(mine_is_older);
    assert!(text.contains("Studio iMac"), "{text}");
    assert!(text.contains("0.9.0") && text.contains("0.1.0"), "{text}");
    assert!(text.contains("Update Laptop"), "{text}");
    let (text, mine_is_older) = incompatibility("Laptop", "0.1.0", 3, &peer(2));
    assert!(!mine_is_older);
    assert!(text.contains("Update Studio iMac"), "{text}");
}

#[tokio::test]
async fn peer_connected_carries_the_version_and_os_of_the_peer() {
    use crate::testing::*;
    use mouser_platform::Platform;
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let id = b.engine.computer_id().to_string();
    let record = logged(a, "peer.connected", |r| r["peer"] == id.as_str()).await;
    assert_eq!(
        record["fields"],
        serde_json::json!({
            "version": "0.1.0",
            "os": crate::platform_os(b.platform.info().os),
        })
    );
}
