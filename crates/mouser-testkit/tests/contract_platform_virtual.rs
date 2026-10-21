//! Consumer side of the platform seam: every `mouser_platform::conformance`
//! check run against `VirtualPlatform`, both fully capable and `limited()`.

use mouser_platform::Platform;
use mouser_platform::conformance::*;
use mouser_testkit::VirtualPlatform;

fn on_both(check: fn(&dyn Platform)) {
    check(&VirtualPlatform::new());
    check(&VirtualPlatform::limited());
}

#[test]
fn contract_rule_1_grab() {
    on_both(rule_1_grab);
}

#[test]
fn contract_rule_2_release() {
    on_both(rule_2_release);
}

#[test]
fn contract_rule_3_hid_usage() {
    on_both(rule_3_hid_usage);
}

#[test]
fn contract_rule_4_displays() {
    on_both(rule_4_displays);
}

#[test]
fn contract_rule_5_clipboard() {
    on_both(rule_5_clipboard);
}

#[test]
fn contract_rule_6_interfaces() {
    on_both(rule_6_interfaces);
}

#[test]
fn contract_rule_7_no_self_capture() {
    on_both(rule_7_no_self_capture);
}

#[test]
fn contract_rule_8_secrets() {
    on_both(rule_8_secrets);
}

#[test]
fn contract_rule_9_limitations() {
    on_both(rule_9_limitations);
}

#[test]
fn contract_all_checks_listed() {
    assert_eq!(ALL.len(), 9);
    for (_, check) in ALL {
        on_both(*check);
    }
}
