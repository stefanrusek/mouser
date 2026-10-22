//! Conformance rules 6 (interfaces), 8 (secrets) and 9 (unsupported is
//! declared) against the real Linux backend on this machine's session.

#![cfg(target_os = "linux")]

use mouser_platform::conformance::{rule_6_interfaces, rule_8_secrets, rule_9_limitations};
use mouser_platform_linux::{LinuxPlatform, Settings};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

#[test]
fn rule_6_interface_classification() {
    rule_6_interfaces(&LinuxPlatform::new());
}

#[test]
fn rule_8_secrets_persist() {
    rule_8_secrets(&LinuxPlatform::new());
}

#[test]
fn rule_9_unsupported_is_declared() {
    // The X11 clipboard connects to its server, so this runs on a private one.
    let mut server = Command::new("Xvfb")
        .args([
            "-displayfd",
            "1",
            "-screen",
            "0",
            "1920x1080x24",
            "-nolisten",
            "tcp",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("Xvfb must be installed to run the X11 tests");
    let mut number = String::new();
    BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut number)
        .unwrap();
    let platform = LinuxPlatform::with_settings(Settings {
        env: HashMap::from([
            ("DISPLAY".to_string(), format!(":{}", number.trim())),
            ("XDG_SESSION_TYPE".to_string(), "x11".to_string()),
        ]),
        ..Settings::system()
    });
    rule_9_limitations(&platform);
    drop(platform);
    server.kill().unwrap();
    server.wait().unwrap();
}
