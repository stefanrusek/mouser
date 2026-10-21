use mouser_proto::ids::{ComputerId, DeskId};

use super::auth::{export, proof, verify};
use crate::identity::Identity;
use crate::transport::Transport;

const KEY: [u8; 32] = [7; 32];
const EXPORTER: [u8; 32] = [9; 32];
const DESK: DeskId = DeskId([3; 16]);
const ALPHA: ComputerId = ComputerId([1; 16]);
const BETA: ComputerId = ComputerId([2; 16]);

#[test]
fn a_member_proves_membership_with_the_desk_key() {
    let mac = proof(&KEY, &EXPORTER, &DESK, &ALPHA);
    assert!(verify(&KEY, &EXPORTER, &DESK, &ALPHA, &mac));
}

#[test]
fn a_computer_without_the_desk_key_cannot_prove_membership() {
    let forged = proof(&[8; 32], &EXPORTER, &DESK, &ALPHA);
    assert!(!verify(&KEY, &EXPORTER, &DESK, &ALPHA, &forged));
}

#[test]
fn the_proof_is_bound_to_the_connection_the_desk_and_the_sender() {
    let mac = proof(&KEY, &EXPORTER, &DESK, &ALPHA);
    assert!(
        !verify(&KEY, &[0; 32], &DESK, &ALPHA, &mac),
        "another connection"
    );
    assert!(
        !verify(&KEY, &EXPORTER, &DeskId([4; 16]), &ALPHA, &mac),
        "another desk"
    );
    assert!(
        !verify(&KEY, &EXPORTER, &DESK, &BETA, &mac),
        "reflected back by the peer"
    );
}

#[tokio::test]
async fn both_ends_of_a_connection_derive_the_same_transcript() {
    let (ia, ib) = (Identity::from_seed([1; 32]), Identity::from_seed([2; 32]));
    let (a, b) = (Transport::bind(&ia, 0), Transport::bind(&ib, 0));
    let to = std::net::SocketAddr::from(([127, 0, 0, 1], b.local_addr().port()));
    let (dialed, accepted) = tokio::join!(a.connect(to, None), async {
        Transport::complete(b.accept().await.unwrap())
            .await
            .unwrap()
    });
    let dialed = dialed.unwrap();
    assert_eq!(export(&dialed), export(&accepted));
    assert_ne!(export(&dialed), [0; 32]);
}

mod on_the_wire {
    use mouser_proto::control::{Desk, Method};
    use mouser_proto::wire::ControlMessage;
    use serde_json::json;

    use crate::session::auth::{export, proof};
    use crate::state::desk_key_name;
    use crate::testing::{Lab, Rogue, call};

    async fn desk_node(lab: &Lab) -> (crate::testing::TestNode, Desk) {
        let node = lab.start("Alpha").await;
        let desk: Desk = call(&node, Method::DeskCreate, json!({"name": "Home office"})).await;
        (node, desk)
    }

    #[tokio::test]
    async fn a_computer_claiming_the_desk_without_its_key_gets_no_desk_traffic() {
        let lab = Lab::new();
        let (node, desk) = desk_node(&lab).await;
        let rogue = Rogue::new(0x51);
        let (mut link, hello) = rogue.dial(&node, Some(desk.desk_id)).await;
        assert_eq!(hello.desk_id, Some(desk.desk_id));
        // The member offers its proof first.
        assert!(matches!(
            link.reader.recv::<ControlMessage>().await,
            Some(ControlMessage::DeskAuth { .. })
        ));
        let forged = proof(
            &[0; 32],
            &export(&link.conn),
            &desk.desk_id,
            &rogue.identity.computer_id(),
        );
        link.writer
            .send(&ControlMessage::DeskAuth { hmac: forged })
            .await
            .unwrap();
        // No DeskState, no Displays: the connection is closed.
        assert_eq!(link.reader.recv::<ControlMessage>().await, None);
    }

    #[tokio::test]
    async fn a_computer_with_the_key_receives_the_desk_state_and_displays() {
        use mouser_platform::Platform;
        let lab = Lab::new();
        let (node, desk) = desk_node(&lab).await;
        let key: [u8; 32] = node
            .platform
            .secrets()
            .get(&desk_key_name(&desk.desk_id))
            .unwrap()
            .unwrap()
            .try_into()
            .unwrap();
        let rogue = Rogue::new(0x52);
        let (mut link, _) = rogue.dial(&node, Some(desk.desk_id)).await;
        let theirs = link.reader.recv::<ControlMessage>().await;
        assert!(matches!(theirs, Some(ControlMessage::DeskAuth { .. })));
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
        let Some(ControlMessage::DeskState(state)) = link.reader.recv::<ControlMessage>().await
        else {
            panic!("desk state expected");
        };
        assert_eq!(state.epoch, desk.state.epoch);
        let Some(ControlMessage::Displays { displays }) =
            link.reader.recv::<ControlMessage>().await
        else {
            panic!("displays expected");
        };
        assert_eq!(displays.len(), 1);
    }

