//! The session engine's side of the service pipe (contracts/service-ipc.md;
//! research V9, V10). The engine connects to the pipe the service made for
//! its console session and says hello first. It asks for Ctrl-Alt-Del,
//! which only the service may send, and says why before it exits; the
//! service tells it to shut down.
//!
//! A reader thread waits for the service's messages at any time while
//! requests are written from other threads. On a pipe opened for ordinary
//! I/O Windows lets one call at a time through, so a write would wait behind
//! the pending read; the pipe is opened overlapped and every read and write
//! waits on an event of its own.

use mouser_platform::{Error, Result};
use mouser_proto::service_ipc::{
    EngineMessage, SasResult, ServiceMessage, StopReason, engine_pipe_name,
};
use mouser_proto::wire::{decode_frame, encode_frame};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, HANDLE, WAIT_OBJECT_0,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_OVERLAPPED, FILE_SHARE_NONE, OPEN_EXISTING, ReadFile, WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::RemoteDesktop::WTSGetActiveConsoleSessionId;
use windows::Win32::System::Threading::{CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects};
use windows::core::HSTRING;

/// The session at the physical console, whose engine this is.
pub fn console_session_id() -> u32 {
    unsafe { WTSGetActiveConsoleSessionId() }
}

/// The pipe the service makes for this console session's engine.
pub(crate) fn engine_pipe() -> String {
    engine_pipe_name(console_session_id())
}

/// A connection to `MouserService`.
pub struct ServiceClient {
    inner: Arc<Inner>,
    reader: Option<JoinHandle<()>>,
}

struct Inner {
    pipe: Handle,
    /// Set to make the reader stop.
    stop: Handle,
    /// One frame written at a time.
    writing: Mutex<()>,
    /// One Ctrl-Alt-Del request at a time, and the service's answers.
    sas: Mutex<Receiver<SasResult>>,
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Default)]
struct State {
    /// The service said to shut down.
    shutdown: bool,
    /// The pipe closed.
    gone: bool,
    on_shutdown: Option<Box<dyn FnOnce() + Send>>,
}

/// A kernel handle, closed on drop; one may be used from any thread.
struct Handle(HANDLE);

unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn os(what: &str, e: impl std::fmt::Display) -> Error {
    Error::Os(format!("{what}: {e}"))
}

fn event() -> Result<Handle> {
    let made = unsafe { CreateEventW(None, true, false, None) };
    made.map(Handle).map_err(|e| os("CreateEventW", e))
}

/// One overlapped read or write of `pipe`, waited for; `stop`, when set,
/// cancels it. The bytes moved, or `None` when stopped.
fn transfer(pipe: &Handle, stop: Option<&Handle>, buf: Buffer) -> Result<Option<usize>> {
    let done = event()?;
    let mut ov = OVERLAPPED {
        hEvent: done.0,
        ..Default::default()
    };
    let started = match buf {
        Buffer::Read(bytes) => unsafe { ReadFile(pipe.0, Some(bytes), None, Some(&mut ov)) },
        Buffer::Write(bytes) => unsafe { WriteFile(pipe.0, Some(bytes), None, Some(&mut ov)) },
    };
    if let Err(e) = started
        && e.code() != ERROR_IO_PENDING.to_hresult()
    {
        return Err(os("the Mouser service is gone", e));
    }
    let waits: Vec<HANDLE> = [Some(done.0), stop.map(|s| s.0)]
        .into_iter()
        .flatten()
        .collect();
    let first = unsafe { WaitForMultipleObjects(&waits, false, INFINITE) };
    let stopped = first != WAIT_OBJECT_0;
    if stopped {
        let _ = unsafe { CancelIoEx(pipe.0, Some(&ov)) };
    }
    let mut moved = 0;
    // Waited for even when cancelled: `ov` and the buffer must outlive it.
    let finished = unsafe { GetOverlappedResult(pipe.0, &ov, &mut moved, true) };
    if stopped {
        return Ok(None);
    }
    finished.map_err(|e| os("the Mouser service is gone", e))?;
    Ok(Some(moved as usize))
}

