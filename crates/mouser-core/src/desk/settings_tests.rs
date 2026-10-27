use mouser_proto::control::{Empty, Method};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ControlMessage;
use serde_json::json;

use crate::testing::*;

/// A desk of 0.3.0 computers with logging on.
async fn desk(lab: &Lab, names: &[&str]) -> Vec<TestNode> {
    let mut nodes = Vec::new();
    for name in names {
        nodes.push(lab.start_version(name, "0.3.0", true).await);
    }
    desk_from(nodes).await
}

async fn set(node: &TestNode, key: &str, value: bool) {
    let _: Empty = call(node, Method::ConfigSet, json!({"key": key, "value": value})).await;
}

async fn get(node: &TestNode, key: &str) -> bool {
    call(node, Method::ConfigGet, json!({"key": key})).await
}

const LANDING: &str = "crossing.landing_highlight";
const COMMAND: &str = "keyboard.command_as_control";

fn settings(epoch: u64, author: ComputerId, landing: bool, command: bool) -> ControlMessage {
    ControlMessage::DeskSettings {
        epoch,
        author,
        landing_highlight: landing,
        command_as_control: command,
    }
}

#[tokio::test]
async fn a_change_reaches_every_computer_and_is_logged_as_from_a_desk_mate() {
    let lab = Lab::new();
    let nodes = desk(&lab, &["Alpha", "Beta", "Gamma"]).await;
    let alpha = nodes[0].engine.computer_id();
    set(&nodes[0], LANDING, false).await;
    for node in &nodes[1..] {
        eventually("the setting to arrive", || async {
            !get(node, LANDING).await
        })
        .await;
        let record = logged(node, "setting.changed", |r| {
            r["fields"]["key"] == LANDING && r["fields"]["origin"] == "desk_mate"
        })
        .await;
        assert_eq!(record["fields"]["from"], alpha.to_string());
        assert_eq!(record["fields"]["old"], true);
        assert_eq!(record["fields"]["new"], false);
        // It is the file that holds it, too.
        let saved = crate::config::load(node.dir.path()).config;
        assert!(!saved.crossing.landing_highlight);
        assert!(saved.keyboard.command_as_control);
    }
    // The author logs its own change as its own.
    let own = logged(&nodes[0], "setting.changed", |r| {
        r["fields"]["key"] == LANDING
    })
    .await;
    assert_eq!(own["fields"]["origin"], "cli");
}

#[tokio::test]
async fn changes_from_different_computers_converge_everywhere() {
    let lab = Lab::new();
    let nodes = desk(&lab, &["Alpha", "Beta"]).await;
    set(&nodes[0], LANDING, false).await;
    eventually("Beta to have it", || async {
        !get(&nodes[1], LANDING).await
    })
    .await;
    // Beta's change comes after what it adopted, so it wins over it.
    set(&nodes[1], COMMAND, false).await;
    eventually("Alpha to have it", || async {
        !get(&nodes[0], COMMAND).await
    })
    .await;
    for node in &nodes {
        assert!(!get(node, LANDING).await);
        assert!(!get(node, COMMAND).await);
    }
}

