use mouser_proto::control::{Empty, Method};
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{DisplayId, EdgeOptions, Side};
use serde_json::json;

use super::edge_options::block_corners;
use crate::testing::*;

fn id(key: &str) -> DisplayId {
    DisplayId {
        computer: ComputerId([1; 16]),
        key: key.into(),
    }
}

#[test]
fn an_edge_without_an_entry_blocks_corners_and_an_entry_decides_for_its_edge_only() {
    let off = EdgeOptions {
        block_corners: false,
        ..EdgeOptions::new(id("a"), Side::Right)
    };
    let options = [off, EdgeOptions::new(id("a"), Side::Left)];
    assert!(block_corners(&[], &id("a"), Side::Right));
    assert!(!block_corners(&options, &id("a"), Side::Right));
    assert!(block_corners(&options, &id("a"), Side::Left));
    assert!(block_corners(&options, &id("b"), Side::Right));
}

fn corner_push(node: &TestNode) {
    node.platform
        .send_input(mouser_platform::LocalInput::Motion {
            dx: 1.0,
            dy: 0.0,
            abs: (1919.0, 2.0),
        });
}

#[tokio::test]
async fn changing_an_edge_through_layout_set_applies_to_that_edge_on_every_member() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let alpha = a
        .engine
        .layout()
        .placements
        .into_iter()
        .map(|p| p.display_id)
        .find(|d| d.computer == a.engine.computer_id())
        .unwrap();
    let mut layout = layout_of(a).await;
    layout["edge_options"] = json!([{
        "display_id": alpha, "side": "right", "block_corners": false
    }]);
    let _: Empty = call(a, Method::LayoutSet, layout).await;
    for node in &nodes {
        eventually("the option on every member", || async {
            let crossing = node.engine.ctx.crossing.lock().unwrap();
            !block_corners(&crossing.map.edge_options, &alpha, Side::Right)
                && block_corners(&crossing.map.edge_options, &alpha, Side::Left)
        })
        .await;
    }
    // Only that edge: Alpha's right edge now crosses in its corner.
    corner_push(a);
    injected_where(&b.platform, "the crossing at the corner", |i| {
        matches!(i, mouser_testkit::Injected::MotionAbs { .. })
    })
    .await;
}
