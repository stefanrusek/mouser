use std::time::Instant;

use mouser_platform::{CaptureLossReason, LocalInput};
use mouser_proto::ids::ComputerId;
use mouser_proto::model::DisplayId;

use super::state::{Control, Remote};
use crate::testing::*;

// v2: replaced in T111
#[tokio::test]
async fn capture_lost_changes_neither_the_crossing_state_nor_the_held_set() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut crossing = node.engine.ctx.crossing.lock().unwrap();
    let key = LocalInput::Key {
        usage: 0x0007_0004,
        down: true,
    };

    // Local: nothing changes.
    crossing.local_input(key, Instant::now());
    let held = crossing.session.held.clone();
    crossing.local_input(
        LocalInput::CaptureLost(CaptureLossReason::HooksSilent),
        Instant::now(),
    );
    assert_eq!(crossing.session.control, Control::Local);
    assert_eq!(crossing.session.held, held);

    // Remote: nothing changes and nothing is sent.
    let peer = ComputerId([9; 16]);
    let remote = Control::Remote(Remote {
        computer: peer,
        display: DisplayId {
            computer: peer,
            key: "d".into(),
        },
        at: (0.0, 0.0),
        speed: (1.0, 1.0),
        back: (5.0, 5.0),
    });
    crossing.session.control = remote.clone();
    let sent = crossing.session.sent.clone();
    crossing.local_input(
        LocalInput::CaptureLost(CaptureLossReason::OtherDesktop),
        Instant::now(),
    );
    assert_eq!(crossing.session.control, remote);
    assert_eq!(crossing.session.held, held);
    assert_eq!(crossing.session.sent, sent);
    assert_eq!(crossing.session.seq, 0);
}
