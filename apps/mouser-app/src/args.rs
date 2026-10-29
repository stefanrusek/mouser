//! The command line, read before anything else starts.

use mouser_platform::Os;
use std::process::ExitCode;

/// Runs the cursor guard for a process id and ends with its code.
pub type Guard = fn(u32) -> ExitCode;

/// The cursor guard of this system (research V5): only Windows has one.
#[cfg(windows)]
pub const GUARD: Option<Guard> = Some(mouser_platform_windows::cursor_guard::run);
/// The cursor guard of this system (research V5): only Windows has one.
#[cfg(not(windows))]
pub const GUARD: Option<Guard> = None;

/// What this launch is for.
#[derive(Debug, PartialEq, Eq)]
pub enum Launch {
    /// The app (or `--headless`), as before.
    Normal,
    /// The cursor guard for the process with this id: no window, no Tauri.
    CursorGuard(u32),
}

/// Read `args` on `os`. `Err` is the exit code to end with.
pub fn parse(args: &[String], os: Os) -> Result<Launch, u8> {
    let Some(at) = args.iter().position(|a| a == "--cursor-guard") else {
        return Ok(Launch::Normal);
    };
    if os != Os::Windows {
        return Err(3);
    }
    let pid = args
        .get(at + 1)
        .and_then(|pid| pid.parse().ok())
        .expect("--cursor-guard is followed by a process id");
    Ok(Launch::CursorGuard(pid))
}

/// What `main` does before anything else starts, given `args` on `os` and
/// that system's `guard`: a guard launch runs the guard, with no window,
/// and the process ends with its code; a refused flag ends with the refusal
/// code. `None` for a normal launch, which carries on.
pub fn first(args: &[String], os: Os, guard: Option<Guard>) -> Option<ExitCode> {
    match parse(args, os) {
        Ok(Launch::Normal) => None,
        Ok(Launch::CursorGuard(pid)) => guard.map(|run| run(pid)),
        Err(code) => Some(ExitCode::from(code)),
    }
}
