use mouser_platform::LocalInput;
use mouser_proto::control::Method;
use mouser_proto::model::DisplayId;
use mouser_proto::wire::{ControlMessage, Modifiers, MotionDatagram, display_id_hash};
use mouser_testkit::{Injected, VirtualPlatform};
use serde_json::json;

use crate::EngineEvent;
use crate::lock::LockExt;
use crate::testing::*;

const A: u32 = 0x0007_0004;
const C: u32 = 0x0007_0006;
const META: u32 = 0x0007_00E3;
const CTRL: u32 = 0x0007_00E0;

fn display_of(node: &TestNode) -> DisplayId {
    DisplayId {
        computer: node.engine.computer_id(),
        key: "virtual-0".into(),
    }
}

fn enter(to: &TestNode, x: f32, y: f32, seq_base: u64) -> ControlMessage {
    ControlMessage::Enter {
        display_id: display_of(to),
        x,
        y,
        seq_base,
        modifiers_down: Modifiers::default(),
    }
}

fn key(seq: u64, usage: u32, down: bool) -> ControlMessage {
    ControlMessage::Key {
        seq,
        usage,
        down,
        modifiers: Modifiers::default(),
    }
}

fn motion(to: &TestNode, seq: u64, x: f32) -> MotionDatagram {
    MotionDatagram {
        seq,
        display_id_hash: display_id_hash(&display_of(to)),
        x,
        y: 20.0,
    }
}

#[tokio::test]
async fn enter_moves_the_pointer_and_announces_the_landing() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let mut events = b.engine.events();
    send_to(a, b, enter(b, 30.0, 40.0, 5));
    injected_where(&b.platform, "the pointer", |i| {
        *i == Injected::MotionAbs { x: 30.0, y: 40.0 }
    })
    .await;
    assert_eq!(
        events.recv().await.unwrap(),
        EngineEvent::Landing {
            display: display_of(b),
            x: 30.0,
            y: 40.0,
            desktop: (30.0, 40.0)
        }
    );
}

#[tokio::test]
async fn stale_and_unknown_motion_is_dropped() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    // Before any Enter nothing is accepted.
    datagram_to(a, b, &motion(b, 50, 1.0));
    send_to(a, b, enter(b, 0.0, 0.0, 10));
    injected_where(&b.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    b.platform.take_injected();
    datagram_to(a, b, &motion(b, 12, 120.0));
    injected_where(&b.platform, "newest", |i| {
        *i == Injected::MotionAbs { x: 120.0, y: 20.0 }
    })
    .await;
    // Older than what was applied, or no newer than the base, or for a display
    // that does not exist.
    datagram_to(a, b, &motion(b, 11, 11.0));
    datagram_to(a, b, &motion(b, 12, 12.0));
    let mut other = motion(b, 99, 13.0);
    other.display_id_hash ^= 1;
    datagram_to(a, b, &other);
    settle().await;
    assert_eq!(
        b.platform.injected().len(),
        1,
        "{:?}",
        b.platform.injected()
    );
}

#[tokio::test]
async fn keys_buttons_and_scroll_are_injected_in_order() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    // Dropped: nobody has entered yet.
    send_to(a, b, key(1, A, true));
    send_to(
        a,
        b,
        ControlMessage::Button {
            seq: 1,
            button: 1,
            down: true,
        },
    );
    send_to(a, b, enter(b, 1.0, 2.0, 1));
    send_to(a, b, key(2, A, true));
    send_to(
        a,
        b,
        ControlMessage::Button {
            seq: 3,
            button: 1,
            down: true,
        },
    );
    send_to(
        a,
        b,
        ControlMessage::Scroll {
            seq: 4,
            dx: 1,
            dy: -2,
        },
    );
    send_to(
        a,
        b,
        ControlMessage::Button {
            seq: 5,
            button: 1,
            down: false,
        },
    );
    send_to(a, b, key(6, A, false));
    injected_where(&b.platform, "the last key", |i| {
        *i == Injected::Key {
            usage: A,
            down: false,
        }
    })
    .await;
    assert_eq!(
        b.platform.injected(),
        [
            Injected::MotionAbs { x: 1.0, y: 2.0 },
            Injected::Key {
                usage: A,
                down: true
            },
            Injected::Button {
                button: 1,
                down: true
            },
            Injected::Scroll { dx: 1.0, dy: -2.0 },
            Injected::Button {
                button: 1,
                down: false
            },
            Injected::Key {
                usage: A,
                down: false
            },
        ]
    );
}

