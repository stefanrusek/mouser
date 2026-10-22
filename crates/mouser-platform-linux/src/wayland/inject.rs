//! `InputInject` on Wayland: a persisted RemoteDesktop session whose libei
//! connection (the portal rejects `NotifyPointer*`/`NotifyKeyboard*` once EIS
//! is connected) carries emulated pointer, button, scroll and key events.

use super::portal::{DesktopSession, start_remote_desktop};
use crate::bus::{block_on, connect};
use crate::keymap;
use crate::permissions::REMOTE_DESKTOP_TOKEN;
use crate::secrets;
use mouser_platform::{Error, InputInject, Result};
use reis::ei;
use reis::event::{Connection, DeviceCapability, EiEvent};
use std::collections::{BTreeSet, HashMap};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The Linux button code of Mouser's button number (1 left, 2 right, 3 middle,
/// 4 back, 5 forward): `BTN_LEFT` is 0x110.
pub(crate) fn button_code(button: u8) -> u32 {
    0x10F + u32::from(button)
}

/// The devices the desktop offers, as they resume and pause.
#[derive(Default)]
pub(crate) struct Offered {
    context: Option<ei::Context>,
    connection: Option<Connection>,
    devices: Vec<reis::event::Device>,
}

#[derive(Default)]
pub(crate) struct Shared {
    offered: Mutex<Offered>,
    changed: Condvar,
}

impl Shared {
    /// The first device `has` accepts, waiting up to `wait` for the desktop
    /// to offer one, with the connection to commit frames on.
    pub(crate) fn device_with(
        &self,
        wait: Duration,
        has: &dyn Fn(&reis::event::Device) -> bool,
    ) -> Result<(reis::event::Device, Connection)> {
        let offered = self.offered.lock().unwrap();
        let (offered, timeout) = self
            .changed
            .wait_timeout_while(offered, wait, |o| !o.devices.iter().any(has))
            .unwrap();
        if timeout.timed_out() {
            return Err(Error::Os(
                "the desktop offers no device for this input".into(),
            ));
        }
        let device = offered.devices.iter().find(|d| has(d)).unwrap().clone();
        Ok((device, offered.connection.clone().unwrap()))
    }

    /// Runs `send` on the first device `has` accepts and commits it as a
    /// frame at `time` microseconds.
    pub(crate) fn emit(
        &self,
        wait: Duration,
        time: u64,
        has: &dyn Fn(&reis::event::Device) -> bool,
        send: &dyn Fn(&reis::event::Device),
    ) -> Result<()> {
        let (device, connection) = self.device_with(wait, has)?;
        send(&device);
        device.device().frame(connection.serial(), time);
        connection.flush().expect("flush eis");
        Ok(())
    }
}

fn read_eis(socket: UnixStream, shared: Arc<Shared>) {
    let context = ei::Context::new(socket).expect("eis socket");
    let (connection, events) = context
        .handshake_blocking("mouser", ei::handshake::ContextType::Sender)
        .expect("eis handshake");
    {
        let mut offered = shared.offered.lock().unwrap();
        offered.context = Some(context.clone());
        offered.connection = Some(connection.clone());
    }
    for event in events {
        let Ok(event) = event else { break };
        match event {
            EiEvent::SeatAdded(seat) => {
                seat.seat.bind_capabilities(
                    DeviceCapability::Pointer
                        | DeviceCapability::PointerAbsolute
                        | DeviceCapability::Keyboard
                        | DeviceCapability::Button
                        | DeviceCapability::Scroll,
                );
                let _ = context.flush();
            }
            EiEvent::DeviceResumed(resumed) => {
                resumed
                    .device
                    .device()
                    .start_emulating(connection.serial(), 1);
                let _ = context.flush();
                shared.offered.lock().unwrap().devices.push(resumed.device);
                shared.changed.notify_all();
            }
            EiEvent::DevicePaused(paused) => {
                shared
                    .offered
                    .lock()
                    .unwrap()
                    .devices
                    .retain(|d| *d != paused.device);
            }
            _ => {}
        }
    }
}

struct Link {
    desktop: DesktopSession,
    socket: UnixStream,
    shared: Arc<Shared>,
    reader: JoinHandle<()>,
    started: Instant,
}

impl Link {
    fn emit(
        &self,
        wait: Duration,
        has: &dyn Fn(&reis::event::Device) -> bool,
        send: &dyn Fn(&reis::event::Device),
    ) -> Result<()> {
        let time = self.started.elapsed().as_micros() as u64;
        self.shared.emit(wait, time, has, send)
    }

