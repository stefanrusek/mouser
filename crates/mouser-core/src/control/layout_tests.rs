//! `layout.get` / `layout.set` (Story 4 scenario 1).

use mouser_proto::control::{Empty, Method};
use serde_json::{Value, json};

use crate::layout::edges;
use crate::testing::*;

fn sides(node: &TestNode) -> Vec<mouser_proto::model::Side> {
    let crossing = node.engine.ctx.crossing.lock().unwrap();
    let found = edges::edges(&crossing.map.placed);
    found.iter().map(|e| e.side).collect()
}

#[tokio::test]
async fn get_returns_the_desks_layout() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let got = layout_of(&nodes[0]).await;
    let layout: mouser_proto::model::Layout = serde_json::from_value(got).unwrap();
    assert_eq!(layout, nodes[0].engine.layout());
    assert_eq!(layout.placements.len(), 2);
}

#[tokio::test]
async fn get_off_a_desk_is_empty() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let got = layout_of(&node).await;
    assert_eq!(got["placements"], json!([]));
    assert_eq!(got["displays"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn set_bumps_the_epoch_and_every_member_derives_the_new_edges() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
    // Alpha | Beta | Gamma, left to right, to start with.
    assert_eq!(sides(&nodes[0]).len(), 4);
    let before = epoch_of(&nodes[0]);
    let layout = layout_of(&nodes[0]).await;
    let alpha_at = nodes[0]
        .engine
        .layout()
        .placements
        .into_iter()
        .find(|p| p.display_id.computer == nodes[0].engine.computer_id())
        .unwrap();
    // Gamma goes below Alpha instead of right of Beta.
    let gamma = nodes[2].engine.computer_id().to_string();
    let mut next = layout.clone();
    for p in next["placements"].as_array_mut().unwrap() {
        if p["display_id"]["computer"] == gamma.as_str() {
            p["x"] = json!(alpha_at.x);
            p["y"] = json!(alpha_at.y + 296);
        }
    }
    let _: Empty = call(&nodes[0], Method::LayoutSet, next.clone()).await;
    assert_eq!(epoch_of(&nodes[0]), before + 1);
    for node in &nodes {
        eventually("the new layout everywhere", || async {
            let l = node.engine.layout();
            epoch_of(node) == before + 1
                && l.placements
                    .iter()
                    .any(|p| p.y == alpha_at.y + 296 && p.x == alpha_at.x)
        })
        .await;
        assert_eq!(layout_of(node).await["placements"], next["placements"]);
    }
    for node in &nodes {
        eventually("edges derived from the new layout", || async {
            let found = sides(node);
            found.contains(&mouser_proto::model::Side::Bottom)
                && found.contains(&mouser_proto::model::Side::Top)
        })
        .await;
    }
}

#[tokio::test]
async fn set_replicates_edge_options_and_the_shared_settings() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let mut layout = layout_of(&nodes[0]).await;
    let display = layout["placements"][0]["display_id"].clone();
    layout["edge_options"] =
        json!([{"display_id": display, "side": "right", "block_corners": false}]);
    layout["equalize_dimensions"] = json!(true);
    layout["block_fullscreen"] = json!(false);
    let _: Empty = call(&nodes[0], Method::LayoutSet, layout.clone()).await;
    for node in &nodes {
        eventually("the options everywhere", || async {
            node.engine.layout().equalize_dimensions
        })
        .await;
        let got = layout_of(node).await;
        assert_eq!(got["edge_options"], layout["edge_options"]);
        assert_eq!(got["block_fullscreen"], false);
        let config: Value = call(node, Method::ConfigGet, json!({"key": "crossing"})).await;
        assert_eq!(config["equalize_dimensions"], true);
        assert_eq!(config["block_fullscreen"], false);
        assert!(node.engine.ctx.crossing.lock().unwrap().map.equalize);
    }
}

