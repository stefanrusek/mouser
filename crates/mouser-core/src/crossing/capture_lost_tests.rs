use mouser_platform::{CaptureLossReason, LocalInput};
use mouser_testkit::Injected;

use super::state::Control;
use crate::lock::LockExt;
use crate::testing::*;

const A: u32 = 0x0007_0004;

#[tokio::test]
async fn capture_lost_while_remote_returns_to_local_and_releases_the_other_computer() {
    let lab = Lab::new();
    let (alpha, knobs) = lab.start_flaky("Alpha").await;
    let nodes = desk_from(vec![alpha, lab.start("Beta").await]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    a.platform.send_input(LocalInput::Key {
        usage: A,
        down: true,
    });
    injected_where(&b.platform, "the key", |i| {
        *i == Injected::Key {
            usage: A,
            down: true,
        }
    })
    .await;
    let grabs = knobs.grabs.guard().clone();
    assert_eq!(grabs, vec![true]);

    a.platform.lose_capture(CaptureLossReason::HooksSilent);

    eventually("Alpha to be local", || async {
        a.engine.ctx.crossing.guard().session.control == Control::Local
    })
    .await;
    assert!(
        !a.platform.pointer_hidden(),
        "the backend showed the pointer"
    );
    // Core did not ungrab a second time: the backend already had.
    assert_eq!(*knobs.grabs.guard(), grabs);
    // Beta released what Alpha held, and no longer takes Alpha's input.
    injected_where(&b.platform, "the release", |i| {
        *i == Injected::Key {
            usage: A,
            down: false,
        }
    })
    .await;
    eventually("Beta to stop receiving", || async {
        b.engine.ctx.crossing.guard().receiving.is_none()
    })
    .await;
    assert_eq!(connected(a).await, 1);
    // The desk user can cross again.
    push_right(a);
    eventually("Alpha to be remote again", || async {
        a.engine.ctx.crossing.guard().session.remote().is_some()
    })
    .await;
}

#[tokio::test]
async fn capture_lost_while_local_changes_nothing() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    a.platform
        .send_input(LocalInput::CaptureLost(CaptureLossReason::OtherDesktop));
    settle().await;
    let crossing = a.engine.ctx.crossing.guard();
    assert_eq!(crossing.session.control, Control::Local);
    assert_eq!(crossing.session.seq, 0);
}

#[tokio::test]
async fn capture_lost_for_a_computer_that_left_the_map_sends_nothing_and_does_not_panic() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    push_right(a);
    eventually("Alpha to be remote", || async {
        a.engine.ctx.crossing.guard().session.remote().is_some()
    })
    .await;
    a.engine
        .ctx
        .crossing
        .guard()
        .map
        .wires
        .remove(&nodes[1].engine.computer_id());
    a.platform
        .lose_capture(CaptureLossReason::ElevatedForeground);
    eventually("Alpha to be local", || async {
        a.engine.ctx.crossing.guard().session.control == Control::Local
    })
    .await;
}
