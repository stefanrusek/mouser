use mouser_platform::{PermissionKind as PlatformKind, PermissionState as PlatformState, Platform};
use mouser_proto::control::{
    ConnectionState, Desk, EngineState, FixAction, Method, ProblemKind, Status,
};
use mouser_proto::model::{PermissionKind, PermissionState};
use mouser_testkit::VirtualPlatform;
use serde_json::{Value, json};

use super::status::tray_summary;
use crate::testing::{Lab, admit, call, connected, eventually, status};

fn validator() -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../mouser-proto/schemas/status.schema.json"
    ))
    .unwrap();
    jsonschema::validator_for(&schema).unwrap()
}

fn assert_valid(status: &Status) {
    let value = serde_json::to_value(status).unwrap();
    let errors: Vec<_> = validator()
        .iter_errors(&value)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{errors:?}\n{value:#}");
}

#[tokio::test]
async fn status_of_an_unjoined_computer_describes_this_computer() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    platform.set_permission(PlatformKind::Accessibility, PlatformState::Missing);
    platform.set_permission(PlatformKind::InputMonitoring, PlatformState::Granted);
    platform.set_permission(PlatformKind::ScreenRecording, PlatformState::NotApplicable);
    let node = lab.start_with("Alpha", platform, |_| {}).await;
    let s = status(&node).await;
    assert_valid(&s);
    assert_eq!(s.version, "0.1.0");
    assert_eq!(s.computer.name, "Alpha");
    assert_eq!(s.computer.id, node.engine.computer_id());
    assert_eq!(s.state, EngineState::Active);
    assert_eq!(s.desk, None);
    assert_eq!(s.active_computer, None);
    assert_eq!(s.computers.len(), 1);
    let me = &s.computers[0];
    assert_eq!(me.connection, ConnectionState::SelfComputer);
    assert_eq!(me.os, mouser_proto::model::Os::Linux);
    assert_eq!(me.displays, 1);
    assert_eq!(
        me.permissions[&PermissionKind::Accessibility],
        PermissionState::Missing
    );
    assert_eq!(
        me.permissions[&PermissionKind::InputMonitoring],
        PermissionState::Granted
    );
    assert_eq!(
        me.permissions[&PermissionKind::ScreenRecording],
        PermissionState::NotApplicable
    );
    assert!(
        me.limitations.iter().any(|l| l.contains("Ctrl-Alt-Del")),
        "{:?}",
        me.limitations
    );
}

#[tokio::test]
async fn each_missing_permission_is_a_problem_with_the_fix_that_opens_its_system_screen() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    platform.set_permission(PlatformKind::Accessibility, PlatformState::Missing);
    platform.set_permission(PlatformKind::InputMonitoring, PlatformState::Missing);
    platform.set_permission(PlatformKind::ScreenRecording, PlatformState::Granted);
    let node = lab.start_with("Alpha", platform.clone(), |_| {}).await;
    let s = status(&node).await;
    assert_valid(&s);
    let mut fixes: Vec<_> = s
        .problems
        .iter()
        .filter(|p| p.kind == ProblemKind::MissingPermission)
        .map(|p| p.fix.unwrap())
        .collect();
    fixes.sort_by_key(|f| f.to_string());
    assert_eq!(
        fixes,
        [
            FixAction::OpenSystemSettings(PermissionKind::Accessibility),
            FixAction::OpenSystemSettings(PermissionKind::InputMonitoring),
        ]
    );
    // Granting one clears its problem.
    platform.set_permission(PlatformKind::Accessibility, PlatformState::Granted);
    let s = status(&node).await;
    assert_eq!(s.problems.len(), 1);
    assert_eq!(
        s.problems[0].fix,
        Some(FixAction::OpenSystemSettings(
            PermissionKind::InputMonitoring
        ))
    );
}

