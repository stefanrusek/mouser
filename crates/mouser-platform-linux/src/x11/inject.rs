//! `InputInject` through XTest.

use super::connect;
use super::echo::Echo;
use crate::keymap;
use mouser_platform::{Error, InputInject, Result};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Instant;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, ConnectionExt as _, KEY_PRESS_EVENT,
    KEY_RELEASE_EVENT, MOTION_NOTIFY_EVENT, Window,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::rust_connection::RustConnection;

/// Pixels of scroll that make one wheel click.
pub(crate) const SCROLL_STEP: f64 = 15.0;

/// X11 button for Mouser's button number: 1 left, 2 right, 3 middle, then
/// back, forward and the rest from X button 8 up.
pub(crate) fn x_button(button: u8) -> u8 {
    match button {
        1 => 1,
        2 => 3,
        3 => 2,
        other => other + 4,
    }
}

/// Whole wheel clicks in `accumulated` pixels, leaving the remainder.
pub(crate) fn clicks(accumulated: &mut f64) -> i32 {
    let whole = (*accumulated / SCROLL_STEP).trunc();
    *accumulated -= whole * SCROLL_STEP;
    whole as i32
}

pub(crate) struct X11Inject {
    env: HashMap<String, String>,
    echo: Arc<Echo>,
    link: Option<(RustConnection, Window)>,
    keys: BTreeSet<u8>,
    buttons: BTreeSet<u8>,
    scroll: (f64, f64),
}

impl X11Inject {
    pub(crate) fn new(env: &HashMap<String, String>, echo: Arc<Echo>) -> Self {
        X11Inject {
            env: env.clone(),
            echo,
            link: None,
            keys: BTreeSet::new(),
            buttons: BTreeSet::new(),
            scroll: (0.0, 0.0),
        }
    }

    /// The XTest connection, opened on first use. XTest requests ignore
    /// other clients' server grabs.
    fn link(&mut self) -> &(RustConnection, Window) {
        self.link.get_or_insert_with(|| {
            let link = connect(&self.env);
            link.0.xtest_grab_control(true).unwrap();
            link
        })
    }

    fn fake(&mut self, kind: u8, detail: u8, x: i16, y: i16) {
        let (conn, root) = self.link();
        conn.xtest_fake_input(kind, detail, 0, *root, x, y, 0)
            .unwrap();
        conn.flush().unwrap();
    }

    fn press_button(&mut self, button: u8, down: bool) {
        self.echo.record_button(button, down, Instant::now());
        let kind = if down {
            BUTTON_PRESS_EVENT
        } else {
            BUTTON_RELEASE_EVENT
        };
        self.fake(kind, button, 0, 0);
    }

    fn click(&mut self, button: u8, times: i32) {
        for _ in 0..times {
            self.press_button(button, true);
            self.press_button(button, false);
        }
    }
}

impl InputInject for X11Inject {
    fn motion_abs(&mut self, x: f64, y: f64) -> Result<()> {
        let (x, y) = (x.round() as i16, y.round() as i16);
        self.echo.record_motion(x, y, Instant::now());
        self.fake(MOTION_NOTIFY_EVENT, 0, x, y);
        Ok(())
    }

    fn button(&mut self, button: u8, down: bool) -> Result<()> {
        let button = x_button(button);
        if down {
            self.buttons.insert(button);
        } else {
            self.buttons.remove(&button);
        }
        self.press_button(button, down);
        Ok(())
    }

    fn scroll(&mut self, dx: f64, dy: f64) -> Result<()> {
        self.scroll.0 += dx;
        self.scroll.1 += dy;
        let (mut horizontal, mut vertical) = self.scroll;
        let (across, down) = (clicks(&mut horizontal), clicks(&mut vertical));
        self.scroll = (horizontal, vertical);
        // Wheel buttons: 4 up, 5 down, 6 left, 7 right.
        self.click(if down > 0 { 4 } else { 5 }, down.abs());
        self.click(if across > 0 { 7 } else { 6 }, across.abs());
        Ok(())
    }

    fn key(&mut self, usage: u32, down: bool) -> Result<()> {
        let keycode = keymap::x11_keycode_for_usage(usage)
            .ok_or_else(|| Error::Os(format!("no X11 key for usage {usage:#x}")))?;
        if down {
            self.keys.insert(keycode);
        } else {
            self.keys.remove(&keycode);
        }
        self.echo.record_key(keycode, down, Instant::now());
        self.fake(
            if down {
                KEY_PRESS_EVENT
            } else {
                KEY_RELEASE_EVENT
            },
            keycode,
            0,
            0,
        );
        Ok(())
    }

    fn secure_attention(&mut self) -> Result<()> {
        Err(Error::Unsupported)
    }
}

impl Drop for X11Inject {
    fn drop(&mut self) {
        for keycode in std::mem::take(&mut self.keys) {
            self.fake(KEY_RELEASE_EVENT, keycode, 0, 0);
        }
        for button in std::mem::take(&mut self.buttons) {
            self.press_button(button, false);
        }
        if let Some((conn, _)) = &self.link {
            // Wait for the server to apply the releases before the connection closes.
            conn.get_input_focus().unwrap().reply().unwrap();
        }
    }
}
