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
use std::sync::Arc;

use mouser_core::engine_headless::FileSecrets;
use mouser_core::paths::Paths;
use mouser_core::{Engine, EngineParams};
use mouser_platform::{Os, Platform};
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
    if machine_scope(os, mode) {
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
