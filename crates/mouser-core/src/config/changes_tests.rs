//! Setting changes (Story 2 scenario 8, data-model "Setting change"): after
//! any config apply, one change per key that differs, to the `config` topic
//! and to the log, with where it came from.

use mouser_proto::config::Config;
use mouser_proto::control::{Empty, Method, SettingChange, SettingOrigin, Topic};
use mouser_proto::ids::ComputerId;
use serde_json::{Value, json};

use super::changes::diff;
use crate::testing::*;

const FROM: ComputerId = ComputerId([5; 16]);

#[test]
fn one_change_for_every_key_that_differs_in_key_order() {
    let old = Config::default();
    let mut new = old.clone();
    new.general.computer_name = "Studio".into();
    new.crossing.block_fullscreen = false;
    new.advanced.allowlist = vec!["10.0.0.0/8".into()];
    let changes = diff(&old, &new, SettingOrigin::App, None);
    assert_eq!(
        changes,
        [
            SettingChange {
                key: "advanced.allowlist".into(),
                old: json!([]),
                new: json!(["10.0.0.0/8"]),
                origin: SettingOrigin::App,
                from: None,
            },
            SettingChange {
                key: "crossing.block_fullscreen".into(),
                old: json!(true),
                new: json!(false),
                origin: SettingOrigin::App,
                from: None,
            },
            SettingChange {
                key: "general.computer_name".into(),
                old: json!(""),
                new: json!("Studio"),
                origin: SettingOrigin::App,
                from: None,
            },
        ]
    );
}

#[test]
fn nothing_differs_nothing_changes_and_a_desk_mate_is_named() {
    let config = Config::default();
    assert_eq!(diff(&config, &config, SettingOrigin::Cli, None), []);
    let mut next = config.clone();
    next.crossing.equalize_dimensions = true;
    let changes = diff(&config, &next, SettingOrigin::DeskMate, Some(FROM));
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].origin, SettingOrigin::DeskMate);
    assert_eq!(changes[0].from, Some(FROM));
}

/// The next `config` notification.
async fn change(
    notes: &mut tokio::sync::broadcast::Receiver<mouser_proto::control::Notification>,
) -> SettingChange {
    serde_json::from_value(recv_topic(notes, Topic::Config).await).unwrap()
}

#[tokio::test]
async fn config_set_publishes_the_change_and_logs_it() {
    let lab = Lab::new();
    let node = lab.start_logged("Alpha").await;
    let mut notes = node.engine.control().notifications();
    let _: Empty = call(
        &node,
        Method::ConfigSet,
        json!({"key": "mouse.buttons_4_5", "value": "back_forward"}),
    )
    .await;
    let published = change(&mut notes).await;
    assert_eq!(
        published,
        SettingChange {
            key: "mouse.buttons_4_5".into(),
            old: json!("passthrough"),
            new: json!("back_forward"),
            origin: SettingOrigin::Cli,
            from: None,
        }
    );
    let record = logged(&node, "setting.changed", |r| {
        r["fields"]["key"] == "mouse.buttons_4_5"
    })
    .await;
    assert_eq!(record["level"], "info");
    assert_eq!(
        record["fields"],
        json!({
            "key": "mouse.buttons_4_5", "old": "passthrough",
            "new": "back_forward", "origin": "cli"
        })
    );
    // Setting it to what it is changes nothing.
    let _: Empty = call(
        &node,
        Method::ConfigSet,
        json!({"key": "mouse.buttons_4_5", "value": "back_forward"}),
    )
    .await;
    assert_eq!(
        log_events(&node, "setting.changed")
            .iter()
            .filter(|r| r["fields"]["key"] == "mouse.buttons_4_5")
            .count(),
        1
    );
}

#[tokio::test]
async fn turning_logging_on_is_a_change_that_is_logged_once_logging_is_on() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let _: Empty = call(
        &node,
        Method::ConfigSet,
        json!({"key": "diagnostics.logging", "value": true}),
    )
    .await;
    let record = logged(&node, "setting.changed", |r| {
        r["fields"]["key"] == "diagnostics.logging"
    })
    .await;
    assert_eq!(record["fields"]["old"], false);
    assert_eq!(record["fields"]["new"], true);
}

#[tokio::test]
async fn a_shared_setting_adopted_from_a_desk_mate_is_a_change_from_that_computer() {
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let mut notes = b.engine.control().notifications();
    let _: Empty = call(
        a,
        Method::ConfigSet,
        json!({"key": "crossing.block_fullscreen", "value": false, "origin": "app"}),
    )
    .await;
    let published = change(&mut notes).await;
    assert_eq!(published.key, "crossing.block_fullscreen");
    assert_eq!(published.old, json!(true));
    assert_eq!(published.new, json!(false));
    assert_eq!(published.origin, SettingOrigin::DeskMate);
    assert_eq!(published.from, Some(a.engine.computer_id()));
    let record = logged(b, "setting.changed", |r| {
        r["fields"]["key"] == "crossing.block_fullscreen"
    })
    .await;
    assert_eq!(
        record["fields"],
        json!({
            "key": "crossing.block_fullscreen", "old": true, "new": false,
            "origin": "desk_mate", "from": a.engine.computer_id().to_string()
        })
    );
    // On Alpha it was an app change.
    let mine: Value = logged(a, "setting.changed", |r| {
        r["fields"]["key"] == "crossing.block_fullscreen"
    })
    .await;
    assert_eq!(mine["fields"]["origin"], "app");
}

#[tokio::test]
async fn the_settings_of_a_desk_just_joined_are_changes_from_the_computer_that_admitted() {
    let lab = Lab::new();
    let a = lab.start_logged("Alpha").await;
    let b = lab.start_logged("Beta").await;
    let _: Empty = call(
        &a,
        Method::ConfigSet,
        json!({"key": "crossing.equalize_dimensions", "value": true}),
    )
    .await;
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    admit(&a, &b).await;
    let record = logged(&b, "setting.changed", |r| {
        r["fields"]["key"] == "crossing.equalize_dimensions"
    })
    .await;
    assert_eq!(record["fields"]["origin"], "desk_mate");
    assert_eq!(record["fields"]["from"], a.engine.computer_id().to_string());
}
