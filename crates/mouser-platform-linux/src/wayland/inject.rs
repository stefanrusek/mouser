//! `InputInject` on Wayland: emulated pointer, button, scroll and key events
//! over the shared RemoteDesktop session's libei connection.

use super::desktop::{Desktop, Link};
use crate::keymap;
use mouser_platform::{Error, InputInject, Result};
use reis::ei;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

/// The Linux button code of Mouser's button number (1 left, 2 right, 3 middle,
/// 4 back, 5 forward): `BTN_LEFT` is 0x110.
pub(crate) fn button_code(button: u8) -> u32 {
    0x10F + u32::from(button)
}

pub(crate) struct WaylandInject {
    desktop: Arc<Desktop>,
    wait: Duration,
    link: Option<Arc<Link>>,
    keys: BTreeSet<u32>,
    buttons: BTreeSet<u8>,
}

impl WaylandInject {
    pub(crate) fn new(desktop: Arc<Desktop>, wait: Duration) -> Self {
        WaylandInject {
            desktop,
            wait,
            link: None,
            keys: BTreeSet::new(),
            buttons: BTreeSet::new(),
        }
    }

    /// The session, started on first use (a persisted grant makes that quick).
    fn link(&mut self) -> &Link {
        self.link.get_or_insert_with(|| self.desktop.link())
    }

    fn send_key(&mut self, code: u32, down: bool) -> Result<()> {
        let wait = self.wait;
        let state = if down {
            ei::keyboard::KeyState::Press
        } else {
            ei::keyboard::KeyState::Released
        };
        self.link()
            .emit(wait, &|d| d.interface::<ei::Keyboard>().is_some(), &|d| {
                d.interface::<ei::Keyboard>().unwrap().key(code, state)
            })
    }

    fn send_button(&mut self, button: u8, down: bool) -> Result<()> {
        let wait = self.wait;
        let state = if down {
            ei::button::ButtonState::Press
        } else {
            ei::button::ButtonState::Released
        };
        self.link()
            .emit(wait, &|d| d.interface::<ei::Button>().is_some(), &|d| {
                d.interface::<ei::Button>()
                    .unwrap()
                    .button(button_code(button), state)
            })
    }
}

impl InputInject for WaylandInject {
    fn motion_abs(&mut self, x: f64, y: f64) -> Result<()> {
        let wait = self.wait;
        self.link().emit(
            wait,
            &|d| d.interface::<ei::PointerAbsolute>().is_some(),
            &|d| {
                d.interface::<ei::PointerAbsolute>()
                    .unwrap()
                    .motion_absolute(x as f32, y as f32)
            },
        )
    }

    fn button(&mut self, button: u8, down: bool) -> Result<()> {
        self.send_button(button, down)?;
        if down {
            self.buttons.insert(button);
        } else {
            self.buttons.remove(&button);
        }
        Ok(())
    }

    /// Mouser scrolls in pixels, positive up and right; libei's y is positive down.
    fn scroll(&mut self, dx: f64, dy: f64) -> Result<()> {
        let wait = self.wait;
        self.link()
            .emit(wait, &|d| d.interface::<ei::Scroll>().is_some(), &|d| {
                d.interface::<ei::Scroll>()
                    .unwrap()
                    .scroll(dx as f32, -dy as f32)
            })
    }

    fn key(&mut self, usage: u32, down: bool) -> Result<()> {
        let code = keymap::evdev_for_usage(usage)
            .map(u32::from)
            .ok_or_else(|| Error::Os(format!("no Linux key for usage {usage:#x}")))?;
        self.send_key(code, down)?;
        if down {
            self.keys.insert(code);
        } else {
            self.keys.remove(&code);
        }
        Ok(())
    }

    fn secure_attention(&mut self) -> Result<()> {
        Err(Error::Unsupported)
    }
}

impl Drop for WaylandInject {
    fn drop(&mut self) {
        for code in std::mem::take(&mut self.keys) {
            self.send_key(code, false).unwrap();
        }
        for button in std::mem::take(&mut self.buttons) {
            self.send_button(button, false).unwrap();
        }
        // The session closes with its last holder.
        self.link.take();
    }
}
