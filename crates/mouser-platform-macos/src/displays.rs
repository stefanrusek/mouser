//! Display enumeration and change watching through the CoreGraphics display
//! APIs. Coordinates are global points: origin at the top-left of the main
//! display, y down, possibly negative (FINDINGS-macos-input section 5).

use core_graphics::display::{
    CGDisplay, CGDisplayChangeSummaryFlags, CGDisplayRegisterReconfigurationCallback,
    CGDisplayRemoveReconfigurationCallback,
};
use mouser_platform::{LocalDisplay, Result, WatchHandle};
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::mpsc::Sender;

/// What the OS reports for one display, before Mouser's interpretation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawDisplay {
    pub vendor: u32,
    pub model: u32,
    pub serial: u32,
    pub builtin: bool,
    /// `CGDisplayBounds`, in points.
    pub origin: (f64, f64),
    pub size_points: (f64, f64),
    /// Current mode `CGDisplayModeGetWidth/Height` (points).
    pub mode_points: (u64, u64),
    /// Current mode `CGDisplayModeGetPixelWidth/Height`.
    pub mode_pixels: (u64, u64),
    /// `CGDisplayScreenSize`; 0 when the display does not say.
    pub screen_mm: (f64, f64),
}

/// A stable key from vendor/model/serial (a `CGDirectDisplayID` is not
/// stable across reboots); the index when the display reports none of them.
fn stable_key(raw: &RawDisplay, index: usize) -> String {
    if raw.vendor == 0 && raw.model == 0 && raw.serial == 0 {
        index.to_string()
    } else {
        format!("{:04x}-{:04x}-{:08x}", raw.vendor, raw.model, raw.serial)
    }
}

pub(crate) fn describe(raw: &RawDisplay, index: usize) -> LocalDisplay {
    // CGDisplayPixelsWide returns points; the backing scale is the mode's
    // pixel width over its point width.
    let scale = (raw.mode_pixels.0 as f64 / raw.mode_points.0 as f64).max(1.0) as f32;
    let physical_size_mm = if raw.screen_mm.0 > 0.0 && raw.screen_mm.1 > 0.0 {
        Some(raw.screen_mm)
    } else {
        None
    };
    LocalDisplay {
        key: stable_key(raw, index),
        name: if raw.builtin {
            "Built-in Display".to_string()
        } else {
            format!("Display {}", index + 1)
        },
        pixel_size: (raw.mode_pixels.0 as u32, raw.mode_pixels.1 as u32),
        logical_size: raw.size_points,
        scale,
        physical_size_mm,
        origin: raw.origin,
    }
}

/// Describes every display; a key already used by an earlier display (two
/// identical monitors reporting serial 0) gets the index appended.
pub(crate) fn assemble(raws: &[RawDisplay]) -> Vec<LocalDisplay> {
    let mut out: Vec<LocalDisplay> = Vec::new();
    for (index, raw) in raws.iter().enumerate() {
        let mut display = describe(raw, index);
        if out.iter().any(|d| d.key == display.key) {
            display.key = format!("{}-{index}", display.key);
        }
        out.push(display);
    }
    out
}

fn read_raw(id: u32) -> RawDisplay {
    let display = CGDisplay::new(id);
    let bounds = display.bounds();
    let mode = display.display_mode().expect("display has a current mode");
    let mm = display.screen_size();
    RawDisplay {
        vendor: display.vendor_number(),
        model: display.model_number(),
        serial: display.serial_number(),
        builtin: display.is_builtin(),
        origin: (bounds.origin.x, bounds.origin.y),
        size_points: (bounds.size.width, bounds.size.height),
        mode_points: (mode.width(), mode.height()),
        mode_pixels: (mode.pixel_width(), mode.pixel_height()),
        screen_mm: (mm.width, mm.height),
    }
}

pub(crate) fn displays() -> Result<Vec<LocalDisplay>> {
    let ids = CGDisplay::active_displays().expect("CGGetActiveDisplayList");
    let raws: Vec<RawDisplay> = ids.into_iter().map(read_raw).collect();
    Ok(assemble(&raws))
}

/// Delivers a fresh snapshot once a reconfiguration completes; the "begin"
/// notification that precedes every change is ignored.
pub(crate) unsafe extern "C" fn reconfigured(_display: u32, flags: u32, user_info: *const c_void) {
    if flags & CGDisplayChangeSummaryFlags::kCGDisplayBeginConfigurationFlag.bits() != 0 {
        return;
    }
    let tx = unsafe { &*(user_info as *const Mutex<Sender<Vec<LocalDisplay>>>) };
    let snapshot = displays().expect("display list after reconfiguration");
    // The receiver going away just means nobody is listening any more.
    let _ = tx.lock().unwrap().send(snapshot);
}

/// Registers for display reconfiguration. CoreGraphics delivers the callback
/// on the run loop of the registering thread (the app's main loop).
pub(crate) fn watch(tx: Sender<Vec<LocalDisplay>>) -> Result<WatchHandle> {
    let user_info = Box::into_raw(Box::new(Mutex::new(tx))) as usize;
    let err =
        unsafe { CGDisplayRegisterReconfigurationCallback(reconfigured, user_info as *const _) };
    assert_eq!(err, 0, "CGDisplayRegisterReconfigurationCallback");
    Ok(WatchHandle::new(move || {
        unsafe { CGDisplayRemoveReconfigurationCallback(reconfigured, user_info as *const _) };
        drop(unsafe { Box::from_raw(user_info as *mut Mutex<Sender<Vec<LocalDisplay>>>) });
    }))
}
