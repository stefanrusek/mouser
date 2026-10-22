//! Conformance rules 6 (interfaces), 8 (secrets) and 9 (unsupported is
//! declared) against the real Windows backend.

#![cfg(windows)]

use mouser_platform::conformance::{rule_6_interfaces, rule_8_secrets, rule_9_limitations};
use mouser_platform_windows::WinPlatform;

#[test]
fn rule_6_interface_classification() {
    rule_6_interfaces(&WinPlatform::new());
}

#[test]
fn rule_8_secrets_persist() {
    rule_8_secrets(&WinPlatform::new());
}

#[test]
fn rule_9_unsupported_is_declared() {
    rule_9_limitations(&WinPlatform::new());
}
