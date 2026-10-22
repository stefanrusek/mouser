//! `InputCapture` on Wayland: an InputCapture portal session with a barrier
//! along every outer edge of the zones. A push through a barrier activates
//! the session and libei delivers the input until Mouser releases it.

use super::portal::{CaptureSession, Zone, start_capture, zones};
use crate::bus::{block_on, connect, spawn};
use crate::keymap;
use crate::permissions::INPUT_CAPTURE_TOKEN;
use crate::secrets;
use ashpd::desktop::input_capture::{
    Activated, ActivatedBarrier, Barrier, BarrierID, ReleaseOptions,
};
use futures_util::StreamExt;
use mouser_platform::{Error, InputCapture, LocalInput, Result};
use reis::ei;
use reis::event::{DeviceCapability, EiEvent};
use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Pixels of scroll per wheel click.
const SCROLL_STEP: f64 = crate::x11::inject::SCROLL_STEP;
/// libei's discrete scroll unit: 120 is one wheel click.
const DISCRETE_CLICK: f64 = 120.0;

/// A zone edge the pointer can be pushed through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    /// The direction out of the screen, as the unit motion that reports a push.
    pub(crate) fn outward(self) -> (f64, f64) {
        match self {
            Side::Left => (-1.0, 0.0),
            Side::Right => (1.0, 0.0),
            Side::Top => (0.0, -1.0),
            Side::Bottom => (0.0, 1.0),
        }
    }
}

/// One barrier: the side it guards and its end points `(x1, y1, x2, y2)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Edge {
    pub(crate) side: Side,
    pub(crate) line: (i32, i32, i32, i32),
}

/// The parts of `lo..hi` not covered by any of `covers`.
fn uncovered(lo: i32, hi: i32, covers: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let mut covers = covers.to_vec();
    covers.sort_unstable();
    let mut free = Vec::new();
    let mut at = lo;
    for (start, end) in covers {
        if start > at {
            free.push((at, start.min(hi)));
        }
        at = at.max(end);
    }
    if at < hi {
        free.push((at, hi));
    }
    free.retain(|(a, b)| a < b);
    free
}

/// A barrier along every part of a zone's boundary that borders no other zone.
/// A barrier sits on the boundary coordinate and spans the pixels along it.
pub(crate) fn outer_edges(zones: &[Zone]) -> Vec<Edge> {
    let right = |z: &Zone| z.x + z.width as i32;
    let bottom = |z: &Zone| z.y + z.height as i32;
    let mut edges = Vec::new();
    for (index, z) in zones.iter().enumerate() {
        let others = || zones.iter().enumerate().filter(move |(i, _)| *i != index);
        let across = |cover: Vec<(i32, i32)>, lo, hi| uncovered(lo, hi, &cover);
        let vertical = |touching: &dyn Fn(&Zone) -> bool| {
            let cover = others()
                .filter(|(_, o)| touching(o))
                .map(|(_, o)| (o.y, bottom(o)))
                .collect();
            across(cover, z.y, bottom(z))
        };
        let horizontal = |touching: &dyn Fn(&Zone) -> bool| {
            let cover = others()
                .filter(|(_, o)| touching(o))
                .map(|(_, o)| (o.x, right(o)))
                .collect();
            across(cover, z.x, right(z))
        };
        for (lo, hi) in vertical(&|o| right(o) == z.x) {
            edges.push(Edge {
                side: Side::Left,
                line: (z.x, lo, z.x, hi - 1),
            });
        }
        for (lo, hi) in vertical(&|o| o.x == right(z)) {
            edges.push(Edge {
                side: Side::Right,
                line: (right(z), lo, right(z), hi - 1),
            });
        }
        for (lo, hi) in horizontal(&|o| bottom(o) == z.y) {
            edges.push(Edge {
                side: Side::Top,
                line: (lo, z.y, hi - 1, z.y),
            });
        }
        for (lo, hi) in horizontal(&|o| o.y == bottom(z)) {
            edges.push(Edge {
                side: Side::Bottom,
                line: (lo, bottom(z), hi - 1, bottom(z)),
            });
        }
    }
    edges
}

