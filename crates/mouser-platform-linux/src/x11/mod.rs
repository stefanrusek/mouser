//! The X11 backend: RandR displays, XInput2 capture with grabs, XTest
//! injection.

pub(crate) mod capture;
pub(crate) mod displays;
pub(crate) mod echo;
pub(crate) mod inject;

use std::collections::HashMap;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConnectionExt as _, CreateWindowAux, EventMask, Window,
    WindowClass,
};
use x11rb::rust_connection::RustConnection;

/// A connection to the display named by `DISPLAY` in `env`, with its root window.
pub(crate) fn connect(env: &HashMap<String, String>) -> (RustConnection, Window) {
    let (conn, screen) =
        x11rb::connect(env.get("DISPLAY").map(String::as_str)).expect("connect to the X server");
    let root = conn.setup().roots[screen].root;
    (conn, root)
}

/// An unmapped window whose only use is to be sent a message that wakes
/// the thread blocked reading this connection's events.
pub(crate) fn create_stop_window(conn: &RustConnection, root: Window) -> Window {
    let stop = conn.generate_id().unwrap();
    conn.create_window(
        0,
        stop,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new(),
    )
    .unwrap();
    stop
}

/// Sends the wake-up message to `stop`, from another connection.
pub(crate) fn wake(env: &HashMap<String, String>, stop: Window) {
    let (conn, _) = connect(env);
    let event = ClientMessageEvent::new(32, stop, AtomEnum::STRING, [0u32; 5]);
    conn.send_event(false, stop, EventMask::NO_EVENT, event)
        .unwrap();
    conn.flush().unwrap();
    conn.get_input_focus().unwrap().reply().unwrap();
}

#[cfg(test)]
mod capture_tests;
#[cfg(test)]
mod displays_tests;
#[cfg(test)]
mod echo_tests;
#[cfg(test)]
mod inject_tests;
#[cfg(test)]
pub(crate) mod testserver;
