use mouser_platform::LocalInput;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::DisplayId;

use super::state::*;

fn remote(c: u8) -> Control {
    Control::Remote(Remote {
        computer: ComputerId([c; 16]),
        display: DisplayId {
            computer: ComputerId([c; 16]),
            key: "d".into(),
        },
        at: (0.0, 0.0),
        speed: (1.0, 1.0),
        back: (5.0, 5.0),
    })
}

#[test]
fn a_session_starts_local_with_nothing_held() {
    let mut s = Session::default();
    assert_eq!(s.control, Control::Local);
    assert_eq!(s.remote(), None);
    assert!(s.remote_mut().is_none());
    assert!(s.held.is_empty() && s.sent.is_empty());
}

#[test]
fn physical_keys_and_buttons_are_tracked_and_motion_is_not() {
    let mut s = Session::default();
    s.track(LocalInput::Key {
        usage: 4,
        down: true,
    });
    s.track(LocalInput::Button {
        button: 1,
        down: true,
    });
    s.track(LocalInput::Key {
        usage: 5,
        down: true,
    });
    s.track(LocalInput::Key {
        usage: 5,
        down: false,
    });
    s.track(LocalInput::Motion {
        dx: 1.0,
        dy: 1.0,
        abs: (1.0, 1.0),
    });
    s.track(LocalInput::Scroll { dx: 1.0, dy: 1.0 });
    assert_eq!(s.held.keys.iter().copied().collect::<Vec<_>>(), [4]);
    assert_eq!(s.held.buttons.iter().copied().collect::<Vec<_>>(), [1]);
    s.track(LocalInput::Key {
        usage: 4,
        down: false,
    });
    s.track(LocalInput::Button {
        button: 1,
        down: false,
    });
    assert!(s.held.is_empty());
}

#[test]
fn leaving_this_computer_releases_what_is_physically_held() {
    let mut s = Session::default();
    s.track(LocalInput::Key {
        usage: 0xE1,
        down: true,
    });
    s.track(LocalInput::Button {
        button: 1,
        down: true,
    });
    s.sent.keys.insert(9);
    let Leave::Local(release) = s.leave() else {
        panic!("leaving the local computer");
    };
    assert_eq!(release.keys.iter().copied().collect::<Vec<_>>(), [0xE1]);
    assert_eq!(release.buttons.iter().copied().collect::<Vec<_>>(), [1]);
    assert!(
        s.sent.is_empty(),
        "nothing counts as sent after a transition"
    );
    // The keys are still physically down.
    assert!(!s.held.is_empty());
}

#[test]
fn leaving_a_remote_computer_has_it_release_everything() {
    let mut s = Session {
        control: remote(3),
        ..Session::default()
    };
    s.sent.keys.insert(4);
    s.sent.buttons.insert(1);
    assert_eq!(s.remote(), Some(ComputerId([3; 16])));
    assert_eq!(s.remote_mut().unwrap().computer, ComputerId([3; 16]));
    assert_eq!(s.leave(), Leave::Remote(ComputerId([3; 16])));
    assert!(s.sent.is_empty());
}

#[test]
fn sequence_numbers_only_grow() {
    let mut s = Session::default();
    assert_eq!((s.next_seq(), s.next_seq(), s.next_seq()), (1, 2, 3));
}

mod control_changed {
    //! Every control move writes `control.changed {from, to, how}` (Story 2
    //! scenario 2); on the computer that is `to`, the application in front
    //! comes with it.

    use mouser_platform::{CaptureLossReason, ForegroundApp, LocalInput};
    use mouser_proto::control::{Empty, Method};
    use serde_json::{Value, json};

    use crate::crossing::panic::usage_for_name;
    use crate::lock::LockExt;
    use crate::testing::*;

    const CTRL: u32 = 0x0007_00E0;
    const SHIFT: u32 = 0x0007_00E1;
    const ALT: u32 = 0x0007_00E2;
    const ESC: u32 = 0x0007_0029;

    fn app(name: &str, admin: bool) -> ForegroundApp {
        ForegroundApp {
            name: name.into(),
            admin,
            outranks_mouser: false,
        }
    }

    fn tap(node: &TestNode, usage: u32) {
        for down in [true, false] {
            node.platform.send_input(LocalInput::Key { usage, down });
        }
    }

    fn key(letter: char) -> u32 {
        usage_for_name(&letter.to_string()).unwrap()
    }

    /// Ctrl+Alt+Shift+`letter`, the QuickJump way.
    fn chord(node: &TestNode, letter: char) {
        for usage in [CTRL, ALT, SHIFT] {
            node.platform
                .send_input(LocalInput::Key { usage, down: true });
        }
        tap(node, key(letter));
        for usage in [SHIFT, ALT, CTRL] {
            node.platform
                .send_input(LocalInput::Key { usage, down: false });
        }
    }

    /// The `control.changed` record `node` wrote for a move from `from` to
    /// `to`, however it happened.
    async fn moved(node: &TestNode, from: &TestNode, to: &TestNode, how: &str) -> Value {
        let (from, to) = (from.engine.computer_id(), to.engine.computer_id());
        logged(node, "control.changed", |r| {
            r["fields"]["from"] == from.to_string()
                && r["fields"]["to"] == to.to_string()
                && r["fields"]["how"] == how
        })
        .await
    }

    async fn two() -> (Lab, Vec<TestNode>) {
        let lab = Lab::new();
        let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
        nodes[0]
            .platform
            .set_foreground_app(Some(app("Terminal", false)));
        nodes[1]
            .platform
            .set_foreground_app(Some(app("regedit.exe", true)));
        (lab, nodes)
    }