/// Mouser's button for a Linux button code.
pub(crate) fn mouser_button(code: u32) -> Option<u8> {
    match code {
        0x110 => Some(1),
        0x111 => Some(2),
        0x112 => Some(3),
        0x113 => Some(4),
        0x114 => Some(5),
        _ => None,
    }
}

/// Scroll in Mouser's units: pixels, positive up and right (libei's y is positive down).
pub(crate) fn scroll_delta(dx: f32, dy: f32) -> LocalInput {
    LocalInput::Scroll {
        dx: f64::from(dx),
        dy: -f64::from(dy),
    }
}

pub(crate) fn scroll_clicks(dx: i32, dy: i32) -> LocalInput {
    LocalInput::Scroll {
        dx: f64::from(dx) / DISCRETE_CLICK * SCROLL_STEP,
        dy: -f64::from(dy) / DISCRETE_CLICK * SCROLL_STEP,
    }
}

/// What the portal last told us about the capture.
#[derive(Default)]
struct Active {
    /// The activation to release, while input is captured.
    id: Option<u32>,
    /// Where the pointer was when the activation began.
    origin: (f64, f64),
    /// Motion libei reported since then. It can arrive before the portal's
    /// `Activated` signal does, so it is kept apart from the origin.
    moved: (f64, f64),
    /// Where to put the cursor when releasing.
    release_to: Option<(f64, f64)>,
}

impl Active {
    fn position(&self) -> (f64, f64) {
        (self.origin.0 + self.moved.0, self.origin.1 + self.moved.1)
    }
}

struct Link {
    tx: Sender<LocalInput>,
    edges: Vec<Edge>,
    active: Mutex<Active>,
}

impl Link {
    /// What a libei event means as local input; motion also moves our copy of
    /// the pointer position.
    fn translate(&self, event: &EiEvent) -> Option<LocalInput> {
        match event {
            EiEvent::PointerMotion(e) => {
                let (dx, dy) = (f64::from(e.dx), f64::from(e.dy));
                let mut active = self.active.lock().unwrap();
                active.moved.0 += dx;
                active.moved.1 += dy;
                Some(LocalInput::Motion {
                    dx,
                    dy,
                    abs: active.position(),
                })
            }
            EiEvent::Button(e) => mouser_button(e.button).map(|button| LocalInput::Button {
                button,
                down: e.state == ei::button::ButtonState::Press,
            }),
            EiEvent::ScrollDelta(e) => Some(scroll_delta(e.dx, e.dy)),
            EiEvent::ScrollDiscrete(e) => Some(scroll_clicks(e.discrete_dx, e.discrete_dy)),
            EiEvent::KeyboardKey(e) => {
                keymap::usage_for_evdev(e.key as u16).map(|usage| LocalInput::Key {
                    usage,
                    down: e.state == ei::keyboard::KeyState::Press,
                })
            }
            _ => None,
        }
    }

    fn signal(&self, signal: Signal) {
        match signal {
            Signal::Activated(a) => {
                self.activated(a.activation_id(), a.cursor_position(), a.barrier_id());
            }
            Signal::Deactivated => *self.active.lock().unwrap() = Active::default(),
        }
    }

    /// The portal activated the capture: input now flows over libei. Report
    /// the push at the edge as a unit motion, which is what a crossing needs.
    fn activated(
        &self,
        id: Option<u32>,
        at: Option<(f32, f32)>,
        barrier: Option<ActivatedBarrier>,
    ) {
        let (dx, dy) = barrier
            .and_then(|b| match b {
                ActivatedBarrier::Barrier(id) => self.edges.get(id.get() as usize - 1),
                ActivatedBarrier::UnknownBarrier => None,
            })
            .map_or((0.0, 0.0), |edge| edge.side.outward());
        let at = at.map_or((0.0, 0.0), |(x, y)| (f64::from(x), f64::from(y)));
        let abs = {
            let mut active = self.active.lock().unwrap();
            active.id = id;
            active.origin = at;
            active.release_to = None;
            active.position()
        };
        let _ = self.tx.send(LocalInput::Motion { dx, dy, abs });
    }
}

