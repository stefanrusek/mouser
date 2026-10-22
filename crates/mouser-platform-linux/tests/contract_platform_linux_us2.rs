//! Conformance rules 1, 2, 3, 4 and 7 against the X11 backend, running on a
//! private Xvfb.

use mouser_platform::Platform;
use mouser_platform::conformance::{
    rule_1_grab, rule_2_release, rule_3_hid_usage, rule_4_displays, rule_7_no_self_capture,
};
use mouser_platform_linux::{LinuxPlatform, Settings};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

struct Xvfb(Child);

fn start_xvfb() -> (Xvfb, String) {
    let mut child = Command::new("Xvfb")
        .args([
            "-displayfd",
            "1",
            "-screen",
            "0",
            "1920x1080x24",
            "-nolisten",
            "tcp",
            "-noreset",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Xvfb must be installed to run the X11 tests");
    let mut number = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut number)
        .unwrap();
    (Xvfb(child), format!(":{}", number.trim()))
}

impl Drop for Xvfb {
    fn drop(&mut self) {
        self.0.kill().unwrap();
        self.0.wait().unwrap();
    }
}

/// Runs `check` against a fresh X11 session. The platform is dropped before
/// the server goes away.
fn on_x11(check: fn(&dyn Platform)) {
    let (_server, display) = start_xvfb();
    let platform = LinuxPlatform::with_settings(Settings {
        env: HashMap::from([
            ("DISPLAY".to_string(), display),
            ("XDG_SESSION_TYPE".to_string(), "x11".to_string()),
        ]),
        sys_class_net: "/sys/class/net".into(),
        os_release: "/etc/os-release".into(),
        exe: "/bin/mouser".into(),
        device_wait: std::time::Duration::from_millis(300),
    });
    check(&platform);
}

#[test]
fn rule_1_grab_swallows_and_parks() {
    on_x11(rule_1_grab);
}

#[test]
fn rule_2_injected_input_is_releasable() {
    on_x11(rule_2_release);
}

#[test]
fn rule_3_keys_by_hid_usage() {
    on_x11(rule_3_hid_usage);
}

#[test]
fn rule_4_display_scale_and_size() {
    on_x11(rule_4_displays);
}

#[test]
fn rule_7_injected_input_is_not_captured_back() {
    on_x11(rule_7_no_self_capture);
}
