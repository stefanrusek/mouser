use std::time::{Duration, Instant};

use mouser_platform::LocalInput;
use mouser_testkit::Injected;

use super::r#override::WINDOW;
use crate::testing::*;

fn bump(node: &TestNode, now: Instant) {
    node.engine.ctx.crossing.lock().unwrap().local_input(
        LocalInput::Motion {
            dx: 1.0,
            dy: 0.0,
            abs: (1919.0, 2.0),
        },
        now,
    );
}

fn back_off(node: &TestNode, now: Instant) {
    node.engine.ctx.crossing.lock().unwrap().local_input(
        LocalInput::Motion {
            dx: -1.0,
            dy: 0.0,
            abs: (1000.0, 540.0),
        },
        now,
    );
}

fn pointer_is_away(node: &TestNode) -> bool {
    node.engine
        .ctx
        .crossing
        .lock()
        .unwrap()
        .session
        .remote()
        .is_some()
}

#[tokio::test]
async fn a_second_bump_within_the_window_crosses_and_a_single_bump_does_not() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let t = Instant::now();
    bump(a, t);
    assert!(!pointer_is_away(a), "one bump does not cross");
    back_off(a, t + Duration::from_millis(100));
    bump(a, t + Duration::from_millis(200));
    assert!(pointer_is_away(a), "the second bump crosses");
    injected_where(&b.platform, "the crossing", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

#[tokio::test]
async fn one_push_that_takes_many_events_is_one_bump() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    let t = Instant::now();
    for ms in 0..5 {
        bump(a, t + Duration::from_millis(ms * 10));
    }
    assert!(!pointer_is_away(a));
}

#[tokio::test]
async fn a_second_bump_after_the_window_is_a_new_first_bump() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    let t = Instant::now();
    bump(a, t);
    back_off(a, t + Duration::from_millis(10));
    bump(a, t + WINDOW + Duration::from_millis(1));
    assert!(!pointer_is_away(a));
    // The late bump started a new window.
    back_off(a, t + WINDOW + Duration::from_millis(20));
    bump(a, t + WINDOW + Duration::from_millis(40));
    assert!(pointer_is_away(a));
}

#[tokio::test]
async fn a_bump_of_another_edge_does_not_count() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    let t = Instant::now();
    bump(a, t);
    back_off(a, t + Duration::from_millis(10));
    // Alpha's left edge touches nothing, so it never counts as a bump of the
    // right edge.
    a.engine.ctx.crossing.lock().unwrap().local_input(
        LocalInput::Motion {
            dx: -1.0,
            dy: 0.0,
            abs: (0.0, 540.0),
        },
        t + Duration::from_millis(20),
    );
    assert!(!pointer_is_away(a));
}

#[tokio::test]
async fn a_full_screen_block_gives_way_to_a_double_bump_too() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    a.platform.set_fullscreen(true);
    let t = Instant::now();
    let mid = |now| {
        a.engine.ctx.crossing.lock().unwrap().local_input(
            LocalInput::Motion {
                dx: 1.0,
                dy: 0.0,
                abs: (1919.0, 540.0),
            },
            now,
        );
    };
    mid(t);
    assert!(!pointer_is_away(a));
    back_off(a, t + Duration::from_millis(10));
    mid(t + Duration::from_millis(20));
    assert!(pointer_is_away(a));
}

#[tokio::test]
async fn a_remote_pointer_is_held_at_a_corner_of_the_edge_back_and_a_double_bump_returns_it() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    push_right(a);
    eventually("the pointer to be on Beta", || async { pointer_is_away(a) }).await;
    let motion = |dx, dy, now| {
        a.engine.ctx.crossing.lock().unwrap().local_input(
            LocalInput::Motion {
                dx,
                dy,
                abs: (0.0, 0.0),
            },
            now,
        );
    };
    let t = Instant::now();
    // Up to the top of Beta's display, then toward Alpha along it: a corner.
    motion(0.0, -5000.0, t);
    motion(-5000.0, 0.0, t);
    assert!(pointer_is_away(a), "the corner holds the pointer on Beta");
    motion(10.0, 0.0, t + Duration::from_millis(50));
    motion(-5000.0, 0.0, t + Duration::from_millis(100));
    assert!(!pointer_is_away(a), "the second bump brings it home");
}
