//! `InputInject`: `CGEventPost` from an `HIDSystemState` source at
//! `kCGHIDEventTap`, every event tagged with Mouser's `kCGEventSourceUserData`
//! so the capture tap ignores it (FINDINGS-macos-input section 3).
//!
//! Every event carries the modifier flags of the keys this injector holds,
//! as a hardware keyboard reports them (device-independent and left/right
//! bits). Events created from the `HIDSystemState` source otherwise start
//! with the system's own modifier state, so a click or key after an injected
//! Command release could still arrive as a Command-click or shortcut.
//!
//! Every button event also carries the click count (`kCGMouseEventClickState`)
//! the window server would give a physical mouse: apps read double- and
//! triple-clicks from that field, not from timing, so the injector counts
//! presses of one button that follow each other within the user's
//! double-click interval and a few points of each other.

use crate::capture::MOUSER_TAG;
use crate::echo::Echo;
use crate::keymap;
use core_graphics::display::CGPoint;
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, CGMouseButton, EventField,
    ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use mouser_platform::{Error, InputInject, Result};
use objc2_app_kit::NSEvent;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Where finished events go, where the pointer is, what time it is and the
/// user's double-click interval; a seam so event construction is testable
/// without posting anything or waiting.
pub(crate) trait Poster: Send {
    fn post(&self, event: &CGEvent);
    fn location(&self) -> (f64, f64);
    fn now(&self) -> Instant;
    fn double_click_interval(&self) -> Duration;
}

pub(crate) struct HidPoster;

impl Poster for HidPoster {
    fn post(&self, event: &CGEvent) {
        event.post(CGEventTapLocation::HID);
    }
    fn location(&self) -> (f64, f64) {
        let at = CGEvent::new(source()).unwrap().location();
        (at.x, at.y)
    }
    fn now(&self) -> Instant {
        Instant::now()
    }
    /// The interval set in System Settings, read each time so a change
    /// applies at once.
    fn double_click_interval(&self) -> Duration {
        Duration::try_from_secs_f64(NSEvent::doubleClickInterval()).unwrap_or(DEFAULT_INTERVAL)
    }
}

/// The system's default double-click interval, should `AppKit` report an
/// unusable one.
const DEFAULT_INTERVAL: Duration = Duration::from_millis(500);

/// How far, in points, a press may land from the previous one and still
/// count as the next click of a multi-click.
const CLICK_SLOP: f64 = 4.0;

/// The previous press, against which the next one is counted.
struct LastPress {
    button: u8,
    at: (f64, f64),
    when: Instant,
    count: i64,
}

fn source() -> CGEventSource {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap()
}

pub(crate) struct MacInject {
    poster: Box<dyn Poster>,
    held_keys: BTreeSet<u32>,
    held_buttons: BTreeSet<u8>,
    last: Option<(f64, f64)>,
    last_press: Option<LastPress>,
    /// The click count of each held button's press, carried by its drags
    /// and its release.
    click_counts: BTreeMap<u8, i64>,
    echo: Arc<Echo>,
}

impl MacInject {
    pub(crate) fn new(poster: Box<dyn Poster>, echo: Arc<Echo>) -> Self {
        MacInject {
            poster,
            held_keys: BTreeSet::new(),
            held_buttons: BTreeSet::new(),
            last: None,
            last_press: None,
            click_counts: BTreeMap::new(),
            echo,
        }
    }

    pub(crate) fn system(echo: Arc<Echo>) -> Self {
        Self::new(Box::new(HidPoster), echo)
    }

    fn post(&self, event: CGEvent) {
        event.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, MOUSER_TAG);
        self.poster.post(&event);
    }

    /// The click count of a press of `button` at `at`: one more than the
    /// previous press when it was the same button, within the double-click
    /// interval and within a few points; otherwise a new single click.
    fn count_press(&mut self, button: u8, at: (f64, f64)) -> i64 {
        let when = self.poster.now();
        let count = match &self.last_press {
            Some(last)
                if last.button == button
                    && when.duration_since(last.when) <= self.poster.double_click_interval()
                    && (at.0 - last.at.0).hypot(at.1 - last.at.1) <= CLICK_SLOP =>
            {
                last.count + 1
            }
            _ => 1,
        };
        self.last_press = Some(LastPress {
            button,
            at,
            when,
            count,
        });
        count
    }

    /// The modifier flags of every modifier key held: device-independent
    /// and the hardware's left/right bit.
    fn flags(&self) -> CGEventFlags {
        let bits = self
            .held_keys
            .iter()
            .filter_map(|usage| keymap::keycode_for_usage(*usage))
            .filter_map(keymap::modifier_mask)
            .fold(0, |all, flag| all | flag);
        CGEventFlags::from_bits_retain(bits)
    }
}

