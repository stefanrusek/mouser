use super::capture::X11Capture;
use super::connect;
use super::echo::Echo;
use super::inject::{X11Inject, clicks, x_button};
use super::testserver::Xvfb;
use mouser_platform::{Error, InputCapture, InputInject, LocalInput};
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use x11rb::protocol::xproto::{ConnectionExt as _, KeyButMask};

#[test]
fn mouser_buttons_map_to_x_buttons() {
    assert_eq!([1, 2, 3, 4, 5, 6].map(x_button), [1, 3, 2, 8, 9, 10]);
}

#[test]
fn scroll_accumulates_whole_wheel_clicks() {
    let mut acc = 20.0;
    assert_eq!(clicks(&mut acc), 1);
    assert_eq!(acc, 5.0);
    acc += 10.0;
    assert_eq!(clicks(&mut acc), 1);
    let mut negative = -31.0;
    assert_eq!(clicks(&mut negative), -2);
    assert_eq!(negative, -1.0);
    let mut small = 3.0;
    assert_eq!(clicks(&mut small), 0);
    assert_eq!(small, 3.0);
}

struct Rig {
    inject: X11Inject,
    conn: x11rb::rust_connection::RustConnection,
    root: u32,
    // Last: the server must outlive everything that talks to it.
    server: Xvfb,
}

fn rig() -> Rig {
    let server = Xvfb::start();
    let inject = X11Inject::new(&server.env(), Arc::new(Echo::default()));
    let (conn, root) = connect(&server.env());
    Rig {
        inject,
        conn,
        root,
        server,
    }
}

impl Rig {
    fn pointer(&self) -> (i16, i16, KeyButMask) {
        let at = self.conn.query_pointer(self.root).unwrap().reply().unwrap();
        (at.root_x, at.root_y, at.mask)
    }

    fn key_down(&self, keycode: usize) -> bool {
        let keys = self.conn.query_keymap().unwrap().reply().unwrap().keys;
        keys[keycode / 8] & (1 << (keycode % 8)) != 0
    }

    fn button_down(&self, mask: KeyButMask) -> bool {
        self.pointer().2.contains(mask)
    }
}

/// XTest input is applied by the server's input loop, a moment after the request.
fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn absolute_motion_places_the_pointer() {
    let mut r = rig();
    r.inject.motion_abs(321.0, 123.4).unwrap();
    eventually("the pointer to arrive", || {
        let (x, y, _) = r.pointer();
        (x, y) == (321, 123)
    });
}

#[test]
fn buttons_press_and_release() {
    let mut r = rig();
    r.inject.button(1, true).unwrap();
    eventually("button 1 down", || r.button_down(KeyButMask::BUTTON1));
    r.inject.button(1, false).unwrap();
    eventually("button 1 up", || !r.button_down(KeyButMask::BUTTON1));
    r.inject.button(2, true).unwrap();
    eventually("the right button down", || {
        r.button_down(KeyButMask::BUTTON3)
    });
    r.inject.button(2, false).unwrap();
}

#[test]
fn keys_press_and_release_by_hid_usage() {
    let mut r = rig();
    r.inject.key(0x0007_0004, true).unwrap();
    eventually("A down", || r.key_down(38));
    r.inject.key(0x0007_0004, false).unwrap();
    eventually("A up", || !r.key_down(38));
}

#[test]
fn an_unmapped_usage_is_an_error() {
    let mut r = rig();
    assert!(matches!(r.inject.key(0x0001_0004, true), Err(Error::Os(_))));
}

#[test]
fn scroll_becomes_wheel_clicks_and_keeps_the_remainder() {
    let mut r = rig();
    // A capture with its own (empty) echo sees exactly what XTest produced.
    let mut capture = X11Capture::new(&r.server.env(), Arc::default());
    let (tx, rx) = channel();
    capture.start(tx).unwrap();
    r.inject.scroll(15.0, 30.0).unwrap();
    r.inject.scroll(-15.0, -15.0).unwrap();
    r.inject.scroll(0.0, 5.0).unwrap();
    r.inject.scroll(0.0, 5.0).unwrap();
    r.inject.scroll(0.0, 5.0).unwrap();
    let scroll = |dx, dy| LocalInput::Scroll { dx, dy };
    let expected = [
        scroll(0.0, 15.0),
        scroll(0.0, 15.0),
        scroll(15.0, 0.0),
        scroll(0.0, -15.0),
        scroll(-15.0, 0.0),
        scroll(0.0, 15.0),
    ];
    for want in expected {
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), want);
    }
    assert!(rx.recv_timeout(Duration::from_millis(300)).is_err());
}

#[test]
fn secure_attention_is_unsupported() {
    let mut r = rig();
    assert_eq!(r.inject.secure_attention(), Err(Error::Unsupported));
}

#[test]
fn held_input_is_released_on_drop() {
    let Rig {
        mut inject,
        conn,
        root,
        server,
    } = rig();
    inject.key(0x0007_0004, true).unwrap();
    inject.button(1, true).unwrap();
    let held = || {
        let keys = conn.query_keymap().unwrap().reply().unwrap().keys;
        let mask = conn.query_pointer(root).unwrap().reply().unwrap().mask;
        (keys[4] & (1 << 6) != 0, mask.contains(KeyButMask::BUTTON1))
    };
    eventually("input to be held", || held() == (true, true));
    drop(inject);
    eventually("input to be released by the drop", || {
        held() == (false, false)
    });
    drop(server);
}

#[test]
fn dropping_an_injector_that_never_connected_is_fine() {
    let server = Xvfb::start();
    drop(X11Inject::new(&server.env(), Arc::default()));
}
