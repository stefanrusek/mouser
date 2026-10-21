use mouser_proto::control::{Desk, DiscoveredDesk, Empty, JoinPending, Method, PendingJoin, Topic};
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{DeskState, Layout, Member, Os};
use serde_json::json;

use super::membership::{rotator, with_member, without_member};
use crate::state::desk_key_name;
use crate::testing::{
    Lab, TestNode, admit, call, call_err, connected, eventually, recv_topic, status,
};

fn member(n: u8) -> Member {
    Member {
        computer_id: ComputerId([n; 16]),
        device_public_key: mouser_proto::ids::DevicePublicKey([n; 32]),
        display_name: format!("Computer {n}"),
        os: Os::Linux,
        mouser_version: "0.1.0".into(),
        joined_at: "2026-10-21T00:00:00.000Z".into(),
    }
}

fn state(members: &[u8]) -> DeskState {
    DeskState {
        epoch: 4,
        author: ComputerId([1; 16]),
        members: members.iter().map(|n| member(*n)).collect(),
        layout: Layout::default(),
        allowlist: Vec::new(),
    }
}

#[test]
fn adding_and_removing_members_bumps_the_epoch_and_sets_the_author() {
    let added = with_member(&state(&[1, 2]), member(3), ComputerId([2; 16]));
    assert_eq!(
        (added.epoch, added.author, added.members.len()),
        (5, ComputerId([2; 16]), 3)
    );
    let removed = without_member(&added, ComputerId([1; 16]), ComputerId([3; 16]));
    assert_eq!((removed.epoch, removed.author), (6, ComputerId([3; 16])));
    assert!(
        removed
            .members
            .iter()
            .all(|m| m.computer_id != ComputerId([1; 16]))
    );
}

#[test]
fn the_lowest_computer_id_rotates_the_key() {
    assert_eq!(rotator(&state(&[7, 3, 5])), ComputerId([3; 16]));
}

async fn three_on_a_desk(lab: &Lab) -> (TestNode, TestNode, TestNode, Desk) {
    let a = lab.start("Alpha").await;
    let desk: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    let b = lab.start("Beta").await;
    admit(&a, &b).await;
    let c = lab.start("Gamma").await;
    admit(&a, &c).await;
    // Every member ends up connected to every other.
    for node in [&a, &b, &c] {
        eventually("a full mesh", || async { connected(node).await == 2 }).await;
    }
    (a, b, c, desk)
}

fn key_of(node: &TestNode, desk: &Desk) -> Vec<u8> {
    use mouser_platform::Platform;
    node.platform
        .secrets()
        .get(&desk_key_name(&desk.desk_id))
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn removing_a_member_removes_it_everywhere_and_rotates_the_key() {
    let lab = Lab::new();
    let (a, b, c, desk) = three_on_a_desk(&lab).await;
    let old_key = key_of(&a, &desk);
    let c_id = c.engine.computer_id();

    let _: Empty = call(&a, Method::DeskRemoveMember, json!({ "computer_id": c_id })).await;

    eventually("Gamma to be removed", || async {
        status(&c).await.desk.is_none()
    })
    .await;
    for node in [&a, &b] {
        eventually("the member list to shrink", || async {
            let s = status(node).await;
            s.computers.len() == 2 && s.computers.iter().all(|x| x.id != c_id)
        })
        .await;
    }
    eventually("the key to reach Beta", || async {
        key_of(&b, &desk) == key_of(&a, &desk)
    })
    .await;
    assert_ne!(key_of(&a, &desk), old_key, "the key rotated");
    assert!(
        c.platform_secret_missing(&desk),
        "Gamma forgot the desk key"
    );
    // Alpha and Beta are still connected under the new key.
    assert_eq!(connected(&a).await, 1);
}

impl TestNode {
    fn platform_secret_missing(&self, desk: &Desk) -> bool {
        use mouser_platform::Platform;
        self.platform
            .secrets()
            .get(&desk_key_name(&desk.desk_id))
            .unwrap()
            .is_none()
    }
}

#[tokio::test]
async fn an_offline_member_can_be_removed_too() {
    let lab = Lab::new();
    let (a, b, c, _desk) = three_on_a_desk(&lab).await;
    let c_id = c.engine.computer_id();
    c.engine.shutdown().await;
    eventually("Gamma to show offline", || async {
        connected(&a).await == 1
    })
    .await;
    let _: Empty = call(&a, Method::DeskRemoveMember, json!({ "computer_id": c_id })).await;
    for node in [&a, &b] {
        eventually("the member list to shrink", || async {
            let s = status(node).await;
            s.computers.len() == 2 && s.computers.iter().all(|x| x.id != c_id)
        })
        .await;
    }
}

#[tokio::test]
async fn a_member_that_leaves_disappears_from_every_list_and_the_key_rotates() {
    let lab = Lab::new();
    let (a, b, c, desk) = three_on_a_desk(&lab).await;
    let old_key = key_of(&a, &desk);
    let leaver = b.engine.computer_id();
    let _: Empty = call(&b, Method::DeskLeave, json!({})).await;
    assert_eq!(status(&b).await.desk, None);
    for node in [&a, &c] {
        eventually("the member list to shrink", || async {
            let s = status(node).await;
            s.computers.len() == 2 && s.computers.iter().all(|x| x.id != leaver)
        })
        .await;
    }
    eventually("Gamma to get the new key", || async {
        key_of(&c, &desk) == key_of(&a, &desk)
    })
    .await;
    assert_ne!(key_of(&a, &desk), old_key);
    assert!(b.platform_secret_missing(&desk));
    eventually("Alpha and Gamma to stay connected", || async {
        connected(&a).await == 1
    })
    .await;
}

#[tokio::test]
async fn the_lowest_member_leaving_hands_rotation_to_the_next() {
    let lab = Lab::new();
    let (a, b, c, desk) = three_on_a_desk(&lab).await;
    let mut ids = [&a, &b, &c].map(|n| (n.engine.computer_id(), n));
    ids.sort_by_key(|(id, _)| *id);
    let [(_, lowest), (_, second), (_, third)] = ids;
    let _: Empty = call(lowest, Method::DeskLeave, json!({})).await;
    for node in [second, third] {
        eventually("two members to remain", || async {
            let s = status(node).await;
            s.computers.len() == 2 && connected(node).await == 1
        })
        .await;
    }
    assert_eq!(key_of(second, &desk), key_of(third, &desk));
}

#[tokio::test]
async fn removing_or_leaving_with_nothing_to_remove_is_refused() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let err = call_err(&a, Method::DeskLeave, json!({})).await;
    assert!(err.message.contains("not on a desk"), "{}", err.message);
    let err = call_err(
        &a,
        Method::DeskRemoveMember,
        json!({"computer_id": ComputerId([1; 16])}),
    )
    .await;
    assert!(err.message.contains("not on a desk"), "{}", err.message);
    let _: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    let err = call_err(
        &a,
        Method::DeskRemoveMember,
        json!({"computer_id": ComputerId([1; 16])}),
    )
    .await;
    assert!(
        err.message.contains("not another member"),
        "{}",
        err.message
    );
    let me = a.engine.computer_id();
    let err = call_err(&a, Method::DeskRemoveMember, json!({ "computer_id": me })).await;
    assert!(
        err.message.contains("not another member"),
        "{}",
        err.message
    );
}

