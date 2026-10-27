//! Wayland capture, injection and displays against the fake portals and a
//! fake libei server.

use super::daemon::{Daemon, wait_until};
use super::eis::Emit;
use super::portals::{InputCaptureConfig, PortalConfig, Portals};
use super::{full_portals, machine, machine_waiting};
use mouser_platform::{Error, InputCapture, InputInject, LocalInput, Platform};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

fn portals(bus: &Daemon, config: PortalConfig) -> Portals {
    Portals::start(&bus.address, config)
}

fn next(rx: &Receiver<LocalInput>) -> LocalInput {
    rx.recv_timeout(Duration::from_secs(10)).unwrap()
}

fn started_capture(
    bus: &Daemon,
    p: &Portals,
) -> (super::Machine, Box<dyn InputCapture>, Receiver<LocalInput>) {
    let m = machine(Some(bus), "wayland");
    let mut capture = m.platform.capture().unwrap();
    let (tx, rx) = channel();
    capture.start(tx).unwrap();
    wait_until("the capture to be enabled", || {
        p.calls.contains("ic.enable")
    });
    (m, capture, rx)
}

#[test]
fn capture_sets_barriers_on_every_outer_edge_and_keeps_the_token() {
    let bus = Daemon::start();
    let p = portals(
        &bus,
        PortalConfig {
            input_capture: Some(InputCaptureConfig::new(2, 15, Some("ic-token"))),
            ..full_portals()
        },
    );
    let (m, mut capture, _rx) = started_capture(&bus, &p);
    let calls = p.calls.all();
    assert!(calls.contains(&"registry.register com.mouser.app".to_string()));
    assert!(calls.contains(&"ic.start persist_mode=Some(2)".to_string()));
    let barriers: Vec<_> = calls
        .iter()
        .filter(|c| c.starts_with("ic.barrier"))
        .collect();
    assert_eq!(
        barriers,
        [
            "ic.barrier zone_set=7 id=1 line=(0, 0, 0, 1079)",
            "ic.barrier zone_set=7 id=2 line=(1920, 0, 1920, 1079)",
            "ic.barrier zone_set=7 id=3 line=(0, 0, 1919, 0)",
            "ic.barrier zone_set=7 id=4 line=(0, 1080, 1919, 1080)",
        ]
    );
    wait_until("the token to be kept", || {
        m.platform
            .secrets()
            .get("portal-token-input-capture")
            .unwrap()
            == Some(b"ic-token".to_vec())
    });
    capture.stop();
}

#[test]
fn the_stored_capture_token_is_replayed_on_the_next_start() {
    let bus = Daemon::start();
    let p = portals(
        &bus,
        PortalConfig {
            input_capture: Some(InputCaptureConfig::new(2, 15, Some("ic-token"))),
            ..full_portals()
        },
    );
    let (m, mut first, _rx) = started_capture(&bus, &p);
    wait_until("the token to be kept", || {
        m.platform
            .secrets()
            .get("portal-token-input-capture")
            .unwrap()
            .is_some()
    });
    first.stop();
    let mut second = m.platform.capture().unwrap();
    let (tx, _rx) = channel();
    second.start(tx).unwrap();
    wait_until("the second start", || {
        p.calls
            .all()
            .iter()
            .filter(|c| c.starts_with("ic.start.token"))
            .count()
            == 2
    });
    let tokens: Vec<_> = p
        .calls
        .all()
        .into_iter()
        .filter(|c| c.starts_with("ic.start.token"))
        .collect();
    assert_eq!(
        tokens,
        ["ic.start.token None", "ic.start.token Some(\"ic-token\")"]
    );
    second.stop();
}

#[test]
fn a_version_one_portal_creates_the_session_without_persistence() {
    let bus = Daemon::start();
    let p = portals(
        &bus,
        PortalConfig {
            input_capture: Some(InputCaptureConfig::new(1, 15, None)),
            ..full_portals()
        },
    );
    let (m, mut capture, _rx) = started_capture(&bus, &p);
    assert!(p.calls.contains("ic.create_session"));
    assert!(!p.calls.contains("ic.create_session2"));
    assert_eq!(
        m.platform
            .secrets()
            .get("portal-token-input-capture")
            .unwrap(),
        None
    );
    capture.stop();
}