enum Buffer<'a> {
    Read(&'a mut [u8]),
    Write(&'a [u8]),
}

/// Fills `buf` from the pipe; `None` when stopped or when there is
/// nothing more to read, an error when the pipe closed.
fn read_exact(pipe: &Handle, stop: &Handle, buf: &mut [u8]) -> Result<Option<()>> {
    let mut at = 0;
    while at < buf.len() {
        match transfer(pipe, Some(stop), Buffer::Read(&mut buf[at..]))? {
            None | Some(0) => return Ok(None),
            Some(n) => at += n,
        }
    }
    Ok(Some(()))
}

/// The next message from the service; `None` when stopped.
fn read_message(pipe: &Handle, stop: &Handle) -> Result<Option<ServiceMessage>> {
    // The length, then the body it gives.
    let mut frame = vec![0u8; 4];
    for from in [0, 4] {
        if from == 4 {
            let len = u32::from_be_bytes(frame[..4].try_into().expect("four bytes"));
            frame.resize(4 + len as usize, 0);
        }
        if read_exact(pipe, stop, &mut frame[from..])?.is_none() {
            return Ok(None);
        }
    }
    let (msg, _) = decode_frame(&frame).map_err(|e| os("the Mouser service said", e))?;
    Ok(Some(msg))
}

/// Connects to the service on pipe `name` and says hello with `version`.
pub(crate) fn connect_to(name: &str, version: &str) -> Result<ServiceClient> {
    let pipe = unsafe {
        CreateFileW(
            &HSTRING::from(name),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            None,
        )
    }
    .map(Handle)
    .map_err(|e| os("the Mouser service is not there", e))?;
    let (answers, sas) = channel();
    let inner = Arc::new(Inner {
        pipe,
        stop: event()?,
        writing: Mutex::new(()),
        sas: Mutex::new(sas),
        state: Mutex::new(State::default()),
        changed: Condvar::new(),
    });
    let client = ServiceClient {
        reader: Some(std::thread::spawn({
            let inner = inner.clone();
            move || read_until_stopped(&inner, &answers)
        })),
        inner,
    };
    client.send(&EngineMessage::Hello {
        version: version.into(),
    })?;
    Ok(client)
}

/// The reader thread: hands each answer on and runs what is registered for
/// a shutdown; ends when the pipe closes or the client is dropped.
fn read_until_stopped(inner: &Inner, answers: &Sender<SasResult>) {
    loop {
        match read_message(&inner.pipe, &inner.stop) {
            Ok(Some(ServiceMessage::SasResult(result))) => {
                // A request that gave up just means nobody is listening.
                let _ = answers.send(result);
            }
            Ok(Some(ServiceMessage::Shutdown)) => {
                let run = {
                    let mut state = inner.state.lock().unwrap();
                    state.shutdown = true;
                    inner.changed.notify_all();
                    state.on_shutdown.take()
                };
                if let Some(run) = run {
                    run();
                }
            }
            Ok(None) => return,
            Err(_) => {
                inner.state.lock().unwrap().gone = true;
                inner.changed.notify_all();
                return;
            }
        }
    }
}

impl ServiceClient {
    /// Connects to `engine_pipe_name(console_session_id())` and sends `Hello { version }` first.
    pub fn connect(version: &str) -> Result<ServiceClient> {
        connect_to(&engine_pipe(), version)
    }

    /// Registers what to run when the service sends `Shutdown`; called at most once per client.
    pub fn on_shutdown(&self, f: Box<dyn FnOnce() + Send>) {
        let mut state = self.inner.state.lock().unwrap();
        if state.shutdown {
            drop(state);
            f();
        } else {
            state.on_shutdown = Some(f);
        }
    }

    /// Sends `Stopping { reason }` before a voluntary exit.
    pub fn stopping(&self, reason: StopReason) -> Result<()> {
        self.send(&EngineMessage::Stopping { reason })
    }

    /// Asks the service for Ctrl-Alt-Del and waits for its answer.
    pub(crate) fn send_sas(&self) -> Result<()> {
        let answers = self.inner.sas.lock().unwrap();
        self.send(&EngineMessage::SendSas)?;
        match answers.recv() {
            Ok(SasResult::Sent) => Ok(()),
            Ok(SasResult::PolicyBlocked) => Err(Error::Os(
                "Ctrl-Alt-Del is blocked by this PC's policy".into(),
            )),
            Ok(SasResult::Failed(why)) => Err(Error::Os(format!(
                "the Mouser service could not send Ctrl-Alt-Del: {why}"
            ))),
            Err(_) => Err(Error::Os("the Mouser service is gone".into())),
        }
    }

    fn send(&self, msg: &EngineMessage) -> Result<()> {
        let frame = encode_frame(msg);
        let _one = self.inner.writing.lock().unwrap();
        let mut at = 0;
        while at < frame.len() {
            let moved = transfer(&self.inner.pipe, None, Buffer::Write(&frame[at..]))?;
            at += moved.expect("a write is never stopped");
        }
        Ok(())
    }

    /// For tests: waits until the service said to shut down or went away.
    #[cfg(test)]
    pub(crate) fn wait_for_shutdown_for_tests(&self) {
        let state = self.inner.state.lock().unwrap();
        let _settled = self
            .inner
            .changed
            .wait_while(state, |s| !s.shutdown && !s.gone)
            .unwrap();
    }
}

impl Drop for ServiceClient {
    fn drop(&mut self) {
        let _ = unsafe { SetEvent(self.inner.stop.0) };
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
