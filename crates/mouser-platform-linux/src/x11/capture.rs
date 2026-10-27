//! `InputCapture` on X11: XInput2 raw events on the root window for what the
//! devices do, `XGrabPointer`/`XGrabKeyboard` on Mouser's own window to
//! swallow it, and `XWarpPointer` to park the pointer while grabbed.

use super::echo::Echo;
use super::{connect, create_stop_window, wake};
use crate::keymap;
use mouser_platform::{Error, InputCapture, LocalInput, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xinput::{
    ConnectionExt as _, EventMask as XiEventMask, Fp3232, KeyEventFlags, XIEventMask,
};
use x11rb::protocol::xproto::{
    ConnectionExt as _, CreateWindowAux, EventMask, GrabMode, GrabStatus, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;

/// `XIAllMasterDevices`.
const ALL_MASTER_DEVICES: u16 = 1;
/// Pixels of scroll per wheel click.
const SCROLL_STEP: f64 = super::inject::SCROLL_STEP;
const FIRST_RETRY: Duration = Duration::from_millis(20);
const LONGEST_RETRY: Duration = Duration::from_millis(500);

/// Mouser's button number for an X button, `None` for buttons we do not forward.
pub(crate) fn mouser_button(button: u32) -> Option<u8> {
    match button {
        1 => Some(1),
        3 => Some(2),
        2 => Some(3),
        8.. => u8::try_from(button - 4).ok(),
        _ => None,
    }
}

/// The input a raw button event stands for: wheel buttons 4-7 scroll on
/// press, the rest are buttons.
pub(crate) fn button_input(button: u32, down: bool) -> Option<LocalInput> {
    let scroll = |dx: f64, dy: f64| down.then_some(LocalInput::Scroll { dx, dy });
    match button {
        4 => scroll(0.0, SCROLL_STEP),
        5 => scroll(0.0, -SCROLL_STEP),
        6 => scroll(-SCROLL_STEP, 0.0),
        7 => scroll(SCROLL_STEP, 0.0),
        other => mouser_button(other).map(|button| LocalInput::Button { button, down }),
    }
}

pub(crate) fn key_input(keycode: u32, down: bool) -> Option<LocalInput> {
    keymap::usage_for_x11_keycode(u8::try_from(keycode).ok()?)
        .map(|usage| LocalInput::Key { usage, down })
}

fn fixed(value: &Fp3232) -> f64 {
    f64::from(value.integral) + f64::from(value.frac) / 4_294_967_296.0
}

/// The pointer delta of a raw motion: valuator 0 is x, valuator 1 is y, and
/// `axisvalues` lists the set valuators in order.
pub(crate) fn motion_delta(mask: &[u32], values: &[Fp3232]) -> (f64, f64) {
    let word = mask.first().copied().unwrap_or(0);
    let mut values = values.iter();
    let mut delta = [0.0, 0.0];
    for (axis, slot) in delta.iter_mut().enumerate() {
        if word & (1 << axis) != 0 {
            *slot = fixed(values.next().expect("a value for every set valuator"));
        }
    }
    (delta[0], delta[1])
}

/// What the grab holds back from the local desktop.
struct Shared {
    /// Where the pointer is held while grabbed.
    park: Mutex<Option<(i16, i16)>>,
    /// Whether the pointer and keyboard grabs are currently held.
    held: AtomicBool,
}

struct Reader {
    stop: Window,
    thread: JoinHandle<()>,
}

struct Grab {
    release: Sender<()>,
    thread: JoinHandle<()>,
}

pub(crate) struct X11Capture {
    env: HashMap<String, String>,
    echo: Arc<Echo>,
    shared: Arc<Shared>,
    reader: Option<Reader>,
    grab: Option<Grab>,
}

impl X11Capture {
    pub(crate) fn new(env: &HashMap<String, String>, echo: Arc<Echo>) -> Self {
        X11Capture {
            env: env.clone(),
            echo,
            shared: Arc::new(Shared {
                park: Mutex::new(None),
                held: AtomicBool::new(false),
            }),
            reader: None,
            grab: None,
        }
    }

    /// Whether the grabs are held (they are not while another client holds one).
    #[cfg(test)]
    pub(crate) fn grab_held(&self) -> bool {
        self.shared.held.load(Ordering::SeqCst)
    }
}

fn pointer_position(conn: &RustConnection, root: Window) -> (i16, i16) {
    let at = conn.query_pointer(root).unwrap().reply().unwrap();
    (at.root_x, at.root_y)
}

/// What an X event means to the capture.
#[derive(Debug, PartialEq)]
pub(crate) enum Raw {
    Stop,
    Motion { dx: f64, dy: f64 },
    Button { code: u32, down: bool },
    Key { code: u32, down: bool, repeat: bool },
    Other,
}

pub(crate) fn classify(event: &Event) -> Raw {
    match event {
        Event::ClientMessage(_) => Raw::Stop,
        Event::XinputRawMotion(e) => {
            let (dx, dy) = motion_delta(&e.valuator_mask, &e.axisvalues);
            Raw::Motion { dx, dy }
        }
        Event::XinputRawButtonPress(e) => Raw::Button {
            code: e.detail,
            down: true,
        },
        Event::XinputRawButtonRelease(e) => Raw::Button {
            code: e.detail,
            down: false,
        },
        Event::XinputRawKeyPress(e) => Raw::Key {
            code: e.detail,
            down: true,
            repeat: e.flags.contains(KeyEventFlags::KEY_REPEAT),
        },
        Event::XinputRawKeyRelease(e) => Raw::Key {
            code: e.detail,
            down: false,
            repeat: false,
        },
        _ => Raw::Other,
    }
}

/// The input to deliver for `raw`, unless it is Mouser's own echo. A motion
/// reports where the pointer ended up (`position`) and then parks it there
/// (`park`); key auto-repeats are not forwarded (the far side repeats itself).
pub(crate) fn interpret(
    raw: &Raw,
    echo: &Echo,
    now: Instant,
    position: &dyn Fn() -> (i16, i16),
    park: &dyn Fn(),
) -> Option<LocalInput> {
    match *raw {
        Raw::Stop | Raw::Other => None,
        Raw::Motion { dx, dy } => {
            let at = position();
            park();
            let abs = (f64::from(at.0), f64::from(at.1));
            (!echo.is_motion(at, now)).then_some(LocalInput::Motion { dx, dy, abs })
        }
        Raw::Button { code, down } => {
            let own = echo.take_button(code as u8, down, now);
            button_input(code, down).filter(|_| !own)
        }
        Raw::Key { code, down, repeat } => {
            let own = echo.take_key(code as u8, down, now);
            key_input(code, down).filter(|_| !own && !repeat)
        }
    }
}

/// Reads raw input until the stop window is messaged.
fn read_events(
    conn: RustConnection,
    root: Window,
    tx: Sender<LocalInput>,
    echo: Arc<Echo>,
    shared: Arc<Shared>,
) {
    let position = || pointer_position(&conn, root);
    let park = || {
        if let Some((x, y)) = *shared.park.lock().unwrap() {
            conn.warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, x, y)
                .unwrap();
            conn.flush().unwrap();
        }
    };
    loop {
        let raw = classify(&conn.wait_for_event().expect("X connection"));
        if raw == Raw::Stop {
            break;
        }
        if let Some(input) = interpret(&raw, &echo, Instant::now(), &position, &park) {
            let _ = tx.send(input);
        }
    }
}

fn try_grab(conn: &RustConnection, window: Window, cursor: u32) -> bool {
    let pointer = conn
        .grab_pointer(
            false,
            window,
            EventMask::NO_EVENT,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            x11rb::NONE,
            cursor,
            x11rb::CURRENT_TIME,
        )
        .unwrap()
        .reply()
        .unwrap()
        .status;
    let keyboard = conn
        .grab_keyboard(
            false,
            window,
            x11rb::CURRENT_TIME,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )
        .unwrap()
        .reply()
        .unwrap()
        .status;
    let both = pointer == GrabStatus::SUCCESS && keyboard == GrabStatus::SUCCESS;
    if !both {
        // A half-acquired pair is released before the next attempt.
        conn.ungrab_pointer(x11rb::CURRENT_TIME).unwrap();
        conn.ungrab_keyboard(x11rb::CURRENT_TIME).unwrap();
        conn.flush().unwrap();
    }
    both
}

/// Holds the pointer and keyboard on a hidden window of our own, retrying
/// with backoff while another client (an open menu) holds a grab, until told
/// to release.
fn hold_grab(env: HashMap<String, String>, shared: Arc<Shared>, release: Receiver<()>) {
    let (conn, root) = connect(&env);
    let window = conn.generate_id().unwrap();
    conn.create_window(
        0,
        window,
        root,
        -1,
        -1,
        1,
        1,
        0,
        WindowClass::INPUT_ONLY,
        0,
        &CreateWindowAux::new().override_redirect(1),
    )
    .unwrap();
    conn.map_window(window).unwrap();
    // A cursor with no pixels: the pointer is hidden while grabbed.
    let (pixmap, cursor) = (conn.generate_id().unwrap(), conn.generate_id().unwrap());
    conn.create_pixmap(1, pixmap, root, 1, 1).unwrap();
    conn.create_cursor(cursor, pixmap, pixmap, 0, 0, 0, 0, 0, 0, 0, 0)
        .unwrap();
    conn.flush().unwrap();
    let mut delay = FIRST_RETRY;
    loop {
        if try_grab(&conn, window, cursor) {
            shared.held.store(true, Ordering::SeqCst);
            let _ = release.recv();
            break;
        }
        if release.recv_timeout(delay) != Err(RecvTimeoutError::Timeout) {
            break;
        }
        delay = (delay * 2).min(LONGEST_RETRY);
    }
    shared.held.store(false, Ordering::SeqCst);
    conn.ungrab_pointer(x11rb::CURRENT_TIME).unwrap();
    conn.ungrab_keyboard(x11rb::CURRENT_TIME).unwrap();
    // The grab is free once the server has answered.
    conn.get_input_focus().unwrap().reply().unwrap();
}

impl InputCapture for X11Capture {
    fn start(&mut self, tx: Sender<LocalInput>) -> Result<()> {
        let (conn, root) = connect(&self.env);
        conn.xinput_xi_query_version(2, 2).unwrap().reply().unwrap();
        conn.xinput_xi_select_events(
            root,
            &[XiEventMask {
                deviceid: ALL_MASTER_DEVICES,
                mask: vec![
                    XIEventMask::RAW_MOTION
                        | XIEventMask::RAW_BUTTON_PRESS
                        | XIEventMask::RAW_BUTTON_RELEASE
                        | XIEventMask::RAW_KEY_PRESS
                        | XIEventMask::RAW_KEY_RELEASE,
                ],
            }],
        )
        .unwrap();
        let stop = create_stop_window(&conn, root);
        // The selection is in force once the server answers a round trip.
        conn.get_input_focus().unwrap().reply().unwrap();
        let (echo, shared) = (self.echo.clone(), self.shared.clone());
        let thread = std::thread::spawn(move || read_events(conn, root, tx, echo, shared));
        self.reader = Some(Reader { stop, thread });
        Ok(())
    }

    fn set_grabbed(&mut self, grabbed: bool) -> Result<()> {
        if grabbed && self.grab.is_none() {
            let (conn, root) = connect(&self.env);
            *self.shared.park.lock().unwrap() = Some(pointer_position(&conn, root));
            let (release, wait) = channel();
            let (env, shared) = (self.env.clone(), self.shared.clone());
            let thread = std::thread::spawn(move || hold_grab(env, shared, wait));
            self.grab = Some(Grab { release, thread });
        }
        if !grabbed && let Some(grab) = self.grab.take() {
            *self.shared.park.lock().unwrap() = None;
            grab.release.send(()).unwrap();
            grab.thread.join().unwrap();
        }
        Ok(())
    }

    fn warp(&mut self, x: f64, y: f64) -> Result<()> {
        let (conn, root) = connect(&self.env);
        let inside = super::displays::displays(&conn, root).iter().any(|d| {
            x >= d.origin.0
                && x < d.origin.0 + d.logical_size.0
                && y >= d.origin.1
                && y < d.origin.1 + d.logical_size.1
        });
        if !inside {
            return Err(Error::Os(format!("({x}, {y}) is outside every display")));
        }
        let (x, y) = (x.round() as i16, y.round() as i16);
        conn.warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, x, y)
            .unwrap();
        // Wait for the server to apply it before this connection closes.
        conn.get_input_focus().unwrap().reply().unwrap();
        // A grabbed pointer is held where it was warped to.
        let mut park = self.shared.park.lock().unwrap();
        if park.is_some() {
            *park = Some((x, y));
        }
        Ok(())
    }

    fn stop(&mut self) {
        self.set_grabbed(false).unwrap();
        if let Some(reader) = self.reader.take() {
            wake(&self.env, reader.stop);
            reader.thread.join().unwrap();
        }
    }
}

impl Drop for X11Capture {
    fn drop(&mut self) {
        self.stop();
    }
}
