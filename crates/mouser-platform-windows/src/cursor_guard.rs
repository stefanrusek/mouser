//! The cursor guard (research V5; Story 1 sc. 6). Mouser hides the pointer
//! by replacing the system cursors with a blank one, and puts them back when
//! it lets go. If Mouser ends without doing that, killed, crashed or cut off
//! mid-grab, the pointer would stay invisible. The guard is a small process
//! of its own, with no window: it waits for Mouser to end and, if the arrow
//! is then still blank, reloads the user's cursors.

use crate::cursor::{arrow_is_blank, recover};
use crate::diag;
use mouser_platform::Result;
use std::process::ExitCode;
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{
    INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

/// Runs the cursor guard for `parent_pid` (research V5). Returns when the parent has ended,
/// after restoring blank system cursors. Never creates a window.
pub fn run(parent_pid: u32) -> ExitCode {
    wait_for(parent_pid);
    after_parent(arrow_is_blank, recover)
}

/// Waits until process `pid` has ended, for whatever reason; at once when
/// it already has, so cannot be opened.
pub(crate) fn wait_for(pid: u32) {
    if let Ok(parent) = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
        unsafe { WaitForSingleObject(parent, INFINITE) };
        let _ = unsafe { CloseHandle(parent) };
    }
}

/// What the guard does once its parent has ended: when the arrow is `blank`,
/// `restore` the user's cursors and say so in the diagnostic log (when
/// logging is on); otherwise nothing. A failure when the cursors could not
/// be restored.
pub(crate) fn after_parent(
    blank: impl FnOnce() -> bool,
    restore: impl FnOnce() -> Result<()>,
) -> ExitCode {
    if !blank() {
        return ExitCode::SUCCESS;
    }
    match restore() {
        Ok(()) => {
            diag::note("pointer.shown", vec![("reason", "guard".into())]);
            ExitCode::SUCCESS
        }
        Err(_) => ExitCode::FAILURE,
    }
}
