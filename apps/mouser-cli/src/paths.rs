//! Where this user's `config.toml` and `logs/` are: beside a portable app
//! (`MouserData/`), otherwise in the per-OS directory.

use std::path::PathBuf;

use mouser_core::paths::{Env, Paths};
use mouser_platform::Os;

/// The operating system this binary was built for.
const OS: Os = if cfg!(target_os = "macos") {
    Os::MacOs
} else if cfg!(windows) {
    Os::Windows
} else {
    Os::Linux
};

/// The directory of `config.toml`, `state.json` and `logs/`.
pub fn dir() -> PathBuf {
    let exe = std::env::current_exe().expect("the executable has a path");
    Paths::resolve(&exe, OS, &Env::from_process()).dir
}

/// The control endpoint of the Mouser to talk to: the service's pipe when it
/// exists (Windows), otherwise this user's socket or pipe.
pub fn control_endpoint() -> PathBuf {
    mouser_core::control::default_path()
}
