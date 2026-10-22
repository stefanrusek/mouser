use super::connect;
use super::fullscreen::foreground_is_fullscreen;
use super::testserver::Xvfb;
use crate::{LinuxPlatform, Settings};
use mouser_platform::{Capability, Platform};
use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt as _, CreateWindowAux, PropMode, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

fn atom(conn: &RustConnection, name: &str) -> u32 {
    conn.intern_atom(false, name.as_bytes())
        .unwrap()
        .reply()
        .unwrap()
        .atom
}

fn window(conn: &RustConnection, root: Window) -> Window {
    let id = conn.generate_id().unwrap();
    conn.create_window(
        0,
        id,
        root,
        0,
        0,
        10,
        10,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new(),
    )
    .unwrap();
    id
}

/// What the window manager does: names the active window on the root, and
/// lists `states` on that window.
fn focus(conn: &RustConnection, root: Window, window: Window, states: &[&str]) {
    let states: Vec<u32> = states.iter().map(|s| atom(conn, s)).collect();
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atom(conn, "_NET_WM_STATE"),
        AtomEnum::ATOM,
        &states,
    )
    .unwrap();
    conn.change_property32(
        PropMode::REPLACE,
        root,
        atom(conn, "_NET_ACTIVE_WINDOW"),
        AtomEnum::WINDOW,
        &[window],
    )
    .unwrap();
    conn.get_input_focus().unwrap().reply().unwrap();
}

#[test]
fn without_a_window_manager_nothing_is_full_screen() {
    let server = Xvfb::start();
    let (conn, root) = connect(&server.env());
    assert!(!foreground_is_fullscreen(&conn, root));
}

#[test]
fn the_active_window_is_full_screen_when_its_state_says_so() {
    let server = Xvfb::start();
    let (conn, root) = connect(&server.env());
    let app = window(&conn, root);
    focus(&conn, root, app, &["_NET_WM_STATE_MAXIMIZED_VERT"]);
    assert!(!foreground_is_fullscreen(&conn, root));
    focus(
        &conn,
        root,
        app,
        &["_NET_WM_STATE_ABOVE", "_NET_WM_STATE_FULLSCREEN"],
    );
    assert!(foreground_is_fullscreen(&conn, root));
}

#[test]
fn an_active_window_without_a_state_is_not_full_screen() {
    let server = Xvfb::start();
    let (conn, root) = connect(&server.env());
    let app = window(&conn, root);
    conn.change_property32(
        PropMode::REPLACE,
        root,
        atom(&conn, "_NET_ACTIVE_WINDOW"),
        AtomEnum::WINDOW,
        &[app],
    )
    .unwrap();
    assert!(!foreground_is_fullscreen(&conn, root));
}

#[test]
fn an_active_window_that_has_gone_is_not_full_screen() {
    let server = Xvfb::start();
    let (conn, root) = connect(&server.env());
    let app = window(&conn, root);
    focus(&conn, root, app, &["_NET_WM_STATE_FULLSCREEN"]);
    conn.destroy_window(app).unwrap();
    conn.get_input_focus().unwrap().reply().unwrap();
    assert!(!foreground_is_fullscreen(&conn, root));
}

fn platform(session: &str, display: HashMap<String, String>) -> (LinuxPlatform, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("os-release"),
        "NAME=\"Testix\"\nVERSION_ID=\"3\"\n",
    )
    .unwrap();
    let settings = Settings {
        env: HashMap::from([("XDG_SESSION_TYPE".to_string(), session.to_string())])
            .into_iter()
            .chain(display)
            .collect(),
        sys_class_net: "/sys/class/net".into(),
        os_release: dir.path().join("os-release"),
        exe: "/opt/mouser/mouser".into(),
        device_wait: std::time::Duration::from_millis(300),
    };
    (LinuxPlatform::with_settings(settings), dir)
}

#[test]
fn the_platform_reads_the_x11_session() {
    let server = Xvfb::start();
    let (p, _dir) = platform("x11", server.env());
    assert!(!p.foreground_is_fullscreen());
    let (conn, root) = connect(&server.env());
    let app = window(&conn, root);
    focus(&conn, root, app, &["_NET_WM_STATE_FULLSCREEN"]);
    assert!(p.foreground_is_fullscreen());
}

#[test]
fn on_wayland_nothing_is_ever_full_screen_and_the_limitation_is_listed() {
    let (p, _dir) = platform("wayland", HashMap::new());
    assert!(!p.foreground_is_fullscreen());
    assert!(p.info().limitations.iter().any(|l| {
        l.capability == Capability::Capture
            && l.text == "Mouser cannot tell when an app is full-screen on Wayland, so the pointer may still cross to another computer during a game or video."
    }));
    let (x11, _dir) = platform("x11", HashMap::new());
    assert!(
        !x11.info()
            .limitations
            .iter()
            .any(|l| l.text.contains("full-screen"))
    );
}
