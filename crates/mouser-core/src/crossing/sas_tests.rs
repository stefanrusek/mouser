use mouser_platform::{Os, PermissionKind, PermissionState, PlatformInfo, SessionType};
use mouser_proto::control::{Empty, Method, ProblemKind};
use mouser_proto::wire::ControlMessage;
use mouser_testkit::{Injected, VirtualPlatform};
use serde_json::json;

use crate::testing::*;

/// A computer of `os`; only Windows can be sent Ctrl-Alt-Del.
fn computer(os: Os) -> VirtualPlatform {
    let platform = VirtualPlatform::new();
    platform.set_info(PlatformInfo {
        os,
        version: "virtual".into(),
        session: SessionType::Native,
        limitations: Vec::new(),
    });
    platform.set_secure_attention_supported(os == Os::Windows);
    platform
}

async fn desk(alpha: VirtualPlatform, beta: VirtualPlatform) -> Vec<TestNode> {
    let lab = Lab::new();
    let nodes = vec![
        lab.start_with("Alpha", alpha, |_| {}).await,
        lab.start_with("Beta", beta, |_| {}).await,
    ];
    desk_from(nodes).await
}

fn is_cad(i: &Injected) -> bool {
    *i == Injected::SecureAttention
}

#[tokio::test]
async fn send_cad_reaches_a_windows_computer_the_pointer_is_on() {
    let nodes = desk(computer(Os::Linux), computer(Os::Windows)).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    eventually("the crossing", || async { a.platform.is_grabbed() }).await;
    let _: Empty = call(a, Method::SendCad, json!({})).await;
    injected_where(&b.platform, "the secure attention sequence", is_cad).await;
}

#[tokio::test]
async fn send_cad_is_refused_for_a_computer_that_is_not_windows() {
    let nodes = desk(computer(Os::Windows), computer(Os::MacOs)).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    eventually("the crossing", || async { a.platform.is_grabbed() }).await;
    let err = call_err(a, Method::SendCad, json!({})).await;
    assert_eq!(err.data.unwrap().kind, ProblemKind::PlatformLimitation);
    assert!(err.message.contains("Beta"), "{}", err.message);
    settle().await;
    assert!(!b.platform.injected().iter().any(is_cad));
    assert!(!a.platform.injected().iter().any(is_cad));
}

#[tokio::test]
async fn send_cad_on_a_windows_computer_with_the_pointer_at_home_signals_it_locally() {
    let nodes = desk(computer(Os::Windows), computer(Os::Linux)).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let _: Empty = call(a, Method::SendCad, json!({})).await;
    assert!(a.platform.injected().iter().any(is_cad));
    assert!(b.platform.injected().is_empty());
}

#[tokio::test]
async fn send_cad_on_a_computer_that_is_not_windows_is_refused() {
    let nodes = desk(computer(Os::MacOs), computer(Os::Linux)).await;
    let err = call_err(&nodes[0], Method::SendCad, json!({})).await;
    assert_eq!(err.data.unwrap().kind, ProblemKind::PlatformLimitation);
    assert!(err.message.contains("Alpha"), "{}", err.message);
}

#[tokio::test]
async fn send_cad_is_refused_while_input_is_not_running() {
    let lab = Lab::new();
    let platform = computer(Os::Windows);
    platform.set_permission(PermissionKind::Accessibility, PermissionState::Missing);
    let node = lab.start_with("Alpha", platform, |_| {}).await;
    let err = call_err(&node, Method::SendCad, json!({})).await;
    assert_eq!(err.data.unwrap().kind, ProblemKind::PlatformLimitation);
}

#[tokio::test]
async fn a_secure_attention_message_only_counts_from_the_controller() {
    let nodes = desk(computer(Os::Linux), computer(Os::Windows)).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    // Alpha does not control Beta: nothing happens.
    send_to(a, b, ControlMessage::SecureAttention);
    settle().await;
    assert!(!b.platform.injected().iter().any(is_cad));
    // The controller can send it, also by hand.
    push_right(a);
    eventually("the crossing", || async { a.platform.is_grabbed() }).await;
    send_to(a, b, ControlMessage::SecureAttention);
    injected_where(&b.platform, "the secure attention sequence", is_cad).await;
}

#[tokio::test]
async fn a_receiver_that_cannot_signal_it_carries_on() {
    let nodes = desk(computer(Os::Windows), computer(Os::Linux)).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    eventually("the crossing", || async { a.platform.is_grabbed() }).await;
    send_to(a, b, ControlMessage::SecureAttention);
    settle().await;
    assert!(!b.platform.injected().iter().any(is_cad));
    let _: mouser_proto::control::Status = status(b).await;
}
