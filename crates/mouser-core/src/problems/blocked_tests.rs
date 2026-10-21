use mouser_platform::LocalInput;
use mouser_proto::control::{FixAction, ProblemKind, Topic};

use crate::testing::*;

fn corner_push(node: &TestNode) {
    node.platform.send_input(LocalInput::Motion {
        dx: 1.0,
        dy: 0.0,
        abs: (1919.0, 1.0),
    });
}

fn away(node: &TestNode) {
    node.platform.send_input(LocalInput::Motion {
        dx: -1.0,
        dy: 0.0,
        abs: (1000.0, 540.0),
    });
}

#[tokio::test]
async fn the_first_block_explains_and_later_blocks_do_not_repeat_it() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    let mut notes = a.engine.control().notifications();
    corner_push(a);
    let problem = recv_topic(&mut notes, Topic::Problem).await;
    assert_eq!(problem["kind"], "crossing_blocked");
    let message = problem["message"].as_str().unwrap();
    assert!(message.contains("corner"), "{message}");
    assert!(message.contains("twice"), "{message}");
    assert!(message.contains("arranger"), "{message}");
    assert_eq!(problem["fix"], FixAction::OpenArrangerEdge.to_string());
    // A later block, long after the double-bump window, says nothing more.
    away(a);
    tokio::time::sleep(std::time::Duration::from_millis(700)).await;
    corner_push(a);
    // The input thread works in order; a status call after it sees the end.
    settle().await;
    while let Ok(note) = notes.try_recv() {
        assert_ne!(note.method, Topic::Problem, "the explanation repeated");
    }
    let open = status(a).await.problems;
    assert_eq!(
        open.iter()
            .filter(|p| p.kind == ProblemKind::CrossingBlocked)
            .count(),
        1
    );
}

#[tokio::test]
async fn a_full_screen_block_names_the_full_screen_application() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    a.platform.set_fullscreen(true);
    let mut notes = a.engine.control().notifications();
    push_right(a);
    let problem = recv_topic(&mut notes, Topic::Problem).await;
    assert!(problem["message"].as_str().unwrap().contains("full-screen"));
}
