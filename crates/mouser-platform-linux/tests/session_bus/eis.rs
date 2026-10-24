//! A fake libei server (the compositor's EIS endpoint) for one client.

use calloop::channel::{Channel, Event as ChannelEvent, Sender, channel};
use calloop::{EventLoop, LoopSignal};
use reis::calloop::{EisRequestSource, EisRequestSourceEvent};
use reis::eis;
use reis::eis::device::DeviceType;
use reis::request::{DeviceCapability, EisRequest, Seat};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// What the test makes the server send to a capturing client.
pub enum Emit {
    Motion(f32, f32),
    Button(u32, bool),
    Key(u32, bool),
    Scroll(f32, f32),
    Discrete(i32, i32),
    /// The desktop pauses the pointer device, and later resumes it.
    Pause,
    Resume,
}

#[derive(Default)]
struct Devices {
    connection: Option<reis::request::Connection>,
    pointer: Option<reis::request::Device>,
    keyboard: Option<reis::request::Device>,
}

pub struct FakeEis {
    /// What a sending client asked for, one line each.
    pub received: Arc<Mutex<Vec<String>>>,
    /// The devices a sending client started emulating on, in order.
    pub starts: Arc<Mutex<Vec<String>>>,
    commands: Sender<Emit>,
    signal: LoopSignal,
    /// Set when the server is dropped; the loop checks it after every wake-up,
    /// because `LoopSignal::stop` is forgotten if it comes before `run` starts.
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FakeEis {
    pub fn serve(stream: UnixStream, offer_keyboard: bool) -> FakeEis {
        let received = Arc::new(Mutex::new(Vec::new()));
        let starts = Arc::new(Mutex::new(Vec::new()));
        let (commands, channel_source) = channel::<Emit>();
        let (signal_tx, signal_rx) = std::sync::mpsc::channel();
        let log = received.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let thread = std::thread::spawn({
            let starts = starts.clone();
            let stopped = stopped.clone();
            move || {
                run(
                    stream,
                    log,
                    starts,
                    offer_keyboard,
                    channel_source,
                    signal_tx,
                    stopped,
                )
            }
        });
        FakeEis {
            received,
            starts,
            commands,
            signal: signal_rx.recv().unwrap(),
            stopped,
            thread: Some(thread),
        }
    }

    pub fn send(&self, what: Emit) {
        self.commands.send(what).unwrap();
    }

    pub fn starts(&self, device: &str) -> usize {
        self.starts
            .lock()
            .unwrap()
            .iter()
            .filter(|d| *d == device)
            .count()
    }

    pub fn received(&self) -> Vec<String> {
        self.received.lock().unwrap().clone()
    }
}

impl Drop for FakeEis {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        self.signal.stop();
        self.signal.wakeup();
        self.thread.take().unwrap().join().unwrap();
    }
}

fn add(
    seat: &Seat,
    connection: &reis::request::Connection,
    name: &str,
    capabilities: impl Into<enumflags2::BitFlags<DeviceCapability>>,
) -> reis::request::Device {
    let device = seat.add_device(Some(name), DeviceType::Virtual, capabilities.into(), |_| {});
    device.resumed();
    if connection.context_type() == eis::handshake::ContextType::Receiver {
        device.start_emulating(1);
    }
    device
}

