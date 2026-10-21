use crate::displays::{RawDisplay, assemble, describe, displays, reconfigured, watch};
use mouser_platform::LocalDisplay;
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::mpsc::{Sender, channel};

fn retina() -> RawDisplay {
    RawDisplay {
        vendor: 0x1e6d,
        model: 0x5b08,
        serial: 0x1234,
        builtin: false,
        origin: (0.0, 0.0),
        size_points: (2560.0, 1440.0),
        mode_points: (2560, 1440),
        mode_pixels: (5120, 2880),
        screen_mm: (599.3, 340.2),
    }
}

#[test]
fn scale_is_mode_pixels_over_mode_points_not_pixels_wide() {
    let d = describe(&retina(), 0);
    assert_eq!(d.scale, 2.0);
    assert_eq!(d.pixel_size, (5120, 2880));
    assert_eq!(d.logical_size, (2560.0, 1440.0));
    assert_eq!(d.origin, (0.0, 0.0));
}

#[test]
fn scale_never_drops_below_one() {
    let mut raw = retina();
    raw.mode_pixels = (1280, 720);
    assert_eq!(describe(&raw, 0).scale, 1.0);
}

#[test]
fn physical_size_is_omitted_when_the_display_reports_zero() {
    assert_eq!(
        describe(&retina(), 0).physical_size_mm,
        Some((599.3, 340.2))
    );
    let mut raw = retina();
    raw.screen_mm = (0.0, 340.2);
    assert_eq!(describe(&raw, 0).physical_size_mm, None);
    raw.screen_mm = (599.3, 0.0);
    assert_eq!(describe(&raw, 0).physical_size_mm, None);
}

#[test]
fn key_is_stable_from_vendor_model_serial() {
    assert_eq!(describe(&retina(), 0).key, "1e6d-5b08-00001234");
    assert_eq!(describe(&retina(), 3).key, "1e6d-5b08-00001234");
}

#[test]
fn key_falls_back_to_the_index_without_identity() {
    let mut raw = retina();
    raw.vendor = 0;
    raw.model = 0;
    raw.serial = 0;
    assert_eq!(describe(&raw, 2).key, "2");
}

#[test]
fn names_distinguish_builtin_from_external() {
    let mut raw = retina();
    assert_eq!(describe(&raw, 1).name, "Display 2");
    raw.builtin = true;
    assert_eq!(describe(&raw, 1).name, "Built-in Display");
}

#[test]
fn identical_monitors_get_distinct_keys_and_negative_origins_survive() {
    let mut second = retina();
    second.origin = (-880.0, -1440.0);
    let all = assemble(&[retina(), second]);
    assert_eq!(all[0].key, "1e6d-5b08-00001234");
    assert_eq!(all[1].key, "1e6d-5b08-00001234-1");
    assert_eq!(all[1].origin, (-880.0, -1440.0));
}

#[test]
fn real_displays_are_consistent() {
    let all = displays().unwrap();
    assert!(!all.is_empty());
    for d in &all {
        assert!(d.scale >= 1.0);
        assert!(d.pixel_size.0 > 0 && d.logical_size.0 > 0.0);
    }
}

fn user_info(tx: &Mutex<Sender<Vec<LocalDisplay>>>) -> *const c_void {
    tx as *const _ as *const c_void
}

#[test]
fn reconfiguration_callback_delivers_a_snapshot_after_the_begin_notification() {
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    // kCGDisplayBeginConfigurationFlag precedes every change: ignored.
    unsafe { reconfigured(1, 1, user_info(&tx)) };
    assert!(rx.try_recv().is_err());
    // kCGDisplayMovedFlag: the change completed.
    unsafe { reconfigured(1, 2, user_info(&tx)) };
    assert_eq!(rx.try_recv().unwrap(), displays().unwrap());
}

#[test]
fn watch_registers_and_dropping_the_handle_unregisters() {
    let (tx, rx) = channel();
    let handle = watch(tx).unwrap();
    drop(handle);
    // The sender was dropped with the handle.
    assert!(rx.recv().is_err());
}
