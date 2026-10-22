use super::capture::*;
use super::connect;
use super::echo::Echo;
use super::testserver::Xvfb;
use mouser_platform::{InputCapture, LocalInput};
use std::cell::Cell;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xinput::Fp3232;
use x11rb::protocol::xproto::{ConnectionExt as _, EventMask, GrabMode, GrabStatus};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

fn fp(integral: i32, frac: u32) -> Fp3232 {
    Fp3232 { integral, frac }
}

// ---- pure translation ----

#[test]
fn motion_deltas_come_from_valuators_zero_and_one() {
    assert_eq!(motion_delta(&[0b11], &[fp(5, 0), fp(-3, 0)]), (5.0, -3.0));
    assert_eq!(motion_delta(&[0b10], &[fp(7, 0)]), (0.0, 7.0));
    assert_eq!(
        motion_delta(&[0b101], &[fp(2, 1 << 31), fp(99, 0)]),
        (2.5, 0.0)
    );
    assert_eq!(motion_delta(&[], &[]), (0.0, 0.0));
}

#[test]
fn buttons_use_mouser_numbering() {
    let button = |button, down| LocalInput::Button { button, down };
    assert_eq!(button_input(1, true), Some(button(1, true)));
    assert_eq!(button_input(3, false), Some(button(2, false)));
    assert_eq!(button_input(2, true), Some(button(3, true)));
    assert_eq!(button_input(8, true), Some(button(4, true)));
    assert_eq!(button_input(9, false), Some(button(5, false)));
    assert_eq!(button_input(10, true), Some(button(6, true)));
    assert_eq!(button_input(0, true), None);
    assert_eq!(button_input(300, true), None);
}

#[test]
fn wheel_buttons_scroll_on_press_only() {
    let scroll = |dx, dy| Some(LocalInput::Scroll { dx, dy });
    assert_eq!(button_input(4, true), scroll(0.0, 15.0));
    assert_eq!(button_input(5, true), scroll(0.0, -15.0));
    assert_eq!(button_input(6, true), scroll(-15.0, 0.0));
    assert_eq!(button_input(7, true), scroll(15.0, 0.0));
    for wheel in 4..=7 {
        assert_eq!(button_input(wheel, false), None);
    }
}

#[test]
fn keys_use_hid_usages_and_unmapped_keys_are_dropped() {
    let a = Some(LocalInput::Key {
        usage: 0x0007_0004,
        down: true,
    });
    assert_eq!(key_input(38, true), a);
    assert_eq!(key_input(8, true), None);
    assert_eq!(key_input(300, true), None);
}

#[test]
fn unknown_events_mean_nothing() {
    assert_eq!(classify(&Event::Unknown(vec![0; 32])), Raw::Other);
}

fn interpret_with(raw: &Raw, echo: &Echo) -> (Option<LocalInput>, u32) {
    let parked = Cell::new(0);
    let input = interpret(raw, echo, Instant::now(), &|| (30, 40), &|| {
        parked.set(parked.get() + 1)
    });
    (input, parked.get())
}

#[test]
fn a_motion_reports_where_the_pointer_is_and_parks_it() {
    let echo = Echo::default();
    let (input, parked) = interpret_with(&Raw::Motion { dx: 1.0, dy: -2.0 }, &echo);
    assert_eq!(
        input,
        Some(LocalInput::Motion {
            dx: 1.0,
            dy: -2.0,
            abs: (30.0, 40.0)
        })
    );
    assert_eq!(parked, 1);
}

