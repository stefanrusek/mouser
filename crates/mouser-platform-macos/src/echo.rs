//! Untagged echoes of Mouser's own absolute pointer positions.
//!
//! Measured: after `motion_abs(x, y)` (and after a capture warp) the HID tap
//! also sees untagged mouse-moved events located exactly at `(x, y)`, with
//! zero or tiny deltas, and settling events within a pixel or two of it, which
//! the user-data tag cannot catch (the OS re-creates them). Left alone, the receiving computer
//! would report its own injected motion as local input and feed it back.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long after an injection or warp an event at that position is an echo.
const WINDOW: Duration = Duration::from_millis(500);

/// How far (points, per axis) from that position an event still counts.
const RADIUS: f64 = 3.0;

#[derive(Default)]
pub(crate) struct Echo {
    last: Mutex<Option<((f64, f64), Instant)>>,
}

impl Echo {
    /// Remember that Mouser just put the pointer at `(x, y)`.
    pub(crate) fn record(&self, x: f64, y: f64) {
        *self.last.lock().unwrap() = Some(((x, y), Instant::now()));
    }

    /// Whether a motion event located at `(x, y)` at `now` is such an echo.
    pub(crate) fn is_echo(&self, x: f64, y: f64, now: Instant) -> bool {
        matches!(
            *self.last.lock().unwrap(),
            Some((at, when)) if (at.0 - x).abs() <= RADIUS && (at.1 - y).abs() <= RADIUS && now.duration_since(when) < WINDOW
        )
    }
}