#[test]
fn a_push_through_a_barrier_activates_and_libei_input_becomes_local_input() {
    let bus = Daemon::start();
    let p = portals(&bus, full_portals());
    let (_m, mut capture, rx) = started_capture(&bus, &p);
    let eis = p.eis(0);
    // libei can deliver motion before the portal says the push activated it.
    eis.send(Emit::Motion(2.0, 1.0));
    assert_eq!(
        next(&rx),
        LocalInput::Motion {
            dx: 2.0,
            dy: 1.0,
            abs: (2.0, 1.0)
        }
    );
    // The right edge (barrier 2) was pushed at y = 400; the early motion counts.
    p.activate(5, (1919.0, 400.0), 2);
    assert_eq!(
        next(&rx),
        LocalInput::Motion {
            dx: 1.0,
            dy: 0.0,
            abs: (1921.0, 401.0)
        }
    );
    eis.send(Emit::Motion(3.0, -2.0));
    assert_eq!(
        next(&rx),
        LocalInput::Motion {
            dx: 3.0,
            dy: -2.0,
            abs: (1924.0, 399.0)
        }
    );
    eis.send(Emit::Button(0x110, true));
    assert_eq!(
        next(&rx),
        LocalInput::Button {
            button: 1,
            down: true
        }
    );
    eis.send(Emit::Button(0x113, false));
    assert_eq!(
        next(&rx),
        LocalInput::Button {
            button: 4,
            down: false
        }
    );
    eis.send(Emit::Button(0x999, true));
    eis.send(Emit::Key(30, true));
    assert_eq!(
        next(&rx),
        LocalInput::Key {
            usage: 0x0007_0004,
            down: true
        }
    );
    eis.send(Emit::Key(0, true));
    eis.send(Emit::Scroll(0.0, 5.0));
    assert_eq!(next(&rx), LocalInput::Scroll { dx: 0.0, dy: -5.0 });
    eis.send(Emit::Discrete(0, -120));
    assert_eq!(next(&rx), LocalInput::Scroll { dx: 0.0, dy: 15.0 });
    // The grab is the activation: asking for it changes nothing, and
    // releasing hands the cursor back where Mouser last warped it.
    assert_eq!(capture.set_grabbed(true), Err(Error::Unsupported));
    capture.warp(100.0, 200.0).unwrap();
    assert_eq!(capture.set_grabbed(false), Err(Error::Unsupported));
    assert!(
        p.calls
            .contains("ic.release id=Some(5) at=Some((100.0, 200.0))")
    );
    assert_eq!(capture.set_grabbed(false), Err(Error::Unsupported));
    assert_eq!(
        p.calls
            .all()
            .iter()
            .filter(|c| c.starts_with("ic.release"))
            .count(),
        1
    );
    // A deactivation forgets the activation: after the next activation has
    // been seen (signals arrive in order) there is nothing of 6 to release.
    p.activate(6, (0.0, 300.0), 1);
    next(&rx);
    p.deactivate(6);
    p.activate(7, (0.0, 310.0), 1);
    next(&rx);
    assert_eq!(capture.set_grabbed(false), Err(Error::Unsupported));
    assert!(p.calls.contains("ic.release id=Some(7) at=None"));
    assert!(
        !p.calls
            .all()
            .iter()
            .any(|c| c.starts_with("ic.release id=Some(6)"))
    );
    capture.stop();
    assert!(p.calls.contains("session.close"));
    assert!(
        rx.try_iter()
            .all(|i| !matches!(i, LocalInput::CaptureLost(_)))
    );
}

