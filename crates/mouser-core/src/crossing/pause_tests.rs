use mouser_platform::LocalInput;
use mouser_proto::control::{Empty, EngineState, Method};
use mouser_testkit::Injected;
use serde_json::json;

use crate::testing::*;

#[tokio::test]
async fn pausing_stops_crossing_and_resuming_restores_it() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let _: Empty = call(a, Method::ActivePause, json!({})).await;
    assert_eq!(status(a).await.state, EngineState::Paused);
    push_right(a);
    settle().await;
    assert!(!a.platform.is_grabbed());
    let _: Empty = call(a, Method::ActiveResume, json!({})).await;
    assert_eq!(status(a).await.state, EngineState::Active);
    push_right(a);
    injected_where(&b.platform, "the crossing", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

#[tokio::test]
async fn pausing_while_the_pointer_is_elsewhere_brings_it_home() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    eventually("the grab", || async { a.platform.is_grabbed() }).await;
    a.platform.send_input(LocalInput::Key {
        usage: 4,
        down: true,
    });
    injected_where(&b.platform, "the key", |i| {
        matches!(i, Injected::Key { down: true, .. })
    })
    .await;
    let _: Empty = call(a, Method::ActivePause, json!({})).await;
    assert!(!a.platform.is_grabbed());
    injected_where(&b.platform, "the release", |i| {
        *i == Injected::Key {
            usage: 4,
            down: false,
        }
    })
    .await;
}
