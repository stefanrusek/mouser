//! `InputInject` with `SendInput`: absolute moves through `SetCursorPos`
//! (which never reaches the low-level hooks), buttons 1-5, wheel scrolling
//! and scan-code keys. Every event carries [`MOUSER_TAG`] so Mouser's own
//! capture ignores it (conformance rule 7). Whatever is still held when the
//! injector goes away is released (rule 2).

use crate::displays::{self, DisplayCache};
use crate::{dpi, keymap};
use mouser_platform::{Error, InputInject, Result};
use std::collections::BTreeSet;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSE_EVENT_FLAGS,
    MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;

/// "MOUS": `dwExtraInfo` of every event Mouser injects.
pub(crate) const MOUSER_TAG: usize = 0x4D4F_5553;

/// Wheel units (120 per notch) per scroll unit: a scroll unit is a pixel
/// and a notch is fifteen of them, matching the X11 backend's `SCROLL_STEP`.
pub(crate) const WHEEL_UNITS_PER_PIXEL: f64 = 8.0;

/// A mouse event of `flags` with `data` (wheel delta or X button number).
pub(crate) fn mouse_input(flags: MOUSE_EVENT_FLAGS, data: i32, tag: usize) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: 0,
                dy: 0,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: tag,
            },
        },
    }
}

/// A scan-code key event.
pub(crate) fn key_input(scan: u16, extended: bool, down: bool, tag: usize) -> INPUT {
    let mut flags: KEYBD_EVENT_FLAGS = KEYEVENTF_SCANCODE;
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: Default::default(),
                wScan: scan,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: tag,
            },
        },
    }
}

/// A failed Win32 call as an [`Error::Os`] naming it.
pub(crate) fn os(call: &str, result: windows::core::Result<()>) -> Result<()> {
    result.map_err(|e| Error::Os(format!("{call}: {e}")))
}

/// `Ok` when `SendInput` took all `expected` events.
pub(crate) fn check_sent(sent: u32, expected: usize) -> Result<()> {
    if sent as usize == expected {
        Ok(())
    } else {
        Err(Error::Os(format!(
            "SendInput accepted {sent} of {expected} events"
        )))
    }
}

pub(crate) fn send(inputs: &[INPUT]) -> Result<()> {
    check_sent(
        unsafe { SendInput(inputs, size_of::<INPUT>() as i32) },
        inputs.len(),
    )
}

/// Down or up flags and X-button data for Mouser button `button` (1 left,
/// 2 right, 3 middle, 4 back, 5 forward).
pub(crate) fn button_event(button: u8, down: bool) -> Result<(MOUSE_EVENT_FLAGS, i32)> {
    Ok(match (button, down) {
        (1, true) => (MOUSEEVENTF_LEFTDOWN, 0),
        (1, false) => (MOUSEEVENTF_LEFTUP, 0),
        (2, true) => (MOUSEEVENTF_RIGHTDOWN, 0),
        (2, false) => (MOUSEEVENTF_RIGHTUP, 0),
        (3, true) => (MOUSEEVENTF_MIDDLEDOWN, 0),
        (3, false) => (MOUSEEVENTF_MIDDLEUP, 0),
        (4 | 5, true) => (MOUSEEVENTF_XDOWN, i32::from(button) - 3),
        (4 | 5, false) => (MOUSEEVENTF_XUP, i32::from(button) - 3),
        _ => return Err(Error::Os(format!("mouse button {button} does not exist"))),
    })
}

/// Called for Ctrl-Alt-Del; the service that can send it is not part of this
/// lane yet.
pub(crate) type SecureAttention = Box<dyn FnMut() -> Result<()> + Send>;

pub(crate) struct WinInject {
    displays: DisplayCache,
    pub(crate) buttons: BTreeSet<u8>,
    pub(crate) keys: BTreeSet<u32>,
    service: Option<SecureAttention>,
}

impl WinInject {
    pub(crate) fn new(service: Option<SecureAttention>) -> Self {
        WinInject {
            displays: DisplayCache::new(),
            buttons: BTreeSet::new(),
            keys: BTreeSet::new(),
            service,
        }
    }

    fn release_all(&mut self) {
        // Best effort: a release the OS refuses cannot be retried from drop.
        for button in std::mem::take(&mut self.buttons) {
            let _ = self.button(button, false);
        }
        for usage in std::mem::take(&mut self.keys) {
            let _ = self.key(usage, false);
        }
    }
}

impl Drop for WinInject {
    fn drop(&mut self) {
        self.release_all();
    }
}

impl InputInject for WinInject {
    fn motion_abs(&mut self, x: f64, y: f64) -> Result<()> {
        let (px, py) = displays::logical_to_pixel(&self.displays.snapshot(), x, y);
        os(
            "SetCursorPos",
            dpi::physical(|| unsafe { SetCursorPos(px, py) }),
        )
    }

    fn button(&mut self, button: u8, down: bool) -> Result<()> {
        let (flags, data) = button_event(button, down)?;
        send(&[mouse_input(flags, data, MOUSER_TAG)])?;
        if down {
            self.buttons.insert(button);
        } else {
            self.buttons.remove(&button);
        }
        Ok(())
    }

    fn scroll(&mut self, dx: f64, dy: f64) -> Result<()> {
        let mut inputs = Vec::new();
        for (flags, amount) in [(MOUSEEVENTF_HWHEEL, dx), (MOUSEEVENTF_WHEEL, dy)] {
            let delta = (amount * WHEEL_UNITS_PER_PIXEL).round() as i32;
            if delta != 0 {
                inputs.push(mouse_input(flags, delta, MOUSER_TAG));
            }
        }
        if inputs.is_empty() {
            return Ok(());
        }
        send(&inputs)
    }

    fn key(&mut self, usage: u32, down: bool) -> Result<()> {
        // A key Windows has no scan code for is dropped, as a keyboard
        // without it would.
        let Some((scan, extended)) = keymap::scan_for_usage(usage) else {
            return Ok(());
        };
        send(&[key_input(scan, extended, down, MOUSER_TAG)])?;
        if down {
            self.keys.insert(usage);
        } else {
            self.keys.remove(&usage);
        }
        Ok(())
    }

    fn secure_attention(&mut self) -> Result<()> {
        match &mut self.service {
            Some(forward) => forward(),
            None => Err(Error::Unsupported),
        }
    }
}
