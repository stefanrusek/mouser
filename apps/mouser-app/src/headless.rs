//! `--headless` (001 Story 12, 002 Story 6): Mouser with no window and no
//! tray on macOS, Windows and Linux. The engine is the same everywhere. On
//! macOS (the login-window agent) and with `--machine` (the Windows service,
//! the Linux machine folder) it finds its secrets in a machine-scope store and
//! its desk membership in the machine directory, both written beforehand from
//! the user's session (see `login_screen`). On Windows and Linux without
//! `--machine` it runs as the signed-in user, in the user's folder (or
//! `MouserData/` when portable) with the platform's own secret store.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use mouser_core::engine_headless::FileSecrets;
use mouser_core::paths::Paths;
use mouser_core::{Engine, EngineParams};
use mouser_platform::{Os, Platform};
use mouser_proto::service_ipc::StopReason;
use mouser_proto::wire::ComputerMode;

/// Whether the command line asks for headless mode.
pub fn is_headless(args: &[String]) -> bool {
    args.iter().any(|a| a == "--headless")
}

/// The machine directory of `os`.
pub fn machine_dir(os: Os) -> PathBuf {
    match os {
        Os::MacOs => PathBuf::from("/Library/Application Support/Mouser"),
        Os::Windows => PathBuf::from(r"C:\ProgramData\Mouser"),
        Os::Linux => PathBuf::from("/var/lib/mouser"),
    }
}

/// Whether `mode` on `os` runs on machine-scope state: the macOS login-window
/// agent and `--machine`.
fn machine_scope(os: Os, mode: ComputerMode) -> bool {
    os == Os::MacOs || mode == ComputerMode::Service
}

/// The directory a headless Mouser on `os` in `mode` reads its configuration
/// and desk membership from: the machine directory, or the current user's
/// folder (`user`, `MouserData/` when portable).
pub fn data_dir(os: Os, mode: ComputerMode, user: &Paths) -> PathBuf {
    if machine_scope(os, mode) {
        machine_dir(os)
    } else {
        user.dir.clone()
    }
}

/// Where the machine-scope secrets are kept inside `machine_dir`.
pub fn secrets_dir(machine_dir: &Path) -> PathBuf {
    machine_dir.join("secrets")
}

/// The engine's parameters for a headless run in `data_dir`.
pub fn params(
    platform: Arc<dyn Platform>,
    os: Os,
    data_dir: PathBuf,
    control_path: PathBuf,
    computer_name: String,
    version: String,
    mode: ComputerMode,
) -> EngineParams {
    let mut params = EngineParams::new(
        platform,
        data_dir.clone(),
        control_path,
        computer_name,
        version,
    );
    params.mode = mode;
    // Windows' own store is machine scope already (DPAPI in the machine
    // directory's `secrets`): files of plain keys there would collide with it.
    if machine_scope(os, mode) && os != Os::Windows {
        params.secrets = Some(Box::new(FileSecrets::new(secrets_dir(&data_dir))));
    }
    params
}

/// Resolves when the system asks Mouser to stop: Ctrl-C or SIGTERM. The
/// handlers are in place when this returns.
#[cfg(unix)]
pub fn stop_signal() -> impl Future<Output = ()> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM can be awaited");
    let mut interrupt = signal(SignalKind::interrupt()).expect("SIGINT can be awaited");
    async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = term.recv() => {}
        }
    }
}

/// Resolves when the system asks Mouser to stop: Ctrl-C, the console closing
/// or the computer shutting down. The handlers are in place when this
/// returns.
#[cfg(windows)]
pub fn stop_signal() -> impl Future<Output = ()> {
    let mut interrupt = tokio::signal::windows::ctrl_c().expect("Ctrl-C can be awaited");
    let mut close = tokio::signal::windows::ctrl_close().expect("console close can be awaited");
    let mut shutdown = tokio::signal::windows::ctrl_shutdown().expect("shutdown can be awaited");
    async move {
        tokio::select! {
            _ = interrupt.recv() => {}
            _ = close.recv() => {}
            _ = shutdown.recv() => {}
        }
    }
}

/// Run the engine headless until `shutdown` completes, then stop it.
pub async fn run(params: EngineParams, shutdown: impl Future<Output = ()>) {
    let engine = Engine::start(params).await;
    mouser_core::faults::install(engine.log());
    shutdown.await;
    engine.shutdown().await;
}

/// What the session engine (Windows `--headless --machine`) has of
/// `MouserService` (contracts/service-ipc.md).
pub trait Service: Send + Sync + 'static {
    /// What to run when the service says to shut down.
    fn on_shutdown(&self, f: Box<dyn FnOnce() + Send>);
    /// Tells the service why the engine is exiting of its own accord.
    fn stopping(&self, reason: StopReason);
}

/// Something that completes when it is time to stop. One type, not a
/// generic, so each function that takes one is a single function.
pub type Stop = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Why a headless run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// The system asked it to stop (Ctrl-C, close, shutdown).
    Asked,
    /// The service said to shut down.
    Service,
}

/// Run the engine until the system asks it to stop or `service_shutdown`
/// completes, then stop it. Which of the two ended it.
pub async fn run_until(params: EngineParams, stop: Stop, service_shutdown: Stop) -> Ended {
    let engine = Engine::start(params).await;
    mouser_core::faults::install(engine.log());
    let ended = tokio::select! {
        () = stop => Ended::Asked,
        () = service_shutdown => Ended::Service,
    };
    engine.shutdown().await;
    ended
}

/// Runs `work` and tells `service` how it ended: `Stopping { Quit }` before
/// a voluntary exit, `Stopping { Fault }` when a fault ended it, which then
/// ends this process as it would have; nothing when the service asked.
pub async fn report_to(
    service: Arc<dyn Service>,
    work: Pin<Box<dyn Future<Output = Ended> + Send>>,
) {
    match tokio::spawn(work).await {
        Ok(Ended::Asked) => service.stopping(StopReason::Quit),
        Ok(Ended::Service) => {}
        Err(fault) => {
            service.stopping(StopReason::Fault);
            std::panic::resume_unwind(fault.into_panic());
        }
    }
}

/// The session engine: the engine on `params` until the system asks it to
/// stop (`stop`) or the service says to shut down, telling the service how
/// it ended.
pub async fn serve(params: EngineParams, service: Arc<dyn Service>, stop: Stop) {
    let (said, shutdown) = tokio::sync::oneshot::channel::<()>();
    service.on_shutdown(Box::new(move || {
        // The engine may already be ending of its own accord.
        let _ = said.send(());
    }));
    let service_shutdown = Box::pin(async {
        // A service that went without a word leaves the engine running.
        if shutdown.await.is_err() {
            std::future::pending::<()>().await;
        }
    });
    report_to(service, Box::pin(run_until(params, stop, service_shutdown))).await;
}