#[test]
fn an_activation_without_a_known_barrier_is_a_motion_without_direction() {
    let bus = Daemon::start();
    let p = portals(&bus, full_portals());
    let (_m, mut capture, rx) = started_capture(&bus, &p);
    p.activate(1, (10.0, 20.0), 0);
    assert_eq!(
        next(&rx),
        LocalInput::Motion {
            dx: 0.0,
            dy: 0.0,
            abs: (10.0, 20.0)
        }
    );
    capture.stop();
}

#[test]
fn start_returns_while_the_approval_dialog_is_open_and_stop_still_works() {
    let bus = Daemon::start();
    let p = portals(
        &bus,
        PortalConfig {
            dialog_ms: 500,
            ..full_portals()
        },
    );
    let m = machine(Some(&bus), "wayland");
    let mut capture = m.platform.capture().unwrap();
    let (tx, _rx) = channel();
    capture.start(tx).unwrap();
    // Nobody has approved yet, and the caller is already free again.
    assert!(!p.calls.contains("ic.start.done"));
    // Until the session exists there is nothing to move or release.
    capture.warp(1.0, 1.0).unwrap();
    assert_eq!(capture.set_grabbed(true), Err(Error::Unsupported));
    assert_eq!(capture.set_grabbed(false), Err(Error::Unsupported));
    capture.stop();
    // The late approval finds Mouser stopped and closes the session.
    wait_until("the late session to be closed", || {
        p.calls.contains("session.close")
    });
}

#[test]
fn the_displays_are_the_capture_zones_and_are_asked_for_once() {
    let bus = Daemon::start();
    let p = portals(
        &bus,
        PortalConfig {
            input_capture: Some(InputCaptureConfig {
                zones: vec![(1920, 1080, 0, 0), (1280, 1024, 1920, 56)],
                ..InputCaptureConfig::new(2, 15, None)
            }),
            ..full_portals()
        },
    );
    let m = machine(Some(&bus), "wayland");
    let displays = m.platform.displays().unwrap();
    assert_eq!(displays.len(), 2);
    assert_eq!(displays[1].origin, (1920.0, 56.0));
    assert_eq!(displays[1].pixel_size, (1280, 1024));
    assert!(p.calls.contains("session.close"));
    m.platform.displays().unwrap();
    let zone_requests = p
        .calls
        .all()
        .iter()
        .filter(|c| *c == "ic.get_zones")
        .count();
    assert_eq!(zone_requests, 1);
}

// ---- injection ----

fn started_inject(bus: &Daemon, p: &Portals) -> (super::Machine, Box<dyn InputInject>) {
    let _ = p;
    let m = machine(Some(bus), "wayland");
    let inject = m.platform.inject().unwrap();
    (m, inject)
}

#[test]
fn injection_goes_over_libei_and_keeps_the_single_use_token() {
    let bus = Daemon::start();
    let p = portals(&bus, full_portals());
    let (m, mut inject) = started_inject(&bus, &p);
    inject.motion_abs(100.5, 200.25).unwrap();
    let eis = p.eis(0);
    inject.button(1, true).unwrap();
    inject.button(1, false).unwrap();
    inject.scroll(3.0, 20.0).unwrap();
    inject.key(0x0007_0004, true).unwrap();
    inject.key(0x0007_0004, false).unwrap();
    wait_until("the events to arrive", || eis.received().len() == 6);
    assert_eq!(
        eis.received(),
        [
            "abs 100.5 200.25",
            "button 0x110 Press",
            "button 0x110 Released",
            "scroll 3 -20",
            "key 30 Press",
            "key 30 Released",
        ]
    );
    assert!(p.calls.contains("rd.connect_to_eis"));
    assert!(p.calls.contains("clipboard.request"));
    assert_eq!(
        m.platform
            .secrets()
            .get("portal-token-remote-desktop")
            .unwrap(),
        Some(b"rd-token-1".to_vec())
    );
    drop(inject);
    assert!(p.calls.contains("session.close"));
}