    #[tokio::test]
    async fn a_crossing_there_and_back_is_logged_with_the_application_where_control_arrived() {
        let (_lab, nodes) = two().await;
        let (a, b) = (&nodes[0], &nodes[1]);
        let (ia, ib) = (a.engine.computer_id(), b.engine.computer_id());
        push_right(a);
        // The computer control left says only how; the one it reached also
        // names the application in front and whether it runs as administrator.
        let left = moved(a, a, b, "crossing").await;
        assert_eq!(left["level"], "info");
        assert_eq!(
            left["fields"],
            json!({"from": ia.to_string(), "to": ib.to_string(), "how": "crossing"})
        );
        let reached = moved(b, a, b, "entered").await;
        assert_eq!(
            reached["fields"],
            json!({
                "from": ia.to_string(), "to": ib.to_string(), "how": "entered",
                "app": "regedit.exe", "app_admin": true
            })
        );
        push_back(&nodes, 0);
        let back = moved(a, b, a, "crossing").await;
        assert_eq!(
            back["fields"],
            json!({
                "from": ib.to_string(), "to": ia.to_string(), "how": "crossing",
                "app": "Terminal", "app_admin": false
            })
        );
    }

    #[tokio::test]
    async fn the_panic_key_pressed_here_or_on_a_desk_mate_is_how_panic() {
        let (_lab, nodes) = two().await;
        let (a, b) = (&nodes[0], &nodes[1]);
        push_right(a);
        moved(a, a, b, "crossing").await;
        tap(a, ESC);
        tap(a, ESC);
        let here = moved(a, b, a, "panic").await;
        assert_eq!(here["fields"]["app"], "Terminal");
        // Again, this time pressed on the computer being driven.
        push_right(a);
        eventually("Beta to have the pointer again", || async {
            a.platform.pointer_hidden() && b.engine.ctx.crossing.guard().receiving.is_some()
        })
        .await;
        tap(b, ESC);
        tap(b, ESC);
        eventually("control to come back", || async {
            !a.platform.pointer_hidden()
        })
        .await;
        let records = log_events(a, "control.changed")
            .into_iter()
            .filter(|r| r["fields"]["how"] == "panic")
            .count();
        assert_eq!(records, 2);
    }

    #[tokio::test]
    async fn pausing_is_how_pause() {
        let (_lab, nodes) = two().await;
        let (a, b) = (&nodes[0], &nodes[1]);
        push_right(a);
        moved(a, a, b, "crossing").await;
        let _: Empty = call(a, Method::ActivePause, json!({})).await;
        moved(a, b, a, "pause").await;
    }

    #[tokio::test]
    async fn quickjump_there_and_back_is_how_quickjump() {
        let (_lab, nodes) = two().await;
        let (a, b) = (&nodes[0], &nodes[1]);
        // Left to right on the desk: Alpha's display is A, Beta's is B.
        chord(a, 'J');
        tap(a, key('B'));
        moved(a, a, b, "quickjump").await;
        chord(a, 'J');
        tap(a, key('A'));
        moved(a, b, a, "quickjump").await;
    }

    #[tokio::test]
    async fn a_lost_link_is_how_link_lost() {
        let (_lab, mut nodes) = two().await;
        let b = nodes.pop().unwrap();
        let a = nodes.pop().unwrap();
        push_right(&a);
        moved(&a, &a, &b, "crossing").await;
        let beta_id = b.engine.computer_id().to_string();
        b.engine.shutdown().await;
        let lost = logged(&a, "control.changed", |r| r["fields"]["how"] == "link_lost").await;
        assert_eq!(lost["fields"]["from"], beta_id);
        assert_eq!(lost["fields"]["app"], "Terminal");
    }

    #[tokio::test]
    async fn the_backend_losing_capture_is_how_capture_lost() {
        let (_lab, nodes) = two().await;
        let (a, b) = (&nodes[0], &nodes[1]);
        push_right(a);
        moved(a, a, b, "crossing").await;
        a.platform.lose_capture(CaptureLossReason::HooksSilent);
        moved(a, b, a, "capture_lost").await;
    }

    #[tokio::test]
    async fn recovering_from_a_fault_is_how_fault() {
        let (_lab, nodes) = two().await;
        let (a, b) = (&nodes[0], &nodes[1]);
        push_right(a);
        moved(a, a, b, "crossing").await;
        a.engine.ctx.crossing.guard().recover_after_fault();
        moved(a, b, a, "fault").await;
    }

    #[tokio::test]
    async fn quitting_while_remote_is_how_quit() {
        let (_lab, mut nodes) = two().await;
        let b = nodes.pop().unwrap();
        let a = nodes.pop().unwrap();
        push_right(&a);
        moved(&a, &a, &b, "crossing").await;
        let (ia, ib) = (a.engine.computer_id(), b.engine.computer_id());
        let path = a.engine.log().path();
        a.engine.shutdown().await;
        let quit = read_log(&path)
            .into_iter()
            .find(|r| r["event"] == "control.changed" && r["fields"]["how"] == "quit")
            .expect("the move is logged before Alpha is gone");
        assert_eq!(quit["fields"]["from"], ib.to_string());
        assert_eq!(quit["fields"]["to"], ia.to_string());
    }

    #[tokio::test]
    async fn going_on_from_one_remote_computer_to_another_is_a_crossing_between_them() {
        let lab = Lab::new();
        let nodes = logged_desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
        let (a, b, g) = (&nodes[0], &nodes[1], &nodes[2]);
        push_right(a);
        moved(a, a, b, "crossing").await;
        // Far enough to leave Beta's display on its right.
        a.platform.send_input(LocalInput::Motion {
            dx: 3000.0,
            dy: 0.0,
            abs: (0.0, 0.0),
        });
        moved(a, b, g, "crossing").await;
        moved(g, a, g, "entered").await;
    }
}
