use mouser_platform::LocalInput;
use mouser_testkit::Injected;

use crate::lock::LockExt;
use crate::testing::*;

const A: u32 = 0x0007_0004;

/// Alpha controls Beta, which has A held down; Gamma controls Delta.
async fn two_pairs(lab: &Lab) -> Vec<TestNode> {
    let nodes = logged_desk_of(lab, &["Alpha", "Beta", "Gamma", "Delta"]).await;
    push_right(&nodes[0]);
    push_right(&nodes[2]);
    for (controller, receiver) in [(0, 1), (2, 3)] {
        injected_where(&nodes[receiver].platform, "entry", |i| {
            matches!(i, Injected::MotionAbs { .. })
        })
        .await;
        eventually("the controller to be remote", || async {
            nodes[controller]
                .engine
                .ctx
                .crossing
                .guard()
                .session
                .remote()
                .is_some()
        })
        .await;
    }
    nodes[0].platform.send_input(LocalInput::Key {
        usage: A,
        down: true,
    });
    injected_where(&nodes[1].platform, "the key", |i| {
        *i == Injected::Key {
            usage: A,
            down: true,
        }
    })
    .await;
    nodes
}

#[tokio::test]
async fn a_foreground_that_outranks_mouser_hands_control_back_with_panic_to_the_controller_only() {
    let lab = Lab::new();
    let nodes = two_pairs(&lab).await;
    let (a, b, c) = (&nodes[0], &nodes[1], &nodes[2]);

    b.platform
        .set_foreground_app(Some(admin_app("regedit.exe", true)));

    eventually("Alpha to be local", || async {
        a.engine.ctx.crossing.guard().session.remote().is_none()
    })
    .await;
    assert!(!a.platform.pointer_hidden());
    // Beta released what it held and takes no more input.
    injected_where(&b.platform, "the release", |i| {
        *i == Injected::Key {
            usage: A,
            down: false,
        }
    })
    .await;
    assert!(b.engine.ctx.crossing.guard().receiving.is_none());
    // Alpha was told with Panic.
    let alpha = a.engine.computer_id();
    let beta = b.engine.computer_id();
    logged(a, "control.changed", |r| {
        r["fields"]["how"] == "panic"
            && r["fields"]["from"] == beta.to_string()
            && r["fields"]["to"] == alpha.to_string()
    })
    .await;
    // Beta says why, in its own log.
    let record = logged(b, "control.changed", |r| {
        r["fields"]["how"] == "elevated_app"
    })
    .await;
    assert_eq!(record["fields"]["from"], beta.to_string());
    assert_eq!(record["fields"]["to"], alpha.to_string());
    assert!(record["fields"].get("app").is_none());
    // A desk-mate that was not receiving from Beta heard nothing: Gamma
    // still has the pointer on Delta.
    settle().await;
    assert!(c.engine.ctx.crossing.guard().session.remote().is_some());
    assert!(nodes[3].engine.ctx.crossing.guard().receiving.is_some());
}

#[tokio::test]
async fn a_foreground_that_does_not_outrank_mouser_keeps_receiving_input() {
    let lab = Lab::new();
    let nodes = two_pairs(&lab).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    // Installed or elevated Mouser: the application is administrator but
    // not above Mouser.
    b.platform
        .set_foreground_app(Some(admin_app("regedit.exe", false)));
    // Several reconcile ticks pass.
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    assert!(a.engine.ctx.crossing.guard().session.remote().is_some());
    assert!(b.engine.ctx.crossing.guard().receiving.is_some());
    a.platform.send_input(LocalInput::Key {
        usage: A,
        down: false,
    });
    injected_where(&b.platform, "the next key", |i| {
        *i == Injected::Key {
            usage: A,
            down: false,
        }
    })
    .await;
    assert!(
        log_events(b, "control.changed")
            .iter()
            .all(|r| r["fields"]["how"] != "elevated_app")
    );
    assert_eq!(status(a).await.problems, []);
}

#[tokio::test]
async fn an_outranking_foreground_does_nothing_when_nothing_is_received() {
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    nodes[1]
        .platform
        .set_foreground_app(Some(admin_app("regedit.exe", true)));
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    assert!(log_events(&nodes[1], "control.changed").is_empty());
    assert_eq!(status(&nodes[1]).await.problems, []);
}
