//! Restarting portable Mouser as administrator (FR-005; research V2; probe
//! P3). The restart starts this executable again as administrator with
//! `--wait-for <pid>`, so it stays portable on the same drive, and hands
//! over: the new instance waits for this one to end before anything else,
//! so the single-instance check is free for it. A declined prompt changes
//! nothing. States: idle → prompting → handing over → (the new instance)
//! elevated running; declined → idle.
//!
//! While an elevated portable instance runs it marks itself in
//! `MouserData/elevated.pid`. A second launch that is not elevated cannot
//! hand its arguments to an elevated instance (probe P3 scenario D: UIPI
//! blocks `WM_COPYDATA`), so it says where Mouser is and ends.

use std::path::{Path, PathBuf};

use mouser_core::logging::Log;
use mouser_core::logging::events::{ElevationOutcome, elevation_restart};
use mouser_core::paths::Paths;
pub use mouser_platform_windows::shell::RunAs;

/// The file an elevated portable instance marks itself with.
pub const MARKER: &str = "elevated.pid";

/// What a second launch says when an elevated instance is running.
pub const ALREADY_ELEVATED: &str =
    "Mouser is already running as administrator on this computer. Open it from the tray icon.";

/// What the restart asks of Windows.
pub trait Shell {
    /// Starts `exe` with `args`, as administrator when `as_admin`.
    fn start(&self, exe: &Path, args: &str, as_admin: bool) -> Result<(), RunAs>;
    /// Waits until process `pid` has ended.
    fn wait_for_process(&self, pid: u32);
    fn process_alive(&self, pid: u32) -> bool;
    /// Shows `text` and waits until it is closed.
    fn message_box(&self, text: &str);
}

/// The real Windows.
pub struct System;

impl Shell for System {
    fn start(&self, exe: &Path, args: &str, as_admin: bool) -> Result<(), RunAs> {
        mouser_platform_windows::shell::start(exe, args, as_admin)
    }

    fn wait_for_process(&self, pid: u32) {
        mouser_platform_windows::shell::wait_for_process(pid);
    }

    fn process_alive(&self, pid: u32) -> bool {
        mouser_platform_windows::shell::process_alive(pid)
    }

    fn message_box(&self, text: &str) {
        mouser_platform_windows::shell::message_box(text);
    }
}

/// Where a restart as administrator stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Prompting,
    HandingOver,
    ElevatedRunning,
}

/// Restarts `exe` (this executable, process `own_pid`) as administrator.
/// The state it ends in: `HandingOver` when the new instance is starting,
/// and the caller shuts the engine down and exits; `Idle` when the prompt
/// was declined or Windows refused, and nothing changes.
pub fn restart(shell: &dyn Shell, exe: &Path, own_pid: u32, log: &Log) -> State {
    log.write(elevation_restart(ElevationOutcome::Prompted));
    match shell.start(exe, &format!("--wait-for {own_pid}"), true) {
        Ok(()) => {
            log.write(elevation_restart(ElevationOutcome::HandedOver));
            State::HandingOver
        }
        Err(RunAs::Declined) => {
            log.write(elevation_restart(ElevationOutcome::Declined));
            State::Idle
        }
        Err(RunAs::Failed(_)) => State::Idle,
    }
}

/// An elevated portable instance's mark; removed when dropped, as the
/// instance exits.
#[derive(Debug)]
pub struct Marker(PathBuf);

impl Marker {
    /// Marks an `elevated` portable run (`paths`) as process `pid`; `None`
    /// for any other run.
    pub fn write(paths: &Paths, elevated: bool, pid: u32) -> Option<Marker> {
        if !(paths.portable && elevated) {
            return None;
        }
        let file = paths.dir.join(MARKER);
        std::fs::write(&file, pid.to_string()).expect("MouserData can be written");
        Some(Marker(file))
    }
}

impl Drop for Marker {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// What `main` does before Tauri on Windows: waits for the instance a
/// `--wait-for <pid>` launch replaces; then, for a portable launch that is
/// not `elevated` while an elevated instance runs, says where Mouser is.
/// Whether to end the launch here.
pub fn before_tauri(args: &[String], paths: &Paths, elevated: bool, shell: &dyn Shell) -> bool {
    if let Some(at) = args.iter().position(|a| a == "--wait-for") {
        let pid = args
            .get(at + 1)
            .and_then(|pid| pid.parse().ok())
            .expect("--wait-for is followed by a process id");
        shell.wait_for_process(pid);
    }
    if !paths.portable || elevated {
        return false;
    }
    let marked = std::fs::read_to_string(paths.dir.join(MARKER)).ok();
    let running = marked
        .and_then(|pid| pid.trim().parse().ok())
        .is_some_and(|pid| shell.process_alive(pid));
    if running {
        shell.message_box(ALREADY_ELEVATED);
    }
    running
}