enum Signal {
    Activated(Activated),
    Deactivated,
}

/// Binds every capability and translates libei events until the socket closes.
fn read_eis(socket: UnixStream, link: Arc<Link>) {
    let context = ei::Context::new(socket).expect("eis socket");
    let (_connection, events) = context
        .handshake_blocking("mouser", ei::handshake::ContextType::Receiver)
        .expect("eis handshake");
    for event in events {
        let Ok(event) = event else { break };
        if let EiEvent::SeatAdded(seat) = &event {
            seat.seat.bind_capabilities(
                DeviceCapability::Pointer
                    | DeviceCapability::Keyboard
                    | DeviceCapability::Button
                    | DeviceCapability::Scroll,
            );
            let _ = context.flush();
        }
        if let Some(input) = link.translate(&event) {
            let _ = link.tx.send(input);
        }
    }
}

/// An established, enabled capture session.
struct Running {
    capture: Arc<CaptureSession>,
    link: Arc<Link>,
    socket: UnixStream,
    reader: JoinHandle<()>,
    listener: tokio::task::JoinHandle<()>,
    /// Ends the listener's wait for signals.
    stop_listening: tokio::sync::oneshot::Sender<()>,
}

/// What the zones are, shared with `displays()` so it can answer without
/// asking the user to approve again.
pub(crate) type SharedZones = Arc<Mutex<Option<Vec<Zone>>>>;

pub(crate) struct WaylandCapture {
    env: HashMap<String, String>,
    zones: SharedZones,
    running: Arc<Mutex<Option<Running>>>,
    stopped: Arc<AtomicBool>,
}

impl WaylandCapture {
    pub(crate) fn new(env: &HashMap<String, String>, zones: SharedZones) -> Self {
        WaylandCapture {
            env: env.clone(),
            zones,
            running: Arc::default(),
            stopped: Arc::default(),
        }
    }
}

/// Everything after the user approved: barriers, libei, activation signals.
async fn enable(capture: CaptureSession, tx: Sender<LocalInput>, shared: &SharedZones) -> Running {
    let (zones, zone_set) = zones(&capture).await;
    let edges = outer_edges(&zones);
    *shared.lock().unwrap() = Some(zones);
    let barriers: Vec<Barrier> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let id = BarrierID::new(i as u32 + 1).expect("barrier ids start at 1");
            Barrier::new(id, e.line)
        })
        .collect();
    let failed = capture
        .portal
        .set_pointer_barriers(&capture.session, &barriers, zone_set, Default::default())
        .await
        .unwrap()
        .response()
        .unwrap();
    assert_eq!(failed.failed_barriers(), [], "the desktop refused barriers");
    let fd = capture
        .portal
        .connect_to_eis(&capture.session, Default::default())
        .await
        .unwrap();
    let socket = UnixStream::from(fd);
    let link = Arc::new(Link {
        tx,
        edges,
        active: Mutex::default(),
    });
    let capture = Arc::new(capture);
    // Subscribe before enabling, so no activation is missed.
    let (subscribed, ready) = tokio::sync::oneshot::channel();
    let (stop_listening, stop) = tokio::sync::oneshot::channel::<()>();
    let listener = {
        let (capture, link) = (capture.clone(), link.clone());
        spawn(async move {
            let activated = capture.portal.receive_activated().await.unwrap();
            let deactivated = capture.portal.receive_deactivated().await.unwrap();
            let signals = Box::pin(futures_util::stream::select(
                activated.map(Signal::Activated),
                deactivated.map(|_| Signal::Deactivated),
            ));
            subscribed.send(()).unwrap();
            signals
                .take_until(stop)
                .for_each(|signal| async { link.signal(signal) })
                .await
        })
    };
    ready.await.unwrap();
    capture
        .portal
        .enable(&capture.session, Default::default())
        .await
        .unwrap();
    let reader = {
        let (socket, link) = (socket.try_clone().expect("clone eis socket"), link.clone());
        std::thread::spawn(move || read_eis(socket, link))
    };
    Running {
        capture,
        link,
        socket,
        reader,
        listener,
        stop_listening,
    }
}

