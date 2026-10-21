//! Conformance rules 6 (interfaces), 8 (secrets) and 9 (unsupported is
//! declared) against the real macOS backend.

use mouser_platform::conformance::{rule_6_interfaces, rule_8_secrets, rule_9_limitations};
use mouser_platform_macos::MacPlatform;

#[test]
fn rule_6_interface_classification() {
    rule_6_interfaces(&MacPlatform::new());
}

#[test]
fn rule_8_secrets_persist() {
    rule_8_secrets(&MacPlatform::new());
}

#[test]
fn rule_9_unsupported_is_declared() {
    rule_9_limitations(&MacPlatform::new());
}
