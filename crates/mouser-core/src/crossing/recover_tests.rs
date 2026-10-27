use mouser_platform::{LocalInput, PermissionKind, PermissionState};
use mouser_proto::wire::ControlMessage;
use mouser_testkit::{Injected, VirtualPlatform};

use super::state::{Control, Pressed};
use crate::lock::LockExt;
use crate::testing::*;

const A: u32 = 0x0007_0004;

#[tokio::test]
async fn as_controller_it_returns_local_ungrabs_and_releases_the_other_computer() {
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
    assert!(a.platform.pointer_hidden());

    a.engine.ctx.crossing.guard().recover_after_fault();

    {
        let crossing = a.engine.ctx.crossing.guard();
        assert_eq!(crossing.session.control, Control::Local);
        assert_eq!(crossing.session.sent, Pressed::default());
    }
    assert!(!a.platform.pointer_hidden());
    assert_eq!(knobs.grabs.guard().last(), Some(&false));
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
}

#[tokio::test]
async fn as_receiver_it_releases_everything_injected_and_drops_the_session() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
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
    send_to(
        a,
        b,
        ControlMessage::Button {
            seq: 100,
            button: 1,
            down: true,
        },
    );
    injected_where(&b.platform, "the button", |i| {
        *i == Injected::Button {
            button: 1,
            down: true,
        }
    })
    .await;

    b.engine.ctx.crossing.guard().recover_after_fault();

    assert!(b.engine.ctx.crossing.guard().receiving.is_none());
    let injected = b.platform.injected();
    assert!(injected.contains(&Injected::Key {
        usage: A,
        down: false
    }));
    assert!(injected.contains(&Injected::Button {
        button: 1,
        down: false
    }));
    assert_eq!(
        b.engine.ctx.crossing.guard().session.control,
        Control::Local
    );
}

#[tokio::test]
async fn it_copes_with_a_controlled_computer_that_left_the_map_and_with_no_input_running() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    push_right(a);
    eventually("Alpha to be remote", || async {
        a.engine.ctx.crossing.guard().session.remote().is_some()
    })
    .await;
    {
        let mut crossing = a.engine.ctx.crossing.guard();
        crossing.map.wires.clear();
        crossing.recover_after_fault();
        assert_eq!(crossing.session.control, Control::Local);
    }
    assert!(!a.platform.pointer_hidden());

    // A computer whose input never started has nothing to ungrab.
    let platform = VirtualPlatform::new();
    platform.set_permission(PermissionKind::Accessibility, PermissionState::Missing);
    let blind = lab.start_with("Blind", platform, |_| {}).await;
    assert!(blind.engine.ctx.crossing.guard().io.is_none());
    blind.engine.ctx.crossing.guard().recover_after_fault();
    assert_eq!(
        blind.engine.ctx.crossing.guard().session.control,
        Control::Local
    );
}
