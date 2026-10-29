//! Conformance rules 10 and 11 (the pointer is visible after a stop or a
//! drop, refusals are errors) against the real Windows backend, with the
//! backend's own "pointer hidden" query.

#![cfg(windows)]

use mouser_platform::conformance::{rule_10_pointer_visible, rule_11_refusal_is_error};
use mouser_platform_windows::{WinPlatform, pointer_hidden};
use std::sync::Mutex;

/// The rules drive the machine's one input queue, set of hooks and cursors.
static LIVE: Mutex<()> = Mutex::new(());

fn live() -> std::sync::MutexGuard<'static, ()> {
    LIVE.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
fn rule_10_the_pointer_is_visible_after_a_stop_or_a_drop() {
    let _live = live();
    assert!(!pointer_hidden(), "the pointer is shown before the rule");
    rule_10_pointer_visible(&WinPlatform::new(), &pointer_hidden);
}

#[test]
fn rule_11_refusals_are_errors() {
    let _live = live();
    rule_11_refusal_is_error(&WinPlatform::new());
}