/// CoreGraphics button for a Mouser button number (1 left, 2 right, 3+ other).
fn cg_button(button: u8) -> CGMouseButton {
    match button {
        1 => CGMouseButton::Left,
        2 => CGMouseButton::Right,
        _ => CGMouseButton::Center,
    }
}

fn button_event_type(button: u8, down: bool) -> CGEventType {
    match (button, down) {
        (1, true) => CGEventType::LeftMouseDown,
        (1, false) => CGEventType::LeftMouseUp,
        (2, true) => CGEventType::RightMouseDown,
        (2, false) => CGEventType::RightMouseUp,
        (_, true) => CGEventType::OtherMouseDown,
        (_, false) => CGEventType::OtherMouseUp,
    }
}

fn drag_event_type(button: u8) -> CGEventType {
    match button {
        1 => CGEventType::LeftMouseDragged,
        2 => CGEventType::RightMouseDragged,
        _ => CGEventType::OtherMouseDragged,
    }
}

impl InputInject for MacInject {
    fn motion_abs(&mut self, x: f64, y: f64) -> Result<()> {
        // A held button makes the move a drag, as a physical mouse would.
        let (ty, button, clicks) = match self.held_buttons.iter().next() {
            Some(held) => (
                drag_event_type(*held),
                cg_button(*held),
                self.click_counts[held],
            ),
            None => (CGEventType::MouseMoved, CGMouseButton::Left, 0),
        };
        let event = CGEvent::new_mouse_event(source(), ty, CGPoint::new(x, y), button).unwrap();
        event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, clicks);
        let (from_x, from_y) = self.last.unwrap_or((x, y));
        event.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, (x - from_x) as i64);
        event.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y, (y - from_y) as i64);
        self.last = Some((x, y));
        self.echo.record(x, y);
        event.set_flags(self.flags());
        self.post(event);
        Ok(())
    }

    fn button(&mut self, button: u8, down: bool) -> Result<()> {
        let (x, y) = self.poster.location();
        let event = CGEvent::new_mouse_event(
            source(),
            button_event_type(button, down),
            CGPoint::new(x, y),
            cg_button(button),
        )
        .unwrap();
        event.set_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER, i64::from(button) - 1);
        // A release carries its press's count; one without a press is a
        // single click.
        let clicks = if down {
            self.held_buttons.insert(button);
            let count = self.count_press(button, (x, y));
            self.click_counts.insert(button, count);
            count
        } else {
            self.held_buttons.remove(&button);
            self.click_counts.remove(&button).unwrap_or(1)
        };
        event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, clicks);
        event.set_flags(self.flags());
        self.post(event);
        Ok(())
    }

    fn scroll(&mut self, dx: f64, dy: f64) -> Result<()> {
        let event =
            CGEvent::new_scroll_event(source(), ScrollEventUnit::PIXEL, 2, dy as i32, dx as i32, 0)
                .unwrap();
        event.set_flags(self.flags());
        self.post(event);
        Ok(())
    }

    fn key(&mut self, usage: u32, down: bool) -> Result<()> {
        // A key this Mac does not have (F21-F24, Print Screen...) is dropped,
        // as it would be by a keyboard that lacks it.
        let Some(keycode) = keymap::keycode_for_usage(usage) else {
            return Ok(());
        };
        let event = CGEvent::new_keyboard_event(source(), keycode, down).unwrap();
        if down {
            self.held_keys.insert(usage);
        } else {
            self.held_keys.remove(&usage);
        }
        if keymap::modifier_flag(keycode).is_some() {
            // Modifiers are reported by the hardware as flag changes.
            event.set_type(CGEventType::FlagsChanged);
        }
        event.set_flags(self.flags() | CGEventFlags::CGEventFlagNonCoalesced);
        self.post(event);
        Ok(())
    }

    fn secure_attention(&mut self) -> Result<()> {
        Err(Error::Unsupported)
    }
}

impl Drop for MacInject {
    fn drop(&mut self) {
        let keys: Vec<u32> = self.held_keys.iter().copied().collect();
        for usage in keys {
            self.key(usage, false).unwrap();
        }
        let buttons: Vec<u8> = self.held_buttons.iter().copied().collect();
        for button in buttons {
            self.button(button, false).unwrap();
        }
    }
}
