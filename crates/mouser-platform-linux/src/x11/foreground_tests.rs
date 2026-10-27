use super::connect;
use super::foreground::foreground_app;
use super::testserver::Xvfb;
use mouser_platform::ForegroundApp;
use std::path::Path;
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

fn activate(conn: &RustConnection, root: Window, window: Window) {
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

fn set_pid(conn: &RustConnection, window: Window, pid: u32) {
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atom(conn, "_NET_WM_PID"),
        AtomEnum::CARDINAL,
        &[pid],
    )
    .unwrap();
    conn.get_input_focus().unwrap().reply().unwrap();
}

/// A `/proc` holding one process.
fn proc_with(pid: u32, comm: &str, euid: u32) -> tempfile::TempDir {
    let proc = tempfile::tempdir().unwrap();
    let dir = proc.path().join(pid.to_string());
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
    std::fs::write(
        dir.join("status"),
        format!("Name:\t{comm}\nUid:\t1000\t{euid}\t1000\t1000\nGid:\t1000\t1000\t1000\t1000\n"),
    )
    .unwrap();
    proc
}

fn app(name: &str, admin: bool) -> Option<ForegroundApp> {
    Some(ForegroundApp {
        name: name.into(),
        admin,
        outranks_mouser: false,
    })
}

#[test]
fn the_active_window_names_its_process_through_its_pid() {
    let server = Xvfb::start();
    let (conn, root) = connect(&server.env());
    let proc = proc_with(4242, "editor", 1000);
    let w = window(&conn, root);
    set_pid(&conn, w, 4242);
    activate(&conn, root, w);
    assert_eq!(
        foreground_app(&conn, root, proc.path()),
        app("editor", false)
    );
}

#[test]
fn a_process_with_effective_uid_zero_is_admin() {
    let server = Xvfb::start();
    let (conn, root) = connect(&server.env());
    let proc = proc_with(7, "installer", 0);
    let w = window(&conn, root);
    set_pid(&conn, w, 7);
    activate(&conn, root, w);
    assert_eq!(
        foreground_app(&conn, root, proc.path()),
        app("installer", true)
    );
}

#[test]
fn without_an_active_window_there_is_no_foreground_app() {
    let server = Xvfb::start();
    let (conn, root) = connect(&server.env());
    assert_eq!(foreground_app(&conn, root, Path::new("/proc")), None);
}

#[test]
fn an_active_window_without_a_pid_or_a_process_cannot_be_named() {
    let server = Xvfb::start();
    let (conn, root) = connect(&server.env());
    let proc = proc_with(1, "init", 0);
    let w = window(&conn, root);
    activate(&conn, root, w);
    assert_eq!(foreground_app(&conn, root, proc.path()), None);
    set_pid(&conn, w, 99);
    assert_eq!(foreground_app(&conn, root, proc.path()), None);
}

#[test]
fn an_active_window_that_has_gone_cannot_be_named() {
    let server = Xvfb::start();
    let (conn, root) = connect(&server.env());
    let w = window(&conn, root);
    activate(&conn, root, w);
    conn.destroy_window(w).unwrap();
    conn.get_input_focus().unwrap().reply().unwrap();
    assert_eq!(foreground_app(&conn, root, Path::new("/proc")), None);
}

#[test]
fn a_status_without_a_uid_line_cannot_be_read() {
    let server = Xvfb::start();
    let (conn, root) = connect(&server.env());
    let proc = proc_with(5, "odd", 1000);
    std::fs::write(proc.path().join("5").join("status"), "Name:\todd\n").unwrap();
    let w = window(&conn, root);
    set_pid(&conn, w, 5);
    activate(&conn, root, w);
    assert_eq!(foreground_app(&conn, root, proc.path()), None);
}
