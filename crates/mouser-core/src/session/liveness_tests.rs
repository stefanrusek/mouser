//! Connection states of desk-mates (data-model "ConnectionState", research
//! R2) and what the desk does about an unreachable one.

use std::time::{Duration, Instant};

use mouser_proto::control::{ConnectionState, Empty, Method, ProblemKind};
use serde_json::json;

use super::liveness::OFFLINE_GRACE;
use crate::testing::*;
use crate::transport::{KEEP_ALIVE, MAX_IDLE};

fn offline_problem(
    status: &mouser_proto::control::Status,
) -> Option<&mouser_proto::control::Problem> {
    status
        .problems
        .iter()
        .find(|p| p.kind == ProblemKind::PeerOffline)
}

#[tokio::test]
async fn silence_for_the_idle_timeout_is_reconnecting() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let started = Instant::now();
    b.sim.as_ref().unwrap().set_blackhole(true);
    // The idle timeout counts from the last packet received, sent up to one
    // keep-alive interval before the blackhole began, so it may fire that early.
    let seen = wait_connection(a, b, ConnectionState::Reconnecting).await;
    let took = started.elapsed();
    assert!(
        took >= MAX_IDLE - KEEP_ALIVE && took < Duration::from_secs(4),
        "declared lost after {took:?}"
    );
    assert!(seen.last_seen.is_some(), "it says when it was last seen");
    assert_eq!(seen.rtt_ms, None);
    // The other side notices the same silence.
    wait_connection(b, a, ConnectionState::Reconnecting).await;
}

#[tokio::test]
async fn rediscovery_or_reconnection_returns_a_lost_computer_to_connected() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    b.sim.as_ref().unwrap().set_blackhole(true);
    wait_connection(a, b, ConnectionState::Reconnecting).await;
    wait_connection(b, a, ConnectionState::Reconnecting).await;

    let back = Instant::now();
    b.sim.as_ref().unwrap().set_blackhole(false);
    let seen = wait_connection(a, b, ConnectionState::Connected).await;
    assert!(
        back.elapsed() < Duration::from_secs(5),
        "{:?}",
        back.elapsed()
    );
    assert!(seen.rtt_ms.is_some());
    wait_connection(b, a, ConnectionState::Connected).await;
    // It was back inside the grace period: nobody was told it was offline.
    assert!(offline_problem(&status(a).await).is_none());
}

#[tokio::test]
async fn longer_absence_is_offline_with_a_problem_until_it_returns() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let started = Instant::now();
    b.sim.as_ref().unwrap().set_blackhole(true);
    // The idle timeout counts from the last packet that arrived, which
    // was sent up to one keep-alive interval before the blackhole began.
    let seen = wait_connection(a, b, ConnectionState::Offline).await;
    let took = started.elapsed();
    assert!(
        took >= MAX_IDLE + OFFLINE_GRACE - KEEP_ALIVE && took < Duration::from_secs(10),
        "offline after {took:?}"
    );
    assert!(seen.last_seen.is_some());
    eventually("the peer_offline problem", || async {
        offline_problem(&status(a).await).is_some()
    })
    .await;
    let current = status(a).await;
    let problem = offline_problem(&current).expect("a peer_offline problem");
    assert_eq!(problem.computer, b.engine.computer_id());
    assert!(problem.message.contains("Beta"), "{}", problem.message);
    assert_eq!(problem.cleared_at, None);

    // Back on the network: connected again and the problem is gone.
    b.sim.as_ref().unwrap().set_blackhole(false);
    wait_connection(a, b, ConnectionState::Connected).await;
    assert!(offline_problem(&status(a).await).is_none());
}

#[tokio::test]
async fn crossing_toward_an_unreachable_computer_leaves_the_pointer_where_it_is() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    b.sim.as_ref().unwrap().set_blackhole(true);
    wait_connection(a, b, ConnectionState::Offline).await;
    push_right(a);
    push_right(a);
    settle().await;
    assert!(!a.platform.is_grabbed(), "the mouse is still Alpha's");
    assert!(a.platform.warps().is_empty(), "the pointer did not move");
    assert_eq!(status(a).await.active_computer, a.engine.computer_id());
}

#[tokio::test]
async fn a_computer_removed_from_the_desk_is_no_longer_reported_offline() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
    let (a, c) = (&nodes[0], &nodes[2]);
    c.sim.as_ref().unwrap().set_blackhole(true);
    wait_connection(a, c, ConnectionState::Offline).await;
    eventually("the peer_offline problem", || async {
        offline_problem(&status(a).await).is_some()
    })
    .await;
    let _: Empty = call(
        a,
        Method::DeskRemoveMember,
        json!({ "computer_id": c.engine.computer_id() }),
    )
    .await;
    eventually("the problem to be cleared", || async {
        offline_problem(&status(a).await).is_none()
    })
    .await;
    assert!(a.engine.ctx.core.lock().unwrap().absent.is_empty());
}

#[tokio::test]
async fn a_computer_that_left_is_not_reconnecting() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let _: Empty = call(b, Method::DeskLeave, json!({})).await;
    eventually("Alpha to forget Beta", || async {
        status(a).await.computers.len() == 1
    })
    .await;
    assert!(a.engine.ctx.core.lock().unwrap().absent.is_empty());
}
