use mouser_proto::control::{
    Desk, DiscoveredDesk, Empty, JoinPending, Method, PendingJoin, PendingState, ProblemKind, Topic,
};
use mouser_proto::ids::DeskId;
use mouser_proto::wire::ControlMessage;
use serde_json::json;

use super::join::{Requests, Sealed, open, seal};
use crate::state::{LocalState, Membership};
use crate::testing::{Lab, admit, call, call_err, connected, eventually, recv_topic, status};

async fn desk_on(node: &crate::testing::TestNode, name: &str) -> Desk {
    call(node, Method::DeskCreate, json!({ "name": name })).await
}

#[test]
fn sealed_desks_round_trip() {
    let sealed = Sealed {
        key: [5; 32],
        name: "Home".into(),
        state: crate::testing::empty_state(),
    };
    assert_eq!(open(&seal(&sealed)), sealed);
}

#[test]
fn requests_get_increasing_ids_and_can_be_taken_or_dropped_with_their_peer() {
    use crate::desk::join::PendingRequest;
    use mouser_proto::ids::{ComputerId, DevicePublicKey};
    let request = |peer: u8| PendingRequest {
        peer: ComputerId([peer; 16]),
        name: format!("Computer {peer}"),
        os: mouser_proto::model::Os::Linux,
        key: DevicePublicKey([peer; 32]),
        desk: DeskId([9; 16]),
    };
    let mut requests = Requests::default();
    let (a, b, c) = (
        requests.add(request(1)),
        requests.add(request(2)),
        requests.add(request(1)),
    );
    assert!(a < b && b < c);
    assert_eq!(requests.take(b).unwrap().peer, ComputerId([2; 16]));
    assert!(requests.take(b).is_none());
    requests.drop_peer(ComputerId([1; 16]));
    assert_eq!(requests.iter().count(), 0);
}

#[tokio::test]
async fn a_new_computer_joins_with_one_click_and_both_list_each_other() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let desk = desk_on(&a, "Home office").await;
    let b = lab.start("Beta").await;

    let offered: Vec<DiscoveredDesk> = call(&b, Method::DesksDiscovered, json!({})).await;
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].name, "Home office");
    assert_eq!(offered[0].members, ["Alpha"]);

    let mut notes = a.engine.control().notifications();
    let pending: JoinPending = call(&b, Method::DeskJoin, json!({"desk_id": desk.desk_id})).await;
    assert_eq!(pending.state, PendingState::Pending);
    let waiting = status(&b).await.desk.unwrap();
    assert_eq!(
        (waiting.id, waiting.pending_join, waiting.name.as_str()),
        (desk.desk_id, Some(true), "Home office")
    );

    let prompt: PendingJoin =
        serde_json::from_value(recv_topic(&mut notes, Topic::JoinRequest).await).unwrap();
    assert_eq!(prompt.computer.name, "Beta");
    assert_eq!(prompt.computer.id, b.engine.computer_id());
    assert_eq!(prompt.desk.name, "Home office");
    let listed: Vec<PendingJoin> = call(&a, Method::JoinPending, json!({})).await;
    assert_eq!(listed, std::slice::from_ref(&prompt));

    let _: Empty = call(
        &a,
        Method::JoinDecide,
        json!({"request_id": prompt.request_id, "approve": true}),
    )
    .await;
    eventually("Beta to become a member", || async {
        let s = status(&b).await;
        s.desk.as_ref().is_some_and(|d| d.pending_join.is_none()) && connected(&b).await == 1
    })
    .await;
    eventually("Alpha to list Beta", || async { connected(&a).await == 1 }).await;

    for node in [&a, &b] {
        let s = status(node).await;
        let names: Vec<_> = s.computers.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names.contains(&"Alpha") && names.contains(&"Beta"));
        assert_eq!(s.desk.unwrap().id, desk.desk_id);
    }
    assert!(
        call::<Vec<PendingJoin>>(&a, Method::JoinPending, json!({}))
            .await
            .is_empty()
    );
    assert!(b.platform.is_autostart_enabled(), "first run completed");
    // The desk key reached Beta through the secret store only.
    let saved = LocalState::load(&b.dir.path().join(crate::state::FILE));
    assert_eq!(saved.membership, Membership::Member(desk.desk_id));
    assert_eq!(saved.desk.unwrap().state.members.len(), 2);
}

#[tokio::test]
async fn a_declined_request_leaves_the_requester_unjoined_and_told() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let desk = desk_on(&a, "Home office").await;
    let b = lab.start("Beta").await;
    let mut a_notes = a.engine.control().notifications();
    let mut b_notes = b.engine.control().notifications();
    let _: JoinPending = call(&b, Method::DeskJoin, json!({"desk_id": desk.desk_id})).await;
    let prompt: PendingJoin =
        serde_json::from_value(recv_topic(&mut a_notes, Topic::JoinRequest).await).unwrap();
    let _: Empty = call(
        &a,
        Method::JoinDecide,
        json!({"request_id": prompt.request_id, "approve": false}),
    )
    .await;

    let problem = recv_topic(&mut b_notes, Topic::Problem).await;
    assert_eq!(problem["kind"], json!(ProblemKind::JoinRefused));
    assert!(
        problem["message"]
            .as_str()
            .unwrap()
            .contains("not admitted to Home office"),
        "{problem}"
    );
    eventually("Beta to be unjoined", || async {
        status(&b).await.desk.is_none()
    })
    .await;
    assert_eq!(status(&a).await.computers.len(), 1, "Alpha is still alone");
    assert!(!b.platform.is_autostart_enabled());
    let saved = LocalState::load(&b.dir.path().join(crate::state::FILE));
    assert_eq!(saved.membership, Membership::Unjoined);
}

