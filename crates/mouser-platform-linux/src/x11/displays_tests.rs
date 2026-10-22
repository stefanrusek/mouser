use super::connect;
use super::displays::{displays, watch};
use super::testserver::Xvfb;
use std::sync::mpsc::channel;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::randr::{ConnectionExt as _, MonitorInfo};
use x11rb::protocol::xproto::ConnectionExt as _;

fn monitor(
    conn: &impl Connection,
    root: u32,
    name: &str,
    (x, y, w, h): (i16, i16, u16, u16),
    (mm_w, mm_h): (u32, u32),
) {
    let atom = conn
        .intern_atom(false, name.as_bytes())
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    conn.randr_set_monitor(
        root,
        MonitorInfo {
            name: atom,
            primary: false,
            automatic: false,
            x,
            y,
            width: w,
            height: h,
            width_in_millimeters: mm_w,
            height_in_millimeters: mm_h,
            outputs: vec![],
        },
    )
    .unwrap()
    .check()
    .unwrap();
}

#[test]
fn randr_monitors_give_rectangles_scale_and_physical_size() {
    let server = Xvfb::start();
    let (conn, root) = connect(&server.env());
    monitor(&conn, root, "DP-1", (0, 0, 1280, 1024), (340, 270));
    monitor(&conn, root, "DP-2", (1280, 0, 640, 480), (0, 0));
    let all = displays(&conn, root);
    let named = |n: &str| all.iter().find(|d| d.name == n).unwrap();
    let left = named("DP-1");
    assert_eq!(left.key, "DP-1");
    assert_eq!(left.pixel_size, (1280, 1024));
    assert_eq!(left.logical_size, (1280.0, 1024.0));
    assert_eq!(left.scale, 1.0);
    assert_eq!(left.physical_size_mm, Some((340.0, 270.0)));
    assert_eq!(left.origin, (0.0, 0.0));
    let right = named("DP-2");
    assert_eq!(right.origin, (1280.0, 0.0));
    assert_eq!(right.physical_size_mm, None);
}

#[test]
fn screen_changes_fire_the_watch_and_dropping_the_handle_stops_it() {
    let server = Xvfb::start();
    let (tx, rx) = channel();
    let handle = watch(&server.env(), tx);
    let (conn, root) = connect(&server.env());
    let resize = |mm_width| {
        conn.randr_set_screen_size(root, 1920, 1080, mm_width, 340)
            .unwrap()
            .check()
            .unwrap();
    };
    resize(600);
    let seen = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(!seen.is_empty());
    drop(handle);
    while rx.try_recv().is_ok() {}
    resize(700);
    assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
}