#[tokio::test]
async fn moving_leaves_the_old_desk_only_after_the_new_one_approves() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let home: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    let b = lab.start("Beta").await;
    admit(&a, &b).await;
    let d = lab.start("Delta").await;
    let lab_desk: Desk = call(&d, Method::DeskCreate, json!({"name": "Lab bench"})).await;

    let mut d_notes = d.engine.control().notifications();
    let _: JoinPending = call(&b, Method::DeskMove, json!({"desk_id": lab_desk.desk_id})).await;
    let waiting = status(&b).await;
    assert_eq!(
        waiting.desk.as_ref().unwrap().id,
        lab_desk.desk_id,
        "shows the desk it is moving to"
    );
    assert_eq!(waiting.desk.unwrap().pending_join, Some(true));
    // Still on the old desk until Delta answers.
    assert_eq!(connected(&a).await, 1);
    assert_eq!(status(&a).await.computers.len(), 2);

    let prompt: PendingJoin =
        serde_json::from_value(recv_topic(&mut d_notes, Topic::JoinRequest).await).unwrap();
    let _: Empty = call(
        &d,
        Method::JoinDecide,
        json!({"request_id": prompt.request_id, "approve": true}),
    )
    .await;

    eventually("Beta to appear only in Lab bench", || async {
        status(&a).await.computers.len() == 1
            && status(&d).await.computers.len() == 2
            && connected(&d).await == 1
    })
    .await;
    let s = status(&b).await;
    assert_eq!(s.desk.unwrap().id, lab_desk.desk_id);
    assert!(s.computers.iter().all(|c| c.name != "Alpha"));
    assert!(key_of(&b, &lab_desk) == key_of(&d, &lab_desk));
    assert!(b.platform_secret_missing(&home), "the old desk key is gone");
}

#[tokio::test]
async fn a_declined_move_keeps_the_old_desk() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let home: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    let b = lab.start("Beta").await;
    admit(&a, &b).await;
    let d = lab.start("Delta").await;
    let lab_desk: Desk = call(&d, Method::DeskCreate, json!({"name": "Lab bench"})).await;
    let mut d_notes = d.engine.control().notifications();
    let _: JoinPending = call(&b, Method::DeskMove, json!({"desk_id": lab_desk.desk_id})).await;
    let prompt: PendingJoin =
        serde_json::from_value(recv_topic(&mut d_notes, Topic::JoinRequest).await).unwrap();
    let _: Empty = call(
        &d,
        Method::JoinDecide,
        json!({"request_id": prompt.request_id, "approve": false}),
    )
    .await;
    eventually("Beta to be back on Home office", || async {
        let s = status(&b).await;
        s.desk
            .is_some_and(|d| d.id == home.desk_id && d.pending_join.is_none())
    })
    .await;
    assert_eq!(connected(&b).await, 1);
    let offered: Vec<DiscoveredDesk> = call(&b, Method::DesksDiscovered, json!({})).await;
    assert_eq!(offered.len(), 1, "own desk is not offered");
}

#[tokio::test]
async fn leave_and_remove_messages_from_the_wrong_party_are_protocol_violations() {
    use mouser_proto::wire::ControlMessage;
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let _: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    let rogue = crate::testing::Rogue::new(0x41);
    for msg in [
        ControlMessage::Leave {
            computer_id: ComputerId([1; 16]),
        },
        ControlMessage::Remove {
            computer_id: a.engine.computer_id(),
        },
        ControlMessage::DeskState(state(&[1])),
        ControlMessage::Displays {
            displays: Vec::new(),
        },
        ControlMessage::Panic,
    ] {
        let (mut link, _) = rogue.dial(&a, None).await;
        link.writer.send(&msg).await.unwrap();
        assert_eq!(link.reader.recv::<ControlMessage>().await, None, "{msg:?}");
    }
    assert_eq!(status(&a).await.computers.len(), 1);
}
