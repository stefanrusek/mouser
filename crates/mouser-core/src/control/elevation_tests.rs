use mouser_proto::control::{Empty, Method, ProblemKind};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ComputerMode;
use serde_json::json;
use tokio::sync::broadcast::Receiver;

use crate::EngineEvent;
use crate::testing::*;

async fn restart(node: &TestNode, computer: ComputerId) -> Result<serde_json::Value, ()> {
    call_raw(
        node,
        Method::ElevationRestart,
        json!({"computer": computer}),
    )
    .await
    .map_err(|e| {
        assert_eq!(e.code, -32001);
        assert_eq!(e.data.unwrap().kind, ProblemKind::PlatformLimitation);
    })
}

/// Wait for `RestartElevated` among the events of `events`.
async fn restart_event(events: &mut Receiver<EngineEvent>) {
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        while events.recv().await.unwrap() != EngineEvent::RestartElevated {}
    })
    .await
    .expect("RestartElevated arrives in time");
}

/// Whether `events` holds a `RestartElevated`, after in-flight messages
/// had time to arrive.
async fn no_restart_event(events: &mut Receiver<EngineEvent>) -> bool {
    settle().await;
    while let Ok(event) = events.try_recv() {
        if event == EngineEvent::RestartElevated {
            return false;
        }
    }
    true
}

async fn pair(
    lab: &Lab,
    caller: (ComputerMode, &str),
    target: (ComputerMode, &str),
) -> Vec<TestNode> {
    let caller = lab.start_windows("Alpha", caller.0, caller.1).await;
    let target = lab.start_windows("Beta", target.0, target.1).await;
    desk_from(vec![caller, target]).await
}

#[tokio::test]
async fn for_this_computer_it_emits_the_event() {
    let lab = Lab::new();
    let nodes = pair(
        &lab,
        (ComputerMode::Portable, "0.3.0"),
        (ComputerMode::Installed, "0.3.0"),
    )
    .await;
    let mut events = nodes[0].engine.events();
    let result = restart(&nodes[0], nodes[0].engine.computer_id())
        .await
        .unwrap();
    assert_eq!(result, json!(Empty {}));
    restart_event(&mut events).await;
}

#[tokio::test]
async fn for_a_peer_of_0_3_it_sends_the_wire_message_and_not_the_event() {
    let lab = Lab::new();
    let nodes = pair(
        &lab,
        (ComputerMode::Installed, "0.3.0"),
        (ComputerMode::Portable, "0.3.0"),
    )
    .await;
    let mut caller = nodes[0].engine.events();
    let mut target = nodes[1].engine.events();
    // The peer's mode arrives with its status.
    let beta = nodes[1].engine.computer_id();
    eventually("Beta's mode", || async {
        status(&nodes[0])
            .await
            .computers
            .iter()
            .any(|c| c.id == beta && c.mode == Some(ComputerMode::Portable))
    })
    .await;
    let result = restart(&nodes[0], beta).await.unwrap();
    assert_eq!(result, json!(Empty {}));
    restart_event(&mut target).await;
    assert!(no_restart_event(&mut caller).await);
}

#[tokio::test]
async fn it_is_refused_for_a_computer_that_is_not_portable_windows() {
    let lab = Lab::new();
    // This computer: installed, or elevated, is not portable.
    for mode in [ComputerMode::Installed, ComputerMode::PortableElevated] {
        let nodes = pair(&lab, (mode, "0.3.0"), (ComputerMode::Portable, "0.3.0")).await;
        let mut events = nodes[0].engine.events();
        restart(&nodes[0], nodes[0].engine.computer_id())
            .await
            .unwrap_err();
        assert!(no_restart_event(&mut events).await);
        for node in nodes {
            node.engine.shutdown().await;
        }
    }
}

#[tokio::test]
async fn it_is_refused_for_a_peer_that_is_not_portable_windows_of_0_3() {
    for (mode, version) in [
        (ComputerMode::Installed, "0.3.0"),
        (ComputerMode::PortableElevated, "0.3.0"),
        (ComputerMode::Portable, "0.2.8"),
    ] {
        let lab = Lab::new();
        let nodes = pair(&lab, (ComputerMode::Installed, "0.3.0"), (mode, version)).await;
        let mut target = nodes[1].engine.events();
        let beta = nodes[1].engine.computer_id();
        eventually("Beta's status", || async {
            status(&nodes[0]).await.computers.len() == 2 && connected(&nodes[0]).await == 1
        })
        .await;
        restart(&nodes[0], beta).await.unwrap_err();
        assert!(no_restart_event(&mut target).await);
        for node in nodes {
            node.engine.shutdown().await;
        }
    }
}

#[tokio::test]
async fn it_is_refused_for_a_peer_that_is_not_windows() {
    let lab = Lab::new();
    let alpha = lab
        .start_windows("Alpha", ComputerMode::Installed, "0.3.0")
        .await;
    let beta = lab.start_version("Beta", "0.3.0", true).await;
    let nodes = desk_from(vec![alpha, beta]).await;
    restart(&nodes[0], nodes[1].engine.computer_id())
        .await
        .unwrap_err();
}

#[tokio::test]
async fn it_is_refused_for_a_computer_that_is_not_on_the_desk() {
    let lab = Lab::new();
    let nodes = pair(
        &lab,
        (ComputerMode::Installed, "0.3.0"),
        (ComputerMode::Portable, "0.3.0"),
    )
    .await;
    restart(&nodes[0], ComputerId([9; 16])).await.unwrap_err();
}

#[tokio::test]
async fn it_needs_a_computer() {
    let lab = Lab::new();
    let node = lab
        .start_windows("Alpha", ComputerMode::Portable, "0.3.0")
        .await;
    let err = call_err(&node, Method::ElevationRestart, json!({})).await;
    assert_eq!(err.code, -32602);
}