#[tokio::test]
async fn release_all_and_panic_release_everything_and_stop_the_input() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    for end in [ControlMessage::ReleaseAll { seq: 9 }, ControlMessage::Panic] {
        send_to(a, b, enter(b, 1.0, 2.0, 1));
        send_to(a, b, key(2, A, true));
        send_to(
            a,
            b,
            ControlMessage::Button {
                seq: 3,
                button: 2,
                down: true,
            },
        );
        send_to(a, b, end);
        send_to(a, b, key(10, C, true));
        send_to(
            a,
            b,
            ControlMessage::Scroll {
                seq: 11,
                dx: 5,
                dy: 5,
            },
        );
        injected_where(&b.platform, "the releases", |i| {
            *i == Injected::Key {
                usage: A,
                down: false,
            }
        })
        .await;
        settle().await;
        let seen = b.platform.take_injected();
        assert!(
            seen.contains(&Injected::Button {
                button: 2,
                down: false
            }),
            "{seen:?}"
        );
        assert!(
            !seen.contains(&Injected::Key {
                usage: C,
                down: true
            }),
            "{seen:?}"
        );
        assert!(
            !seen.iter().any(|i| matches!(i, Injected::Scroll { .. })),
            "{seen:?}"
        );
    }
    // A release with nothing entered is nothing.
    send_to(a, b, ControlMessage::ReleaseAll { seq: 12 });
    settle().await;
    assert!(b.platform.injected().is_empty());
}

#[tokio::test]
async fn losing_the_controller_releases_what_it_left_pressed() {
    let lab = Lab::new();
    let mut nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let b = nodes.pop().unwrap();
    let a = nodes.pop().unwrap();
    send_to(&a, &b, enter(&b, 1.0, 2.0, 1));
    send_to(&a, &b, key(2, A, true));
    injected_where(&b.platform, "the key", |i| {
        *i == Injected::Key {
            usage: A,
            down: true,
        }
    })
    .await;
    a.engine.shutdown().await;
    injected_where(&b.platform, "the release", |i| {
        *i == Injected::Key {
            usage: A,
            down: false,
        }
    })
    .await;
}

#[tokio::test]
async fn a_new_enter_starts_clean() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    send_to(a, b, enter(b, 1.0, 2.0, 1));
    send_to(a, b, key(2, A, true));
    send_to(a, b, enter(b, 3.0, 4.0, 5));
    injected_where(&b.platform, "the second entry", |i| {
        *i == Injected::MotionAbs { x: 3.0, y: 4.0 }
    })
    .await;
    let seen = b.platform.injected();
    let release = seen.iter().position(|i| {
        *i == Injected::Key {
            usage: A,
            down: false,
        }
    });
    let second = seen
        .iter()
        .position(|i| *i == Injected::MotionAbs { x: 3.0, y: 4.0 });
    assert!(release < second, "{seen:?}");
}

#[tokio::test]
async fn enter_from_a_stranger_or_for_a_missing_display_is_a_protocol_violation() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let stranger = mouser_proto::ids::ComputerId([77; 16]);
    let err = b
        .engine
        .ctx
        .on_message(stranger, enter(b, 1.0, 1.0, 1))
        .unwrap_err();
    assert!(err.contains("not on the desk"), "{err}");
    let mut missing = enter(b, 1.0, 1.0, 1);
    if let ControlMessage::Enter { display_id, .. } = &mut missing {
        display_id.key = "nope".into();
    }
    let err = b
        .engine
        .ctx
        .on_message(a.engine.computer_id(), missing)
        .unwrap_err();
    assert!(err.contains("does not have"), "{err}");
}

