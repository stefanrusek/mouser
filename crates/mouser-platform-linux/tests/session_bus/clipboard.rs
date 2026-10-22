//! The Wayland clipboard through the portal on the RemoteDesktop session,
//! and the GNOME limitation read from the shell's version.

use crate::daemon::{Daemon, FAKES, wait_until};
use crate::portals::{PortalConfig, Portals};
use crate::{Machine, full_portals, machine_with};
use mouser_platform::{Capability, ClipRep, Clipboard, Error, Platform};
use std::time::Duration;
use zbus::interface;

const TEXT: &str = "text/plain;charset=utf-8";

fn machine(bus: &Daemon, desktop: &str) -> Machine {
    machine_with(
        Some(bus),
        "wayland",
        Duration::from_secs(5),
        &[("XDG_CURRENT_DESKTOP", desktop)],
    )
}

fn clipboard(m: &Machine) -> Box<dyn Clipboard> {
    m.platform.clipboard().unwrap()
}

#[test]
fn what_is_written_goes_to_the_portal_and_reads_back() {
    let bus = Daemon::start();
    let p = Portals::start(&bus.address, full_portals());
    let m = machine(&bus, "KDE");
    let mut clip = clipboard(&m);
    let start = clip.change_count();
    let reps = vec![
        ClipRep::Text("hello".into()),
        ClipRep::Html("<b>hi</b>".into()),
        ClipRep::Files(vec!["/tmp/x y".into()]),
    ];
    clip.write(reps.clone()).unwrap();
    assert_eq!(clip.change_count(), start + 1);
    assert!(p.calls.contains(&format!(
        "clipboard.set_selection {TEXT},text/html,text/uri-list,x-special/gnome-copied-files"
    )));
    assert!(p.calls.contains("clipboard.request"));
    assert_eq!(clip.read().unwrap(), reps);
}

#[test]
fn an_application_pasting_receives_what_was_written() {
    let bus = Daemon::start();
    let p = Portals::start(&bus.address, full_portals());
    let m = machine(&bus, "KDE");
    let mut clip = clipboard(&m);
    clip.write(vec![ClipRep::Html("<i>x</i>".into())]).unwrap();
    p.transfer("text/html", 7);
    assert_eq!(p.written(7), b"<i>x</i>");
    assert!(p.calls.contains("clipboard.write_done 7 true"));
    p.transfer("image/png", 8);
    wait_until("the refusal", || {
        p.calls.contains("clipboard.write_done 8 false")
    });
}

#[test]
fn another_applications_selection_is_followed_and_read() {
    let bus = Daemon::start();
    let p = Portals::start(&bus.address, full_portals());
    let m = machine(&bus, "KDE");
    let mut clip = clipboard(&m);
    let start = clip.change_count();
    p.set_contents(&[(TEXT, b"copied elsewhere"), ("image/png", &[1, 2, 3])]);
    p.owner_changed(&[TEXT, "image/png", "application/x-other"], false);
    wait_until("the change", || clip.change_count() == start + 1);
    assert_eq!(
        clip.read().unwrap(),
        [
            ClipRep::Text("copied elsewhere".into()),
            ClipRep::Png(vec![1, 2, 3])
        ]
    );
    // The portal also reports our own selections; they are not changes.
    clip.write(vec![ClipRep::Text("mine".into())]).unwrap();
    p.owner_changed(&[TEXT], true);
    p.owner_changed(&[], false);
    wait_until("the clear", || clip.change_count() == start + 3);
    assert_eq!(clip.read().unwrap(), []);
}

#[test]
fn an_owner_change_without_a_session_flag_or_types_clears_the_clipboard() {
    let bus = Daemon::start();
    let p = Portals::start(&bus.address, full_portals());
    let m = machine(&bus, "KDE");
    let mut clip = clipboard(&m);
    let start = clip.change_count();
    p.owner_changed(&[], false);
    wait_until("the change", || clip.change_count() == start + 1);
}

#[test]
fn a_selection_the_portal_cannot_deliver_fails_the_read() {
    let bus = Daemon::start();
    let p = Portals::start(&bus.address, full_portals());
    let m = machine(&bus, "KDE");
    let mut clip = clipboard(&m);
    let start = clip.change_count();
    p.owner_changed(&[TEXT], false);
    wait_until("the change", || clip.change_count() == start + 1);
    let error = clip.read().unwrap_err();
    assert!(
        matches!(&error, Error::Os(text) if text.contains("nothing offered as")),
        "{error:?}"
    );
}

