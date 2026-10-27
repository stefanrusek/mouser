use mouser_platform::LocalInput;
use mouser_testkit::Injected;

use super::state::Control;
use crate::EngineEvent;
use crate::lock::LockExt;
use crate::testing::*;

const A: u32 = 0x0007_0004;
const ESC: u32 = 0x0007_0029;

fn key(usage: u32, down: bool) -> LocalInput {
    LocalInput::Key { usage, down }
}

#[tokio::test]
async fn a_panic_handling_one_captured_event_recovers_and_the_thread_keeps_going() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let mut events = a.engine.events();
    // A held key is released locally before the pointer leaves: that inject
    // is the one that panics.
    a.platform.send_input(key(A, true));
    settle().await;
    a.platform.panic_on_next_inject();
    push_right(a);

    let at = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let EngineEvent::Fault { r#where } = events.recv().await.unwrap() {
                return r#where;
            }
        }
    })
    .await
    .expect("the fault is reported");
    assert_eq!(at, "mouser-input");
    assert_eq!(
        a.engine.ctx.crossing.guard().session.control,
        Control::Local
    );
    assert!(!a.platform.pointer_hidden());

    // The thread still handles events: crossing, then the panic key.
    push_right(a);
    injected_where(&b.platform, "entry on Beta", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    for _ in 0..2 {
        a.platform.send_input(key(ESC, true));
        a.platform.send_input(key(ESC, false));
    }
    eventually("the panic key to bring control back", || async {
        a.engine.ctx.crossing.guard().session.control == Control::Local
    })
    .await;
    assert!(!a.platform.pointer_hidden());
}
