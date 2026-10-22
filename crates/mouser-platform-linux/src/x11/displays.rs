//! RandR monitors as `LocalDisplay`s, and a watch on layout changes.

use super::{connect, create_stop_window, wake};
use mouser_platform::{LocalDisplay, WatchHandle};
use std::collections::HashMap;
use std::sync::mpsc::Sender;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::randr::{ConnectionExt as _, NotifyMask};
use x11rb::protocol::xproto::{ConnectionExt as _, Window};
use x11rb::rust_connection::RustConnection;

/// The monitors of the screen. X11 applications work in device pixels, so
/// every display has scale 1.0 and logical size equal to its pixel size.
pub(crate) fn displays(conn: &RustConnection, root: Window) -> Vec<LocalDisplay> {
    conn.randr_get_monitors(root, true)
        .unwrap()
        .reply()
        .unwrap()
        .monitors
        .into_iter()
        .map(|m| {
            let name = String::from_utf8(conn.get_atom_name(m.name).unwrap().reply().unwrap().name)
                .expect("monitor name");
            LocalDisplay {
                key: name.clone(),
                name,
                pixel_size: (u32::from(m.width), u32::from(m.height)),
                logical_size: (f64::from(m.width), f64::from(m.height)),
                scale: 1.0,
                physical_size_mm: (m.width_in_millimeters > 0 && m.height_in_millimeters > 0)
                    .then_some((
                        f64::from(m.width_in_millimeters),
                        f64::from(m.height_in_millimeters),
                    )),
                origin: (f64::from(m.x), f64::from(m.y)),
            }
        })
        .collect()
}

/// Sends the current display list whenever RandR reports a change, until the
/// returned handle is dropped.
pub(crate) fn watch(env: &HashMap<String, String>, tx: Sender<Vec<LocalDisplay>>) -> WatchHandle {
    let (conn, root) = connect(env);
    let stop = create_stop_window(&conn, root);
    conn.randr_select_input(root, NotifyMask::SCREEN_CHANGE)
        .unwrap();
    conn.flush().unwrap();
    let thread = std::thread::spawn(move || {
        loop {
            let event = conn.wait_for_event().expect("X connection");
            if matches!(event, Event::ClientMessage(_)) {
                break;
            }
            if matches!(event, Event::RandrScreenChangeNotify(_)) {
                let _ = tx.send(displays(&conn, root));
            }
        }
    });
    let env = env.clone();
    WatchHandle::new(move || {
        wake(&env, stop);
        thread.join().unwrap();
    })
}
