//! The QuickJump letters: one borderless, transparent, click-through,
//! always-on-top window per local display, each showing the display's letter
//! (Story 11 sc. 1-3).

use std::sync::atomic::{AtomicU32, Ordering};

use mouser_platform::LocalDisplay;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::DisplayId;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Runtime, WebviewUrl,
    WebviewWindowBuilder,
};

pub const LABEL_PREFIX: &str = "quickjump-";
/// Tells a letter window that QuickJump ended.
pub const EVENT: &str = "mouser://quickjump";

/// One letter window to open.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub label: String,
    pub letter: char,
    pub origin: (f64, f64),
    pub size: (f64, f64),
}

/// The windows for the letters that name `own`'s displays.
pub fn plan(
    own: &ComputerId,
    displays: &[LocalDisplay],
    letters: &[(DisplayId, char)],
) -> Vec<Plan> {
    displays
        .iter()
        .enumerate()
        .filter_map(|(index, display)| {
            let (_, letter) = letters
                .iter()
                .find(|(id, _)| id.computer == *own && id.key == display.key)?;
            Some(Plan {
                label: format!("{LABEL_PREFIX}{index}"),
                letter: *letter,
                origin: display.origin,
                size: display.logical_size,
            })
        })
        .collect()
}

/// Each showing gets fresh labels: a destroyed window's label is free only
/// once the event loop has finished removing it.
static SHOWING: AtomicU32 = AtomicU32::new(0);

/// Replace whatever letters are showing with `plans`.
pub fn show<R: Runtime>(app: &AppHandle<R>, plans: &[Plan]) {
    hide(app);
    let showing = SHOWING.fetch_add(1, Ordering::Relaxed);
    for plan in plans {
        let url = format!("quickjump.html?letter={}", plan.letter);
        let window = WebviewWindowBuilder::new(
            app,
            format!("{}-{showing}", plan.label),
            WebviewUrl::App(url.into()),
        )
        .title("Mouser QuickJump")
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()
        .expect("QuickJump window can be created");
        window
            .set_position(LogicalPosition::new(plan.origin.0, plan.origin.1))
            .expect("QuickJump window can be moved");
        window
            .set_size(LogicalSize::new(plan.size.0, plan.size.1))
            .expect("QuickJump window can be sized");
        window.show().expect("QuickJump window can be shown");
        // Only once shown: on GTK a hidden window has no native window yet.
        window
            .set_ignore_cursor_events(true)
            .expect("QuickJump window ignores the pointer");
    }
}

/// Remove every letter window.
pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    for (label, window) in app.webview_windows() {
        if label.starts_with(LABEL_PREFIX) {
            window
                .emit_to(&label, EVENT, ())
                .expect("QuickJump window can be told");
            window.destroy().expect("QuickJump window can be removed");
        }
    }
}
