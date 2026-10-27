//! Conformance rules 10 (the pointer is visible after stop/drop) and 11 (OS
//! refusal is an error) against the real macOS backend.

use mouser_platform::conformance::{rule_10_pointer_visible, rule_11_refusal_is_error};
use mouser_platform_macos::MacPlatform;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGCursorIsVisible() -> bool;
}

fn pointer_hidden() -> bool {
    // SAFETY: a plain C query with no arguments.
    !unsafe { CGCursorIsVisible() }
}

#[test]
fn rule_10_pointer_is_visible_after_stop_and_drop() {
    rule_10_pointer_visible(&MacPlatform::new(), &pointer_hidden);
}

#[test]
fn rule_11_refusal_is_an_error() {
    rule_11_refusal_is_error(&MacPlatform::new());
}
