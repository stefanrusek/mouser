use mouser_platform::Os;
use mouser_proto::wire::{ComputerMode, ControlMessage};

use crate::EngineEvent;
use crate::testing::*;

/// Alpha asks Beta (a Windows computer running as `mode`) to restart as
/// administrator; returns the events Beta raised once the message arrived.
async fn asked(mode: ComputerMode) -> Vec<EngineEvent> {
    let lab = Lab::new();
    let alpha = lab
        .start_windows("Alpha", ComputerMode::Installed, "0.3.0")
        .await;
    let beta = lab.start_windows("Beta", mode, "0.3.0").await;
    let nodes = desk_from(vec![alpha, beta]).await;
    let mut events = nodes[1].engine.events();
    let link = link_of(&nodes[0], &nodes[1]);
    send_to(&nodes[0], &nodes[1], ControlMessage::RestartElevated);
    settle().await;
    // Handling it is not a protocol violation, which would end the link.
    assert_eq!(link_of(&nodes[0], &nodes[1]), link);
    assert_eq!(connected(&nodes[0]).await, 1);
    assert_eq!(connected(&nodes[1]).await, 1);
    std::iter::from_fn(|| events.try_recv().ok()).collect()
}

#[tokio::test]
async fn portable_windows_that_is_not_elevated_raises_the_event() {
    let events = asked(ComputerMode::Portable).await;
    assert_eq!(
        events
            .iter()
            .filter(|e| **e == EngineEvent::RestartElevated)
            .count(),
        1
    );
}

#[tokio::test]
async fn any_other_computer_ignores_it_and_the_link_stays_up() {
    for mode in [
        ComputerMode::Installed,
        ComputerMode::PortableElevated,
        ComputerMode::Service,
        ComputerMode::Headless,
    ] {
        let events = asked(mode).await;
        assert!(!events.contains(&EngineEvent::RestartElevated), "{mode:?}");
    }
}

#[tokio::test]
async fn a_computer_that_is_not_windows_ignores_it() {
    let lab = Lab::new();
    let alpha = lab
        .start_windows("Alpha", ComputerMode::Installed, "0.3.0")
        .await;
    let beta = lab
        .start_as("Beta", Os::Linux, ComputerMode::Portable, "0.3.0")
        .await;
    let nodes = desk_from(vec![alpha, beta]).await;
    let mut events = nodes[1].engine.events();
    send_to(&nodes[0], &nodes[1], ControlMessage::RestartElevated);
    settle().await;
    assert_eq!(connected(&nodes[1]).await, 1);
    assert!(events.try_recv().is_err());
}
