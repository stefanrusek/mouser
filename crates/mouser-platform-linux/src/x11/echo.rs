//! Mouser's own XTest input comes back through XInput2 as raw events. The
//! injector records what it sent so the capture can drop exactly those
//! events (conformance rule 7).

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long after an injection a matching raw event is an echo.
const WINDOW: Duration = Duration::from_millis(500);

#[derive(Default)]
struct Pending {
    /// Where the last absolute motion put the pointer.
    motion: Option<((i16, i16), Instant)>,
    /// `(X button, down)` presses and releases not yet seen coming back.
    buttons: Vec<(u8, bool, Instant)>,
    /// `(X keycode, down)` likewise.
    keys: Vec<(u8, bool, Instant)>,
}

#[derive(Default)]
pub(crate) struct Echo {
    pending: Mutex<Pending>,
}

fn take(list: &mut Vec<(u8, bool, Instant)>, code: u8, down: bool, now: Instant) -> bool {
    list.retain(|(_, _, at)| now.duration_since(*at) < WINDOW);
    match list.iter().position(|(c, d, _)| *c == code && *d == down) {
        Some(index) => {
            list.remove(index);
            true
        }
        None => false,
    }
}

impl Echo {
    pub(crate) fn record_motion(&self, x: i16, y: i16, now: Instant) {
        self.pending.lock().unwrap().motion = Some(((x, y), now));
    }

    pub(crate) fn record_button(&self, button: u8, down: bool, now: Instant) {
        self.pending
            .lock()
            .unwrap()
            .buttons
            .push((button, down, now));
    }

    pub(crate) fn record_key(&self, keycode: u8, down: bool, now: Instant) {
        self.pending.lock().unwrap().keys.push((keycode, down, now));
    }

    /// Whether a motion that left the pointer at `at` is Mouser's own.
    pub(crate) fn is_motion(&self, at: (i16, i16), now: Instant) -> bool {
        matches!(
            self.pending.lock().unwrap().motion,
            Some((target, when)) if target == at && now.duration_since(when) < WINDOW
        )
    }

    /// Consumes the matching recorded button event, if any.
    pub(crate) fn take_button(&self, button: u8, down: bool, now: Instant) -> bool {
        take(&mut self.pending.lock().unwrap().buttons, button, down, now)
    }

    /// Consumes the matching recorded key event, if any.
    pub(crate) fn take_key(&self, keycode: u8, down: bool, now: Instant) -> bool {
        take(&mut self.pending.lock().unwrap().keys, keycode, down, now)
    }
}