#[tokio::test]
async fn status_lists_every_member_with_connection_displays_and_validates_against_the_schema() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let _: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    let two_displays = VirtualPlatform::new();
    let mut second = two_displays.displays().unwrap()[0].clone();
    second.key = "virtual-1".into();
    second.origin = (1920.0, 0.0);
    let mut both = two_displays.displays().unwrap();
    both.push(second);
    two_displays.set_displays(both);
    let b = lab.start_with("Beta", two_displays, |_| {}).await;
    admit(&a, &b).await;
    eventually("Alpha to learn Beta's displays", || async {
        status(&a)
            .await
            .computers
            .iter()
            .any(|c| c.name == "Beta" && c.displays == 2)
    })
    .await;

    let s = status(&a).await;
    assert_valid(&s);
    assert_eq!(s.desk.as_ref().unwrap().name, "Home office");
    assert_eq!(s.desk.as_ref().unwrap().pending_join, None);
    let beta = s.computers.iter().find(|c| c.name == "Beta").unwrap();
    assert_eq!(beta.connection, ConnectionState::Connected);
    assert_eq!(beta.id, b.engine.computer_id());
    assert_eq!(beta.version, "0.1.0");
    assert!(beta.rtt_ms.is_some() && beta.last_seen.is_some());
    // Tray: active, two computers, three displays.
    let tray = tray_summary(&s);
    assert_eq!(
        (tray.state, tray.computers, tray.displays),
        (EngineState::Active, 2, 3)
    );

    // A member that goes offline is still listed, as offline.
    b.engine.shutdown().await;
    eventually("Beta to show offline", || async {
        status(&a)
            .await
            .computers
            .iter()
            .any(|c| c.name == "Beta" && c.connection == ConnectionState::Offline)
    })
    .await;
    let s = status(&a).await;
    assert_valid(&s);
    let beta = s.computers.iter().find(|c| c.name == "Beta").unwrap();
    assert_eq!(
        (beta.rtt_ms, beta.last_seen.clone(), beta.displays),
        (None, None, 0)
    );
    assert_eq!(connected(&a).await, 0);
}

#[tokio::test]
async fn pausing_and_resuming_changes_the_state_the_tray_shows() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let _: mouser_proto::control::Empty = call(&node, Method::ActivePause, json!({})).await;
    let s = status(&node).await;
    assert_eq!(tray_summary(&s).state, EngineState::Paused);
    assert_valid(&s);
    let _: mouser_proto::control::Empty = call(&node, Method::ActiveResume, json!({})).await;
    assert_eq!(status(&node).await.state, EngineState::Active);
}

#[tokio::test]
async fn every_permission_kind_is_reported_by_its_wire_name() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    for kind in [
        PlatformKind::Accessibility,
        PlatformKind::InputMonitoring,
        PlatformKind::ScreenRecording,
        PlatformKind::FullDiskAccess,
        PlatformKind::InputCapturePortal,
        PlatformKind::RemoteDesktopPortal,
    ] {
        platform.set_permission(kind, PlatformState::Granted);
    }
    let node = lab.start_with("Alpha", platform, |_| {}).await;
    let s = status(&node).await;
    assert_valid(&s);
    let kinds: Vec<_> = s.computers[0].permissions.keys().copied().collect();
    assert_eq!(
        kinds,
        [
            PermissionKind::Accessibility,
            PermissionKind::InputMonitoring,
            PermissionKind::ScreenRecording,
            PermissionKind::FullDiskAccess,
            PermissionKind::InputCapturePortal,
            PermissionKind::RemoteDesktopPortal,
        ]
    );
    assert!(s.problems.is_empty());
}

#[tokio::test]
async fn a_member_that_upgraded_past_us_is_shown_as_incompatible() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let desk: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    let b = lab.start("Beta").await;
    admit(&a, &b).await;
    let (platform, dir) = (b.platform.clone(), b.dir);
    b.engine.shutdown().await;
    eventually("Alpha to notice", || async { connected(&a).await == 0 }).await;
    let b2 = lab.boot_with_protocol("Beta", platform, dir, 2).await;
    eventually("Alpha to show Beta as incompatible", || async {
        status(&a)
            .await
            .computers
            .iter()
            .any(|c| c.name == "Beta" && c.connection == ConnectionState::Incompatible)
    })
    .await;
    let s = status(&a).await;
    assert_valid(&s);
    assert_eq!(s.desk.unwrap().id, desk.desk_id);
    assert!(
        s.problems
            .iter()
            .any(|p| p.kind == ProblemKind::IncompatibleVersion)
    );
    // When the incompatible computer goes away it is simply offline.
    b2.engine.shutdown().await;
    eventually("Beta to show offline", || async {
        status(&a)
            .await
            .computers
            .iter()
            .any(|c| c.name == "Beta" && c.connection == ConnectionState::Offline)
    })
    .await;
}
