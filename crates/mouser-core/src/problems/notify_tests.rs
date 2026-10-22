use mouser_platform::{
    Capability, Limitation, Os, PermissionKind, PermissionState, PlatformInfo, SecureInputHolder,
    SessionType,
};
use mouser_proto::control::{Desk, Method, Topic};
use mouser_testkit::VirtualPlatform;
use serde_json::{Value, json};

use crate::testing::*;

/// A problem notification says what is wrong and offers at most one fix.
fn assert_notice(problem: &Value, kind: &str, fix: Option<&str>) {
    assert_eq!(problem["kind"], kind, "{problem}");
    assert!(!problem["message"].as_str().unwrap().is_empty());
    assert_eq!(problem["fix"].as_str(), fix, "{problem}");
}

#[tokio::test]
async fn a_refused_computer_is_announced_without_a_fix() {
    let lab = Lab::new();
    let a = lab
        .start_with("Alpha", VirtualPlatform::new(), |c| {
            c.advanced.allowlist = vec!["10.0.0.0/8".into()];
        })
        .await;
    let desk: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    let b = lab.start("Beta").await;
    let mut notes = a.engine.control().notifications();
    let _: Value = call(&b, Method::DeskJoin, json!({"desk_id": desk.desk_id})).await;
    assert_notice(
        &recv_topic(&mut notes, Topic::Problem).await,
        "join_refused",
        None,
    );
}

#[tokio::test]
async fn an_incompatible_version_offers_the_update_to_the_older_computer() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let mut notes = a.engine.control().notifications();
    let _b = lab.start_with_protocol("Beta", 2).await;
    assert_notice(
        &recv_topic(&mut notes, Topic::Problem).await,
        "incompatible_version",
        Some("open_update"),
    );
}

#[tokio::test]
async fn what_a_desk_mate_reports_is_announced_here_once() {
    let lab = Lab::new();
    let platform = VirtualPlatform::new();
    platform.set_permission(PermissionKind::Accessibility, PermissionState::Missing);
    platform.set_info(PlatformInfo {
        os: Os::Linux,
        version: "6".into(),
        session: SessionType::Native,
        limitations: vec![Limitation {
            capability: Capability::FileDrag,
            text: "Dragging files is not available here.".into(),
        }],
    });
    let a = lab.start("Alpha").await;
    let b = lab.start_with("Beta", platform.clone(), |_| {}).await;
    let _: Desk = call(&a, Method::DeskCreate, json!({"name": "Home"})).await;
    let mut notes = a.engine.control().notifications();
    admit(&a, &b).await;
    let beta = json!(b.engine.computer_id());
    let mut kinds = Vec::new();
    while kinds.len() < 2 {
        let note = recv_topic(&mut notes, Topic::Problem).await;
        if note["computer"] == beta {
            kinds.push(note["kind"].as_str().unwrap().to_string());
        }
    }
    kinds.sort();
    assert_eq!(kinds, ["missing_permission", "platform_limitation"]);
    // Secure Input is announced by the notice Alpha raises for it, not twice.
    platform.set_secure_input(Some(SecureInputHolder {
        pid: 3,
        app_name: "Terminal".into(),
    }));
    let secure = recv_topic(&mut notes, Topic::Problem).await;
    assert_notice(&secure, "secure_input", None);
    settle().await;
    while let Ok(note) = notes.try_recv() {
        if note.method == Topic::Problem {
            assert_ne!(note.params["kind"], "secure_input", "announced twice");
        }
    }
}
