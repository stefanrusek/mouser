use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ControlMessage;

use crate::testing::*;

/// A v2 message arriving from a desk-mate is ignored and the link stays up.
async fn ignored(msg: ControlMessage) {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let link = link_of(&nodes[0], &nodes[1]);
    // Handling it is not a protocol violation (which would end the link).
    assert_eq!(
        nodes[0]
            .engine
            .ctx
            .on_message(nodes[1].engine.computer_id(), msg.clone()),
        Ok(())
    );
    send_to(&nodes[1], &nodes[0], msg);
    settle().await;
    assert_eq!(link_of(&nodes[0], &nodes[1]), link);
    assert_eq!(connected(&nodes[0]).await, 1);
    assert_eq!(connected(&nodes[1]).await, 1);
}

// v2: replaced in T159
#[tokio::test]
async fn an_inbound_logs_request_is_ignored_and_the_link_stays_up() {
    ignored(ControlMessage::LogsRequest { request: [7; 16] }).await;
}

// v2: replaced in T205
#[tokio::test]
async fn an_inbound_desk_settings_is_ignored_and_the_link_stays_up() {
    ignored(ControlMessage::DeskSettings {
        epoch: 1,
        author: ComputerId([1; 16]),
        landing_highlight: true,
        command_as_control: false,
    })
    .await;
}

// v2: replaced in T226
#[tokio::test]
async fn an_inbound_restart_elevated_is_ignored_and_the_link_stays_up() {
    ignored(ControlMessage::RestartElevated).await;
}
