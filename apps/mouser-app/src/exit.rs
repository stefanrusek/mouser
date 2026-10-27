//! The ways the app ends, each of which stops the engine first so the pointer
//! is shown and held input is released (research V4).

use tauri::{AppHandle, Manager, Runtime};

use crate::host::Host;

/// The app is exiting, however it got there (`app.exit`, the updater's
/// restart, logoff or shutdown).
pub fn on_exit<R: Runtime>(app: &AppHandle<R>) {
    tauri::async_runtime::block_on(app.state::<Host>().shutdown());
}

/// Tray Quit: stop the engine, then `exit` the app. The engine goes first
/// because exiting from the tray menu does not reach the exit event in time
/// (tauri#12310).
pub fn quit<R: Runtime>(app: &AppHandle<R>, exit: impl FnOnce()) {
    on_exit(app);
    exit();
}
