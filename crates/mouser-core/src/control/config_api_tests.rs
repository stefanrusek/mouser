//! `config.get` / `config.set` (Story 9 scenario 2, FR-005).

use mouser_proto::config::{Config, DESK_SHARED_KEYS};
use mouser_proto::control::{Empty, Method, Topic};
use serde_json::{Value, json};

use crate::crossing::buttons::Buttons45;
use crate::testing::*;

/// Every key of contracts/config.md, and a value other than its default
/// that is valid for it.
fn keys() -> Vec<(&'static str, Value)> {
    vec![
        ("general.start_at_login", json!(false)),
        ("general.notifications", json!(false)),
        ("general.computer_name", json!("Desk Mac")),
        ("hotkeys.enable_disable", json!("Ctrl+Alt+E")),
        ("hotkeys.quickjump", json!("Ctrl+Alt+Q")),
        ("hotkeys.targeted_paste", json!("Ctrl+Alt+P")),
        ("hotkeys.panic", json!("F5*3")),
        ("mouse.buttons_4_5", json!("back_forward")),
        ("crossing.block_fullscreen", json!(false)),
        ("crossing.equalize_dimensions", json!(true)),
        ("diagnostics.logging", json!(true)),
        ("diagnostics.max_log_mb", json!(10)),
        ("updates.automatic", json!(false)),
        ("updates.release_url", json!("http://localhost/latest.json")),
        ("advanced.port", json!(48000)),
        ("advanced.allowlist", json!(["10.0.0.0/8", "fd00::/8"])),
    ]
}

async fn get(node: &TestNode, key: &str) -> Value {
    call(node, Method::ConfigGet, json!({ "key": key })).await
}

async fn set(node: &TestNode, key: &str, value: Value) {
    let _: Empty = call(node, Method::ConfigSet, json!({"key": key, "value": value})).await;
}

fn epoch(node: &TestNode) -> u64 {
    node.engine
        .ctx
        .core
        .lock()
        .unwrap()
        .state
        .desk
        .as_ref()
        .unwrap()
        .state
        .epoch
}

#[tokio::test]
async fn every_key_is_readable_and_the_whole_config_comes_without_a_key() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let config: Config =
        serde_json::from_value(call::<Value>(&node, Method::ConfigGet, json!({})).await).unwrap();
    assert_eq!(config, node.engine.ctx.core.lock().unwrap().config);
    // No params at all is the whole config too.
    let all = node
        .engine
        .control()
        .call(Method::ConfigGet, None)
        .await
        .unwrap();
    assert_eq!(all["schema"], 1);
    let defaults = serde_json::to_value(Config::default()).unwrap();
    for (key, _) in keys() {
        let (section, name) = key.split_once('.').unwrap();
        let value = get(&node, key).await;
        if key == "advanced.port" {
            assert_eq!(value, all[section][name]);
        } else {
            assert_eq!(value, defaults[section][name], "{key}");
        }
    }
    // A whole section is a key too.
    assert_eq!(get(&node, "crossing").await, all["crossing"]);
    assert_eq!(get(&node, "schema").await, 1);
}

#[tokio::test]
async fn every_key_is_writable_and_persisted() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    for (key, value) in keys() {
        set(&node, key, value.clone()).await;
        assert_eq!(get(&node, key).await, value, "{key}");
    }
    // The file holds what was set, so a restart sees it.
    let saved = crate::config::load_or_create(node.dir.path());
    assert_eq!(saved, node.engine.ctx.core.lock().unwrap().config);
    assert_eq!(saved.advanced.port, 48000);
    assert_eq!(saved.advanced.allowlist, ["10.0.0.0/8", "fd00::/8"]);
    assert_eq!(saved.hotkeys.panic, "F5*3");
}

