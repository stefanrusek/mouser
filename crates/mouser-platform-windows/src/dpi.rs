//! Per-Monitor-V2 DPI awareness for the calling thread, so monitor rectangles,
//! cursor positions and hook coordinates are all physical pixels whatever the
//! host process declared.

use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext,
};

/// Runs `f` with this thread Per-Monitor-V2 aware, then restores the previous
/// awareness.
pub(crate) fn physical<T>(f: impl FnOnce() -> T) -> T {
    let old = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let out = f();
    unsafe { SetThreadDpiAwarenessContext(old) };
    out
}

/// Makes this (dedicated) thread Per-Monitor-V2 aware for its whole life.
pub(crate) fn physical_thread() {
    unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}
