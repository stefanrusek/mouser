//! The command line, read before anything else starts.

use mouser_platform::Os;

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
