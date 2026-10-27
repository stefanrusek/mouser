use mouser_platform::{Os, PlatformInfo, SecureInputHolder, SessionType};
use mouser_proto::control::{Empty, Method, ProblemKind};
use mouser_testkit::VirtualPlatform;
use serde_json::json;

use crate::testing::*;

async fn name_seen_by(node: &TestNode, who: &TestNode) -> String {
    let id = who.engine.computer_id();
    status(node)
        .await
        .computers
        .into_iter()
        .find(|c| c.id == id)
        .expect("the computer is listed")
        .name
}

#[tokio::test]
async fn a_rename_is_trimmed_takes_a_new_epoch_and_reaches_every_computer() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
    let before = epoch_of(&nodes[0]);
    let beta = nodes[1].engine.computer_id();
    let _: Empty = call(
        &nodes[0],
        Method::DeskRename,
        json!({"computer": beta, "name": "  Studio PC  "}),
    )
    .await;
    assert_eq!(epoch_of(&nodes[0]), before + 1);
    for node in &nodes {
        eventually("the new name", || async {
            name_seen_by(node, &nodes[1]).await == "Studio PC"
        })
        .await;
    }
    // The arranger names it too.
    let layout = layout_of(&nodes[2]).await;
    let names: Vec<_> = layout["displays"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["computer_name"].as_str().unwrap().to_string())
        .collect();
    assert!(names.contains(&"Studio PC".to_string()), "{names:?}");
    assert!(!names.contains(&"Beta".to_string()), "{names:?}");
}

#[tokio::test]
async fn a_name_of_one_to_sixty_four_characters_is_accepted() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let beta = nodes[1].engine.computer_id();
    for name in ["x".to_string(), "y".repeat(64)] {
        let _: Empty = call(
            &nodes[0],
            Method::DeskRename,
            json!({"computer": beta, "name": name}),
        )
        .await;
        assert_eq!(name_seen_by(&nodes[0], &nodes[1]).await, name);
    }
}

#[tokio::test]
async fn an_invalid_name_or_computer_is_refused_and_nothing_changes() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let beta = nodes[1].engine.computer_id();
    let before = epoch_of(&nodes[0]);
    for name in ["", "   ", &"z".repeat(65)] {
        let err = call_err(
            &nodes[0],
            Method::DeskRename,
            json!({"computer": beta, "name": name}),
        )
        .await;
        assert_eq!(err.code, -32602, "{name:?}");
    }
    let stranger = mouser_proto::ids::ComputerId([9; 16]);
    let err = call_err(
        &nodes[0],
        Method::DeskRename,
        json!({"computer": stranger, "name": "Nobody"}),
    )
    .await;
    assert_eq!(err.code, -32602);
    assert_eq!(epoch_of(&nodes[0]), before);
    assert_eq!(name_seen_by(&nodes[0], &nodes[1]).await, "Beta");
}

#[tokio::test]
async fn without_a_desk_a_rename_is_refused_as_join_refused() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let me = node.engine.computer_id();
    let err = call_err(
        &node,
        Method::DeskRename,
        json!({"computer": me, "name": "Studio"}),
    )
    .await;
    assert_eq!(err.code, -32001);
    assert_eq!(err.data.unwrap().kind, ProblemKind::JoinRefused);
}

#[tokio::test]
async fn problems_name_the_computer_by_its_desk_wide_name() {
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
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home"})).await;
    admit(&a, &b).await;
    let nodes = [a, b];
    ready(&nodes).await;
    let mac_id = nodes[0].engine.computer_id();
    let _: Empty = call(
        &nodes[1],
        Method::DeskRename,
        json!({"computer": mac_id, "name": "Studio Mac"}),
    )
    .await;
    eventually("the Mac to take its new name", || async {
        name_seen_by(&nodes[0], &nodes[0]).await == "Studio Mac"
    })
    .await;
    mac.set_secure_input(Some(SecureInputHolder {
        pid: 42,
        app_name: "1Password".into(),
    }));
    eventually("the notice on the other computer", || async {
        status(&nodes[1])
            .await
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::SecureInput && p.message.contains("Studio Mac"))
    })
    .await;
}

#[tokio::test]
async fn a_computer_renamed_from_its_own_window_keeps_the_name_as_its_own_setting() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let alpha = nodes[0].engine.computer_id();
    let _: Empty = call(
        &nodes[0],
        Method::DeskRename,
        json!({"computer": alpha, "name": "Desk Mac"}),
    )
    .await;
    let name: String = call(
        &nodes[0],
        Method::ConfigGet,
        json!({"key": "general.computer_name"}),
    )
    .await;
    assert_eq!(name, "Desk Mac");
    assert_eq!(
        crate::config::load(nodes[0].dir.path())
            .config
            .general
            .computer_name,
        "Desk Mac"
    );
    eventually("the other computer to hear of it", || async {
        name_seen_by(&nodes[1], &nodes[0]).await == "Desk Mac"
    })
    .await;
    // It introduces itself by that name from now on.
    let advert = nodes[0].discovery.announcements().pop().unwrap();
    assert_eq!(advert.display_name, "Desk Mac");
}