#[tokio::test]
async fn a_computer_that_cannot_inject_ignores_enter() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let b = lab
        .start_with("Limited", VirtualPlatform::limited(), |_| {})
        .await;
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home"})).await;
    admit(&a, &b).await;
    let nodes = [a, b];
    ready(&nodes).await;
    send_to(&nodes[0], &nodes[1], enter(&nodes[1], 1.0, 1.0, 1));
    send_to(&nodes[0], &nodes[1], key(2, A, true));
    settle().await;
    assert!(nodes[1].platform.injected().is_empty());
    assert_eq!(
        status(&nodes[1]).await.active_computer,
        nodes[1].engine.computer_id()
    );
}

#[tokio::test]
async fn input_from_devices_on_the_active_computer_is_not_blocked() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    send_to(a, b, enter(b, 1.0, 2.0, 1));
    injected_where(&b.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    let typed = LocalInput::Key {
        usage: A,
        down: true,
    };
    assert!(
        b.platform.send_input(typed),
        "reaches Beta's own applications"
    );
    assert_eq!(b.platform.local_delivered(), [typed]);
    assert!(!b.platform.is_grabbed());
}

#[tokio::test]
async fn shortcuts_are_translated_for_the_receiving_system() {
    let lab = Lab::new();
    let mac = VirtualPlatform::new();
    mac.set_info(mouser_platform::PlatformInfo {
        os: mouser_platform::Os::MacOs,
        version: "26".into(),
        session: mouser_platform::SessionType::Native,
        limitations: Vec::new(),
    });
    let a = lab.start_with("Mac", mac, |_| {}).await;
    let b = lab.start("Linux").await;
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home"})).await;
    admit(&a, &b).await;
    let nodes = [a, b];
    ready(&nodes).await;
    let (mac, linux) = (&nodes[0], &nodes[1]);
    // Command-C on the Mac copies on Linux: Control-C.
    send_to(mac, linux, enter(linux, 1.0, 1.0, 1));
    send_to(mac, linux, key(2, META, true));
    send_to(mac, linux, key(3, C, true));
    send_to(mac, linux, key(4, C, false));
    send_to(mac, linux, key(5, META, false));
    injected_where(&linux.platform, "the shortcut", |i| {
        *i == Injected::Key {
            usage: CTRL,
            down: false,
        }
    })
    .await;
    let keys: Vec<_> = linux
        .platform
        .injected()
        .into_iter()
        .filter(|i| matches!(i, Injected::Key { .. }))
        .collect();
    assert_eq!(
        keys,
        [
            Injected::Key {
                usage: CTRL,
                down: true
            },
            Injected::Key {
                usage: C,
                down: true
            },
            Injected::Key {
                usage: C,
                down: false
            },
            Injected::Key {
                usage: CTRL,
                down: false
            },
        ]
    );
    // And Control-C on Linux is Command-C on the Mac.
    send_to(linux, mac, enter(mac, 1.0, 1.0, 1));
    send_to(linux, mac, key(2, CTRL, true));
    injected_where(&mac.platform, "command", |i| {
        *i == Injected::Key {
            usage: META,
            down: true,
        }
    })
    .await;
}