    fn open(env: &HashMap<String, String>) -> Link {
        let mut store = secrets::open(env);
        let token = store
            .get(REMOTE_DESKTOP_TOKEN)
            .unwrap()
            .map(|t| String::from_utf8(t).expect("token is text"));
        let env = env.clone();
        let (desktop, fd) = block_on(async move {
            let conn = connect(&env).await.expect("session bus");
            let desktop = start_remote_desktop(conn, token).await;
            let fd = desktop
                .remote
                .connect_to_eis(&desktop.session, Default::default())
                .await
                .unwrap();
            (desktop, fd)
        });
        if let Some(token) = &desktop.token {
            store.set(REMOTE_DESKTOP_TOKEN, token.as_bytes()).unwrap();
        }
        let socket = UnixStream::from(fd);
        let shared = Arc::new(Shared::default());
        let reader = {
            let (socket, shared) = (
                socket.try_clone().expect("clone eis socket"),
                shared.clone(),
            );
            std::thread::spawn(move || read_eis(socket, shared))
        };
        Link {
            desktop,
            socket,
            shared,
            reader,
            started: Instant::now(),
        }
    }
}

pub(crate) struct WaylandInject {
    env: HashMap<String, String>,
    wait: Duration,
    link: Option<Link>,
    keys: BTreeSet<u32>,
    buttons: BTreeSet<u8>,
}

impl WaylandInject {
    pub(crate) fn new(env: &HashMap<String, String>, wait: Duration) -> Self {
        WaylandInject {
            env: env.clone(),
            wait,
            link: None,
            keys: BTreeSet::new(),
            buttons: BTreeSet::new(),
        }
    }

    /// The session, started on first use (a persisted grant makes that quick).
    fn link(&mut self) -> &Link {
        self.link.get_or_insert_with(|| Link::open(&self.env))
    }

    fn send_key(&mut self, code: u32, down: bool) -> Result<()> {
        let wait = self.wait;
        let state = if down {
            ei::keyboard::KeyState::Press
        } else {
            ei::keyboard::KeyState::Released
        };
        self.link()
            .emit(wait, &|d| d.interface::<ei::Keyboard>().is_some(), &|d| {
                d.interface::<ei::Keyboard>().unwrap().key(code, state)
            })
    }

    fn send_button(&mut self, button: u8, down: bool) -> Result<()> {
        let wait = self.wait;
        let state = if down {
            ei::button::ButtonState::Press
        } else {
            ei::button::ButtonState::Released
        };
        self.link()
            .emit(wait, &|d| d.interface::<ei::Button>().is_some(), &|d| {
                d.interface::<ei::Button>()
                    .unwrap()
                    .button(button_code(button), state)
            })
    }
}

impl InputInject for WaylandInject {
    fn motion_abs(&mut self, x: f64, y: f64) -> Result<()> {
        let wait = self.wait;
        self.link().emit(
            wait,
            &|d| d.interface::<ei::PointerAbsolute>().is_some(),
            &|d| {
                d.interface::<ei::PointerAbsolute>()
                    .unwrap()
                    .motion_absolute(x as f32, y as f32)
            },
        )
    }

    fn button(&mut self, button: u8, down: bool) -> Result<()> {
        self.send_button(button, down)?;
        if down {
            self.buttons.insert(button);
        } else {
            self.buttons.remove(&button);
        }
        Ok(())
    }

    /// Mouser scrolls in pixels, positive up and right; libei's y is positive down.
    fn scroll(&mut self, dx: f64, dy: f64) -> Result<()> {
        let wait = self.wait;
        self.link()
            .emit(wait, &|d| d.interface::<ei::Scroll>().is_some(), &|d| {
                d.interface::<ei::Scroll>()
                    .unwrap()
                    .scroll(dx as f32, -dy as f32)
            })
    }

    fn key(&mut self, usage: u32, down: bool) -> Result<()> {
        let code = keymap::evdev_for_usage(usage)
            .map(u32::from)
            .ok_or_else(|| Error::Os(format!("no Linux key for usage {usage:#x}")))?;
        self.send_key(code, down)?;
        if down {
            self.keys.insert(code);
        } else {
            self.keys.remove(&code);
        }
        Ok(())
    }

    fn secure_attention(&mut self) -> Result<()> {
        Err(Error::Unsupported)
    }
}

impl Drop for WaylandInject {
    fn drop(&mut self) {
        for code in std::mem::take(&mut self.keys) {
            self.send_key(code, false).unwrap();
        }
        for button in std::mem::take(&mut self.buttons) {
            self.send_button(button, false).unwrap();
        }
        if let Some(link) = self.link.take() {
            // The portal objects are dropped on the portal runtime, which they need.
            let desktop = link.desktop;
            block_on(async move {
                desktop.session.close().await.unwrap();
                drop(desktop);
            });
            link.socket.shutdown(std::net::Shutdown::Both).unwrap();
            link.reader.join().unwrap();
        }
    }
}
