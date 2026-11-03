//! The ways the app ends, each of which stops the engine first so the pointer
//! is shown and held input is released (research V4).

use tauri::{AppHandle, Manager, Runtime};

use crate::host::Host;

/// The app is exiting, however it got there (`app.exit`, the updater's
/// restart, logoff or shutdown).
pub fn on_exit<R: Runtime>(app: &AppHandle<R>) {
    tauri::async_runtime::block_on(app.state::<Host>().shutdown());
    #[cfg(target_os = "windows")]
    if let Some(elevated) = app.try_state::<Elevated>() {
        // An elevated portable instance is no longer running.
        elevated.0.lock().unwrap().take();
    }
}

/// An elevated portable instance's mark (FR-005), removed on exit.
#[cfg(target_os = "windows")]
pub struct Elevated(pub std::sync::Mutex<Option<crate::elevation::Marker>>);

/// How the app exits; tests put a fake here.
pub struct Exit(pub std::sync::Arc<dyn Fn() + Send + Sync>);

/// Tray Quit: stop the engine, then `exit` the app. The engine goes first
/// because exiting from the tray menu does not reach the exit event in time
/// (tauri#12310). In UI mode the session engine is asked to quit too, so
/// `MouserService` stops.
pub fn quit<R: Runtime>(app: &AppHandle<R>, exit: impl FnOnce()) {
    tauri::async_runtime::block_on(app.state::<Host>().quit());
    on_exit(app);
    exit();
}
