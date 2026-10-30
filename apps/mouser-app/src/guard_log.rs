//! The cursor guard's diagnostic log (research V5; T528/T529): when the
//! guard puts the pointer back, `pointer.shown {reason: guard}` goes to the
//! log of the engine it guards, as a record core writes, while that
//! computer's logging is on. The guard creates nothing else: no config, no
//! key, and no log at all while logging is off.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use mouser_core::engine_headless::SecretStore;
use mouser_core::identity::{DEVICE_KEY, Identity};
use mouser_core::logging::events::{self, PointerShown};
use mouser_core::logging::{Context, Log};
use mouser_core::paths::Paths;
use mouser_platform::Os;
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ComputerMode;

use crate::diagnostics::Sink;

/// Where the guarded engine keeps its config and log, and the mode it runs
/// in. Only the session engine runs as LocalSystem, so a guard that does is
/// that engine's, in the machine folder; any other guard is its user's
/// Mouser's (`user`: the portable folder or the user's own).
pub fn location(local_system: bool, user: &Paths) -> (PathBuf, ComputerMode) {
    match (local_system, user.portable) {
        (true, _) => (
            crate::headless::machine_dir(Os::Windows),
            ComputerMode::Service,
        ),
        (false, true) => (user.dir.clone(), ComputerMode::Portable),
        (false, false) => (user.dir.clone(), ComputerMode::Installed),
    }
}

/// The computer's id from its device key in `secrets`, without making one:
/// a computer with none yet is named by zeros.
fn computer_id(secrets: &dyn SecretStore) -> ComputerId {
    let seed = secrets.get(DEVICE_KEY).ok().flatten();
    seed.and_then(|seed| <[u8; 32]>::try_from(seed).ok())
        .map_or(ComputerId([0; 16]), |seed| {
            Identity::from_seed(seed).computer_id()
        })
}

/// The sink the guard gives the platform when the engine in `dir` (running
/// in `mode`) has logging on: the platform's `pointer.shown` is written to
/// its log by core. `None`, and nothing created, when logging is off or
/// the engine has no config yet.
pub fn guard_sink(
    dir: &Path,
    mode: ComputerMode,
    secrets: &dyn SecretStore,
    host_name: &str,
) -> Option<Sink> {
    if !dir.join(mouser_core::config::FILE).is_file() {
        return None;
    }
    let config = mouser_core::config::load_or_create(dir);
    if !config.diagnostics.logging {
        return None;
    }
    let name = match config.general.computer_name.as_str() {
        "" => host_name.to_string(),
        name => name.to_string(),
    };
    let context = Context {
        mode,
        ..Context::fallback()
    };
    let (dir, id, max_mb) = (
        dir.to_path_buf(),
        computer_id(secrets),
        config.diagnostics.max_log_mb,
    );
    Some(Arc::new(move |_level, event, _fields| {
        // The guard's only diagnostic; anything else is not the guard's.
        // The log is opened (and its header written) only for the record,
        // so a guard that never shows the pointer leaves the engine's log
        // as it was.
        if event == "pointer.shown" {
            let log = Log::with_context(&dir, id, name.clone(), true, max_mb, context.clone());
            log.write(events::pointer_shown(PointerShown::Guard));
        }
    }))
}

/// `mouser-app --cursor-guard <pid>`: the cursor guard, writing to the
/// guarded engine's log.
pub fn run(parent_pid: u32) -> std::process::ExitCode {
    use mouser_platform_windows::{WinPlatform, cursor_guard, set_diagnostics, shell};
    let exe = std::env::current_exe().expect("the executable has a path");
    let user = Paths::resolve(&exe, Os::Windows, &mouser_core::paths::Env::from_process());
    let (dir, mode) = location(shell::this_process_is_local_system(), &user);
    // Portable keeps its keys beside it; installed Mouser in the machine store.
    let machine_store =
        || WinPlatform::secret_store(crate::headless::machine_dir(Os::Windows).join("secrets"));
    let secrets = user.secrets().unwrap_or_else(machine_store);
    let host_name = std::env::var("COMPUTERNAME").unwrap_or_default();
    set_diagnostics(guard_sink(&dir, mode, secrets.as_ref(), &host_name));
    cursor_guard::run(parent_pid)
}
