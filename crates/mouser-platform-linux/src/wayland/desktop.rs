//! The one RemoteDesktop grant a platform shares between input injection and
//! the clipboard: a persisted portal session whose libei connection carries
//! emulated pointer, button, scroll and key events (the portal rejects
//! `NotifyPointer*`/`NotifyKeyboard*` once EIS is connected). The session
//! lives while anything holds its [`Link`].

use super::portal::{DesktopSession, start_remote_desktop};
use super::portal_clip::PortalClip;
use crate::bus::{block_on, connect};
use crate::permissions::REMOTE_DESKTOP_TOKEN;
use crate::secrets::Vault;
use mouser_platform::{Error, Result};
use reis::ei;
use reis::event::{Connection, DeviceCapability, EiEvent};
use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

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
    /// Waits until the libei handshake has finished.
    fn wait_connected(&self) {
        let offered = self.offered.lock().unwrap();
        drop(
            self.changed
                .wait_while(offered, |o| o.connection.is_none())
                .unwrap(),
        );
    }

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
    shared.changed.notify_all();
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

pub(crate) struct Link {
    pub(crate) desktop: Option<DesktopSession>,
    socket: UnixStream,
    shared: Arc<Shared>,
    reader: Option<JoinHandle<()>>,
    started: Instant,
}

impl Link {
    pub(crate) fn emit(
        &self,
        wait: Duration,
        has: &dyn Fn(&reis::event::Device) -> bool,
        send: &dyn Fn(&reis::event::Device),
    ) -> Result<()> {
        let time = self.started.elapsed().as_micros() as u64;
        self.shared.emit(wait, time, has, send)
    }

    fn open(env: &HashMap<String, String>, vault: &Vault) -> Link {
        let mut store = vault.open();
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
        shared.wait_connected();
        Link {
            desktop: Some(desktop),
            socket,
            shared,
            reader: Some(reader),
            started: Instant::now(),
        }
    }
}

impl Link {
    /// The portal clipboard, when the desktop offered one and the grant has it.
    pub(crate) fn clipboard(&self) -> Option<&PortalClip> {
        self.desktop.as_ref().unwrap().clipboard.as_ref()
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        // The portal objects are dropped on the portal runtime, which they need.
        let desktop = self.desktop.take().unwrap();
        block_on(async move {
            desktop.session.close().await.unwrap();
            drop(desktop);
        });
        self.socket.shutdown(std::net::Shutdown::Both).unwrap();
        let joined = self.reader.take().unwrap().join();
        // A second panic while one is unwinding would abort the process.
        if !std::thread::panicking() {
            joined.unwrap();
        }
    }
}

/// Opens the shared session on first use, and again after everything let go.
pub(crate) struct Desktop {
    env: HashMap<String, String>,
    vault: Vault,
    current: Mutex<Weak<Link>>,
}

impl Desktop {
    pub(crate) fn new(env: HashMap<String, String>, vault: Vault) -> Arc<Desktop> {
        Arc::new(Desktop {
            env,
            vault,
            current: Mutex::default(),
        })
    }

    /// The session, started if nobody holds one (a persisted grant makes that
    /// quick; otherwise it waits for the approval dialog).
    pub(crate) fn link(&self) -> Arc<Link> {
        let mut current = self.current.lock().unwrap();
        if let Some(link) = current.upgrade() {
            return link;
        }
        let link = Arc::new(Link::open(&self.env, &self.vault));
        *current = Arc::downgrade(&link);
        link
    }
}