fn run(
    stream: UnixStream,
    log: Arc<Mutex<Vec<String>>>,
    starts: Arc<Mutex<Vec<String>>>,
    offer_keyboard: bool,
    commands: Channel<Emit>,
    signal: std::sync::mpsc::Sender<LoopSignal>,
    stopped: Arc<AtomicBool>,
) {
    let mut event_loop: EventLoop<()> = EventLoop::try_new().unwrap();
    let handle = event_loop.handle();
    let devices = Arc::new(Mutex::new(Devices::default()));
    let context = eis::Context::new(stream).unwrap();
    let state = devices.clone();
    handle
        .insert_source(
            EisRequestSource::new(context, 1),
            move |event, connection, _| {
                Ok(match event {
                    Ok(EisRequestSourceEvent::Connected) => {
                        state.lock().unwrap().connection = Some(connection.clone());
                        let _ = connection.add_seat(
                            Some("default"),
                            DeviceCapability::Pointer
                                | DeviceCapability::PointerAbsolute
                                | DeviceCapability::Keyboard
                                | DeviceCapability::Scroll
                                | DeviceCapability::Button,
                        );
                        let _ = connection.flush();
                        calloop::PostAction::Continue
                    }
                    Ok(EisRequestSourceEvent::Request(request)) => {
                        match request {
                            EisRequest::Bind(bind) => {
                                let mut d = state.lock().unwrap();
                                let relative = DeviceCapability::Pointer
                                    | DeviceCapability::Button
                                    | DeviceCapability::Scroll;
                                let absolute = DeviceCapability::PointerAbsolute
                                    | DeviceCapability::Button
                                    | DeviceCapability::Scroll;
                                if bind
                                    .capabilities
                                    .contains(DeviceCapability::PointerAbsolute)
                                {
                                    d.pointer =
                                        Some(add(&bind.seat, connection, "pointer-abs", absolute));
                                } else if bind.capabilities.contains(DeviceCapability::Pointer) {
                                    d.pointer =
                                        Some(add(&bind.seat, connection, "pointer", relative));
                                }
                                if offer_keyboard
                                    && bind.capabilities.contains(DeviceCapability::Keyboard)
                                {
                                    d.keyboard = Some(add(
                                        &bind.seat,
                                        connection,
                                        "keyboard",
                                        DeviceCapability::Keyboard,
                                    ));
                                }
                            }
                            EisRequest::PointerMotionAbsolute(e) => log
                                .lock()
                                .unwrap()
                                .push(format!("abs {} {}", e.dx_absolute, e.dy_absolute)),
                            EisRequest::Button(e) => log
                                .lock()
                                .unwrap()
                                .push(format!("button {:#x} {:?}", e.button, e.state)),
                            EisRequest::ScrollDelta(e) => log
                                .lock()
                                .unwrap()
                                .push(format!("scroll {} {}", e.dx, e.dy)),
                            EisRequest::DeviceStartEmulating(e) => starts
                                .lock()
                                .unwrap()
                                .push(e.device.name().unwrap().to_string()),
                            EisRequest::KeyboardKey(e) => log
                                .lock()
                                .unwrap()
                                .push(format!("key {} {:?}", e.key, e.state)),
                            _ => {}
                        }
                        let _ = connection.flush();
                        calloop::PostAction::Continue
                    }
                    Err(_) => calloop::PostAction::Remove,
                })
            },
        )
        .unwrap();
    let send_state = devices;
    handle
        .insert_source(commands, move |event, _, _| {
            let ChannelEvent::Msg(what) = event else {
                return;
            };
            let d = send_state.lock().unwrap();
            let (device, time) = match what {
                Emit::Motion(dx, dy) => {
                    let p = d.pointer.as_ref().unwrap();
                    p.interface::<eis::Pointer>()
                        .unwrap()
                        .motion_relative(dx, dy);
                    (p, 1)
                }
                Emit::Button(code, down) => {
                    let p = d.pointer.as_ref().unwrap();
                    let state = if down {
                        eis::button::ButtonState::Press
                    } else {
                        eis::button::ButtonState::Released
                    };
                    p.interface::<eis::Button>().unwrap().button(code, state);
                    (p, 2)
                }
                Emit::Scroll(dx, dy) => {
                    let p = d.pointer.as_ref().unwrap();
                    p.interface::<eis::Scroll>().unwrap().scroll(dx, dy);
                    (p, 3)
                }
                Emit::Discrete(dx, dy) => {
                    let p = d.pointer.as_ref().unwrap();
                    p.interface::<eis::Scroll>()
                        .unwrap()
                        .scroll_discrete(dx, dy);
                    (p, 4)
                }
                Emit::Pause => {
                    let p = d.pointer.as_ref().unwrap();
                    p.paused();
                    let _ = d.connection.as_ref().unwrap().flush();
                    return;
                }
                Emit::Resume => {
                    let p = d.pointer.as_ref().unwrap();
                    p.resumed();
                    let _ = d.connection.as_ref().unwrap().flush();
                    return;
                }
                Emit::Key(code, down) => {
                    let k = d.keyboard.as_ref().unwrap();
                    let state = if down {
                        eis::keyboard::KeyState::Press
                    } else {
                        eis::keyboard::KeyState::Released
                    };
                    k.interface::<eis::Keyboard>().unwrap().key(code, state);
                    (k, 5)
                }
            };
            device.frame(time);
            let _ = d.connection.as_ref().unwrap().flush();
        })
        .unwrap();
    signal.send(event_loop.get_signal()).unwrap();
    let loop_signal = event_loop.get_signal();
    event_loop
        .run(None, &mut (), |_| {
            if stopped.load(Ordering::Acquire) {
                loop_signal.stop();
            }
        })
        .unwrap();
}

/// Stopping the server as soon as it is serving must still stop it: a stop
/// that arrives before the loop has started running used to be forgotten, and
/// dropping the server then waited for the loop for ever.
#[test]
fn a_server_dropped_the_moment_it_is_serving_stops() {
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for _ in 0..3000 {
            let (server, _client) = UnixStream::pair().unwrap();
            drop(FakeEis::serve(server, false));
        }
        done.send(()).unwrap();
    });
    finished
        .recv_timeout(std::time::Duration::from_secs(60))
        .expect("every server stopped when dropped");
}
