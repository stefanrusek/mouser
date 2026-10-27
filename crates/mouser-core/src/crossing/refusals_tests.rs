//! `input.refused` and `input.refused_run_ended` (Story 2 scenario 4): the
//! first refusal of a kind is written, the repeats are counted, and the run
//! ends with the next accepted input of that kind or a control change.

use std::sync::atomic::Ordering;

use mouser_platform::{ForegroundApp, LocalInput};
use mouser_proto::control::{LoggingSetResult, Method};
use mouser_testkit::{Injected, InputKind};
use serde_json::{Value, json};

use crate::lock::LockExt;
use crate::testing::*;

const A: u32 = 0x0007_0004;
const C: u32 = 0x0007_0006;
const D: u32 = 0x0007_0007;
const E: u32 = 0x0007_0008;

fn app(name: &str, admin: bool) -> ForegroundApp {
    ForegroundApp {
        name: name.into(),
        admin,
        outranks_mouser: false,
    }
}

fn key(node: &TestNode, usage: u32, down: bool) {
    node.platform.send_input(LocalInput::Key { usage, down });
}

/// Alpha has the pointer on Beta, and Beta's foreground application is
/// `regedit.exe`, which runs as administrator.
async fn driving() -> (Lab, Vec<TestNode>) {
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    nodes[1]
        .platform
        .set_foreground_app(Some(app("regedit.exe", true)));
    push_right(&nodes[0]);
    injected_where(&nodes[1].platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    (lab, nodes)
}

async fn refused(node: &TestNode, kind: &str) -> Value {
    logged(node, "input.refused", |r| r["fields"]["kind"] == kind).await
}

async fn ended(node: &TestNode, kind: &str) -> Value {
    logged(node, "input.refused_run_ended", |r| {
        r["fields"]["kind"] == kind
    })
    .await
}

fn count(node: &TestNode, event: &str, kind: &str) -> usize {
    log_events(node, event)
        .iter()
        .filter(|r| r["fields"]["kind"] == kind)
        .count()
}

#[tokio::test]
async fn the_first_refusal_is_logged_with_the_controller_and_the_run_is_counted_when_it_ends() {
    let (_lab, nodes) = driving().await;
    let (a, b) = (&nodes[0], &nodes[1]);
    b.platform.refuse_input(&[InputKind::Key]);
    for usage in [A, C, D] {
        key(a, usage, true);
    }
    let first = refused(b, "key").await;
    assert_eq!(first["level"], "warn");
    assert_eq!(first["peer"], a.engine.computer_id().to_string());
    assert_eq!(
        first["fields"],
        json!({"kind": "key", "app": "regedit.exe", "app_admin": true})
    );
    // Two more refusals of the same kind are part of the same run.
    eventually("all three keys to be refused", || async {
        !b.platform
            .injected()
            .iter()
            .any(|i| matches!(i, Injected::Key { .. }))
    })
    .await;
    settle().await;
    assert_eq!(count(b, "input.refused", "key"), 1);
    assert_eq!(count(b, "input.refused_run_ended", "key"), 0);

    // The next input the system accepts ends the run.
    b.platform.accept_input();
    key(a, A, false);
    let end = ended(b, "key").await;
    assert_eq!(end["level"], "info");
    assert_eq!(end["peer"], a.engine.computer_id().to_string());
    assert_eq!(end["fields"], json!({"kind": "key", "count": 3}));

    // A refusal after that starts a new run.
    b.platform.refuse_input(&[InputKind::Key]);
    key(a, E, true);
    eventually("a second run to be logged", || async {
        count(b, "input.refused", "key") == 2
    })
    .await;
}

#[tokio::test]
async fn buttons_motion_and_scroll_are_kinds_of_their_own() {
    let (_lab, nodes) = driving().await;
    let (a, b) = (&nodes[0], &nodes[1]);
    b.platform
        .refuse_input(&[InputKind::Button, InputKind::Motion, InputKind::Scroll]);
    a.platform.send_input(LocalInput::Button {
        button: 1,
        down: true,
    });
    a.platform
        .send_input(LocalInput::Scroll { dx: 0.0, dy: -3.0 });
    a.platform.send_input(LocalInput::Motion {
        dx: 5.0,
        dy: 0.0,
        abs: (0.0, 0.0),
    });
    for kind in ["button", "scroll", "motion"] {
        let record = refused(b, kind).await;
        assert_eq!(record["fields"]["app"], "regedit.exe", "{kind}");
        assert_eq!(record["peer"], a.engine.computer_id().to_string());
    }
    // A key is not refused, and does not end the other kinds' runs.
    key(a, A, true);
    injected_where(&b.platform, "the key", |i| {
        matches!(i, Injected::Key { .. })
    })
    .await;
    assert_eq!(count(b, "input.refused_run_ended", "button"), 0);

    b.platform.accept_input();
    a.platform.send_input(LocalInput::Button {
        button: 1,
        down: false,
    });
    a.platform
        .send_input(LocalInput::Scroll { dx: 0.0, dy: 3.0 });
    a.platform.send_input(LocalInput::Motion {
        dx: 5.0,
        dy: 0.0,
        abs: (0.0, 0.0),
    });
    for kind in ["button", "scroll", "motion"] {
        let end = ended(b, kind).await;
        assert!(end["fields"]["count"].as_u64().unwrap() >= 1, "{kind}");
    }
}

#[tokio::test]
async fn a_control_change_ends_the_run_too() {
    let (_lab, nodes) = driving().await;
    let (a, b) = (&nodes[0], &nodes[1]);
    b.platform.refuse_input(&[InputKind::Key]);
    key(a, A, true);
    key(a, C, true);
    refused(b, "key").await;
    // Control goes back to Alpha: Beta's session ends.
    push_back(&nodes, 0);
    let end = ended(b, "key").await;
    assert_eq!(end["fields"]["count"], 2);
}

#[tokio::test]
async fn a_grab_and_a_warp_the_system_refuses_have_no_controller() {
    let lab = Lab::new();
    let (alpha, knobs) = lab.start_flaky("Alpha").await;
    let _: LoggingSetResult = call(&alpha, Method::LoggingSet, json!({"enabled": true})).await;
    let nodes = desk_from(vec![alpha, lab.start_logged("Beta").await]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    a.platform.set_foreground_app(Some(app("Terminal", false)));

    // Two attempts to cross are refused: one run, of two.
    knobs.refuse_grab.store(true, Ordering::SeqCst);
    push_right(a);
    let first = refused(a, "grab").await;
    assert!(first.get("peer").is_none());
    assert_eq!(
        first["fields"],
        json!({"kind": "grab", "app": "Terminal", "app_admin": false})
    );
    push_right(a);
    eventually("the second attempt to be refused by the system", || async {
        knobs.grabs.guard().len() >= 2
    })
    .await;
    assert_eq!(count(a, "input.refused", "grab"), 1);
    // The system accepts the grab: the run ends with its count.
    knobs.refuse_grab.store(false, Ordering::SeqCst);
    push_right(a);
    injected_where(&b.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    let end = ended(a, "grab").await;
    assert_eq!(end["fields"], json!({"kind": "grab", "count": 2}));
    assert!(end.get("peer").is_none());

    // Putting the pointer back is refused twice, once per visit.
    knobs.refuse_warp.store(true, Ordering::SeqCst);
    push_back(&nodes, 0);
    refused(a, "warp").await;
    push_right(a);
    eventually("the pointer to be on Beta again", || async {
        a.platform.pointer_hidden()
    })
    .await;
    let end = ended(a, "warp").await;
    assert_eq!(end["fields"], json!({"kind": "warp", "count": 1}));
    push_back(&nodes, 0);
    eventually("a second run of refused warps", || async {
        count(a, "input.refused", "warp") == 2
    })
    .await;
}
