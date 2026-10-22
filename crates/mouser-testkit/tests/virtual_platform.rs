//! VirtualPlatform scripting and recording capabilities.

use mouser_platform::*;
use mouser_testkit::{Injected, VirtualPlatform};
use std::path::PathBuf;
use std::sync::mpsc::channel;

fn display(key: &str) -> LocalDisplay {
    LocalDisplay {
        key: key.into(),
        name: key.into(),
        pixel_size: (2880, 1800),
        logical_size: (1440.0, 900.0),
        scale: 2.0,
        physical_size_mm: None,
        origin: (1920.0, 0.0),
    }
}

#[test]
fn scripted_displays_notify_watchers_until_dropped() {
    let vp = VirtualPlatform::new();
    assert_eq!(vp.displays().unwrap().len(), 1);
    let (tx, rx) = channel();
    let handle = vp.watch_displays(tx).unwrap();
    vp.set_displays(vec![display("a"), display("b")]);
    assert_eq!(rx.try_recv().unwrap().len(), 2);
    assert_eq!(vp.displays().unwrap()[1].key, "b");
    drop(handle);
    vp.set_displays(vec![]);
    assert!(rx.try_recv().is_err());
}

#[test]
fn local_input_is_delivered_and_swallowed_while_grabbed() {
    let vp = VirtualPlatform::new();
    let mut capture = vp.capture().unwrap();
    let (tx, rx) = channel();
    capture.start(tx).unwrap();
    assert!(vp.is_capturing());

    let key = LocalInput::Key {
        usage: 0x0007_0004,
        down: true,
    };
    assert!(vp.send_input(key));
    capture.set_grabbed(true).unwrap();
    assert!(vp.is_grabbed());
    let motion = LocalInput::Motion {
        dx: 1.0,
        dy: 2.0,
        abs: (3.0, 4.0),
    };
    assert!(!vp.send_input(motion));
    capture.set_grabbed(false).unwrap();
    capture.warp(5.0, 6.0).unwrap();

    assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec![key, motion]);
    assert_eq!(vp.local_delivered(), vec![key]);
    assert_eq!(vp.warps(), vec![(5.0, 6.0)]);

    // Dropping the capture stops it and releases the grab.
    capture.set_grabbed(true).unwrap();
    drop(capture);
    assert!(!vp.is_capturing());
    assert!(!vp.is_grabbed());
}

#[test]
fn input_without_capture_reaches_local_apps() {
    let vp = VirtualPlatform::new();
    let scroll = LocalInput::Scroll { dx: 0.0, dy: 1.0 };
    assert!(vp.send_input(scroll));
    assert_eq!(vp.local_delivered(), vec![scroll]);
}

#[test]
fn injections_are_recorded_and_held_input_is_released_on_drop() {
    let vp = VirtualPlatform::new();
    let mut inject = vp.inject().unwrap();
    inject.motion_abs(1.0, 2.0).unwrap();
    inject.scroll(0.0, -1.0).unwrap();
    inject.key(0x0007_0004, true).unwrap();
    inject.key(0x0007_0005, true).unwrap();
    inject.key(0x0007_0005, false).unwrap();
    inject.button(1, true).unwrap();
    inject.button(2, true).unwrap();
    inject.button(2, false).unwrap();
    assert_eq!(inject.secure_attention(), Err(Error::Unsupported));
    drop(inject);
    assert_eq!(
        vp.injected(),
        vec![
            Injected::MotionAbs { x: 1.0, y: 2.0 },
            Injected::Scroll { dx: 0.0, dy: -1.0 },
            Injected::Key {
                usage: 0x0007_0004,
                down: true
            },
            Injected::Key {
                usage: 0x0007_0005,
                down: true
            },
            Injected::Key {
                usage: 0x0007_0005,
                down: false
            },
            Injected::Button {
                button: 1,
                down: true
            },
            Injected::Button {
                button: 2,
                down: true
            },
            Injected::Button {
                button: 2,
                down: false
            },
            Injected::Button {
                button: 1,
                down: false
            },
            Injected::Key {
                usage: 0x0007_0004,
                down: false
            },
        ]
    );
    assert_eq!(vp.take_injected().len(), 10);
    assert!(vp.injected().is_empty());
}

#[test]
fn secure_attention_can_be_enabled_like_windows() {
    let vp = VirtualPlatform::new();
    assert_eq!(vp.info().limitations.len(), 1);
    vp.set_secure_attention_supported(true);
    assert!(vp.info().limitations.is_empty());
    vp.inject().unwrap().secure_attention().unwrap();
    assert_eq!(vp.injected(), vec![Injected::SecureAttention]);
    vp.set_secure_attention_supported(false);
    assert_eq!(vp.info().limitations.len(), 1);
}

#[test]
fn clipboard_is_shared_with_a_change_counter() {
    let vp = VirtualPlatform::new();
    let mut clip = vp.clipboard().unwrap();
    assert_eq!(clip.change_count(), 0);
    clip.write(vec![ClipRep::Text("hi".into())]).unwrap();
    assert_eq!(clip.change_count(), 1);
    vp.set_local_clipboard(vec![ClipRep::Html("<i>x</i>".into())]);
    assert_eq!(clip.change_count(), 2);
    assert_eq!(clip.read().unwrap(), vec![ClipRep::Html("<i>x</i>".into())]);
    assert_eq!(vp.clipboard_contents(), clip.read().unwrap());
}

