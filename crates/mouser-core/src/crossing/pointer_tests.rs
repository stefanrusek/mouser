//! `pointer.hidden` and `pointer.shown` (Story 2 scenario 3): every grab and
//! every return goes through one wrapper that logs it.

use mouser_platform::{CaptureLossReason, LocalInput};
use mouser_proto::control::{Empty, Method};
use serde_json::{Value, json};

use crate::crossing::panic::usage_for_name;
use crate::lock::LockExt;
use crate::testing::*;

async fn hidden(node: &TestNode, reason: &str) -> Value {
    logged(node, "pointer.hidden", |r| r["fields"]["reason"] == reason).await
}

async fn shown(node: &TestNode, reason: &str) -> Value {
    logged(node, "pointer.shown", |r| r["fields"]["reason"] == reason).await
}

fn tap(node: &TestNode, usage: u32) {
    for down in [true, false] {
        node.platform.send_input(LocalInput::Key { usage, down });
    }
}

#[tokio::test]
async fn crossing_out_hides_the_pointer_and_coming_back_shows_it_first() {
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    push_right(a);
    let hide = hidden(a, "crossing_out").await;
    assert_eq!(hide["level"], "info");
    assert!(a.platform.pointer_hidden());
    push_back(&nodes, 0);
    let show = shown(a, "returned").await;
    assert_eq!(show["level"], "info");
    // The record says the pointer is back: it was shown before it was written.
    assert!(!a.platform.pointer_hidden());
    let order: Vec<_> = log_records(a)
        .iter()
        .map(|r| r["event"].as_str().unwrap().to_string())
        .filter(|e| e.starts_with("pointer."))
        .collect();
    assert_eq!(order, ["pointer.hidden", "pointer.shown"]);
}

#[tokio::test]
async fn quickjump_hides_the_pointer_for_quickjump() {
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    for usage in [0x0007_00E0, 0x0007_00E2, 0x0007_00E1] {
        a.platform.send_input(LocalInput::Key { usage, down: true });
    }
    tap(a, usage_for_name("J").unwrap());
    for usage in [0x0007_00E1, 0x0007_00E2, 0x0007_00E0] {
        a.platform
            .send_input(LocalInput::Key { usage, down: false });
    }
    tap(a, usage_for_name("B").unwrap());
    hidden(a, "quickjump").await;
    assert!(a.platform.pointer_hidden());
    // Probing whether input can be grabbed and then grabbing it hides the
    // pointer once.
    assert_eq!(log_events(a, "pointer.hidden").len(), 1);
}

#[tokio::test]
async fn pausing_and_the_panic_key_show_the_pointer_as_returned() {
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    push_right(a);
    hidden(a, "crossing_out").await;
    let _: Empty = call(a, Method::ActivePause, json!({})).await;
    shown(a, "returned").await;
    assert!(!a.platform.pointer_hidden());
}

#[tokio::test]
async fn recovering_from_a_fault_shows_the_pointer_as_fault() {
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    push_right(a);
    hidden(a, "crossing_out").await;
    a.engine.ctx.crossing.guard().recover_after_fault();
    shown(a, "fault").await;
    assert!(!a.platform.pointer_hidden());
    // Nothing was hidden any more: recovering again shows nothing.
    a.engine.ctx.crossing.guard().recover_after_fault();
    assert_eq!(log_events(a, "pointer.shown").len(), 1);
}

#[tokio::test]
async fn quitting_while_remote_shows_the_pointer_as_quit() {
    let lab = Lab::new();
    let mut nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let _b = nodes.pop().unwrap();
    let a = nodes.pop().unwrap();
    push_right(&a);
    hidden(&a, "crossing_out").await;
    let (path, platform) = (a.engine.log().path(), a.platform.clone());
    a.engine.shutdown().await;
    assert!(!platform.pointer_hidden());
    assert!(
        read_log(&path)
            .iter()
            .any(|r| r["event"] == "pointer.shown" && r["fields"]["reason"] == "quit")
    );
}

#[tokio::test]
async fn capture_lost_is_logged_without_asking_the_backend_to_ungrab_again() {
    let lab = Lab::new();
    let (alpha, knobs) = lab.start_flaky("Alpha").await;
    let _: mouser_proto::control::LoggingSetResult =
        call(&alpha, Method::LoggingSet, json!({"enabled": true})).await;
    let nodes = desk_from(vec![alpha, lab.start_logged("Beta").await]).await;
    let a = &nodes[0];
    push_right(a);
    hidden(a, "crossing_out").await;
    a.platform.lose_capture(CaptureLossReason::HooksSilent);
    shown(a, "capture_lost").await;
    // The backend ungrabbed by itself (rule 10); core only grabbed once.
    assert_eq!(*knobs.grabs.guard(), [true]);
}

#[tokio::test]
async fn a_grab_the_system_refuses_hides_nothing_and_a_platform_that_cannot_grab_logs_nothing() {
    let lab = Lab::new();
    let (alpha, knobs) = lab.start_flaky("Alpha").await;
    let _: mouser_proto::control::LoggingSetResult =
        call(&alpha, Method::LoggingSet, json!({"enabled": true})).await;
    let nodes = desk_from(vec![alpha, lab.start_logged("Beta").await]).await;
    let a = &nodes[0];
    knobs
        .refuse_grab
        .store(true, std::sync::atomic::Ordering::SeqCst);
    push_right(a);
    logged(a, "input.refused", |r| r["fields"]["kind"] == "grab").await;
    assert!(log_events(a, "pointer.hidden").is_empty());

    let blind = lab.start_ungrabbable("Gamma").await;
    let _: mouser_proto::control::LoggingSetResult =
        call(&blind, Method::LoggingSet, json!({"enabled": true})).await;
    let nodes = desk_from(vec![blind, lab.start("Delta").await]).await;
    push_right(&nodes[0]);
    injected_where(&nodes[1].platform, "entry", |i| {
        matches!(i, mouser_testkit::Injected::MotionAbs { .. })
    })
    .await;
    assert!(log_events(&nodes[0], "pointer.hidden").is_empty());
    assert!(log_events(&nodes[0], "input.refused").is_empty());
    push_back(&nodes, 0);
    eventually("control to come back", || async {
        nodes[0]
            .engine
            .ctx
            .crossing
            .guard()
            .session
            .remote()
            .is_none()
    })
    .await;
    assert!(log_events(&nodes[0], "pointer.shown").is_empty());
}
