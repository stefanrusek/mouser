//! `InputInject`: `CGEventPost` from an `HIDSystemState` source at
//! `kCGHIDEventTap`, every event tagged with Mouser's `kCGEventSourceUserData`
//! so the capture tap ignores it (FINDINGS-macos-input section 3).

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
use std::collections::BTreeSet;
use std::sync::Arc;

/// Where finished events go and where the pointer is; a seam so event
/// construction is testable without posting anything.
pub(crate) trait Poster: Send {
    fn post(&self, event: &CGEvent);
    fn location(&self) -> (f64, f64);
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
}

fn source() -> CGEventSource {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap()
}

pub(crate) struct MacInject {
    poster: Box<dyn Poster>,
    held_keys: BTreeSet<u32>,
    held_buttons: BTreeSet<u8>,
    last: Option<(f64, f64)>,
    echo: Arc<Echo>,
}

impl MacInject {
    pub(crate) fn new(poster: Box<dyn Poster>, echo: Arc<Echo>) -> Self {
        MacInject {
            poster,
            held_keys: BTreeSet::new(),
            held_buttons: BTreeSet::new(),
            last: None,
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

    /// Device-independent modifier flags of every modifier key held.
    fn flags(&self) -> CGEventFlags {
        let bits = self
            .held_keys
            .iter()
            .filter_map(|usage| keymap::keycode_for_usage(*usage))
            .filter_map(keymap::modifier_flag)
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
        let (ty, button) = match self.held_buttons.iter().next() {
            Some(held) => (drag_event_type(*held), cg_button(*held)),
            None => (CGEventType::MouseMoved, CGMouseButton::Left),
        };
        let event = CGEvent::new_mouse_event(source(), ty, CGPoint::new(x, y), button).unwrap();
        let (from_x, from_y) = self.last.unwrap_or((x, y));
        event.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, (x - from_x) as i64);
        event.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y, (y - from_y) as i64);
        self.last = Some((x, y));
        self.echo.record(x, y);
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
        event.set_integer_value_field(EventField::MOUSE_EVENT_CLICK_STATE, 1);
        if down {
            self.held_buttons.insert(button);
        } else {
            self.held_buttons.remove(&button);
        }
        self.post(event);
        Ok(())
    }

    fn scroll(&mut self, dx: f64, dy: f64) -> Result<()> {
        let event =
            CGEvent::new_scroll_event(source(), ScrollEventUnit::PIXEL, 2, dy as i32, dx as i32, 0)
                .unwrap();
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
        event.set_flags(self.flags());
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
