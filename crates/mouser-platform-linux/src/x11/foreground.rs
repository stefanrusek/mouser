//! The application in front, from the window manager's EWMH hints and `/proc`.

use mouser_platform::ForegroundApp;
use std::path::Path;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, Window};
use x11rb::rust_connection::RustConnection;

fn first_value(
    conn: &RustConnection,
    window: Window,
    property: &str,
    kind: AtomEnum,
) -> Option<u32> {
    let atom = conn
        .intern_atom(false, property.as_bytes())
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    conn.get_property(false, window, atom, kind, 0, 1)
        .unwrap()
        .reply()
        .ok()?
        .value32()?
        .next()
}

/// `_NET_ACTIVE_WINDOW` -> `_NET_WM_PID` -> `<proc>/<pid>/comm` for the name;
/// `admin` is whether that process's effective uid is 0. `None` when any link
/// is missing (no window manager, a window without a pid, a process that has
/// gone).
pub(crate) fn foreground_app(
    conn: &RustConnection,
    root: Window,
    proc: &Path,
) -> Option<ForegroundApp> {
    let window = first_value(conn, root, "_NET_ACTIVE_WINDOW", AtomEnum::WINDOW)?;
    let pid = first_value(conn, window, "_NET_WM_PID", AtomEnum::CARDINAL)?;
    let dir = proc.join(pid.to_string());
    let name = std::fs::read_to_string(dir.join("comm"))
        .ok()?
        .trim_end()
        .to_string();
    let status = std::fs::read_to_string(dir.join("status")).ok()?;
    let euid = status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))?
        .split_whitespace()
        .nth(1)?;
    Some(ForegroundApp {
        name,
        admin: euid == "0",
        outranks_mouser: false,
    })
}
