use mouser_proto::control::{Empty, Method};
use mouser_testkit::Injected;
use serde_json::json;

use crate::testing::*;

fn crossed(node: &TestNode) -> bool {
    node.platform
        .injected()
        .iter()
        .any(|i| matches!(i, Injected::MotionAbs { .. }))
}

#[tokio::test]
async fn a_full_screen_application_in_front_stops_the_crossing() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    a.platform.set_fullscreen(true);
    push_right(a);
    settle().await;
    assert!(!a.platform.is_grabbed());
    assert!(!crossed(b));
    // Without it the same push crosses.
    a.platform.set_fullscreen(false);
    push_right(a);
    injected_where(&b.platform, "the crossing", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

#[tokio::test]
async fn with_full_screen_blocking_off_a_full_screen_application_does_not_matter() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let mut layout = layout_of(a).await;
    layout["block_fullscreen"] = json!(false);
    let _: Empty = call(a, Method::LayoutSet, layout).await;
    eventually("the setting on Alpha's input path", || async {
        !a.engine.ctx.crossing.lock().unwrap().map.block_fullscreen
    })
    .await;
    a.platform.set_fullscreen(true);
    push_right(a);
    injected_where(&b.platform, "the crossing", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}
