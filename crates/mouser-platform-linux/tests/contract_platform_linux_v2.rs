//! Conformance rules 10 (pointer visible) and 11 (refusal is an error)
//! against the X11 backend on a private Xvfb.

#![cfg(target_os = "linux")]

use mouser_platform::conformance::{rule_10_pointer_visible, rule_11_refusal_is_error};
use mouser_platform_linux::{LinuxPlatform, Settings};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use x11rb::protocol::xfixes::ConnectionExt as _;

/// The cursor on screen is invisible when no pixel of its image is opaque.
fn pointer_hidden(display: &str) -> bool {
    let (conn, _) = x11rb::connect(Some(display)).unwrap();
    conn.xfixes_query_version(4, 0).unwrap().reply().unwrap();
    let image = conn.xfixes_get_cursor_image().unwrap().reply().unwrap();
    image.cursor_image.iter().all(|argb| argb >> 24 == 0)
}

#[test]
fn rules_10_and_11_hold_on_x11() {
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
    let display = format!(":{}", number.trim());
    let platform = LinuxPlatform::with_settings(Settings {
        env: HashMap::from([
            ("DISPLAY".to_string(), display.clone()),
            ("XDG_SESSION_TYPE".to_string(), "x11".to_string()),
        ]),
        ..Settings::system()
    });
    rule_10_pointer_visible(&platform, &|| pointer_hidden(&display));
    rule_11_refusal_is_error(&platform);
    drop(platform);
    server.kill().unwrap();
    server.wait().unwrap();
}
