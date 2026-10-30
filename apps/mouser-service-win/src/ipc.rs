//! The service's end of the session engine's pipe (contracts/service-ipc.md):
//! created for one console session before the engine is launched, open to
//! SYSTEM only and to no other computer, and served only to the process the
//! service launched.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use mouser_proto::service_ipc::{EngineMessage, SasResult, ServiceMessage, engine_pipe_name};
use mouser_proto::wire::{FrameError, decode_frame, encode_frame};

use crate::winapi::{ApiResult, Handle, WinApi};

/// Only SYSTEM may open the pipe.
pub const SDDL: &str = "D:P(A;;GA;;;SY)";
const PIPE_ACCESS_DUPLEX: u32 = 0x0000_0003;
const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
const PIPE_TYPE_BYTE: u32 = 0x0000_0000;
const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x0000_0008;
/// `ERROR_PIPE_CONNECTED`: the client connected before the wait began.
const ERROR_PIPE_CONNECTED: u32 = 535;

/// Whether the engine is connected, and whether it is to shut down.
#[derive(Default)]
struct Link {
    connected: bool,
    shutdown: bool,
}

pub struct Pipe {
    api: Arc<dyn WinApi>,
    handle: Handle,
    name: String,
    /// Held while writing: answers and `Shutdown` come from two threads.
    link: Mutex<Link>,
    ended: AtomicBool,
}

impl Pipe {
    /// The pipe for the engine of console session `session`.
    pub fn create(api: Arc<dyn WinApi>, session: u32) -> ApiResult<Self> {
        let name = engine_pipe_name(session);
        let handle = api.create_pipe(
            &name,
            SDDL,
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
            PIPE_TYPE_BYTE | PIPE_REJECT_REMOTE_CLIENTS,
        )?;
        Ok(Self {
            api,
            handle,
            name,
            link: Mutex::new(Link::default()),
            ended: AtomicBool::new(false),
        })
    }

    pub fn handle(&self) -> Handle {
        self.handle
    }

    fn write(&self, msg: &ServiceMessage) -> ApiResult<()> {
        self.api.write_pipe(self.handle, &encode_frame(msg))
    }

    /// Tells the engine to shut down: now when it is connected, or as soon
    /// as it connects.
    pub fn shutdown(&self) {
        let mut link = self.link.lock().unwrap();
        link.shutdown = true;
        if link.connected {
            let _ = self.write(&ServiceMessage::Shutdown);
        }
    }

    /// The engine has gone: a wait for it to connect ends, whether or not it
    /// has begun (the service connects as the client itself; a pipe with a
    /// client already is not waiting).
    pub fn end(&self) {
        self.ended.store(true, Ordering::SeqCst);
        if let Ok(client) = self.api.open_pipe_client(&self.name) {
            self.api.close(client);
        }
    }

    /// Waits for process `engine` to connect, disconnecting any other
    /// client; false when the wait ends without it.
    fn accept(&self, engine: u32) -> bool {
        loop {
            match self.api.connect_pipe(self.handle) {
                Err(e) if e.code != ERROR_PIPE_CONNECTED => return false,
                _ => {}
            }
            if self.ended.load(Ordering::SeqCst) {
                return false;
            }
            if self.api.pipe_client_pid(self.handle).ok() == Some(engine) {
                let mut link = self.link.lock().unwrap();
                link.connected = true;
                if link.shutdown {
                    let _ = self.write(&ServiceMessage::Shutdown);
                }
                return true;
            }
            if self.api.disconnect_pipe(self.handle).is_err() {
                return false;
            }
        }
    }

    /// Serves process `engine` until it goes: each `SendSas` is answered
    /// with `sas()`. A `Hello` of another version and `Stopping` change
    /// nothing (the catalog has no event for them).
    pub fn serve(&self, engine: u32, sas: &dyn Fn() -> SasResult) {
        if !self.accept(engine) {
            return;
        }
        let mut buf = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            match decode_frame::<EngineMessage>(&buf) {
                Ok((msg, used)) => {
                    buf.drain(..used);
                    if msg == EngineMessage::SendSas {
                        let answer = ServiceMessage::SasResult(sas());
                        let _link = self.link.lock().unwrap();
                        if self.write(&answer).is_err() {
                            return;
                        }
                    }
                }
                Err(FrameError::Incomplete) => match self.api.read_pipe(self.handle, &mut chunk) {
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(_) => return,
                },
                Err(_) => return,
            }
        }
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        self.api.close(self.handle);
    }
}
