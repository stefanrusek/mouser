use mouser_proto::control::{Desk, JoinPending, Method, ProblemKind};
use mouser_testkit::VirtualPlatform;
use serde_json::json;

use crate::state::{LocalState, Membership};
use crate::testing::{Lab, admit, call, connected, eventually, status};

#[tokio::test]
async fn completing_first_run_registers_start_at_login_when_configured() {
    let lab = Lab::new();
    let on = lab.start("Alpha").await;
    assert!(
        !on.platform.is_autostart_enabled(),
        "nothing before first run completes"
    );
    let _: Desk = call(&on, Method::DeskCreate, json!({"name": "Home office"})).await;
    assert!(on.platform.is_autostart_enabled());

    let off = lab
        .start_with("Beta", VirtualPlatform::new(), |c| {
            c.general.start_at_login = false
        })
        .await;
    let _: Desk = call(&off, Method::DeskCreate, json!({"name": "Other"})).await;
    assert!(!off.platform.is_autostart_enabled());
}

#[tokio::test]
async fn a_restarted_computer_is_back_on_its_desk_with_no_action() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let desk: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    let b = lab.start("Beta").await;
    admit(&a, &b).await;

    // Restart Beta: same platform (secrets), same directory.
    let (platform, dir) = (b.platform.clone(), b.dir);
    let id = b.engine.computer_id();
    b.engine.shutdown().await;
    eventually("Alpha to notice", || async { connected(&a).await == 0 }).await;
    let b = lab.boot("Beta", platform, dir).await;

    assert_eq!(b.engine.computer_id(), id, "same identity");
    assert_eq!(status(&b).await.desk.unwrap().id, desk.desk_id);
    eventually("Beta to reconnect by itself", || async {
        connected(&b).await == 1 && connected(&a).await == 1
    })
    .await;
}

#[tokio::test]
async fn a_join_that_never_finished_is_over_after_a_restart() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let desk: Desk = call(&a, Method::DeskCreate, json!({"name": "Home office"})).await;
    let b = lab.start("Beta").await;
    let _: JoinPending = call(&b, Method::DeskJoin, json!({"desk_id": desk.desk_id})).await;
    let (platform, dir) = (b.platform.clone(), b.dir);
    b.engine.shutdown().await;
    // A crash leaves the request persisted as pending; the restart gives up on it.
    LocalState {
        membership: Membership::Pending(desk.desk_id),
        desk: None,
        ..LocalState::default()
    }
    .save(&dir.path().join(crate::state::FILE))
    .unwrap();
    let b = lab.boot("Beta", platform, dir).await;
    let s = status(&b).await;
    assert_eq!(s.desk, None);
    assert!(
        s.problems
            .iter()
            .any(|p| p.kind == ProblemKind::JoinRefused && p.message.contains("interrupted"))
    );
}