#[test]
fn permissions_and_settings_requests() {
    let vp = VirtualPlatform::new();
    vp.set_permission(PermissionKind::Accessibility, PermissionState::Missing);
    vp.set_permission(PermissionKind::InputMonitoring, PermissionState::Granted);
    assert_eq!(
        vp.permissions(),
        vec![
            (PermissionKind::Accessibility, PermissionState::Missing),
            (PermissionKind::InputMonitoring, PermissionState::Granted),
        ]
    );
    vp.open_permission_settings(PermissionKind::Accessibility)
        .unwrap();
    assert_eq!(
        vp.opened_permission_settings(),
        vec![PermissionKind::Accessibility]
    );
}

#[test]
fn network_changes_fire_watchers_and_replace_interfaces() {
    let vp = VirtualPlatform::new();
    let (tx, rx) = channel();
    let handle = vp.watch_network(tx).unwrap();
    vp.set_interfaces(vec![]);
    assert!(vp.interfaces().is_empty());
    assert!(rx.try_recv().is_ok());
    vp.trigger_network_change();
    assert!(rx.try_recv().is_ok());
    drop(handle);
    vp.trigger_network_change();
    assert!(rx.try_recv().is_err());
}

#[test]
fn reported_problems_reach_watchers_until_dropped() {
    let vp = VirtualPlatform::new();
    let (tx, rx) = channel();
    let handle = vp.watch_problems(tx).unwrap();
    vp.report_problem("lost the keyboard");
    assert_eq!(
        rx.try_recv().unwrap(),
        PlatformProblem::Raised("lost the keyboard".into())
    );
    vp.clear_problem("lost the keyboard");
    assert_eq!(
        rx.try_recv().unwrap(),
        PlatformProblem::Cleared("lost the keyboard".into())
    );
    drop(handle);
    vp.report_problem("again");
    assert!(rx.try_recv().is_err());
}

#[test]
fn state_flags_secure_input_secrets_autostart() {
    let vp = VirtualPlatform::default();
    assert!(!vp.foreground_is_fullscreen());
    vp.set_fullscreen(true);
    assert!(vp.foreground_is_fullscreen());
    assert!(!vp.foreground_elevated());
    vp.set_foreground_elevated(true);
    assert!(vp.foreground_elevated());
    assert_eq!(vp.secure_input(), None);
    let holder = SecureInputHolder {
        pid: 7,
        app_name: "Terminal".into(),
    };
    vp.set_secure_input(Some(holder.clone()));
    assert_eq!(vp.secure_input(), Some(holder));

    vp.secrets().set("k", b"v").unwrap();
    assert_eq!(vp.secrets().get("k").unwrap(), Some(b"v".to_vec()));
    assert_eq!(vp.secrets().get("other").unwrap(), None);

    let mut auto = vp.autostart();
    assert!(!auto.enabled());
    auto.set(true).unwrap();
    assert!(vp.autostart().enabled());
    assert!(vp.is_autostart_enabled());
}

#[test]
fn file_drag_is_scripted() {
    let vp = VirtualPlatform::new();
    let mut drag = vp.file_drag().unwrap();
    assert_eq!(drag.outgoing(), None);
    vp.script_outgoing_drag(Some(vec![PathBuf::from("/a/b.txt")]));
    assert_eq!(drag.outgoing(), Some(vec![PathBuf::from("/a/b.txt")]));
    assert_eq!(drag.outgoing(), None);

    vp.script_drop_target(PathBuf::from("/drop/here"));
    let target = drag.begin_incoming(vec!["b.txt".into()]).unwrap();
    assert_eq!(target, DropTarget(PathBuf::from("/drop/here")));
    assert_eq!(vp.incoming_drags(), vec![vec!["b.txt".to_string()]]);

    // A cancelled drag is still recorded, and ends in an error.
    vp.script_drag_cancelled(true);
    assert!(drag.begin_incoming(vec!["c.txt".into()]).is_err());
    assert_eq!(vp.incoming_drags().len(), 2);
}

#[test]
fn secret_writes_can_be_made_to_fail() {
    let vp = VirtualPlatform::new();
    let mut store = vp.secrets();
    store.set("k", b"v").unwrap();
    vp.script_secret_writes_fail(true);
    assert!(store.set("k", b"w").is_err());
    assert!(store.delete("k").is_err());
    assert_eq!(store.get("k").unwrap(), Some(b"v".to_vec()));
    vp.script_secret_writes_fail(false);
    store.delete("k").unwrap();
    assert_eq!(store.get("k").unwrap(), None);
}

#[test]
fn info_can_be_replaced() {
    let vp = VirtualPlatform::new();
    vp.set_info(PlatformInfo {
        os: Os::MacOs,
        version: "26.5".into(),
        session: SessionType::Native,
        limitations: vec![],
    });
    assert_eq!(vp.info().os, Os::MacOs);
}

#[test]
fn limited_platform_reports_unsupported_everywhere() {
    let vp = VirtualPlatform::limited();
    assert!(matches!(vp.capture(), Err(Error::Unsupported)));
    assert!(matches!(vp.inject(), Err(Error::Unsupported)));
    assert!(matches!(vp.clipboard(), Err(Error::Unsupported)));
    assert!(vp.file_drag().is_none());
    assert_eq!(vp.info().limitations.len(), 4);
}

#[test]
fn clones_share_one_virtual_computer() {
    let vp = VirtualPlatform::new();
    let core_side: std::sync::Arc<dyn Platform> = std::sync::Arc::new(vp.clone());
    core_side.inject().unwrap().key(0x0007_0009, true).unwrap();
    assert_eq!(vp.injected().len(), 2);
}