#[tokio::test]
async fn invalid_values_are_refused_as_key_colon_reason_and_change_nothing() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let before = node.engine.ctx.core.lock().unwrap().config.clone();
    let cases = [
        ("mouse.buttons_4_5", json!("sideways"), "must be"),
        ("advanced.port", json!(0), "between 1 and 65535"),
        ("advanced.port", json!(70000), "between 1 and 65535"),
        ("advanced.port", json!(-1), "invalid value"),
        (
            "advanced.allowlist",
            json!(["10.0.0.0/8", "nope"]),
            "not a CIDR",
        ),
        ("general.notifications", json!("yes"), "invalid type"),
        ("hotkeys.panic", json!("Escape"), "<key>*<count>"),
        ("hotkeys.panic", json!("Home*2"), "not a known key"),
        ("schema", json!(2), "read-only"),
        ("crossing", json!({}), "is a section"),
        ("nope.key", json!(1), "unknown key"),
        ("general.nope", json!(1), "unknown key"),
    ];
    for (key, value, reason) in cases {
        let error = call_err(
            &node,
            Method::ConfigSet,
            json!({"key": key, "value": value}),
        )
        .await;
        assert_eq!(error.code, -32602, "{key}");
        assert!(
            error.message.starts_with(&format!("{key}: ")) && error.message.contains(reason),
            "{key}: {}",
            error.message
        );
    }
    let error = call_err(&node, Method::ConfigGet, json!({"key": "nope.key"})).await;
    assert_eq!(error.code, -32602);
    assert_eq!(error.message, "nope.key: unknown key");
    let error = call_err(&node, Method::ConfigGet, json!({"key": "schema.x"})).await;
    assert_eq!(error.message, "schema.x: unknown key");
    let error = call_err(&node, Method::ConfigSet, json!({"key": "schema"})).await;
    assert_eq!(error.code, -32602);
    // The previous values are still in effect, in memory and on disk.
    assert_eq!(node.engine.ctx.core.lock().unwrap().config, before);
    assert_eq!(crate::config::load_or_create(node.dir.path()), before);
}

#[tokio::test]
async fn settings_the_engine_uses_take_effect_without_a_restart() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    assert_eq!(
        node.engine.ctx.crossing.lock().unwrap().buttons,
        Buttons45::Passthrough
    );
    set(&node, "mouse.buttons_4_5", json!("back_forward")).await;
    assert_eq!(
        node.engine.ctx.crossing.lock().unwrap().buttons,
        Buttons45::BackForward
    );
    set(&node, "mouse.buttons_4_5", json!("passthrough")).await;
    assert_eq!(
        node.engine.ctx.crossing.lock().unwrap().buttons,
        Buttons45::Passthrough
    );

    // The panic key is now F5 pressed three times.
    set(&node, "hotkeys.panic", json!("F5*3")).await;
    let f5 = 0x0007_003E;
    let now = std::time::Instant::now();
    {
        let mut crossing = node.engine.ctx.crossing.lock().unwrap();
        assert!(
            !crossing.panic.pressed(0x0007_0029, now),
            "Escape no longer"
        );
        assert!(!crossing.panic.pressed(f5, now));
        assert!(!crossing.panic.pressed(f5, now));
        assert!(crossing.panic.pressed(f5, now));
    }

    // The name shown to others follows, and is announced again.
    let announced = node.discovery.announcements().len();
    set(&node, "general.computer_name", json!("Desk Mac")).await;
    assert_eq!(node.engine.ctx.display_name(), "Desk Mac");
    let adverts = node.discovery.announcements();
    assert_eq!(adverts.len(), announced + 1);
    assert_eq!(adverts.last().unwrap().display_name, "Desk Mac");
}

#[tokio::test]
async fn start_at_login_is_applied_once_the_computer_is_on_a_desk() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    // Not on a desk: only the setting changes; first run applies it later.
    set(&node, "general.start_at_login", json!(false)).await;
    assert!(!node.platform.is_autostart_enabled());
    set(&node, "general.start_at_login", json!(true)).await;
    assert!(!node.platform.is_autostart_enabled());
    let _: mouser_proto::control::Desk =
        call(&node, Method::DeskCreate, json!({"name": "Home"})).await;
    assert!(node.platform.is_autostart_enabled());
    set(&node, "general.start_at_login", json!(false)).await;
    assert!(!node.platform.is_autostart_enabled());
}

#[tokio::test]
async fn desk_shared_keys_propagate_with_a_new_epoch() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    for (key, value) in keys()
        .into_iter()
        .filter(|(k, _)| DESK_SHARED_KEYS.contains(k))
    {
        let before = epoch(a);
        set(a, key, value.clone()).await;
        assert_eq!(epoch(a), before + 1, "{key}");
        eventually("the desk-mate to adopt the new state", || async {
            epoch(b) == before + 1
        })
        .await;
        assert_eq!(get(b, key).await, value, "{key}");
    }
    // The shared values are in the replicated state itself, and persisted.
    let state = a
        .engine
        .ctx
        .core
        .lock()
        .unwrap()
        .state
        .desk
        .clone()
        .unwrap()
        .state;
    assert!(!state.layout.block_fullscreen);
    assert!(state.layout.equalize_dimensions);
    assert_eq!(state.allowlist, ["10.0.0.0/8", "fd00::/8"]);
    let theirs = b
        .engine
        .ctx
        .core
        .lock()
        .unwrap()
        .state
        .desk
        .clone()
        .unwrap()
        .state;
    assert_eq!(theirs, state);
    assert!(
        !crate::config::load_or_create(b.dir.path())
            .crossing
            .block_fullscreen
    );
    // The desk-mate writing back wins with a higher epoch, as any edit does.
    set(b, "crossing.block_fullscreen", json!(true)).await;
    eventually("the first computer to adopt it", || async {
        epoch(a) == epoch(b)
    })
    .await;
    assert_eq!(get(a, "crossing.block_fullscreen").await, true);
}

