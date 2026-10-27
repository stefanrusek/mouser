//! Conformance rules 10 (the pointer is visible after stop/drop) and 11 (OS
//! refusal is an error) against the real macOS backend.

use mouser_platform::conformance::{rule_10_pointer_visible, rule_11_refusal_is_error};
use mouser_platform_macos::MacPlatform;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGCursorIsVisible() -> bool;
    fn _CGSDefaultConnection() -> i32;
    fn CGSGetCursorScale(connection: i32, scale: *mut f32) -> i32;
}

/// `CGCursorIsVisible` reads the window server's shared state, but
/// `CGDisplayShowCursor` is a one-way message that may still be queued when
/// `stop()` returns. A request that waits for the window server's reply is
/// answered only after every message this process sent before it, so the
/// state read afterwards includes the show.
fn pointer_hidden() -> bool {
    let mut scale = 0.0;
    // SAFETY: plain C calls; `scale` outlives the call.
    unsafe {
        assert_eq!(CGSGetCursorScale(_CGSDefaultConnection(), &mut scale), 0);
        !CGCursorIsVisible()
    }
}

#[test]
fn rule_10_pointer_is_visible_after_stop_and_drop() {
    rule_10_pointer_visible(&MacPlatform::new(), &pointer_hidden);
}

#[test]
fn rule_11_refusal_is_an_error() {
    rule_11_refusal_is_error(&MacPlatform::new());
}
