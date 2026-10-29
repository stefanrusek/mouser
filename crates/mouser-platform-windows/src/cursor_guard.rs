//! The cursor guard (research V5; Story 1 sc. 6). Mouser hides the pointer
//! by replacing the system cursors with a blank one, and puts them back when
//! it lets go. If Mouser ends without doing that, killed, crashed or cut off
//! mid-grab, the pointer would stay invisible. The guard is a small process
//! of its own, with no window: it waits for Mouser to end and, if the arrow
//! is then still blank, reloads the user's cursors. A normal capture stop
//! ends the guard through a named event, without killing it.

use crate::cursor::{arrow_is_blank, recover};
use crate::diag;
use mouser_platform::{Error, Result};
use std::process::ExitCode;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateEventW, INFINITE, OpenEventW, OpenProcess, PROCESS_SYNCHRONIZE, ResetEvent,
    SYNCHRONIZATION_SYNCHRONIZE, SetEvent, WaitForMultipleObjects,
};
use windows::core::HSTRING;

/// Runs the cursor guard for `parent_pid` (research V5). Returns when the parent has ended,
/// after restoring blank system cursors, or when the parent has stopped its capture (it sets the
/// named event `Local\MouserCursorGuard.<parent_pid>`; the cursors are then left alone, the parent
/// has shown the pointer itself). Never creates a window.
pub fn run(parent_pid: u32) -> ExitCode {
    match wait_for(parent_pid) {
        Ended::Stopped => ExitCode::SUCCESS,
        Ended::Parent => after_parent(&arrow_is_blank, &recover),
    }
}

/// Why the guard stopped waiting.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Ended {
    /// The parent ended, for whatever reason.
    Parent,
    /// The parent stopped its capture.
    Stopped,
}

/// The name of the event with which process `parent_pid` stops its guard.
fn stop_name(parent_pid: u32) -> HSTRING {
    HSTRING::from(format!(r"Local\MouserCursorGuard.{parent_pid}"))
}

/// The event with which a parent stops its guard. The parent makes it
/// before it starts the guard, so a guard that starts late still sees a stop
/// made meanwhile: once set, it stays set.
pub(crate) struct StopEvent(HANDLE);

// A kernel handle may be used and closed from any thread.
unsafe impl Send for StopEvent {}

impl StopEvent {
    /// The stop event of process `parent_pid`, not set.
    pub(crate) fn create(parent_pid: u32) -> Result<StopEvent> {
        let name = stop_name(parent_pid);
        let event = unsafe { CreateEventW(None, true, false, &name) }
            .map_err(|e| Error::Os(format!("CreateEventW: {e}")))?;
        // An event left set by an earlier stop of this process starts over.
        let _ = unsafe { ResetEvent(event) };
        Ok(StopEvent(event))
    }

    /// Stops the guard.
    pub(crate) fn set(&self) {
        let _ = unsafe { SetEvent(self.0) };
    }
}

impl Drop for StopEvent {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Waits until process `pid` has ended or has stopped its capture. A stop
/// counts even when the parent has also ended since: it showed the pointer
/// before it set the event. A parent that cannot be opened has already
/// ended.
pub(crate) fn wait_for(pid: u32) -> Ended {
    let name = stop_name(pid);
    let stop = unsafe { OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, &name) }.ok();
    let parent = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }.ok();
    // The stop first: when both are signalled the wait names the first.
    let handles: Vec<HANDLE> = stop.into_iter().chain(parent).collect();
    let ms = if parent.is_some() { INFINITE } else { 0 };
    // With nothing to wait on this fails at once: the parent has ended.
    let first = unsafe { WaitForMultipleObjects(&handles, false, ms) };
    for handle in handles {
        let _ = unsafe { CloseHandle(handle) };
    }
    if stop.is_some() && first == WAIT_OBJECT_0 {
        Ended::Stopped
    } else {
        Ended::Parent
    }
}

/// What the guard does once its parent has ended: when the arrow is `blank`,
/// `restore` the user's cursors and say so in the diagnostic log (when
/// logging is on); otherwise nothing. A failure when the cursors could not
/// be restored.
pub(crate) fn after_parent(blank: &dyn Fn() -> bool, restore: &dyn Fn() -> Result<()>) -> ExitCode {
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