#[tokio::test]
async fn buttons_4_and_5_follow_the_receivers_setting() {
    let lab = Lab::new();
    let a = lab.start("Alpha").await;
    let b = lab
        .start_with("Beta", VirtualPlatform::new(), |c| {
            c.mouse.buttons_4_5 = "back_forward".into()
        })
        .await;
    let _: mouser_proto::control::Desk =
        call(&a, Method::DeskCreate, json!({"name": "Home"})).await;
    admit(&a, &b).await;
    let nodes = [a, b];
    ready(&nodes).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    send_to(b, a, enter(a, 1.0, 1.0, 1));
    send_to(
        b,
        a,
        ControlMessage::Button {
            seq: 2,
            button: 4,
            down: true,
        },
    );
    send_to(
        b,
        a,
        ControlMessage::Button {
            seq: 3,
            button: 4,
            down: false,
        },
    );
    injected_where(&a.platform, "passed through", |i| {
        *i == Injected::Button {
            button: 4,
            down: false,
        }
    })
    .await;
    send_to(a, b, enter(b, 1.0, 1.0, 1));
    send_to(
        a,
        b,
        ControlMessage::Button {
            seq: 2,
            button: 5,
            down: true,
        },
    );
    send_to(
        a,
        b,
        ControlMessage::Button {
            seq: 3,
            button: 5,
            down: false,
        },
    );
    injected_where(&b.platform, "forward", |i| {
        *i == Injected::Key {
            usage: 0x0007_004F,
            down: true,
        }
    })
    .await;
    assert!(
        !b.platform
            .injected()
            .iter()
            .any(|i| matches!(i, Injected::Button { .. }))
    );
}

/// Regression: input that is not newer than the last accepted (queued on a
/// link that was replaced) is dropped, whatever kind it is.
#[tokio::test]
async fn stream_input_that_is_not_newer_is_dropped() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    send_to(a, b, enter(b, 1.0, 2.0, 10));
    send_to(a, b, key(11, A, true));
    send_to(a, b, key(11, C, true));
    send_to(a, b, key(5, C, true));
    send_to(
        a,
        b,
        ControlMessage::Button {
            seq: 6,
            button: 1,
            down: true,
        },
    );
    send_to(
        a,
        b,
        ControlMessage::Scroll {
            seq: 7,
            dx: 1,
            dy: 1,
        },
    );
    send_to(a, b, key(12, A, false));
    injected_where(&b.platform, "the newer key", |i| {
        *i == Injected::Key {
            usage: A,
            down: false,
        }
    })
    .await;
    assert_eq!(
        b.platform.injected(),
        [
            Injected::MotionAbs { x: 1.0, y: 2.0 },
            Injected::Key {
                usage: A,
                down: true
            },
            Injected::Key {
                usage: A,
                down: false
            }
        ]
    );
}

fn button_msg(seq: u64, button: u8, down: bool) -> ControlMessage {
    ControlMessage::Button { seq, button, down }
}

fn scroll_msg(seq: u64) -> ControlMessage {
    ControlMessage::Scroll { seq, dx: 0, dy: 3 }
}