#[test]
fn the_clipboard_follows_nothing_once_the_bus_is_gone() {
    let bus = Daemon::start();
    let p = Portals::start(&bus.address, full_portals());
    let m = machine(&bus, "KDE");
    let mut clip = clipboard(&m);
    clip.write(vec![ClipRep::Text("x".into())]).unwrap();
    drop(bus);
    std::thread::sleep(Duration::from_millis(500));
    // Closing the session needs the bus, which is an error by design.
    std::mem::forget(clip);
    drop(p);
}

#[test]
fn the_clipboard_shares_the_input_session() {
    let bus = Daemon::start();
    let p = Portals::start(&bus.address, full_portals());
    let m = machine(&bus, "KDE");
    let mut inject = m.platform.inject().unwrap();
    inject.motion_abs(1.0, 1.0).unwrap();
    let mut clip = clipboard(&m);
    clip.write(vec![ClipRep::Text("x".into())]).unwrap();
    assert_eq!(p.calls.all().iter().filter(|c| *c == "rd.start").count(), 1);
    drop(inject);
    assert!(!p.calls.contains("session.close"));
    drop(clip);
    assert!(p.calls.contains("session.close"));
}

#[test]
fn without_the_clipboard_in_the_grant_gnome_cannot_reach_it() {
    let bus = Daemon::start();
    let _p = Portals::start(
        &bus.address,
        PortalConfig {
            clipboard_enabled: false,
            ..full_portals()
        },
    );
    let m = machine(&bus, "ubuntu:GNOME");
    let mut clip = clipboard(&m);
    assert_eq!(clip.change_count(), 0);
    let error = clip.read().unwrap_err();
    assert_eq!(
        error,
        Error::Os("this desktop did not give Mouser access to the clipboard".into())
    );
    assert!(clip.write(vec![]).is_err());
}

#[test]
fn a_desktop_without_a_clipboard_portal_still_injects_input() {
    let bus = Daemon::start();
    let _p = Portals::start(
        &bus.address,
        PortalConfig {
            clipboard: false,
            ..full_portals()
        },
    );
    let m = machine(&bus, "GNOME");
    let mut inject = m.platform.inject().unwrap();
    inject.motion_abs(1.0, 1.0).unwrap();
    assert!(clipboard(&m).read().is_err());
}

struct Shell(&'static str);

#[interface(name = "org.gnome.Shell")]
impl Shell {
    #[zbus(property, name = "ShellVersion")]
    fn shell_version(&self) -> String {
        self.0.to_string()
    }
}

fn shell(bus: &Daemon, version: &'static str) -> zbus::Connection {
    FAKES
        .block_on(
            zbus::connection::Builder::address(bus.address.as_str())
                .unwrap()
                .name("org.gnome.Shell")
                .unwrap()
                .serve_at("/org/gnome/Shell", Shell(version))
                .unwrap()
                .build(),
        )
        .unwrap()
}

fn sharing_limits(m: &Machine) -> usize {
    m.platform
        .info()
        .limitations
        .iter()
        .filter(|l| {
            l.capability == Capability::Clipboard
                && l.text.contains(
                    "cannot share the clipboard from this computer while it shares its mouse",
                )
        })
        .count()
}

#[test]
fn gnome_before_51_lists_the_clipboard_sharing_limitation() {
    for (version, listed) in [("46.0", 1), ("50.1", 1), ("51.0", 0)] {
        let bus = Daemon::start();
        let _p = Portals::start(&bus.address, full_portals());
        let _shell = shell(&bus, version);
        assert_eq!(
            sharing_limits(&machine(&bus, "ubuntu:GNOME")),
            listed,
            "{version}"
        );
    }
}

#[test]
fn an_unreadable_shell_version_counts_as_before_51_and_other_desktops_have_no_limitation() {
    let bus = Daemon::start();
    let _p = Portals::start(&bus.address, full_portals());
    assert_eq!(sharing_limits(&machine(&bus, "GNOME")), 1);
    assert_eq!(sharing_limits(&machine(&bus, "KDE")), 0);
}
