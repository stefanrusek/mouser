use mouser_proto::control::{Desk, EngineState, Method, TrayConnection};
use serde_json::json;

use crate::testing::*;

async fn connection(node: &TestNode) -> TrayConnection {
    status(node).await.tray_summary.connection
}

#[tokio::test]
async fn no_desk_is_alone() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    assert_eq!(connection(&node).await, TrayConnection::Alone);
}

#[tokio::test]
async fn a_desk_with_no_other_member_is_alone() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let _: Desk = call(&node, Method::DeskCreate, json!({"name": "Home"})).await;
    assert_eq!(connection(&node).await, TrayConnection::Alone);
}

#[tokio::test]
async fn every_member_connected_is_connected_and_pausing_changes_nothing() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
    for node in &nodes {
        assert_eq!(connection(node).await, TrayConnection::Connected);
    }
    let _: mouser_proto::control::Empty = call(&nodes[0], Method::ActivePause, json!({})).await;
    let s = status(&nodes[0]).await;
    assert_eq!(s.tray_summary.state, EngineState::Paused);
    assert_eq!(s.tray_summary.connection, TrayConnection::Connected);
    // A paused desk-mate is still connected.
    eventually("Beta to see Alpha paused and connected", || async {
        status(&nodes[1]).await.tray_summary.connection == TrayConnection::Connected
    })
    .await;
}

#[tokio::test]
async fn a_member_that_is_not_connected_is_disconnected_also_while_paused() {
    let lab = Lab::new();
    let mut nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
    let gamma = nodes.pop().unwrap();
    gamma.engine.shutdown().await;
    let alpha = &nodes[0];
    eventually("the loss to show", || async {
        connection(alpha).await == TrayConnection::Disconnected
    })
    .await;
    let _: mouser_proto::control::Empty = call(alpha, Method::ActivePause, json!({})).await;
    let s = status(alpha).await;
    assert_eq!(s.tray_summary.state, EngineState::Paused);
    assert_eq!(s.tray_summary.connection, TrayConnection::Disconnected);
}

#[tokio::test]
async fn a_member_the_owner_removed_never_counts() {
    let lab = Lab::new();
    let mut nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let beta = nodes.pop().unwrap();
    let gone = beta.engine.computer_id();
    beta.engine.shutdown().await;
    eventually("the loss to show", || async {
        connection(&nodes[0]).await == TrayConnection::Disconnected
    })
    .await;
    let _: mouser_proto::control::Empty = call(
        &nodes[0],
        Method::DeskRemoveMember,
        json!({"computer_id": gone}),
    )
    .await;
    assert_eq!(connection(&nodes[0]).await, TrayConnection::Alone);
}

#[tokio::test]
async fn leaving_the_desk_is_alone() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let _: mouser_proto::control::Empty = call(&nodes[1], Method::DeskLeave, json!({})).await;
    assert_eq!(connection(&nodes[1]).await, TrayConnection::Alone);
}
