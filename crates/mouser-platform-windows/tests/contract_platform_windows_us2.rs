//! Conformance rules 1, 2, 3, 4 and 7 (grab, release, key by HID usage,
//! displays, no self-capture) against the real Windows backend.

#![cfg(windows)]

use mouser_platform::conformance::{
    rule_1_grab, rule_2_release, rule_3_hid_usage, rule_4_displays, rule_7_no_self_capture,
};
use mouser_platform_windows::WinPlatform;
use std::sync::Mutex;

/// The rules drive the machine's one input queue and set of hooks.
static LIVE: Mutex<()> = Mutex::new(());

fn live() -> std::sync::MutexGuard<'static, ()> {
    LIVE.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
fn rule_1_grab_and_release() {
    let _live = live();
    rule_1_grab(&WinPlatform::new());
}

#[test]
fn rule_2_injected_input_is_releasable() {
    let _live = live();
    rule_2_release(&WinPlatform::new());
}

#[test]
fn rule_3_keys_by_hid_usage() {
    let _live = live();
    rule_3_hid_usage(&WinPlatform::new());
}

#[test]
fn rule_4_display_scale_and_size() {
    rule_4_displays(&WinPlatform::new());
}

#[test]
fn rule_7_injected_input_is_not_captured() {
    let _live = live();
    rule_7_no_self_capture(&WinPlatform::new());
}
