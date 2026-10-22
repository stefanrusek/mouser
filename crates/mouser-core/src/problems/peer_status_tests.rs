use mouser_platform::{
    Capability, Limitation, Os, PermissionKind as Kind, PermissionState as State, PlatformInfo,
    SecureInputHolder, SessionType,
};
use mouser_proto::control::{ConnectionState, ProblemKind};
use mouser_proto::model::{PermissionKind, PermissionState};
use mouser_testkit::VirtualPlatform;

use crate::testing::*;

async fn desk_with(platform: VirtualPlatform) -> (Lab, Vec<TestNode>) {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let b = lab.start_with("Beta", platform, |_| {}).await;
    let _: mouser_proto::control::Desk = call(
        &a,
        mouser_proto::control::Method::DeskCreate,
        serde_json::json!({"name": "Home"}),
    )
    .await;
    admit(&a, &b).await;
    let nodes = vec![a, b];
    ready(&nodes).await;
    (lab, nodes)
}

fn beta_row(s: &mouser_proto::control::Status) -> &mouser_proto::control::StatusComputer {
    s.computers
        .iter()
        .find(|c| c.name == "Beta")
        .expect("Beta is listed")
}

#[tokio::test]
async fn status_shows_every_member_with_its_permissions_limitations_and_rtt() {
    let platform = VirtualPlatform::new();
    platform.set_permission(Kind::Accessibility, State::Missing);
    platform.set_permission(Kind::ScreenRecording, State::Granted);
    platform.set_info(PlatformInfo {
        os: Os::Linux,
        version: "6".into(),
        session: SessionType::Native,
        limitations: vec![Limitation {
            capability: Capability::FileDrag,
            text: "Dragging files is not available here; copy and paste them instead.".into(),
        }],
    });
    let (_lab, nodes) = desk_with(platform).await;
    let a = &nodes[0];
    eventually("Beta's status to arrive", || async {
        !beta_row(&status(a).await).permissions.is_empty()
    })
    .await;
    let s = status(a).await;
    let row = beta_row(&s);
    assert_eq!(row.connection, ConnectionState::Connected);
    assert!(row.rtt_ms.is_some(), "the transport's round trip time");
    assert_eq!(
        row.permissions[&PermissionKind::Accessibility],
        PermissionState::Missing
    );
    assert_eq!(
        row.permissions[&PermissionKind::ScreenRecording],
        PermissionState::Granted
    );
    assert_eq!(
        row.limitations,
        ["Dragging files is not available here; copy and paste them instead."]
    );
    // Its problems are listed too, about Beta.
    let missing = s
        .problems
        .iter()
        .find(|p| p.kind == ProblemKind::MissingPermission)
        .expect("Beta's missing permission");
    assert_eq!(missing.computer, nodes[1].engine.computer_id());
}

#[tokio::test]
async fn a_change_is_sent_as_it_happens_and_a_cleared_problem_goes_away() {
    let platform = VirtualPlatform::new();
    platform.set_permission(Kind::Accessibility, State::Missing);
    let (_lab, nodes) = desk_with(platform.clone()).await;
    let a = &nodes[0];
    let missing = |s: &mouser_proto::control::Status| {
        s.problems
            .iter()
            .any(|p| p.kind == ProblemKind::MissingPermission)
    };
    eventually("the missing permission to show", || async {
        missing(&status(a).await)
    })
    .await;
    platform.set_permission(Kind::Accessibility, State::Granted);
    eventually("the grant to show", || async {
        let s = status(a).await;
        !missing(&s)
            && beta_row(&s).permissions[&PermissionKind::Accessibility] == PermissionState::Granted
    })
    .await;
}

#[tokio::test]
async fn a_secure_input_holder_is_listed_once() {
    let platform = VirtualPlatform::new();
    let (_lab, nodes) = desk_with(platform.clone()).await;
    platform.set_secure_input(Some(SecureInputHolder {
        pid: 3,
        app_name: "Terminal".into(),
    }));
    let a = &nodes[0];
    eventually("the notice", || async {
        status(a)
            .await
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::SecureInput)
    })
    .await;
    settle().await;
    let count = status(a)
        .await
        .problems
        .iter()
        .filter(|p| p.kind == ProblemKind::SecureInput)
        .count();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn what_a_computer_reported_goes_when_it_disconnects() {
    let platform = VirtualPlatform::new();
    platform.set_permission(Kind::Accessibility, State::Missing);
    let (_lab, mut nodes) = desk_with(platform).await;
    let b = nodes.pop().unwrap();
    let a = &nodes[0];
    eventually("the problem to show", || async {
        status(a)
            .await
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::MissingPermission)
    })
    .await;
    b.engine.shutdown().await;
    eventually("the problem to go", || async {
        let s = status(a).await;
        s.problems
            .iter()
            .all(|p| p.kind != ProblemKind::MissingPermission)
            && beta_row(&s).permissions.is_empty()
    })
    .await;
}
