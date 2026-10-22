//! Whether the focused window covers the screen, from the window manager's
//! EWMH hints.

use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, Window};
use x11rb::rust_connection::RustConnection;

fn atom(conn: &RustConnection, name: &str) -> u32 {
    conn.intern_atom(false, name.as_bytes())
        .unwrap()
        .reply()
        .unwrap()
        .atom
}

/// `_NET_ACTIVE_WINDOW` names a window whose `_NET_WM_STATE` holds
/// `_NET_WM_STATE_FULLSCREEN`. A window that vanished between the two reads
/// (or no window manager at all) is not full-screen.
pub(crate) fn foreground_is_fullscreen(conn: &RustConnection, root: Window) -> bool {
    let active = conn
        .get_property(
            false,
            root,
            atom(conn, "_NET_ACTIVE_WINDOW"),
            AtomEnum::WINDOW,
            0,
            1,
        )
        .unwrap()
        .reply()
        .unwrap();
    let Some(window) = active.value32().and_then(|mut w| w.next()) else {
        return false;
    };
    let fullscreen = atom(conn, "_NET_WM_STATE_FULLSCREEN");
    conn.get_property(
        false,
        window,
        atom(conn, "_NET_WM_STATE"),
        AtomEnum::ATOM,
        0,
        64,
    )
    .unwrap()
    .reply()
    .ok()
    .and_then(|state| Some(state.value32()?.any(|a| a == fullscreen)))
    .unwrap_or(false)
}
