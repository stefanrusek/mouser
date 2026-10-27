use mouser_platform::{
    Capability, Limitation, Os, PermissionKind as Kind, PermissionState as State, PlatformInfo,
    SecureInputHolder, SessionType,
};
use mouser_proto::control::{ConnectionState, Desk, Method, Problem, ProblemKind};
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{PermissionKind, PermissionState};
use mouser_proto::wire::{ComputerMode, ControlMessage, PeerStatus, encode_status_v1};
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

fn status_of(problems: Vec<Problem>) -> PeerStatus {
    PeerStatus {
        permissions: Default::default(),
        limitations: vec![],
        secure_input_holder: None,
        problems,
        mode: ComputerMode::Portable,
    }
}

fn problem(id: &str, kind: ProblemKind, computer: ComputerId) -> Problem {
    Problem {
        problem_id: id.into(),
        kind,
        computer,
        message: format!("message of {id}"),
        app: None,
        fix: None,
        raised_at: "2026-01-01T00:00:00.000Z".into(),
        cleared_at: None,
    }
}

async fn node_with_desk(lab: &Lab) -> (TestNode, Desk) {
    let node = lab.start_version("Alpha", "0.3.0", false).await;
    let desk: Desk = call(
        &node,
        Method::DeskCreate,
        serde_json::json!({"name": "Home"}),
    )
    .await;
    (node, desk)
}

#[tokio::test]
async fn start_up_limitation_problems_from_a_0_2_x_peer_are_dropped_on_merge() {
    let lab = Lab::new();
    let (node, desk) = node_with_desk(&lab).await;
    let mut link = member_rogue_as(&node, &desk, 0x91, "0.2.8").await;
    let rogue = crate::identity::Identity::from_seed([0x91; 32]).computer_id();
    let kept = [
        problem("secure_input:x", ProblemKind::SecureInput, rogue),
        // What the platform noticed while running is not a start-up limitation.
        problem(
            "platform_limitation:platform_problem:00ff",
            ProblemKind::PlatformLimitation,
            rogue,
        ),
    ];
    let mut all = vec![
        problem(
            "platform_limitation:capture",
            ProblemKind::PlatformLimitation,
            rogue,
        ),
        problem(
            "platform_limitation:tray",
            ProblemKind::PlatformLimitation,
            rogue,
        ),
    ];
    all.extend(kept.clone());
    link.writer
        .send_raw(&encode_status_v1(&status_of(all)))
        .await;
    eventually("the peer's problems to merge", || async {
        status(&node)
            .await
            .problems
            .iter()
            .any(|p| p.problem_id == "secure_input:x")
    })
    .await;
    let mut ids: Vec<_> = status(&node)
        .await
        .problems
        .into_iter()
        .map(|p| p.problem_id)
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        [
            "platform_limitation:platform_problem:00ff",
            "secure_input:x"
        ]
    );
}

/// The `Status` the node sends the rogue behind `link`.
async fn status_sent(link: &mut crate::transport::Link) -> PeerStatus {
    loop {
        if let ControlMessage::Status(status) = link
            .reader
            .recv::<ControlMessage>()
            .await
            .expect("the node keeps the link")
        {
            return status;
        }
    }
}

#[tokio::test]
async fn mode_goes_only_to_peers_from_0_3_0_and_the_001_shape_to_older_ones() {
    let lab = Lab::new();
    let node = lab
        .start_released(
            "Alpha",
            |_| {},
            |p| {
                p.mouser_version = "0.3.0".into();
                p.mode = ComputerMode::Headless;
            },
        )
        .await;
    node.platform
        .set_permission(Kind::Accessibility, State::Missing);
    let desk: Desk = call(
        &node,
        Method::DeskCreate,
        serde_json::json!({"name": "Home"}),
    )
    .await;
    let mut newer = member_rogue_as(&node, &desk, 0x92, "0.3.0").await;
    let mut older = member_rogue_as(&node, &desk, 0x93, "0.2.8").await;
    let current = status_sent(&mut newer).await;
    assert_eq!(current.mode, ComputerMode::Headless);
    assert_eq!(current.problems.len(), 1);
    assert_eq!(current.problems[0].kind, ProblemKind::MissingPermission);
    let legacy = status_sent(&mut older).await;
    assert_eq!(legacy.to_v1(), current.to_v1());
}

#[tokio::test]
async fn a_peers_mode_is_shown_from_0_3_0_and_unknown_for_0_2_x() {
    let lab = Lab::new();
    let mut nodes = Vec::new();
    for (name, version, mode) in [
        ("Alpha", "0.3.0", ComputerMode::Installed),
        ("Beta", "0.3.0", ComputerMode::Service),
        ("Gamma", "0.2.8", ComputerMode::PortableElevated),
    ] {
        nodes.push(
            lab.start_released(
                name,
                |_| {},
                |p| {
                    p.mouser_version = version.into();
                    p.mode = mode;
                },
            )
            .await,
        );
    }
    let nodes = desk_from(nodes).await;
    let expected = [
        ("Alpha", Some(ComputerMode::Installed)),
        ("Beta", Some(ComputerMode::Service)),
        ("Gamma", None),
    ];
    eventually("every mode to show", || async {
        let s = status(&nodes[0]).await;
        expected.iter().all(|(name, mode)| {
            s.computers
                .iter()
                .any(|c| c.name == *name && c.mode == *mode)
        })
    })
    .await;
    // Gamma runs 0.2.x as far as Alpha can tell, whatever its status says.
    settle().await;
    let s = status(&nodes[0]).await;
    assert_eq!(
        s.computers.iter().find(|c| c.name == "Gamma").unwrap().mode,
        None
    );
}

#[tokio::test]
async fn two_0_2_x_computers_still_exchange_problems_in_the_001_shape() {
    let lab = Lab::new();
    let a = lab.start_version("Alpha", "0.2.8", false).await;
    let b = lab
        .start_released("Beta", |_| {}, |p| p.mouser_version = "0.2.8".into())
        .await;
    b.platform
        .set_permission(Kind::Accessibility, State::Missing);
    let nodes = desk_from(vec![a, b]).await;
    eventually("Beta's problem to arrive", || async {
        let s = status(&nodes[0]).await;
        s.problems.iter().any(|p| {
            p.kind == ProblemKind::MissingPermission && p.computer == nodes[1].engine.computer_id()
        })
    })
    .await;
    let s = status(&nodes[0]).await;
    assert_eq!(beta_row(&s).mode, None);
}