#[tokio::test]
async fn settings_that_are_not_shared_stay_on_this_computer() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let before = epoch(a);
    set(a, "mouse.buttons_4_5", json!("back_forward")).await;
    set(a, "advanced.port", json!(48001)).await;
    assert_eq!(epoch(a), before);
    settle().await;
    assert_eq!(epoch(b), before);
    assert_eq!(get(b, "mouse.buttons_4_5").await, "passthrough");
}

#[tokio::test]
async fn a_computer_that_joins_takes_over_the_desks_shared_settings() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home"})).await;
    set(&a, "advanced.allowlist", json!(["127.0.0.0/8"])).await;
    set(&a, "crossing.equalize_dimensions", json!(true)).await;
    let b = lab.start("Beta").await;
    admit(&a, &b).await;
    assert_eq!(get(&b, "advanced.allowlist").await, json!(["127.0.0.0/8"]));
    assert_eq!(get(&b, "crossing.equalize_dimensions").await, true);
}

#[tokio::test]
async fn shared_settings_written_off_a_desk_seed_the_next_desk() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    set(&node, "crossing.block_fullscreen", json!(false)).await;
    let _: mouser_proto::control::Desk =
        call(&node, Method::DeskCreate, json!({"name": "Home"})).await;
    let state = node
        .engine
        .ctx
        .core
        .lock()
        .unwrap()
        .state
        .desk
        .clone()
        .unwrap()
        .state;
    assert!(!state.layout.block_fullscreen);
    assert_eq!(state.epoch, 1);
}

#[tokio::test]
async fn a_status_notification_reports_the_change() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut notes = node.engine.control().notifications();
    set(&node, "general.computer_name", json!("Renamed")).await;
    let status = recv_topic(&mut notes, Topic::Status).await;
    assert_eq!(status["computer"]["name"], "Renamed");
}

#[tokio::test]
async fn config_set_records_the_origin_it_was_given_and_cli_by_default() {
    let lab = Lab::new();
    let node = lab.start_logged("Alpha").await;
    let _: Empty = call(
        &node,
        Method::ConfigSet,
        json!({"key": "mouse.buttons_4_5", "value": "back_forward", "origin": "app"}),
    )
    .await;
    let _: Empty = call(
        &node,
        Method::ConfigSet,
        json!({"key": "general.notifications", "value": false}),
    )
    .await;
    let _: Empty = call(
        &node,
        Method::ConfigSet,
        json!({"key": "general.start_at_login", "value": false, "origin": "cli"}),
    )
    .await;
    for (key, origin) in [
        ("mouse.buttons_4_5", "app"),
        ("general.notifications", "cli"),
        ("general.start_at_login", "cli"),
    ] {
        let record = logged(&node, "setting.changed", |r| r["fields"]["key"] == key).await;
        assert_eq!(record["fields"]["origin"], origin, "{key}");
    }
}

#[tokio::test]
async fn the_logging_checkbox_is_a_change_made_in_the_app() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let _: mouser_proto::control::LoggingSetResult =
        call(&node, Method::LoggingSet, json!({"enabled": true})).await;
    let record = logged(&node, "setting.changed", |r| {
        r["fields"]["key"] == "diagnostics.logging"
    })
    .await;
    assert_eq!(record["fields"]["origin"], "app");
}

#[tokio::test]
async fn the_two_new_boolean_keys_refuse_anything_else_in_plain_words() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    for key in ["crossing.landing_highlight", "keyboard.command_as_control"] {
        let err = call_err(
            &node,
            Method::ConfigSet,
            json!({"key": key, "value": "yes"}),
        )
        .await;
        assert_eq!(err.code, -32602);
        assert_eq!(err.message, format!("{key}: must be true or false"));
        assert_eq!(get(&node, key).await, json!(true));
    }
}