#[test]
fn echoes_of_our_own_injection_are_dropped_once() {
    let echo = Echo::default();
    let now = Instant::now();
    echo.record_motion(30, 40, now);
    echo.record_button(1, true, now);
    echo.record_key(38, false, now);
    let motion = Raw::Motion { dx: 1.0, dy: 1.0 };
    assert_eq!(interpret_with(&motion, &echo).0, None);
    let button = Raw::Button {
        code: 1,
        down: true,
    };
    assert_eq!(interpret_with(&button, &echo).0, None);
    assert!(interpret_with(&button, &echo).0.is_some());
    let key = Raw::Key {
        code: 38,
        down: false,
        repeat: false,
    };
    assert_eq!(interpret_with(&key, &echo).0, None);
    assert!(interpret_with(&key, &echo).0.is_some());
}

#[test]
fn key_auto_repeat_and_unrelated_events_are_not_forwarded() {
    let echo = Echo::default();
    let repeat = Raw::Key {
        code: 38,
        down: true,
        repeat: true,
    };
    assert_eq!(interpret_with(&repeat, &echo).0, None);
    assert_eq!(interpret_with(&Raw::Other, &echo), (None, 0));
    assert_eq!(interpret_with(&Raw::Stop, &echo), (None, 0));
}

// ---- against an X server ----

struct Session {
    capture: X11Capture,
    rx: Receiver<LocalInput>,
    conn: RustConnection,
    root: u32,
    // Last: the server must outlive everything that talks to it.
    server: Xvfb,
}

fn session() -> Session {
    let server = Xvfb::start();
    let mut capture = X11Capture::new(&server.env(), Arc::default());
    let (tx, rx) = channel();
    capture.start(tx).unwrap();
    let (conn, root) = connect(&server.env());
    Session {
        capture,
        rx,
        conn,
        root,
        server,
    }
}

impl Session {
    /// A hardware-like event from another client.
    fn fake(&self, kind: u8, detail: u8, x: i16, y: i16) {
        self.conn
            .xtest_fake_input(kind, detail, 0, self.root, x, y, 0)
            .unwrap();
        self.conn.flush().unwrap();
    }

    fn next(&self) -> LocalInput {
        self.rx.recv_timeout(Duration::from_secs(10)).unwrap()
    }

    fn pointer(&self) -> (i16, i16) {
        let at = self.conn.query_pointer(self.root).unwrap().reply().unwrap();
        (at.root_x, at.root_y)
    }

    fn try_grab(&self) -> GrabStatus {
        self.conn
            .grab_pointer(
                false,
                self.root,
                EventMask::NO_EVENT,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                x11rb::NONE,
                x11rb::NONE,
                x11rb::CURRENT_TIME,
            )
            .unwrap()
            .reply()
            .unwrap()
            .status
    }
}

const MOTION: u8 = 6;
const KEY_PRESS: u8 = 2;
const KEY_RELEASE: u8 = 3;
const BUTTON_PRESS: u8 = 4;
const BUTTON_RELEASE: u8 = 5;

