//! The KDE fallback: when the grant has no portal clipboard the clipboard
//! goes through `ext-data-control`. wl-clipboard-rs reads the compositor's
//! socket from the process environment. Nothing may change the environment of
//! a running multi-threaded test binary, so the scenario runs in a child
//! process of this binary that starts with `WAYLAND_DISPLAY` already set.

use crate::compositor::Compositor;
use crate::daemon::{Daemon, Process, wait_until};
use crate::portals::{PortalConfig, Portals};
use crate::{Machine, full_portals, machine_with};
use mouser_platform::{ClipRep, Platform};
use std::io::Read;
use std::time::Duration;
use wl_clipboard_rs::copy::{self, MimeSource, MimeType as CopyMime, Options, Source};
use wl_clipboard_rs::paste::{ClipboardType, MimeType, Seat, get_contents};

const TEXT: &str = "text/plain;charset=utf-8";

fn machine(bus: &Daemon) -> Machine {
    machine_with(
        Some(bus),
        "wayland",
        Duration::from_secs(5),
        &[("XDG_CURRENT_DESKTOP", "KDE")],
    )
}

/// Another application copying: serves `text` until something replaces it.
fn app_copies(text: &str) {
    let mut options = Options::new();
    options.foreground(true);
    let prepared = options
        .prepare_copy_multi(vec![MimeSource {
            source: Source::Bytes(text.as_bytes().into()),
            mime_type: CopyMime::Specific(TEXT.into()),
        }])
        .unwrap();
    std::thread::spawn(move || {
        let _ = prepared.serve();
    });
}

fn app_pastes() -> String {
    let (mut pipe, _) = get_contents(
        ClipboardType::Regular,
        Seat::Unspecified,
        MimeType::Specific(TEXT),
    )
    .unwrap();
    let mut text = String::new();
    pipe.read_to_string(&mut text).unwrap();
    text
}

const NAME: &str = "datacontrol::without_the_portal_clipboard_kde_uses_data_control";
/// Set in the child: the compositor socket it listens on (and `WAYLAND_DISPLAY`).
const CHILD: &str = "MOUSER_TEST_DATACONTROL_SOCKET";

#[test]
fn without_the_portal_clipboard_kde_uses_data_control() {
    match std::env::var_os(CHILD) {
        None => {
            let dir = tempfile::tempdir().unwrap();
            let socket = dir.path().join("wayland-test");
            // Output goes to files, not pipes: whatever the child leaves
            // running cannot keep this test waiting for the end of a pipe.
            let (out, err) = (dir.path().join("out"), dir.path().join("err"));
            let mut child = Process(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
                    .env(CHILD, &socket)
                    .env("WAYLAND_DISPLAY", &socket)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::fs::File::create(&out).unwrap())
                    .stderr(std::fs::File::create(&err).unwrap())
                    .spawn()
                    .unwrap(),
            );
            let mut status = None;
            wait_until("the child test", || {
                status = child.0.try_wait().unwrap();
                status.is_some()
            });
            assert!(
                status.unwrap().success(),
                "{}\n{}",
                std::fs::read_to_string(&out).unwrap(),
                std::fs::read_to_string(&err).unwrap()
            );
        }
        Some(socket) => scenario(socket.into()),
    }
}

fn scenario(socket: std::path::PathBuf) {
    let bus = Daemon::start();
    let p = Portals::start(
        &bus.address,
        PortalConfig {
            clipboard_enabled: false,
            ..full_portals()
        },
    );
    let m = machine(&bus);

    // Nothing to reach yet: no compositor is listening on the socket.
    let mut lost = m.platform.clipboard().unwrap();
    assert_eq!(lost.change_count(), 0);
    assert!(lost.read().is_err());
    drop(lost);

    let compositor = Compositor::start(socket);
    let mut clip = m.platform.clipboard().unwrap();
    let start = clip.change_count();
    assert_eq!(start, 1, "the first event is the selection as it is");
    assert_eq!(clip.read().unwrap(), []);

    app_copies("from an app");
    wait_until("the app's copy", || clip.change_count() > start);
    assert_eq!(clip.read().unwrap(), [ClipRep::Text("from an app".into())]);

    let before = clip.change_count();
    let reps = vec![
        ClipRep::Text("from mouser".into()),
        ClipRep::Html("<b>m</b>".into()),
        ClipRep::Png(vec![0x89, 1, 2, 3]),
        ClipRep::Files(vec!["/tmp/a b".into()]),
    ];
    clip.write(reps.clone()).unwrap();
    assert!(clip.change_count() > before);
    assert_eq!(clip.read().unwrap(), reps);
    assert_eq!(app_pastes(), "from mouser");
    // The portal was only asked for the grant, never for clipboard data.
    assert!(
        !p.calls
            .all()
            .iter()
            .any(|c| c.starts_with("clipboard.set_selection"))
    );

    copy::clear(copy::ClipboardType::Regular, copy::Seat::All).unwrap();
    wait_until("the clear", || clip.read().unwrap().is_empty());

    drop(compositor);
    assert!(clip.read().is_err());
    drop(clip);
}
