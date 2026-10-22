//! `--headless` (Story 12): Mouser with no window and no tray, run by the
//! macOS login-window agent (or the Windows service). The engine is the same;
//! it finds its secrets in a machine-scope store and its desk membership in
//! the machine directory, both written beforehand from the user's session
//! (see `login_screen`).

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mouser_core::engine_headless::FileSecrets;
use mouser_core::{Engine, EngineParams};
use mouser_platform::{Os, Platform};

/// Whether the command line asks for headless mode.
pub fn is_headless(args: &[String]) -> bool {
    args.iter().any(|a| a == "--headless")
}

/// Why headless mode cannot start on `os`, or `Ok` where it is supported.
/// Only the macOS login-window agent runs headless in this version.
pub fn check_supported(os: Os) -> Result<(), &'static str> {
    match os {
        Os::MacOs => Ok(()),
        Os::Windows | Os::Linux => Err("headless mode is only supported on macOS in this version"),
    }
}

/// The directory a headless Mouser reads its configuration and desk
/// membership from.
pub fn machine_dir(os: Os) -> PathBuf {
    match os {
        Os::MacOs => PathBuf::from("/Library/Application Support/Mouser"),
        Os::Windows => PathBuf::from(r"C:\ProgramData\Mouser"),
        Os::Linux => PathBuf::from("/var/lib/mouser"),
    }
}

/// Where the machine-scope secrets are kept inside `machine_dir`.
pub fn secrets_dir(machine_dir: &Path) -> PathBuf {
    machine_dir.join("secrets")
}

/// Run the engine headless until `shutdown` completes, then stop it.
pub async fn run(
    platform: Arc<dyn Platform>,
    machine_dir: PathBuf,
    control_path: PathBuf,
    computer_name: String,
    version: String,
    shutdown: impl Future<Output = ()>,
) {
    let mut params = EngineParams::new(
        platform,
        machine_dir.clone(),
        control_path,
        computer_name,
        version,
    );
    params.secrets = Some(Box::new(FileSecrets::new(secrets_dir(&machine_dir))));
    let engine = Engine::start(params).await;
    shutdown.await;
    engine.shutdown().await;
}
