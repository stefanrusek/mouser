//! Conformance rule 5 against the X11 clipboard on a private Xvfb, and the
//! targets a second client (any other application) sees.

#![cfg(target_os = "linux")]

use mouser_platform::conformance::rule_5_clipboard;
use mouser_platform::{ClipRep, Platform};
use mouser_platform_linux::{LinuxPlatform, Settings};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, Property, WindowClass,
};
use x11rb::rust_connection::RustConnection;

struct Xvfb(Child);

impl Drop for Xvfb {
    fn drop(&mut self) {
        self.0.kill().unwrap();
        self.0.wait().unwrap();
    }
}

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

fn platform(display: &str) -> LinuxPlatform {
    LinuxPlatform::with_settings(Settings {
        env: HashMap::from([
            ("DISPLAY".to_string(), display.to_string()),
            ("XDG_SESSION_TYPE".to_string(), "x11".to_string()),
        ]),
        sys_class_net: "/sys/class/net".into(),
        os_release: "/etc/os-release".into(),
        exe: "/bin/mouser".into(),
        device_wait: std::time::Duration::from_millis(300),
    })
}

#[test]
fn rule_5_clipboard_round_trip() {
    let (_server, display) = start_xvfb();
    rule_5_clipboard(&platform(&display));
}

/// Another application pasting: asks for `target` the ICCCM way, following
/// `INCR` transfers.
struct Paster {
    conn: RustConnection,
    window: u32,
}

impl Paster {
    fn new(display: &str) -> Paster {
        let (conn, screen) = x11rb::connect(Some(display)).unwrap();
        let root = conn.setup().roots[screen].root;
        let window = conn.generate_id().unwrap();
        conn.create_window(
            0,
            window,
            root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .unwrap();
        Paster { conn, window }
    }

    fn atom(&self, name: &str) -> u32 {
        self.conn
            .intern_atom(false, name.as_bytes())
            .unwrap()
            .reply()
            .unwrap()
            .atom
    }

    fn take(&self, property: u32) -> x11rb::protocol::xproto::GetPropertyReply {
        self.conn
            .get_property(true, self.window, property, AtomEnum::ANY, 0, u32::MAX / 4)
            .unwrap()
            .reply()
            .unwrap()
    }

    fn paste(&self, target: &str) -> Option<Vec<u8>> {
        let property = self.atom("PASTE");
        self.conn
            .convert_selection(
                self.window,
                self.atom("CLIPBOARD"),
                self.atom(target),
                property,
                x11rb::CURRENT_TIME,
            )
            .unwrap();
        self.conn.flush().unwrap();
        loop {
            if let Event::SelectionNotify(n) = self.conn.wait_for_event().unwrap() {
                if n.property == 0 {
                    return None;
                }
                break;
            }
        }
        let first = self.take(property);
        if first.type_ != self.atom("INCR") {
            return Some(first.value);
        }
        let mut data = Vec::new();
        loop {
            if let Event::PropertyNotify(p) = self.conn.wait_for_event().unwrap()
                && p.atom == property
                && p.state == Property::NEW_VALUE
            {
                let chunk = self.take(property).value;
                if chunk.is_empty() {
                    return Some(data);
                }
                data.extend(chunk);
            }
        }
    }
}

#[test]
fn another_application_pastes_every_type_the_clipboard_offers() {
    let (_server, display) = start_xvfb();
    let p = platform(&display);
    let mut clipboard = p.clipboard().unwrap();
    let png: Vec<u8> = (0..200_000u32).map(|i| (i % 253) as u8).collect();
    clipboard
        .write(vec![
            ClipRep::Text("héllo".into()),
            ClipRep::Html("<b>x</b>".into()),
            ClipRep::Rtf(b"{\\rtf1 x}".to_vec()),
            ClipRep::Png(png.clone()),
            ClipRep::Files(vec!["/tmp/a b".into()]),
        ])
        .unwrap();
    let paster = Paster::new(&display);
    let listed = paster.paste("TARGETS").unwrap();
    let targets: Vec<u32> = listed
        .as_chunks::<4>()
        .0
        .iter()
        .map(|a| u32::from_ne_bytes(*a))
        .collect();
    for name in [
        "UTF8_STRING",
        "text/html",
        "text/rtf",
        "image/png",
        "text/uri-list",
        "x-special/gnome-copied-files",
    ] {
        assert!(targets.contains(&paster.atom(name)), "{name}");
    }
    assert_eq!(paster.paste("UTF8_STRING").unwrap(), "héllo".as_bytes());
    assert_eq!(paster.paste("text/html").unwrap(), b"<b>x</b>");
    assert_eq!(paster.paste("text/rtf").unwrap(), b"{\\rtf1 x}");
    assert_eq!(paster.paste("image/png").unwrap(), png);
    assert_eq!(
        paster.paste("text/uri-list").unwrap(),
        b"file:///tmp/a%20b\r\n"
    );
    assert_eq!(
        paster.paste("x-special/gnome-copied-files").unwrap(),
        b"copy\nfile:///tmp/a%20b"
    );
    drop(clipboard);
}