#[test]
fn a_device_the_desktop_pauses_comes_back_when_it_resumes() {
    let bus = Daemon::start();
    let p = portals(&bus, full_portals());
    let (_m, mut inject) = started_inject(&bus, &p);
    inject.motion_abs(1.0, 1.0).unwrap();
    let eis = p.eis(0);
    wait_until("the first start", || eis.starts("pointer-abs") == 1);
    eis.send(Emit::Pause);
    eis.send(Emit::Resume);
    // Resuming makes Mouser start emulating again, so the pause came first.
    wait_until("the second start", || eis.starts("pointer-abs") == 2);
    inject.motion_abs(2.0, 2.0).unwrap();
    wait_until("the second motion", || {
        eis.received()
            .iter()
            .filter(|r| r.starts_with("abs"))
            .count()
            == 2
    });
}

#[test]
fn a_portal_that_returns_no_token_leaves_none_stored() {
    let bus = Daemon::start();
    let p = portals(
        &bus,
        PortalConfig {
            no_rd_token: true,
            ..full_portals()
        },
    );
    let (m, mut inject) = started_inject(&bus, &p);
    inject.motion_abs(1.0, 1.0).unwrap();
    assert_eq!(
        m.platform
            .secrets()
            .get("portal-token-remote-desktop")
            .unwrap(),
        None
    );
}

#[test]
fn the_stored_token_is_replayed_and_replaced() {
    let bus = Daemon::start();
    let p = portals(&bus, full_portals());
    let (m, mut first) = started_inject(&bus, &p);
    first.motion_abs(1.0, 1.0).unwrap();
    drop(first);
    let mut second = m.platform.inject().unwrap();
    second.motion_abs(2.0, 2.0).unwrap();
    drop(second);
    let selects: Vec<_> = p
        .calls
        .all()
        .into_iter()
        .filter(|c| c.starts_with("rd.select_devices "))
        .collect();
    assert_eq!(
        selects,
        [
            "rd.select_devices persist_mode=Some(2) token=None",
            "rd.select_devices persist_mode=Some(2) token=Some(\"rd-token-1\")"
        ]
    );
    assert_eq!(
        m.platform
            .secrets()
            .get("portal-token-remote-desktop")
            .unwrap(),
        Some(b"rd-token-2".to_vec())
    );
}

#[test]
fn held_input_is_released_when_the_injector_is_dropped() {
    let bus = Daemon::start();
    let p = portals(&bus, full_portals());
    let (_m, mut inject) = started_inject(&bus, &p);
    inject.key(0x0007_0004, true).unwrap();
    inject.button(2, true).unwrap();
    let eis = p.eis(0);
    wait_until("the presses", || eis.received().len() == 2);
    drop(inject);
    wait_until("the releases", || eis.received().len() == 4);
    let received = eis.received();
    assert!(
        received.contains(&"key 30 Released".to_string()),
        "{received:?}"
    );
    assert!(
        received.contains(&"button 0x111 Released".to_string()),
        "{received:?}"
    );
}

#[test]
fn an_unmapped_usage_is_an_error_and_secure_attention_is_unsupported() {
    let bus = Daemon::start();
    let p = portals(&bus, full_portals());
    let (_m, mut inject) = started_inject(&bus, &p);
    assert!(matches!(
        inject.key(0x0001_0004, true),
        Err(mouser_platform::Error::Os(_))
    ));
    assert_eq!(
        inject.secure_attention(),
        Err(mouser_platform::Error::Unsupported)
    );
}

#[test]
fn input_the_desktop_has_no_device_for_fails_after_the_wait() {
    let bus = Daemon::start();
    let p = portals(
        &bus,
        PortalConfig {
            no_keyboard: true,
            ..full_portals()
        },
    );
    let m = machine_waiting(Some(&bus), "wayland", Duration::from_millis(300));
    let mut inject = m.platform.inject().unwrap();
    inject.motion_abs(1.0, 1.0).unwrap();
    assert!(matches!(
        inject.key(0x0007_0004, true),
        Err(mouser_platform::Error::Os(_))
    ));
    drop(inject);
    assert!(p.calls.contains("session.close"));
}