/// Closes the session and the libei connection and waits for the reader.
/// The portal objects are dropped on the portal runtime, which they need.
fn teardown(running: Running) {
    let (capture, listener) = (running.capture, running.listener);
    running.stop_listening.send(()).unwrap();
    block_on(async move {
        listener.await.unwrap();
        capture.session.close().await.unwrap();
        drop(capture);
    });
    running.socket.shutdown(std::net::Shutdown::Both).unwrap();
    running.reader.join().unwrap();
}

impl InputCapture for WaylandCapture {
    /// Returns at once: the portal may wait for the user to approve Mouser,
    /// so the session is set up on its own thread, which also keeps the
    /// restore token the portal returns.
    fn start(&mut self, tx: Sender<LocalInput>) -> Result<()> {
        let (env, shared) = (self.env.clone(), self.zones.clone());
        let (running, stopped) = (self.running.clone(), self.stopped.clone());
        std::thread::spawn(move || {
            let mut store = secrets::open(&env);
            let token = store
                .get(INPUT_CAPTURE_TOKEN)
                .unwrap()
                .map(|t| String::from_utf8(t).expect("token is text"));
            let established = block_on({
                let env = env.clone();
                async move {
                    let conn = connect(&env).await.expect("session bus");
                    let capture = start_capture(conn, token).await;
                    let token = capture.token.clone();
                    (enable(capture, tx, &shared).await, token)
                }
            });
            let (established, token) = established;
            if let Some(token) = token {
                store.set(INPUT_CAPTURE_TOKEN, token.as_bytes()).unwrap();
            }
            let mut slot = running.lock().unwrap();
            if stopped.load(Ordering::SeqCst) {
                drop(slot);
                teardown(established);
            } else {
                *slot = Some(established);
            }
        });
        Ok(())
    }

    /// Input is captured when the pointer is pushed through a barrier, not on
    /// request, so a grab is `Unsupported` (core then refuses QuickJump and
    /// hotkey crossings). Asking to release still hands the input back to
    /// the desktop before saying so.
    fn set_grabbed(&mut self, grabbed: bool) -> Result<()> {
        if let (false, Some(running)) = (grabbed, &*self.running.lock().unwrap()) {
            let released = std::mem::take(&mut *running.link.active.lock().unwrap());
            if let Some(id) = released.id {
                let release_to = released.release_to;
                let capture = running.capture.clone();
                block_on(async move {
                    capture
                        .portal
                        .release(
                            &capture.session,
                            ReleaseOptions::default()
                                .set_activation_id(id)
                                .set_cursor_position(release_to),
                        )
                        .await
                        .unwrap();
                });
            }
        }
        Err(Error::Unsupported)
    }

    /// The cursor cannot be moved while the desktop owns it; the position is
    /// kept for the next release.
    fn warp(&mut self, x: f64, y: f64) -> Result<()> {
        if let Some(running) = &*self.running.lock().unwrap() {
            running.link.active.lock().unwrap().release_to = Some((x, y));
        }
        Ok(())
    }

    fn stop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let running = self.running.lock().unwrap().take();
        if let Some(running) = running {
            teardown(running);
        }
    }
}

impl Drop for WaylandCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
