//! Story 3 scenario 5 and the edge case "no key or button stays stuck down
//! on any computer": the computer that has the pointer is lost.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use mouser_platform::LocalInput;
use mouser_proto::control::ConnectionState;
use mouser_testkit::Injected;

use crate::testing::*;

const A_KEY: u32 = 0x0007_0004;
const C_KEY: u32 = 0x0007_0006;

/// What is still held down after `injected`.
fn held(injected: &[Injected]) -> (BTreeSet<u32>, BTreeSet<u8>) {
    let (mut keys, mut buttons) = (BTreeSet::new(), BTreeSet::new());
    for i in injected {
        match i {
            Injected::Key { usage, down: true } => keys.insert(*usage),
            Injected::Key { usage, down: false } => keys.remove(usage),
            Injected::Button { button, down: true } => buttons.insert(*button),
            Injected::Button {
                button,
                down: false,
            } => buttons.remove(button),
            _ => false,
        };
    }
    (keys, buttons)
}

#[tokio::test]
async fn losing_the_active_computer_returns_the_mouse_and_keyboard_and_releases_everything() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    // Alpha's mouse and keyboard now drive Beta, with a key and a button held.
    push_right(a);
    injected_where(&b.platform, "the pointer on Beta", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    assert!(a.platform.is_grabbed());
    a.platform.send_input(LocalInput::Key {
        usage: A_KEY,
        down: true,
    });
    a.platform.send_input(LocalInput::Button {
        button: 1,
        down: true,
    });
    injected_where(&b.platform, "the held button", |i| {
        *i == Injected::Button {
            button: 1,
            down: true,
        }
    })
    .await;
    assert_eq!(held(&b.platform.injected()).0, BTreeSet::from([A_KEY]));
    assert!(
        !a.platform
            .local_delivered()
            .iter()
            .any(|i| matches!(i, LocalInput::Key { .. } | LocalInput::Button { .. })),
        "Alpha saw none of the typing"
    );

    // Beta becomes unreachable while it has the pointer.
    let lost = Instant::now();
    b.sim.as_ref().unwrap().set_blackhole(true);
    wait_connection(a, b, ConnectionState::Reconnecting).await;
    assert!(
        lost.elapsed() < Duration::from_secs(4),
        "{:?}",
        lost.elapsed()
    );

    // The pointer and keyboard are Alpha's again.
    assert!(!a.platform.is_grabbed());
    assert_eq!(a.platform.warps(), [(1919.0, 540.0)], "where it left");
    assert_eq!(status(a).await.active_computer, None);
    a.platform.send_input(LocalInput::Key {
        usage: C_KEY,
        down: true,
    });
    a.platform.send_input(LocalInput::Key {
        usage: C_KEY,
        down: false,
    });
    let delivered = a.platform.local_delivered();
    assert_eq!(
        delivered[delivered.len() - 2..],
        [
            LocalInput::Key {
                usage: C_KEY,
                down: true
            },
            LocalInput::Key {
                usage: C_KEY,
                down: false
            }
        ],
        "keys typed afterwards go to Alpha"
    );
    // Nothing was injected on Alpha, so nothing of Beta's session is stuck there.
    assert!(a.platform.injected().is_empty());

    // Beta notices the loss too and lets go of what Alpha left pressed.
    eventually("Beta to release the key and button", || async {
        held(&b.platform.injected()) == (BTreeSet::new(), BTreeSet::new())
    })
    .await;
    // Nor does Beta accept input from a controller it has lost.
    assert!(b.engine.ctx.crossing.lock().unwrap().receiving.is_none());
}

#[tokio::test]
async fn a_lost_controller_leaves_nothing_pressed_on_the_receiver() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "the pointer on Beta", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    a.platform.send_input(LocalInput::Key {
        usage: A_KEY,
        down: true,
    });
    injected_where(&b.platform, "the key", |i| {
        *i == Injected::Key {
            usage: A_KEY,
            down: true,
        }
    })
    .await;
    // The controller is the one that goes away.
    a.sim.as_ref().unwrap().set_blackhole(true);
    wait_connection(b, a, ConnectionState::Reconnecting).await;
    assert_eq!(
        held(&b.platform.injected()),
        (BTreeSet::new(), BTreeSet::new())
    );
}

#[tokio::test]
async fn the_controlled_display_leaving_the_layout_releases_what_the_still_connected_computer_holds()
 {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "the pointer on Beta", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    // Command is held on Beta.
    const LEFT_CMD: u32 = 0x0007_00E3;
    a.platform.send_input(LocalInput::Key {
        usage: LEFT_CMD,
        down: true,
    });
    injected_where(&b.platform, "Command held", |i| {
        *i == Injected::Key {
            usage: LEFT_CMD,
            down: true,
        }
    })
    .await;
    // Beta's display drops out of the layout (replaced by another) while Beta stays connected.
    b.platform
        .set_displays(vec![screen("replacement", (0.0, 0.0))]);
    eventually("Alpha to take control back", || async {
        !a.platform.is_grabbed()
    })
    .await;
    assert_eq!(connected(a).await, 1, "Beta is still connected");
    eventually("Beta to release Command", || async {
        held(&b.platform.injected()).0.is_empty()
    })
    .await;
}