#[test]
fn pointer_motion_arrives_as_deltas_with_the_absolute_position() {
    let s = session();
    s.fake(MOTION, 0, 100, 100);
    let first = s.next();
    assert!(
        matches!(
            first,
            LocalInput::Motion {
                abs: (100.0, 100.0),
                ..
            }
        ),
        "{first:?}"
    );
    s.fake(MOTION, 1, 7, -3);
    match s.next() {
        LocalInput::Motion { dx, dy, abs } => {
            assert!(dx > 0.0 && dy < 0.0, "{dx} {dy}");
            assert_eq!(abs, (107.0, 97.0));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn buttons_wheel_and_keys_arrive_as_local_input() {
    let s = session();
    s.fake(BUTTON_PRESS, 3, 0, 0);
    assert_eq!(
        s.next(),
        LocalInput::Button {
            button: 2,
            down: true
        }
    );
    s.fake(BUTTON_RELEASE, 3, 0, 0);
    assert_eq!(
        s.next(),
        LocalInput::Button {
            button: 2,
            down: false
        }
    );
    s.fake(BUTTON_PRESS, 4, 0, 0);
    s.fake(BUTTON_RELEASE, 4, 0, 0);
    assert_eq!(s.next(), LocalInput::Scroll { dx: 0.0, dy: 15.0 });
    s.fake(KEY_PRESS, 38, 0, 0);
    assert_eq!(
        s.next(),
        LocalInput::Key {
            usage: 0x0007_0004,
            down: true
        }
    );
    s.fake(KEY_RELEASE, 38, 0, 0);
    assert_eq!(
        s.next(),
        LocalInput::Key {
            usage: 0x0007_0004,
            down: false
        }
    );
    // An unmapped key and a stopped capture deliver nothing more.
    s.fake(KEY_PRESS, 8, 0, 0);
    s.fake(KEY_RELEASE, 8, 0, 0);
    assert!(s.rx.recv_timeout(Duration::from_millis(300)).is_err());
}

#[test]
fn stopping_ends_delivery_and_dropping_a_stopped_capture_is_fine() {
    let mut s = session();
    s.capture.stop();
    s.fake(BUTTON_PRESS, 1, 0, 0);
    s.fake(BUTTON_RELEASE, 1, 0, 0);
    assert!(s.rx.recv_timeout(Duration::from_millis(300)).is_err());
    s.capture.stop();
}

#[test]
fn grabbing_holds_the_pointer_still_and_swallows_local_input() {
    let mut s = session();
    s.fake(MOTION, 0, 200, 300);
    s.next();
    s.capture.set_grabbed(true).unwrap();
    s.capture.set_grabbed(true).unwrap();
    wait_until(|| s.capture.grab_held());
    // Another client cannot take the grab while Mouser holds it.
    assert_eq!(s.try_grab(), GrabStatus::ALREADY_GRABBED);
    // The device moves, the pointer does not, and the motion is still reported.
    s.fake(MOTION, 1, 50, 20);
    match s.next() {
        LocalInput::Motion { dx, dy, .. } => assert!(dx > 0.0 && dy > 0.0),
        other => panic!("{other:?}"),
    }
    s.conn.get_input_focus().unwrap().reply().unwrap();
    wait_until(|| s.pointer() == (200, 300));
    // Releasing lets the pointer move again and frees the grab.
    s.capture.set_grabbed(false).unwrap();
    assert!(!s.capture.grab_held());
    assert_eq!(s.try_grab(), GrabStatus::SUCCESS);
    s.conn.ungrab_pointer(x11rb::CURRENT_TIME).unwrap();
}

#[test]
fn a_grab_held_by_another_client_is_retried_until_it_is_free() {
    let mut s = session();
    assert_eq!(s.try_grab(), GrabStatus::SUCCESS);
    s.capture.set_grabbed(true).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(!s.capture.grab_held());
    s.conn.ungrab_pointer(x11rb::CURRENT_TIME).unwrap();
    s.conn.flush().unwrap();
    wait_until(|| s.capture.grab_held());
    s.capture.set_grabbed(false).unwrap();
}

#[test]
fn releasing_while_still_retrying_gives_up_the_attempt() {
    let mut s = session();
    assert_eq!(s.try_grab(), GrabStatus::SUCCESS);
    s.capture.set_grabbed(true).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    s.capture.set_grabbed(false).unwrap();
    assert!(!s.capture.grab_held());
}

#[test]
fn warping_moves_the_pointer_and_moves_the_parking_spot() {
    let mut s = session();
    s.capture.warp(120.0, 140.0).unwrap();
    assert_eq!(s.pointer(), (120, 140));
    s.capture.set_grabbed(true).unwrap();
    wait_until(|| s.capture.grab_held());
    s.capture.warp(500.0, 400.0).unwrap();
    assert_eq!(s.pointer(), (500, 400));
    s.fake(MOTION, 1, 10, 10);
    wait_until(|| s.pointer() == (500, 400));
    s.capture.set_grabbed(false).unwrap();
    assert_eq!(s.server.display.chars().next(), Some(':'));
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}
