use mouser_platform::{Os, PlatformInfo, SecureInputHolder, SessionType};
use mouser_proto::control::ProblemKind;
use mouser_testkit::VirtualPlatform;

use crate::testing::*;

async fn has_problem(node: &TestNode, text: &str) -> bool {
    status(node)
        .await
        .problems
        .iter()
        .any(|p| p.kind == ProblemKind::SecureInput && p.message.contains(text))
}

#[tokio::test]
async fn a_secure_input_holder_is_named_to_the_user_everywhere_and_clears() {
    let lab = Lab::new();
    let mac = VirtualPlatform::new();
    mac.set_info(PlatformInfo {
        os: Os::MacOs,
        version: "26".into(),
        session: SessionType::Native,
        limitations: Vec::new(),
    });
    let a = lab.start_with("Mac", mac.clone(), |_| {}).await;
    let b = lab.start("Linux").await;
    let _: mouser_proto::control::Desk = call(
        &a,
        mouser_proto::control::Method::DeskCreate,
        serde_json::json!({"name": "Home"}),
    )
    .await;
    admit(&a, &b).await;
    let nodes = [a, b];
    ready(&nodes).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let mut notes = a.engine.control().notifications();
    mac.set_secure_input(Some(SecureInputHolder {
        pid: 42,
        app_name: "1Password".into(),
    }));
    eventually("the Mac's own notice", || async {
        has_problem(a, "1Password").await
    })
    .await;
    eventually("the notice on the other computer", || async {
        has_problem(b, "1Password").await
    })
    .await;
    // It names the computer too, and the connection stays up.
    assert!(has_problem(b, "Mac").await);
    assert_eq!(connected(b).await, 1);
    let note = recv_topic(&mut notes, mouser_proto::control::Topic::Problem).await;
    assert!(note["message"].as_str().unwrap().contains("1Password"));
    // Another application takes over: the notice follows.
    mac.set_secure_input(Some(SecureInputHolder {
        pid: 43,
        app_name: "Terminal".into(),
    }));
    eventually("the new holder", || async {
        has_problem(b, "Terminal").await && !has_problem(b, "1Password").await
    })
    .await;
    // It clears when the holder is gone.
    mac.set_secure_input(None);
    eventually("the notice to clear", || async {
        !has_problem(a, "Terminal").await && !has_problem(b, "Terminal").await
    })
    .await;
    assert_eq!(connected(a).await, 1);
}

#[tokio::test]
async fn a_notice_about_a_computer_that_disconnects_goes_away() {
    let lab = Lab::new();
    let mac = VirtualPlatform::new();
    let mut nodes = vec![
        lab.start_with("Mac", mac.clone(), |_| {}).await,
        lab.start("Linux").await,
    ];
    let _: mouser_proto::control::Desk = call(
        &nodes[0],
        mouser_proto::control::Method::DeskCreate,
        serde_json::json!({"name": "Home"}),
    )
    .await;
    admit(&nodes[0], &nodes[1]).await;
    mac.set_secure_input(Some(SecureInputHolder {
        pid: 42,
        app_name: "1Password".into(),
    }));
    eventually("the notice", || async {
        has_problem(&nodes[1], "1Password").await
    })
    .await;
    let a = nodes.remove(0);
    a.engine.shutdown().await;
    eventually("the notice to go", || async {
        !has_problem(&nodes[0], "1Password").await
    })
    .await;
}

#[tokio::test]
async fn a_status_from_a_stranger_is_a_protocol_violation() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let err = a
        .engine
        .ctx
        .on_peer_status(
            mouser_proto::ids::ComputerId([5; 16]),
            a.engine.ctx.peer_status(),
        )
        .unwrap_err();
    assert!(err.contains("not on the desk"));
}

#[tokio::test]
async fn a_computer_tells_its_desk_mates_its_permissions_limitations_and_problems() {
    use mouser_platform::{PermissionKind as Kind, PermissionState as State};
    use mouser_proto::model::{PermissionKind, PermissionState};
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    platform.set_permission(Kind::Accessibility, State::Missing);
    platform.set_permission(Kind::FullDiskAccess, State::NotApplicable);
    let node = lab.start_with("Alpha", platform, |_| {}).await;
    let status = node.engine.ctx.peer_status();
    assert_eq!(
        status.permissions[&PermissionKind::Accessibility],
        PermissionState::Missing
    );
    assert_eq!(
        status.permissions[&PermissionKind::FullDiskAccess],
        PermissionState::NotApplicable
    );
    assert_eq!(
        status.limitations,
        ["Ctrl-Alt-Del cannot be sent to this computer."]
    );
    assert_eq!(status.secure_input_holder, None);
    assert_eq!(status.problems, [], "limitations are not problems");
}

#[tokio::test]
async fn a_repeated_status_changes_nothing() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let before = status(b).await.problems;
    let again = mouser_proto::wire::ControlMessage::Status(a.engine.ctx.peer_status());
    send_to(a, b, again.clone());
    settle().await;
    // The first may differ from what Beta last heard (its mode, say); the
    // same status again changes nothing.
    let heard = status(b).await.problems;
    send_to(a, b, again);
    settle().await;
    assert_eq!(status(b).await.problems, heard);
    assert_eq!(heard, before);
}