#[tokio::test]
async fn the_higher_epoch_then_author_wins() {
    let lab = Lab::new();
    let nodes = desk(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let lowest = ComputerId([0; 16]);
    set(a, LANDING, false).await;
    eventually("Beta to have it", || async { !get(b, LANDING).await }).await;
    // Alpha now holds epoch 1 by itself. The same epoch from a lower author
    // loses; so does an older one from any author.
    send_to(b, a, settings(1, lowest, true, false));
    send_to(b, a, settings(0, ComputerId([255; 16]), true, false));
    settle().await;
    assert!(!get(a, LANDING).await);
    assert!(get(a, COMMAND).await);
    // A higher epoch wins, whoever wrote it.
    send_to(b, a, settings(2, lowest, true, false));
    eventually("the newer settings", || async {
        get(a, LANDING).await && !get(a, COMMAND).await
    })
    .await;
    // The same epoch from a higher author wins as well.
    send_to(b, a, settings(2, ComputerId([255; 16]), false, true));
    eventually("the higher author", || async {
        !get(a, LANDING).await && get(a, COMMAND).await
    })
    .await;
    // The same settings again change nothing (and say nothing).
    let said = log_events(a, "setting.changed").len();
    send_to(b, a, settings(2, ComputerId([255; 16]), false, true));
    settle().await;
    assert_eq!(log_events(a, "setting.changed").len(), said);
}

#[tokio::test]
async fn a_new_connection_is_sent_the_current_settings() {
    let lab = Lab::new();
    let mut nodes = desk(&lab, &["Alpha", "Beta"]).await;
    let b = nodes.pop().unwrap();
    let (platform, dir) = (b.platform.clone(), b.dir);
    b.engine.shutdown().await;
    set(&nodes[0], COMMAND, false).await;
    let b = lab
        .boot_with("Beta", platform, dir, |p| p.mouser_version = "0.3.0".into())
        .await;
    eventually("Beta to hear of the change after it reconnects", || async {
        !get(&b, COMMAND).await
    })
    .await;
}

#[tokio::test]
async fn nothing_is_sent_while_nothing_was_ever_changed() {
    let lab = Lab::new();
    let alpha = lab.start_version("Alpha", "0.3.0", true).await;
    // Beta starts with a setting of its own; a desk where nobody has
    // changed anything does not overrule it.
    let beta = lab
        .start_released(
            "Beta",
            |c| c.crossing.landing_highlight = false,
            |p| p.mouser_version = "0.3.0".into(),
        )
        .await;
    let nodes = desk_from(vec![alpha, beta]).await;
    settle().await;
    assert!(get(&nodes[0], LANDING).await);
    assert!(!get(&nodes[1], LANDING).await);
}

#[tokio::test]
async fn a_computer_older_than_0_3_0_is_never_sent_them() {
    let lab = Lab::new();
    let old = lab.start_version("Gamma", "0.2.8", true).await;
    let new_a = lab.start_version("Alpha", "0.3.0", true).await;
    let new_b = lab.start_version("Beta", "0.3.0", true).await;
    let nodes = desk_from(vec![new_a, new_b, old]).await;
    set(&nodes[0], LANDING, false).await;
    set(&nodes[0], COMMAND, false).await;
    eventually("Beta to have them", || async {
        !get(&nodes[1], LANDING).await && !get(&nodes[1], COMMAND).await
    })
    .await;
    settle().await;
    // Gamma runs this code but is told it is 0.2.8: had it been sent the
    // settings it would have adopted them.
    assert!(get(&nodes[2], LANDING).await);
    assert!(get(&nodes[2], COMMAND).await);
    assert_eq!(connected(&nodes[2]).await, 2);
}

#[tokio::test]
async fn the_version_of_the_settings_survives_a_restart() {
    let lab = Lab::new();
    let mut nodes = desk(&lab, &["Alpha", "Beta"]).await;
    set(&nodes[0], LANDING, false).await;
    eventually("Beta to have it", || async {
        !get(&nodes[1], LANDING).await
    })
    .await;
    let a = nodes.remove(0);
    let (platform, dir) = (a.platform.clone(), a.dir);
    a.engine.shutdown().await;
    let a = lab
        .boot_with("Alpha", platform, dir, |p| {
            p.mouser_version = "0.3.0".into()
        })
        .await;
    eventually("Alpha and Beta to meet again", || async {
        connected(&a).await == 1
    })
    .await;
    // Had Alpha forgotten its epoch this would be the same one again, which
    // Beta already holds.
    set(&a, COMMAND, false).await;
    eventually("Beta to have the second change", || async {
        !get(&nodes[0], COMMAND).await
    })
    .await;
}

#[tokio::test]
async fn leaving_the_desk_forgets_the_version() {
    let lab = Lab::new();
    let nodes = desk(&lab, &["Alpha", "Beta"]).await;
    set(&nodes[0], LANDING, false).await;
    let version = |node: &TestNode| node.engine.ctx.core.lock().unwrap().state.settings;
    assert_eq!(version(&nodes[0]).epoch, 1);
    let _: Empty = call(&nodes[0], Method::DeskLeave, json!({})).await;
    assert_eq!(version(&nodes[0]), crate::state::SettingsVersion::default());
}

#[tokio::test]
async fn settings_from_a_computer_that_is_not_on_the_desk_are_a_protocol_violation() {
    let lab = Lab::new();
    let nodes = desk(&lab, &["Alpha", "Beta"]).await;
    let stranger = ComputerId([77; 16]);
    let err = nodes[0]
        .engine
        .ctx
        .on_message(stranger, settings(9, stranger, false, false))
        .unwrap_err();
    assert!(err.contains("not on the desk"), "{err}");
    assert!(get(&nodes[0], LANDING).await);
}

#[tokio::test]
async fn off_a_desk_a_change_is_only_this_computers_and_has_no_version() {
    let lab = Lab::new();
    let node = lab.start_version("Alpha", "0.3.0", true).await;
    set(&node, LANDING, false).await;
    assert!(!get(&node, LANDING).await);
    assert_eq!(
        node.engine.ctx.core.lock().unwrap().state.settings,
        crate::state::SettingsVersion::default()
    );
}

#[tokio::test]
async fn a_computer_starts_with_the_settings_in_its_file() {
    let lab = Lab::new();
    let node = lab
        .start_with("Alpha", mouser_testkit::VirtualPlatform::new(), |c| {
            c.crossing.landing_highlight = false;
            c.keyboard.command_as_control = false;
        })
        .await;
    let crossing = node.engine.ctx.crossing.lock().unwrap();
    assert!(!crossing.landing_highlight);
    assert!(!crossing.command_as_control);
}