/// A desk where Alpha has entered Beta; returns the nodes.
async fn entered() -> (Lab, Vec<TestNode>) {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    send_to(&nodes[0], &nodes[1], enter(&nodes[1], 1.0, 2.0, 1));
    injected_where(&nodes[1].platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    nodes[1].platform.take_injected();
    (lab, nodes)
}

async fn still_linked(nodes: &[TestNode]) {
    settle().await;
    assert_eq!(connected(&nodes[0]).await, 1);
    assert_eq!(connected(&nodes[1]).await, 1);
}

#[tokio::test]
async fn a_refused_key_is_skipped_and_later_input_is_injected_in_order() {
    use mouser_testkit::InputKind;
    let (_lab, nodes) = entered().await;
    let (a, b) = (&nodes[0], &nodes[1]);
    b.platform.refuse_input(&[InputKind::Key]);
    send_to(a, b, key(2, A, true));
    settle().await;
    b.platform.accept_input();
    send_to(a, b, button_msg(3, 1, true));
    send_to(a, b, key(4, C, true));
    send_to(a, b, key(5, C, false));
    injected_where(&b.platform, "the later key", |i| {
        *i == Injected::Key {
            usage: C,
            down: false,
        }
    })
    .await;
    assert_eq!(
        b.platform.injected(),
        vec![
            Injected::Button {
                button: 1,
                down: true
            },
            Injected::Key {
                usage: C,
                down: true
            },
            Injected::Key {
                usage: C,
                down: false
            },
        ]
    );
    still_linked(&nodes).await;
}

#[tokio::test]
async fn a_refused_button_scroll_and_motion_are_skipped_in_turn() {
    use mouser_testkit::InputKind;
    let (_lab, nodes) = entered().await;
    let (a, b) = (&nodes[0], &nodes[1]);
    b.platform.refuse_input(&[InputKind::Button]);
    send_to(a, b, button_msg(2, 1, true));
    settle().await;
    b.platform.refuse_input(&[InputKind::Scroll]);
    send_to(a, b, scroll_msg(3));
    settle().await;
    b.platform.refuse_input(&[InputKind::Motion]);
    datagram_to(a, b, &motion(b, 4, 50.0));
    settle().await;
    b.platform.accept_input();
    assert!(b.platform.injected().is_empty());
    datagram_to(a, b, &motion(b, 5, 60.0));
    send_to(a, b, scroll_msg(6));
    send_to(a, b, button_msg(7, 1, true));
    injected_where(&b.platform, "the button", |i| {
        *i == Injected::Button {
            button: 1,
            down: true,
        }
    })
    .await;
    let injected = b.platform.injected();
    assert!(injected.contains(&Injected::MotionAbs { x: 60.0, y: 20.0 }));
    assert!(injected.contains(&Injected::Scroll { dx: 0.0, dy: 3.0 }));
    still_linked(&nodes).await;
}

#[tokio::test]
async fn a_refused_entry_pointer_move_still_starts_the_session() {
    use mouser_testkit::InputKind;
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    b.platform.refuse_input(&[InputKind::Motion]);
    send_to(a, b, enter(b, 1.0, 2.0, 1));
    settle().await;
    b.platform.accept_input();
    send_to(a, b, key(2, A, true));
    injected_where(&b.platform, "the key", |i| {
        *i == Injected::Key {
            usage: A,
            down: true,
        }
    })
    .await;
}

#[tokio::test]
async fn a_refused_release_still_clears_the_key_and_button_from_the_held_set() {
    use mouser_testkit::InputKind;
    let (_lab, nodes) = entered().await;
    let (a, b) = (&nodes[0], &nodes[1]);
    send_to(a, b, key(2, A, true));
    send_to(a, b, button_msg(3, 1, true));
    injected_where(&b.platform, "the press", |i| {
        *i == Injected::Button {
            button: 1,
            down: true,
        }
    })
    .await;
    assert_eq!(
        b.engine
            .ctx
            .crossing
            .guard()
            .receiving
            .as_ref()
            .unwrap()
            .injected
            .keys
            .len(),
        1
    );
    b.platform
        .refuse_input(&[InputKind::Key, InputKind::Button]);
    send_to(a, b, key(4, A, false));
    send_to(a, b, button_msg(5, 1, false));
    settle().await;
    {
        let crossing = b.engine.ctx.crossing.guard();
        let held = &crossing.receiving.as_ref().unwrap().injected;
        assert!(held.keys.is_empty() && held.buttons.is_empty(), "{held:?}");
    }
    // Releasing at the end of the session while still refused is skipped too.
    send_to(a, b, key(6, C, true));
    b.platform.accept_input();
    send_to(a, b, key(6, C, true));
    send_to(a, b, key(7, C, true));
    settle().await;
    b.platform
        .refuse_input(&[InputKind::Key, InputKind::Button]);
    send_to(a, b, ControlMessage::ReleaseAll { seq: 8 });
    settle().await;
    assert!(b.engine.ctx.crossing.guard().receiving.is_none());
    still_linked(&nodes).await;
}

#[tokio::test]
async fn a_refused_press_is_not_recorded_as_held() {
    use mouser_testkit::InputKind;
    let (_lab, nodes) = entered().await;
    let (a, b) = (&nodes[0], &nodes[1]);
    b.platform
        .refuse_input(&[InputKind::Key, InputKind::Button]);
    send_to(a, b, key(2, A, true));
    send_to(a, b, button_msg(3, 1, true));
    settle().await;
    let crossing = b.engine.ctx.crossing.guard();
    let held = &crossing.receiving.as_ref().unwrap().injected;
    assert!(held.keys.is_empty() && held.buttons.is_empty(), "{held:?}");
}