    /// A rogue that holds the desk key and has passed `DeskAuth`.
    async fn member_rogue(
        node: &crate::testing::TestNode,
        desk: &Desk,
        seed: u8,
    ) -> (crate::transport::Link, Rogue) {
        use mouser_platform::Platform;
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
        assert!(matches!(
            link.reader.recv::<ControlMessage>().await,
            Some(ControlMessage::DeskState(_))
        ));
        assert!(matches!(
            link.reader.recv::<ControlMessage>().await,
            Some(ControlMessage::Displays { .. })
        ));
        (link, rogue)
    }

    #[tokio::test]
    async fn only_a_newer_desk_state_replaces_the_key_and_state() {
        use crate::desk::join::{Sealed, seal};
        use mouser_platform::Platform;
        let lab = Lab::new();
        let (node, desk) = desk_node(&lab).await;
        let (mut link, _rogue) = member_rogue(&node, &desk, 0x55).await;
        let old_key = node
            .platform
            .secrets()
            .get(&desk_key_name(&desk.desk_id))
            .unwrap()
            .unwrap();
        let rekey = |epoch: u64, key: u8| {
            let mut state = desk.state.clone();
            state.epoch = epoch;
            ControlMessage::JoinDecision {
                approved: true,
                sealed: Some(seal(&Sealed {
                    key: [key; 32],
                    name: desk.name.clone(),
                    state,
                })),
            }
        };
        link.writer.send(&rekey(0, 0xaa)).await.unwrap();
        // The stale state is ignored, the newer one adopted.
        link.writer.send(&rekey(9, 0xbb)).await.unwrap();
        crate::testing::eventually("the newer key", || async {
            node.platform
                .secrets()
                .get(&desk_key_name(&desk.desk_id))
                .unwrap()
                .unwrap()
                == vec![0xbb; 32]
        })
        .await;
        assert_ne!(old_key, vec![0xbb; 32]);
    }

    #[tokio::test]
    async fn a_new_connection_from_the_same_computer_replaces_the_old_one() {
        let lab = Lab::new();
        let node = lab.start("Alpha").await;
        let rogue = Rogue::new(0x54);
        let (mut first, _) = rogue.dial(&node, None).await;
        let (mut second, _) = rogue.dial(&node, None).await;
        assert_eq!(
            first.reader.recv::<ControlMessage>().await,
            None,
            "the old link was closed"
        );
        // The new link works: an unexpected message gets it dropped in turn.
        second.writer.send(&ControlMessage::Panic).await.unwrap();
        assert_eq!(second.reader.recv::<ControlMessage>().await, None);
    }

    #[tokio::test]
    async fn a_proof_when_not_on_a_desk_is_a_protocol_violation() {
        let lab = Lab::new();
        let node = lab.start("Alpha").await;
        let rogue = Rogue::new(0x53);
        let (mut link, _) = rogue.dial(&node, None).await;
        link.writer
            .send(&ControlMessage::DeskAuth { hmac: [0; 32] })
            .await
            .unwrap();
        assert_eq!(link.reader.recv::<ControlMessage>().await, None);
    }

    #[tokio::test]
    async fn a_desk_mate_that_cannot_be_reached_is_retried_and_harmless() {
        use crate::discovery::Advert;
        use mouser_proto::ids::ComputerId;
        let lab = Lab::new();
        let (node, desk) = desk_node(&lab).await;
        // A desk-mate (with a higher id, so we dial it) whose port nobody listens on.
        lab.browser().announce(&Advert {
            computer: ComputerId([0xff; 16]),
            display_name: "Ghost".into(),
            protocol: mouser_proto::wire::PROTOCOL_VERSION,
            desk: Some((desk.desk_id, "Home office".into())),
            port: crate::testing::free_port(),
        });
        crate::testing::eventually("the dial attempt to finish", || async {
            let core = node.engine.ctx.core.lock().unwrap();
            core.dialing.is_empty() && core.peers.is_empty()
        })
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        assert_eq!(crate::testing::connected(&node).await, 0);
        assert!(node.engine.ctx.core.lock().unwrap().dialing.len() <= 1);
    }

    #[tokio::test]
    async fn desks_that_see_each_other_never_connect() {
        let lab = Lab::new();
        let a = lab.start("Alpha").await;
        let b = lab.start("Beta").await;
        let _: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
        let _: Desk = call(&b, Method::DeskCreate, json!({"name": "Lab bench"})).await;
        tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        for node in [&a, &b] {
            assert_eq!(crate::testing::connected(node).await, 0);
            assert_eq!(crate::testing::status(node).await.computers.len(), 1);
        }
    }
}