#[tokio::test]
async fn deciding_on_an_unknown_request_is_an_error() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    desk_on(&a, "Home office").await;
    let err = call_err(
        &a,
        Method::JoinDecide,
        json!({"request_id": 99, "approve": true}),
    )
    .await;
    assert!(
        err.message.contains("no such join request"),
        "{}",
        err.message
    );
}

#[tokio::test]
async fn joining_is_refused_in_the_wrong_state_and_for_the_wrong_desk() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let desk = desk_on(&a, "Home office").await;
    let b = lab.start("Beta").await;

    let err = call_err(&b, Method::DeskJoin, json!({"desk_id": DeskId([0xee; 16])})).await;
    assert!(err.message.contains("not found"), "{}", err.message);
    let err = call_err(&b, Method::DeskMove, json!({"desk_id": desk.desk_id})).await;
    assert!(
        err.message.contains("not on a desk to move from"),
        "{}",
        err.message
    );
    let err = call_err(&a, Method::DeskJoin, json!({"desk_id": desk.desk_id})).await;
    assert!(
        err.message.contains("leave the current desk first"),
        "{}",
        err.message
    );

    let _: JoinPending = call(&b, Method::DeskJoin, json!({"desk_id": desk.desk_id})).await;
    let err = call_err(&b, Method::DeskJoin, json!({"desk_id": desk.desk_id})).await;
    assert!(err.message.contains("already waiting"), "{}", err.message);
    let err = call_err(&b, Method::DeskCreate, json!({"name": "Mine"})).await;
    assert!(err.message.contains("already"), "{}", err.message);
}

#[tokio::test]
async fn a_computer_that_changed_desks_since_discovery_is_not_joined_blindly() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    desk_on(&a, "Home office").await;
    let b = lab.start("Beta").await;
    // Discovery still advertises the old desk id while Alpha is on another.
    let stale = DeskId([0x77; 16]);
    let mut advert = lab.registry.advert_of(a.engine.computer_id()).unwrap();
    advert.desk = Some((stale, "Home office".into()));
    lab.browser().announce(&advert);
    let err = call_err(&b, Method::DeskJoin, json!({"desk_id": stale})).await;
    assert!(
        err.message.contains("no longer on the desk"),
        "{}",
        err.message
    );
    assert_eq!(status(&b).await.desk, None);
}

#[tokio::test]
async fn a_member_that_goes_away_before_deciding_ends_the_pending_join() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let desk = desk_on(&a, "Home office").await;
    let b = lab.start("Beta").await;
    let mut a_notes = a.engine.control().notifications();
    let mut b_notes = b.engine.control().notifications();
    let _: JoinPending = call(&b, Method::DeskJoin, json!({"desk_id": desk.desk_id})).await;
    recv_topic(&mut a_notes, Topic::JoinRequest).await;
    a.engine.shutdown().await;
    let problem = recv_topic(&mut b_notes, Topic::Problem).await;
    assert!(
        problem["message"]
            .as_str()
            .unwrap()
            .contains("not answered"),
        "{problem}"
    );
    assert_eq!(status(&b).await.desk, None);
}

#[tokio::test]
async fn a_join_request_for_another_desk_or_with_a_borrowed_key_is_a_protocol_violation() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let desk = desk_on(&a, "Home office").await;
    let rogue = crate::testing::Rogue::new(0x31);

    for (desk_id, key, expect_closed) in [
        (DeskId([0xdd; 16]), rogue.identity.public_key(), true),
        (
            desk.desk_id,
            mouser_proto::ids::DevicePublicKey([1; 32]),
            true,
        ),
    ] {
        let (mut link, _) = rogue.dial(&a, None).await;
        link.writer
            .send(&ControlMessage::JoinRequest {
                desk_id,
                device_public_key: key,
                display_name: "Rogue".into(),
                os: mouser_proto::model::Os::Linux,
            })
            .await
            .unwrap();
        assert_eq!(
            link.reader.recv::<ControlMessage>().await,
            None,
            "closed: {expect_closed}"
        );
    }
    assert!(
        call::<Vec<PendingJoin>>(&a, Method::JoinPending, json!({}))
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn unexpected_and_misplaced_decisions_are_protocol_violations() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    desk_on(&a, "Home office").await;
    let rogue = crate::testing::Rogue::new(0x32);
    let (mut link, _) = rogue.dial(&a, None).await;
    // A decision nobody asked for.
    link.writer
        .send(&ControlMessage::JoinDecision {
            approved: false,
            sealed: None,
        })
        .await
        .unwrap();
    assert_eq!(link.reader.recv::<ControlMessage>().await, None);
}

#[tokio::test]
async fn a_member_of_an_incompatible_version_cannot_be_joined() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let desk = desk_on(&a, "Home office").await;
    let b = lab.start_with_protocol("Beta", 2).await;
    let _: JoinPending = call(&b, Method::DeskJoin, json!({"desk_id": desk.desk_id})).await;
    eventually("Beta to give up", || async {
        status(&b).await.desk.is_none()
    })
    .await;
    let problems = status(&b).await.problems;
    assert!(
        problems
            .iter()
            .any(|p| p.kind == ProblemKind::IncompatibleVersion)
    );
    assert!(problems.iter().any(|p| p.kind == ProblemKind::JoinRefused));
}

#[tokio::test]
async fn admit_helper_makes_two_computers_members() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    desk_on(&a, "Home office").await;
    let b = lab.start("Beta").await;
    admit(&a, &b).await;
    assert_eq!(connected(&a).await, 1);
}
