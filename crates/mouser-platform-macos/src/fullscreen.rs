//! Whether the foreground app is full screen on the cursor's display, so
//! edge crossing can be blocked (`crossing.block_fullscreen`).
//!
//! Full screen means: the frontmost app's first layer-0 window on the cursor's
//! display has the display's bounds (within 1 pt), or any window at or above
//! the shielding level covers the display.

use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;
use core_graphics::display::{CGDisplay, CGPoint, CGShieldingWindowLevel};
use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer,
    kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly, kCGWindowOwnerPID,
};
use objc2_app_kit::NSWorkspace;

/// Tolerance, in points, when comparing a window with its display.
const TOLERANCE: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Window {
    pub pid: u32,
    pub layer: i32,
    pub bounds: Rect,
}

fn matches_display(bounds: Rect, display: Rect) -> bool {
    (bounds.x - display.x).abs() <= TOLERANCE
        && (bounds.y - display.y).abs() <= TOLERANCE
        && (bounds.w - display.w).abs() <= TOLERANCE
        && (bounds.h - display.h).abs() <= TOLERANCE
}

fn center_on(bounds: Rect, display: Rect) -> bool {
    let (cx, cy) = (bounds.x + bounds.w / 2.0, bounds.y + bounds.h / 2.0);
    cx >= display.x && cx < display.x + display.w && cy >= display.y && cy < display.y + display.h
}

/// The decision. `windows` is on-screen windows, front to back.
pub(crate) fn is_fullscreen(
    frontmost_pid: u32,
    display: Rect,
    windows: &[Window],
    shielding_level: i32,
) -> bool {
    let shielded = windows
        .iter()
        .any(|w| w.layer >= shielding_level && matches_display(w.bounds, display));
    let frontmost = windows
        .iter()
        .find(|w| w.pid == frontmost_pid && w.layer == 0 && center_on(w.bounds, display))
        .is_some_and(|w| matches_display(w.bounds, display));
    shielded || frontmost
}

/// What the OS says; a seam so the decision is testable.
pub(crate) trait Probe {
    fn frontmost_pid(&self) -> u32;
    /// Bounds of the display holding the pointer, in global points.
    fn cursor_display(&self) -> Rect;
    fn windows(&self) -> Vec<Window>;
    fn shielding_level(&self) -> i32;
}

pub(crate) struct SystemProbe;

fn number(dict: &CFDictionary<CFString, CFType>, key: &str) -> f64 {
    dict.find(CFString::new(key))
        .unwrap()
        .downcast::<CFNumber>()
        .unwrap()
        .to_f64()
        .unwrap()
}

impl Probe for SystemProbe {
    fn frontmost_pid(&self) -> u32 {
        let app = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .unwrap();
        app.processIdentifier() as u32
    }

    fn cursor_display(&self) -> Rect {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).unwrap();
        let at: CGPoint = CGEvent::new(source).unwrap().location();
        let (ids, _) = CGDisplay::displays_with_point(at, 1).unwrap();
        let b = CGDisplay::new(ids[0]).bounds();
        Rect {
            x: b.origin.x,
            y: b.origin.y,
            w: b.size.width,
            h: b.size.height,
        }
    }

    fn windows(&self) -> Vec<Window> {
        let list = copy_window_info(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        )
        .unwrap();
        let (layer_key, pid_key, bounds_key) = unsafe {
            (
                CFString::wrap_under_get_rule(kCGWindowLayer),
                CFString::wrap_under_get_rule(kCGWindowOwnerPID),
                CFString::wrap_under_get_rule(kCGWindowBounds),
            )
        };
        list.iter()
            .map(|item| {
                let dict: CFDictionary<CFString, CFType> =
                    unsafe { CFDictionary::wrap_under_get_rule(*item as _) };
                let get = |key: &CFString| dict.find(key.clone()).unwrap().clone();
                let bounds: CFDictionary<CFString, CFType> = unsafe {
                    CFDictionary::wrap_under_get_rule(get(&bounds_key).as_CFTypeRef() as _)
                };
                Window {
                    pid: get(&pid_key)
                        .downcast::<CFNumber>()
                        .unwrap()
                        .to_i32()
                        .unwrap() as u32,
                    layer: get(&layer_key)
                        .downcast::<CFNumber>()
                        .unwrap()
                        .to_i32()
                        .unwrap(),
                    bounds: Rect {
                        x: number(&bounds, "X"),
                        y: number(&bounds, "Y"),
                        w: number(&bounds, "Width"),
                        h: number(&bounds, "Height"),
                    },
                }
            })
            .collect()
    }

    fn shielding_level(&self) -> i32 {
        unsafe { CGShieldingWindowLevel() }
    }
}

/// Whether the foreground app is full screen on the cursor's display.
pub(crate) fn foreground_is_fullscreen(probe: &dyn Probe) -> bool {
    is_fullscreen(
        probe.frontmost_pid(),
        probe.cursor_display(),
        &probe.windows(),
        probe.shielding_level(),
    )
}
