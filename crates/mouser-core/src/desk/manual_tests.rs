use std::net::SocketAddr;

use mouser_proto::control::{Desk, Empty, JoinPending, Method, PendingJoin, PendingState, Topic};
use serde_json::json;

use super::manual::resolve;
use crate::testing::{Lab, call, call_err, connected, eventually, recv_topic, status};

#[tokio::test]
async fn resolve_accepts_ip_addresses_and_host_names_and_prefers_ipv4() {
    assert_eq!(
        resolve("192.168.1.5:47801").await,
        Some("192.168.1.5:47801".parse().unwrap())
    );
    let local = resolve("localhost:47801").await.unwrap();
    assert!(local.ip().is_loopback() && local.port() == 47801);
    assert!(local.is_ipv4(), "IPv4 first");
    assert_eq!(
        resolve("[::1]:5").await,
        Some("[::1]:5".parse::<SocketAddr>().unwrap())
    );
}

#[tokio::test]
async fn an_unusable_address_resolves_to_nothing() {
    assert_eq!(resolve("not an address").await, None);
    assert_eq!(resolve("localhost").await, None, "the port is required");
}

#[tokio::test]
async fn joining_by_address_follows_the_same_approval_flow() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let desk: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    let b = lab.start("Beta").await;
    // Beta never discovers Alpha: it is given the address instead.
    let address = format!("127.0.0.1:{}", a.engine.local_addr().port());
    let mut notes = a.engine.control().notifications();
    let pending: JoinPending = call(&b, Method::DeskJoin, json!({ "address": address })).await;
    assert_eq!(pending.state, PendingState::Pending);
    let prompt: PendingJoin =
        serde_json::from_value(recv_topic(&mut notes, Topic::JoinRequest).await).unwrap();
    assert_eq!(prompt.computer.name, "Beta");
    let _: Empty = call(
        &a,
        Method::JoinDecide,
        json!({"request_id": prompt.request_id, "approve": true}),
    )
    .await;
    eventually("Beta to join", || async {
        connected(&b).await == 1 && connected(&a).await == 1
    })
    .await;
    assert_eq!(status(&b).await.desk.unwrap().id, desk.desk_id);
}

#[tokio::test]
async fn bad_or_unreachable_addresses_and_computers_without_a_desk_are_reported() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let b = lab.start("Beta").await;
    let err = call_err(&b, Method::DeskJoin, json!({"address": "no such host"})).await;
    assert!(err.message.contains("not an address"), "{}", err.message);
    let nobody = format!("127.0.0.1:{}", crate::testing::free_port());
    let err = call_err(&b, Method::DeskJoin, json!({ "address": nobody })).await;
    assert!(err.message.contains("could not reach"), "{}", err.message);
    // Alpha is reachable but is not on a desk.
    let address = format!("127.0.0.1:{}", a.engine.local_addr().port());
    let err = call_err(&b, Method::DeskJoin, json!({ "address": address })).await;
    assert!(err.message.contains("not on a desk"), "{}", err.message);
    assert_eq!(status(&b).await.desk, None);
}

#[tokio::test]
async fn a_request_by_address_that_is_declined_names_no_desk() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let _: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    let b = lab.start("Beta").await;
    let mut a_notes = a.engine.control().notifications();
    let mut b_notes = b.engine.control().notifications();
    let address = format!("127.0.0.1:{}", a.engine.local_addr().port());
    let _: JoinPending = call(&b, Method::DeskJoin, json!({ "address": address })).await;
    let prompt: PendingJoin =
        serde_json::from_value(recv_topic(&mut a_notes, Topic::JoinRequest).await).unwrap();
    let _: Empty = call(
        &a,
        Method::JoinDecide,
        json!({"request_id": prompt.request_id, "approve": false}),
    )
    .await;
    let problem = recv_topic(&mut b_notes, Topic::Problem).await;
    assert!(
        problem["message"]
            .as_str()
            .unwrap()
            .contains("not admitted to that desk"),
        "{problem}"
    );
}
