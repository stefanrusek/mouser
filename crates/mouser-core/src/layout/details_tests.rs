//! Display details in `layout.get` (Story 4 scenario 2).

use mouser_proto::control::{ConnectionState, Method};
use serde_json::{Value, json};

use crate::testing::*;

fn detail<'a>(layout: &'a Value, computer: &str) -> &'a Value {
    layout["displays"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["computer_name"] == computer)
        .unwrap_or_else(|| panic!("no display of {computer}"))
}

#[tokio::test]
async fn each_display_carries_resolution_scale_owner_and_online_state() {
    let lab = Lab::new();
    let beta = mouser_testkit::VirtualPlatform::new();
    let mut retina = screen("retina", (0.0, 0.0));
    retina.pixel_size = (3840, 2160);
    retina.logical_size = (1920.0, 1080.0);
    retina.scale = 2.0;
    beta.set_displays(vec![retina]);
    let a = lab.start("Alpha").await;
    let b = lab.start_with("Beta", beta, |_| {}).await;
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home"})).await;
    admit(&a, &b).await;
    eventually("both computers' displays known", || async {
        let layout = layout_of(&a).await;
        let displays = layout["displays"].as_array().unwrap();
        ["Alpha", "Beta"]
            .iter()
            .all(|name| displays.iter().any(|d| d["computer_name"] == *name))
    })
    .await;
    let layout = layout_of(&a).await;
    let own = detail(&layout, "Alpha");
    assert_eq!(own["pixel_size"], json!([1920, 1080]));
    assert_eq!(own["scale"], 1.0);
    assert_eq!(own["online"], true);
    assert_eq!(own["included"], true);
    assert_eq!(own["last_seen"], Value::Null);
    let theirs = detail(&layout, "Beta");
    assert_eq!(theirs["pixel_size"], json!([3840, 2160]));
    assert_eq!(theirs["scale"], 2.0);
    assert_eq!(theirs["display_id"]["key"], "retina");
    assert_eq!(theirs["online"], true);
    assert_eq!(theirs["last_seen"], Value::Null);
    // Beta sees Alpha the same way.
    let seen = layout_of(&b).await;
    assert_eq!(detail(&seen, "Alpha")["pixel_size"], json!([1920, 1080]));
}

#[tokio::test]
async fn an_unreachable_computer_shows_when_it_was_last_seen() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    nodes[1].sim.as_ref().unwrap().set_blackhole(true);
    let seen = wait_connection(&nodes[0], &nodes[1], ConnectionState::Reconnecting).await;
    let layout = layout_of(&nodes[0]).await;
    let theirs = detail(&layout, "Beta");
    assert_eq!(theirs["online"], false);
    assert_eq!(theirs["last_seen"], json!(seen.last_seen.unwrap()));
    assert_eq!(detail(&layout, "Alpha")["online"], true);
}

#[tokio::test]
async fn a_computer_that_went_away_shows_when_it_was_last_seen_too() {
    let lab = Lab::new();
    let mut nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    nodes.pop().unwrap().engine.shutdown().await;
    let a = &nodes[0];
    eventually("Beta to show as away", || async {
        detail(&layout_of(a).await, "Beta")["online"] == false
    })
    .await;
    let layout = layout_of(a).await;
    assert!(detail(&layout, "Beta")["last_seen"].is_string());
    // Its displays stay on the desk meanwhile.
    assert_eq!(layout["displays"].as_array().unwrap().len(), 2);
}

fn needs_review(layout: &Value, computer: &str) -> Value {
    detail(layout, computer)["needs_review"].clone()
}

#[tokio::test]
async fn each_display_says_whether_its_placement_awaits_review() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let layout = layout_of(&nodes[0]).await;
    // Beta was placed automatically; Alpha's own display was never moved.
    assert_eq!(needs_review(&layout, "Beta"), json!(true));
    assert_eq!(needs_review(&layout, "Alpha"), json!(false));
}

#[tokio::test]
async fn placing_or_moving_a_display_with_layout_set_clears_only_its_flag() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
    let a = &nodes[0];
    let layout = layout_of(a).await;
    assert_eq!(needs_review(&layout, "Beta"), json!(true));
    assert_eq!(needs_review(&layout, "Gamma"), json!(true));
    // Gamma goes below Alpha; Beta stays where it was put.
    let alpha = a
        .engine
        .layout()
        .placements
        .into_iter()
        .find(|p| p.display_id.computer == a.engine.computer_id())
        .unwrap();
    let gamma = nodes[2].engine.computer_id().to_string();
    let mut next = layout.clone();
    for p in next["placements"].as_array_mut().unwrap() {
        if p["display_id"]["computer"] == gamma.as_str() {
            p["x"] = json!(alpha.x);
            p["y"] = json!(alpha.y + 296);
        }
    }
    let _: mouser_proto::control::Empty = call(a, Method::LayoutSet, next).await;
    let layout = layout_of(a).await;
    assert_eq!(needs_review(&layout, "Gamma"), json!(false));
    assert_eq!(needs_review(&layout, "Beta"), json!(true));
    assert_eq!(a.engine.needs_review().len(), 1);
}
