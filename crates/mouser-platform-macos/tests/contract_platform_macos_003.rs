//! Spec 003 Story 2 against the real macOS backend: the pointer arrives while
//! a game holds the mouse, and the cursor ends up usable. The game itself is
//! scripted in the unit tests (`capture_guard_tests`, `Cursor` is crate-private);
//! here the real backend is driven through the public trait and the window
//! server is asked whether the cursor is shown.

use mouser_platform::conformance::rule_10_pointer_visible;
use mouser_platform::{InputCapture, Platform};
use mouser_platform_macos::MacPlatform;
use std::sync::Mutex;
use std::sync::mpsc::channel;
use std::time::Duration;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGCursorIsVisible() -> bool;
    fn _CGSDefaultConnection() -> i32;
    fn CGSGetCursorScale(connection: i32, scale: *mut f32) -> i32;
}

static SERIAL: Mutex<()> = Mutex::new(());

/// Waits for the window server's reply, so queued show/hide messages are done.
fn pointer_hidden() -> bool {
    let mut scale = 0.0;
    // SAFETY: plain C calls; `scale` outlives the call.
    unsafe {
        assert_eq!(CGSGetCursorScale(_CGSDefaultConnection(), &mut scale), 0);
        !CGCursorIsVisible()
    }
}

fn grabbed_capture() -> Box<dyn InputCapture> {
    let mut capture = MacPlatform::new().capture().unwrap();
    let (tx, rx) = channel();
    std::mem::forget(rx);
    capture.start(tx).unwrap();
    capture.set_grabbed(true).unwrap();
    capture
}

#[test]
fn arrival_leaves_the_cursor_shown_and_it_stays_so_after_the_window() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut capture = grabbed_capture();
    assert!(pointer_hidden(), "grabbed: hidden");
    capture.set_grabbed(false).unwrap();
    assert!(!pointer_hidden(), "arrived: shown at once");
    // Story 2 sc.2: nobody acts afterwards, and the cursor behaves normally.
    std::thread::sleep(Duration::from_millis(1200));
    assert!(!pointer_hidden(), "shown after the settle window");
    capture.stop();
    assert!(!pointer_hidden());
}

#[test]
fn the_recovery_key_during_the_window_leaves_a_usable_state() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let mut capture = grabbed_capture();
    capture.set_grabbed(false).unwrap();
    // Story 2 sc.3: stop mid-window, as the recovery key does.
    capture.stop();
    assert!(!pointer_hidden(), "shown after stop inside the window");
    drop(capture);
    std::thread::sleep(Duration::from_millis(100));
    assert!(!pointer_hidden());
}

#[test]
fn rule_10_still_holds_with_the_settle_window() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    rule_10_pointer_visible(&MacPlatform::new(), &pointer_hidden);
}
