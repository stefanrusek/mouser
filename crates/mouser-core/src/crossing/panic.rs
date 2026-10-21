//! The panic key (Story 2 scenario 8): a key pressed a configured number of
//! times in quick succession while the pointer is on another computer.

use std::time::{Duration, Instant};

/// Presses further apart than this do not add up.
pub const WINDOW: Duration = Duration::from_millis(500);

/// The HID usage of a key name accepted in `hotkeys.panic`: letters, digits,
/// F1-F12 and Escape, Space, Enter, Tab, Backspace.
pub fn usage_for_name(name: &str) -> Option<u32> {
    const PAGE: u32 = 0x0007_0000;
    let fixed = match name {
        "Escape" => Some(0x29),
        "Space" => Some(0x2C),
        "Enter" => Some(0x28),
        "Tab" => Some(0x2B),
        "Backspace" => Some(0x2A),
        _ => None,
    };
    let single = name.chars().next().filter(|_| name.len() == 1);
    let id = match (fixed, single) {
        (Some(id), _) => id,
        (None, Some(c @ 'A'..='Z')) => 0x04 + (c as u32 - 'A' as u32),
        (None, Some(c @ '1'..='9')) => 0x1E + (c as u32 - '1' as u32),
        (None, Some('0')) => 0x27,
        _ => {
            let n: u32 = name.strip_prefix('F')?.parse().ok()?;
            if !(1..=12).contains(&n) {
                return None;
            }
            0x3A + (n - 1)
        }
    };
    Some(PAGE | id)
}

/// `"Escape*2"`: which key, pressed how many times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanicSpec {
    pub usage: u32,
    pub presses: u32,
}

impl PanicSpec {
    pub fn parse(spec: &str) -> Result<Self, String> {
        let (name, count) = spec
            .split_once('*')
            .ok_or_else(|| format!("{spec:?} is not <key>*<count>"))?;
        let usage = usage_for_name(name).ok_or_else(|| format!("{name:?} is not a known key"))?;
        let presses = match count.parse::<u32>() {
            Ok(n) if n >= 1 => n,
            _ => return Err(format!("{count:?} is not a number of presses, 1 or more")),
        };
        Ok(Self { usage, presses })
    }
}

/// Counts presses of the panic key.
#[derive(Debug)]
pub struct PanicDetector {
    spec: PanicSpec,
    count: u32,
    last: Option<Instant>,
}

impl PanicDetector {
    pub fn new(spec: PanicSpec) -> Self {
        Self {
            spec,
            count: 0,
            last: None,
        }
    }

    /// A key went down at `now`; true when this press completes the panic
    /// gesture. Any other key, or a pause, starts the count over.
    pub fn pressed(&mut self, usage: u32, now: Instant) -> bool {
        if usage != self.spec.usage {
            self.reset();
            return false;
        }
        let close = self.last.is_some_and(|t| now.duration_since(t) <= WINDOW);
        self.count = if close { self.count + 1 } else { 1 };
        self.last = Some(now);
        if self.count >= self.spec.presses {
            self.reset();
            return true;
        }
        false
    }

    pub fn reset(&mut self) {
        self.count = 0;
        self.last = None;
    }
}
