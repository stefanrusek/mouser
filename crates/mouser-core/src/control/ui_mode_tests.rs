use mouser_platform::{PermissionKind as Kind, PermissionState as State};
use mouser_proto::control::{Empty, FixAction, Method, ProblemKind, Topic};
use mouser_proto::ids::ComputerId;
use mouser_proto::model::DisplayId;
use mouser_testkit::VirtualPlatform;
use serde_json::json;

use super::ui_mode::ui_event;
use crate::testing::*;
use crate::{ControlClient, EngineEvent};

#[tokio::test]
async fn problem_fix_opens_the_system_screen_of_a_missing_permission() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    platform.set_permission(Kind::Accessibility, State::Missing);
    let node = lab.start_with("Alpha", platform.clone(), |_| {}).await;
    // Permission problems are raised as status is built.
    assert_eq!(status(&node).await.problems.len(), 1);
    let result = call_raw(
        &node,
        Method::ProblemFix,
        json!({"problem_id": "missing_permission:accessibility", "computer": node.engine.computer_id()}),
    )
    .await
    .unwrap();
    assert_eq!(result, json!(Empty {}));
    assert_eq!(platform.opened_permission_settings(), [Kind::Accessibility]);
}

#[tokio::test]
async fn problem_fix_of_an_arranger_edge_notice_clears_it_and_says_so() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let me = node.engine.computer_id();
    node.engine.ctx.problems.raise(
        ProblemKind::CrossingBlocked,
        "edge",
        me,
        "blocked".into(),
        Some(FixAction::OpenArrangerEdge),
    );
    let mut notes = node.engine.control().notifications();
    call_raw(
        &node,
        Method::ProblemFix,
        json!({"problem_id": "crossing_blocked:edge", "computer": me}),
    )
    .await
    .unwrap();
    assert!(node.engine.ctx.problems.open().is_empty());
    // Like the in-process fix, the new status is announced.
    let status = recv_topic(&mut notes, Topic::Status).await;
    assert_eq!(status["problems"], json!([]));
}

#[tokio::test]
async fn problem_fix_is_refused_as_the_in_process_fix_is() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let me = node.engine.computer_id();
    node.engine.ctx.problems.raise(
        ProblemKind::JoinRefused,
        "x",
        me,
        "Kitchen was not admitted".into(),
        Some(FixAction::ApproveJoin),
    );
    let fix = |problem_id: &str, computer: ComputerId| {
        call_err(
            &node,
            Method::ProblemFix,
            json!({"problem_id": problem_id, "computer": computer}),
        )
    };
    let err = fix("join_refused:x", me).await;
    assert_eq!(err.code, -32602);
    assert!(err.message.contains("fixed by the app"), "{}", err.message);
    let err = fix("missing_permission:accessibility", me).await;
    assert_eq!(err.code, -32602);
    assert!(
        err.message.contains("not an open problem"),
        "{}",
        err.message
    );
    // A problem is the pair (problem_id, computer): the same id about another
    // computer is not open.
    let err = fix("join_refused:x", ComputerId([9; 16])).await;
    assert_eq!(err.code, -32602);
    assert!(
        err.message.contains("not an open problem"),
        "{}",
        err.message
    );
    let err = call_err(&node, Method::ProblemFix, json!({"problem_id": "x"})).await;
    assert_eq!(err.code, -32602);
}

#[tokio::test]
async fn transfer_resolve_answers_the_overwrite_question() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let src = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("a.txt"), b"new").unwrap();
    let drop = tempfile::tempdir().unwrap();
    std::fs::write(drop.path().join("a.txt"), b"old").unwrap();
    b.platform.script_drop_target(drop.path().to_path_buf());
    let mut notes = b.engine.control().notifications();
    a.platform
        .script_outgoing_drag(Some(vec![src.path().join("a.txt")]));
    push_right(a);
    let asked = loop {
        let note = recv_topic(&mut notes, Topic::Transfer).await;
        if note["state"] == "awaiting_overwrite_decision" {
            break note;
        }
    };
    let result = call_raw(
        b,
        Method::TransferResolve,
        json!({"transfer_id": asked["transfer_id"], "overwrite": true}),
    )
    .await
    .unwrap();
    assert_eq!(result, json!(Empty {}));
    loop {
        let note = recv_topic(&mut notes, Topic::Transfer).await;
        if note["state"] == "completed" {
            break;
        }
    }
    assert_eq!(std::fs::read(drop.path().join("a.txt")).unwrap(), b"new");
}

#[tokio::test]
async fn transfer_resolve_needs_its_params() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let err = call_err(&node, Method::TransferResolve, json!({"transfer_id": "t"})).await;
    assert_eq!(err.code, -32602);
}

fn display(key: &str) -> DisplayId {
    DisplayId {
        computer: ComputerId([5; 16]),
        key: key.into(),
    }
}

#[test]
fn every_ui_facing_engine_event_has_its_control_api_form() {
    use mouser_proto::control::EngineEvent as Wire;
    let cases = [
        (
            EngineEvent::Landing {
                display: display("main"),
                x: 1.5,
                y: 2.5,
                desktop: (3.0, 4.0),
            },
            Some(Wire::Landing {
                display: display("main"),
                x: 1.5,
                y: 2.5,
                desktop: (3.0, 4.0),
            }),
        ),
        (
            EngineEvent::ReviewOffered {
                computer: ComputerId([6; 16]),
                name: "Laptop".into(),
            },
            Some(Wire::ReviewOffered {
                computer: ComputerId([6; 16]),
                name: "Laptop".into(),
            }),
        ),
        (
            EngineEvent::QuickJumpShow {
                letters: vec![(display("main"), 'A')],
            },
            Some(Wire::QuickJumpShow {
                letters: vec![(display("main"), 'A')],
            }),
        ),
        (EngineEvent::QuickJumpHide, Some(Wire::QuickJumpHide)),
        (EngineEvent::RestartElevated, Some(Wire::RestartElevated)),
        // A recovered fault is for tests, not for the UI.
        (
            EngineEvent::Fault {
                r#where: "control".into(),
            },
            None,
        ),
    ];
    for (event, wire) in cases {
        assert_eq!(ui_event(event), wire);
    }
}

#[tokio::test]
async fn engine_events_reach_the_connections_that_subscribed_to_them() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut ui = ControlClient::connect(&node.control_path).await.unwrap();
    ui.subscribe(vec![Topic::EngineEvent]).await.unwrap();
    let mut other = ControlClient::connect(&node.control_path).await.unwrap();
    other.subscribe(vec![Topic::Problem]).await.unwrap();
    let events = &node.engine.ctx.events;
    events
        .send(EngineEvent::Fault {
            r#where: "control".into(),
        })
        .unwrap();
    events.send(EngineEvent::QuickJumpHide).unwrap();
    // The fault is not sent; the next event is.
    let note = ui.next_notification().await.unwrap();
    assert_eq!(note.method, Topic::EngineEvent);
    assert_eq!(note.params, json!({"event": "quick_jump_hide"}));
    // A connection that did not subscribe gets only its own topics.
    node.engine.ctx.problems.raise(
        ProblemKind::JoinRefused,
        "x",
        node.engine.computer_id(),
        "refused".into(),
        None,
    );
    let note = other.next_notification().await.unwrap();
    assert_eq!(note.method, Topic::Problem);
}
