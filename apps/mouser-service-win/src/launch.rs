//! Starting the session engine (research V9): `mouser-app.exe --headless
//! --machine` from beside `mouser-service.exe`, in the console session,
//! as SYSTEM with that session's `winlogon.exe` token, on the interactive
//! desktop, once its pipe exists.

use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;

use mouser_proto::service_ipc::SasResult;

use crate::ipc::Pipe;
use crate::session::Event;
use crate::winapi::{ApiError, ApiResult, Handle, WinApi};

/// The engine's executable, beside the service's.
pub const ENGINE: &str = "mouser-app.exe";
/// The interactive window station's default desktop.
pub const DESKTOP: &str = r"winsta0\default";
/// `ERROR_NOT_FOUND`.
const ERROR_NOT_FOUND: u32 = 1168;

/// How the service answers the engine's Ctrl-Alt-Del requests.
pub type Answer = Arc<dyn Fn() -> SasResult + Send + Sync>;

/// A session engine the service started.
pub struct Engine {
    pub pid: u32,
    pub session: u32,
    api: Arc<dyn WinApi>,
    process: Handle,
    pipe: Arc<Pipe>,
    server: JoinHandle<()>,
    watcher: JoinHandle<()>,
}

/// The `winlogon.exe` of `session`.
fn winlogon(api: &dyn WinApi, session: u32) -> ApiResult<u32> {
    api.processes()?
        .into_iter()
        .find(|p| p.session == session && p.name.eq_ignore_ascii_case("winlogon.exe"))
        .map(|p| p.pid)
        .ok_or_else(|| ApiError {
            code: ERROR_NOT_FOUND,
            message: format!("session {session} has no winlogon.exe"),
        })
}

/// Starts the engine of `session` from `dir`; `exited` hears when it ends.
pub fn launch(
    api: &Arc<dyn WinApi>,
    session: u32,
    dir: &Path,
    sas: Answer,
    exited: Sender<Event>,
) -> ApiResult<Engine> {
    let pipe = Arc::new(Pipe::create(api.clone(), session)?);
    let token = api.duplicate_token(winlogon(api.as_ref(), session)?)?;
    let exe = dir.join(ENGINE);
    let launched = api.create_process_as_user(
        token,
        &exe,
        &format!("\"{}\" --headless --machine", exe.display()),
        DESKTOP,
    );
    api.close(token);
    let launched = launched?;
    let pid = launched.pid;
    let server = std::thread::spawn({
        let pipe = pipe.clone();
        move || pipe.serve(pid, &*sas)
    });
    let watcher = std::thread::spawn({
        let api = api.clone();
        move || {
            let _ = api.wait_process(launched.process);
            let _ = exited.send(Event::EngineExited { pid });
        }
    });
    Ok(Engine {
        pid,
        session,
        api: api.clone(),
        process: launched.process,
        pipe,
        server,
        watcher,
    })
}

impl Engine {
    /// Tells the engine to shut down; it ends by itself.
    pub fn shutdown(&self) {
        self.pipe.shutdown();
    }

    /// Waits for the engine to end and lets go of it: its exit code.
    pub fn finish(self) -> u32 {
        let code = self.api.wait_process(self.process).unwrap_or(u32::MAX);
        self.pipe.end();
        let _ = self.server.join();
        let _ = self.watcher.join();
        self.api.close(self.process);
        code
    }
}