#[tokio::test]
async fn an_invalid_layout_is_refused_and_the_previous_one_stays() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let before = layout_of(&nodes[0]).await;
    let epoch = epoch_of(&nodes[0]);
    // Beta on top of Alpha.
    let mut overlapping = before.clone();
    for p in overlapping["placements"].as_array_mut().unwrap() {
        p["x"] = json!(0);
        p["y"] = json!(0);
    }
    let error = call_err(&nodes[0], Method::LayoutSet, overlapping).await;
    assert_eq!(error.code, -32602);
    assert!(error.message.ends_with("overlap"), "{}", error.message);
    // Beta far away.
    let mut cut_off = before.clone();
    cut_off["placements"][1]["x"] = json!(100_000);
    let error = call_err(&nodes[0], Method::LayoutSet, cut_off).await;
    assert!(
        error.message.contains("cannot be reached"),
        "{}",
        error.message
    );
    // Not a layout at all.
    let error = call_err(&nodes[0], Method::LayoutSet, json!({"placements": 3})).await;
    assert_eq!(error.code, -32602);
    assert_eq!(layout_of(&nodes[0]).await, before);
    assert_eq!(epoch_of(&nodes[0]), epoch);
}

#[tokio::test]
async fn set_off_a_desk_is_refused() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let error = call_err(&node, Method::LayoutSet, layout_of(&node).await).await;
    assert_eq!(error.message, "This computer is not on a desk");
}

mod logged {
    //! `layout.changed` (Story 2 scenario 7): what the desk owner did in the
    //! arranger, written by the computer the layout was set on.

    use mouser_proto::control::{Empty, LoggingSetResult, Method};
    use serde_json::{Value, json};

    use crate::testing::*;

    async fn logging_on(nodes: &[TestNode]) {
        for node in nodes {
            let _: LoggingSetResult =
                call(node, Method::LoggingSet, json!({"enabled": true})).await;
        }
    }

    fn changes(node: &TestNode) -> Vec<Value> {
        log_events(node, "layout.changed")
            .into_iter()
            .map(|r| r["fields"].clone())
            .collect()
    }

    #[tokio::test]
    async fn moving_a_computer_is_logged_by_the_computer_that_did_it() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        logging_on(&nodes).await;
        let (a, b) = (&nodes[0], &nodes[1]);
        let mut layout = layout_of(a).await;
        let beta = b.engine.computer_id().to_string();
        // Beta goes below Alpha instead of beside it.
        let alpha_at = a
            .engine
            .layout()
            .placements
            .into_iter()
            .find(|p| p.display_id.computer == a.engine.computer_id())
            .unwrap();
        for p in layout["placements"].as_array_mut().unwrap() {
            if p["display_id"]["computer"] == beta.as_str() {
                p["x"] = json!(alpha_at.x);
                p["y"] = json!(alpha_at.y + 296);
            }
        }
        let _: Empty = call(a, Method::LayoutSet, layout.clone()).await;
        let record = logged(a, "layout.changed", |r| r["fields"]["change"] == "moved").await;
        assert_eq!(record["level"], "info");
        assert_eq!(
            record["fields"],
            json!({"by": a.engine.computer_id().to_string(), "computer": beta, "change": "moved"})
        );
        // The same layout again, with another shared setting, moves nobody.
        layout["equalize_dimensions"] = json!(true);
        let _: Empty = call(a, Method::LayoutSet, layout).await;
        assert_eq!(changes(a).len(), 1);
    }

    #[tokio::test]
    async fn removing_and_including_a_display_is_logged_for_its_computer() {
        let lab = Lab::new();
        let nodes = docked_desk(&lab).await;
        logging_on(&nodes).await;
        let b = &nodes[1];
        let beta = b.engine.computer_id().to_string();
        let full = layout_of(b).await;
        let mut removal = full.clone();
        removal["placements"]
            .as_array_mut()
            .unwrap()
            .retain(|p| p["display_id"]["key"] != "b2");
        removal["displays"] =
            json!([{"display_id": {"computer": beta, "key": "b2"}, "included": false}]);
        let _: Empty = call(b, Method::LayoutSet, removal).await;
        logged(b, "layout.changed", |r| {
            r["fields"]["change"] == "display_removed"
        })
        .await;
        let mut undo = full;
        undo["displays"] =
            json!([{"display_id": {"computer": beta, "key": "b2"}, "included": true}]);
        let _: Empty = call(b, Method::LayoutSet, undo).await;
        let record = logged(b, "layout.changed", |r| {
            r["fields"]["change"] == "display_included"
        })
        .await;
        assert_eq!(
            record["fields"],
            json!({"by": beta, "computer": beta, "change": "display_included"})
        );
        // Taking the display out moved nobody, and putting it back placed it.
        assert!(
            changes(b).iter().all(|c| c["change"] != "moved"),
            "{:?}",
            changes(b)
        );
    }
}
