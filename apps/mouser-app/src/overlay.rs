//! The landing highlight: a transparent, click-through, always-on-top window
//! that flashes a ring where the pointer arrived (Story 2 sc. 9).

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalPosition, Manager, Runtime, WebviewUrl, WebviewWindowBuilder,
};

pub const LABEL: &str = "landing";
/// Tells the overlay page where to draw.
pub const EVENT: &str = "mouser://landing";
/// The overlay window is a square this many logical pixels wide.
pub const SIZE: f64 = 160.0;

#[derive(Clone, Serialize)]
struct Spot {
    x: f64,
    y: f64,
}

/// Create the hidden overlay window (once, at startup, so its page is ready).
pub fn create<R: Runtime>(app: &AppHandle<R>) {
    crate::portable::with_webview_data(
        app,
        WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("landing.html".into())),
    )
    .title("Mouser pointer")
    .inner_size(SIZE, SIZE)
    .transparent(true)
    .decorations(false)
    .shadow(false)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false)
    .build()
    .expect("landing overlay can be created");
}

/// Flash the highlight around `desktop`, a point in desktop coordinates.
pub fn show_landing<R: Runtime>(app: &AppHandle<R>, desktop: (f64, f64)) {
    let window = app
        .get_webview_window(LABEL)
        .expect("the landing overlay was created at startup");
    let half = SIZE / 2.0;
    window
        .set_position(LogicalPosition::new(desktop.0 - half, desktop.1 - half))
        .expect("landing overlay can be moved");
    window.show().expect("landing overlay can be shown");
    // Only once shown: on GTK a hidden window has no native window yet, and
    // setting this before then aborts the app at startup (tao unwraps None).
    window
        .set_ignore_cursor_events(true)
        .expect("landing overlay ignores the pointer");
    window
        .emit_to(LABEL, EVENT, Spot { x: half, y: half })
        .expect("landing overlay can be told");
}
