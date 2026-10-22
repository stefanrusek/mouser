use mouser_platform::{PermissionKind as Kind, PermissionState as State};
use mouser_proto::control::{FixAction, ProblemKind, Topic};
use mouser_testkit::VirtualPlatform;

use crate::testing::*;

const ALL: [Kind; 6] = [
    Kind::Accessibility,
    Kind::InputMonitoring,
    Kind::ScreenRecording,
    Kind::FullDiskAccess,
    Kind::InputCapturePortal,
    Kind::RemoteDesktopPortal,
];

#[tokio::test]
async fn a_permission_revoked_while_running_is_announced_with_its_fix() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    platform.set_permission(Kind::Accessibility, State::Granted);
    let node = lab.start_with("Alpha", platform.clone(), |_| {}).await;
    let mut notes = node.engine.control().notifications();
    platform.set_permission(Kind::Accessibility, State::Missing);
    let problem = recv_topic(&mut notes, Topic::Problem).await;
    assert_eq!(problem["kind"], "missing_permission");
    assert_eq!(problem["fix"], "open_system_settings:accessibility");
    assert!(
        problem["message"]
            .as_str()
            .unwrap()
            .contains("Accessibility")
    );
    // Granting it again clears the notice without anyone asking for status.
    platform.set_permission(Kind::Accessibility, State::Granted);
    let cleared = recv_topic(&mut notes, Topic::Problem).await;
    assert!(cleared["cleared_at"].is_string());
}

#[tokio::test]
async fn the_fix_opens_the_system_screen_of_every_kind_of_permission() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    for kind in ALL {
        platform.set_permission(kind, State::Missing);
    }
    let node = lab.start_with("Alpha", platform.clone(), |_| {}).await;
    let control = node.engine.control();
    let open: Vec<_> = status(&node)
        .await
        .problems
        .into_iter()
        .filter(|p| p.kind == ProblemKind::MissingPermission)
        .collect();
    assert_eq!(open.len(), 6);
    for problem in &open {
        control.fix(&problem.problem_id).unwrap();
    }
    let opened = platform.opened_permission_settings();
    assert_eq!(opened.len(), 6);
    for kind in ALL {
        assert!(opened.contains(&kind), "{kind:?}");
    }
    // The problem stays until the permission is granted.
    assert_eq!(
        status(&node)
            .await
            .problems
            .iter()
            .filter(|p| p.kind == ProblemKind::MissingPermission)
            .count(),
        6
    );
}

#[tokio::test]
async fn fixes_that_are_not_the_engines_to_carry_out_are_refused() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let control = node.engine.control();
    let err = control.fix("missing_permission:accessibility").unwrap_err();
    assert!(err.contains("not an open problem"), "{err}");
    node.engine.ctx.problems.raise(
        ProblemKind::JoinRefused,
        "x",
        node.engine.computer_id(),
        "Kitchen was not admitted".into(),
        Some(FixAction::ApproveJoin),
    );
    let err = control.fix("join_refused:x").unwrap_err();
    assert!(err.contains("fixed by the app"), "{err}");
}

#[tokio::test]
async fn another_computers_problem_is_not_ours_to_fix() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    platform.set_permission(Kind::Accessibility, State::Missing);
    let a = lab.start("Alpha").await;
    let b = lab.start_with("Beta", platform.clone(), |_| {}).await;
    let _: mouser_proto::control::Desk = call(
        &a,
        mouser_proto::control::Method::DeskCreate,
        serde_json::json!({"name": "Home"}),
    )
    .await;
    admit(&a, &b).await;
    eventually("Beta's problem", || async {
        status(&a)
            .await
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::MissingPermission)
    })
    .await;
    let err = a
        .engine
        .control()
        .fix("missing_permission:accessibility")
        .unwrap_err();
    assert!(err.contains("not an open problem"), "{err}");
    assert!(a.platform.opened_permission_settings().is_empty());
}
