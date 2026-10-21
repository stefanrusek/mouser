//! `InputCapture`: one HID-level filtering event tap delivering `LocalInput`.
//!
//! Grabbed ("remote mode", FINDINGS-macos-input section 2): the tap drops
//! keys, buttons, scroll, gestures and `NX_SYSDEFINED`; the cursor is
//! disassociated from the mouse, parked and hidden; motion is still reported
//! from `kCGMouseEventDeltaX/Y`. Events carrying Mouser's own
//! `kCGEventSourceUserData` tag were injected by this process and are ignored
//! (conformance rule 7).

use crate::echo::Echo;
use crate::keymap;
use crate::tap::{self, Create, Tap, Verdict};
use core_graphics::display::{CGDisplay, CGPoint};
use core_graphics::event::{CGEvent, EventField};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use mouser_platform::{InputCapture, LocalInput, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// `kCGEventSourceUserData` value on every event Mouser injects ("MOUSER").
pub(crate) const MOUSER_TAG: i64 = 0x4D4F_5553_4552;

// CGEventType raw values (the core-graphics enum lacks several of them).
const LEFT_MOUSE_DOWN: u32 = 1;
const LEFT_MOUSE_UP: u32 = 2;
const RIGHT_MOUSE_DOWN: u32 = 3;
const RIGHT_MOUSE_UP: u32 = 4;
const MOUSE_MOVED: u32 = 5;
const LEFT_MOUSE_DRAGGED: u32 = 6;
const RIGHT_MOUSE_DRAGGED: u32 = 7;
const KEY_DOWN: u32 = 10;
const KEY_UP: u32 = 11;
const FLAGS_CHANGED: u32 = 12;
const SCROLL_WHEEL: u32 = 22;
const OTHER_MOUSE_DOWN: u32 = 25;
const OTHER_MOUSE_UP: u32 = 26;
const OTHER_MOUSE_DRAGGED: u32 = 27;

/// Every event class the tap sees: pointer, buttons, keys, `NX_SYSDEFINED`
/// (14), scroll, and the gesture family (18-20, 29-34, 37).
const TAPPED_TYPES: &[u32] = &[
    LEFT_MOUSE_DOWN,
    LEFT_MOUSE_UP,
    RIGHT_MOUSE_DOWN,
    RIGHT_MOUSE_UP,
    MOUSE_MOVED,
    LEFT_MOUSE_DRAGGED,
    RIGHT_MOUSE_DRAGGED,
    KEY_DOWN,
    KEY_UP,
    FLAGS_CHANGED,
    14,
    18,
    19,
    20,
    SCROLL_WHEEL,
    OTHER_MOUSE_DOWN,
    OTHER_MOUSE_UP,
    OTHER_MOUSE_DRAGGED,
    29,
    30,
    31,
    32,
    33,
    34,
    37,
];

pub(crate) fn event_mask() -> u64 {
    TAPPED_TYPES.iter().fold(0, |mask, ty| mask | (1 << ty))
}

/// The cursor operations a grab needs; a seam so the grab sequence is
/// testable without moving the real pointer.
pub(crate) trait Cursor: Send + Sync {
    fn position(&self) -> (f64, f64);
    fn warp(&self, x: f64, y: f64);
    fn associate(&self, on: bool);
    fn hide(&self);
    fn show(&self);
}

pub(crate) struct SystemCursor;

impl Cursor for SystemCursor {
    fn position(&self) -> (f64, f64) {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).unwrap();
        let at = CGEvent::new(source).unwrap().location();
        (at.x, at.y)
    }
    fn warp(&self, x: f64, y: f64) {
        CGDisplay::warp_mouse_cursor_position(CGPoint::new(x, y))
            .expect("CGWarpMouseCursorPosition");
    }
    fn associate(&self, on: bool) {
        CGDisplay::associate_mouse_and_mouse_cursor_position(on)
            .expect("CGAssociateMouseAndMouseCursorPosition");
    }
    fn hide(&self) {
        allow_hiding_in_background();
        CGDisplay::main()
            .hide_cursor()
            .expect("CGDisplayHideCursor");
    }
    fn show(&self) {
        CGDisplay::main()
            .show_cursor()
            .expect("CGDisplayShowCursor");
    }
}

// Private CoreGraphics (SkyLight) calls, as Synergy/Barrier use: without
// `SetsCursorInBackground`, CGDisplayHideCursor does nothing unless the
// caller is the frontmost app, and Mouser (a menu-bar app) never is.
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn _CGSDefaultConnection() -> i32;
    fn CGSSetConnectionProperty(
        connection: i32,
        target: i32,
        key: core_foundation::string::CFStringRef,
        value: core_foundation::base::CFTypeRef,
    ) -> i32;
}

/// Let this process hide the cursor while another app is frontmost.
pub(crate) fn allow_hiding_in_background() {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::string::CFString;
    let key = CFString::from_static_string("SetsCursorInBackground");
    let value = CFBoolean::true_value();
    // SAFETY: plain C calls with valid, retained CF objects.
    let error = unsafe {
        let connection = _CGSDefaultConnection();
        CGSSetConnectionProperty(
            connection,
            connection,
            key.as_concrete_TypeRef(),
            value.as_CFTypeRef(),
        )
    };
    assert_eq!(error, 0, "CGSSetConnectionProperty(SetsCursorInBackground)");
}

/// State shared with the tap callback.
pub(crate) struct State {
    pub(crate) grabbed: AtomicBool,
    pub(crate) tx: Mutex<Option<Sender<LocalInput>>>,
    pub(crate) echo: Arc<Echo>,
}

impl State {
    pub(crate) fn new(echo: Arc<Echo>) -> Self {
        State {
            echo,
            grabbed: AtomicBool::new(false),
            tx: Mutex::new(None),
        }
    }
}

/// Mouser's HID button numbering: 1 left, 2 right, 3 middle, then the rest.
fn button_for(ty: u32, ev: &CGEvent) -> u8 {
    match ty {
        LEFT_MOUSE_DOWN | LEFT_MOUSE_UP => 1,
        RIGHT_MOUSE_DOWN | RIGHT_MOUSE_UP => 2,
        _ => (ev.get_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER) + 1) as u8,
    }
}

pub(crate) fn translate(ty: u32, ev: &CGEvent) -> Option<LocalInput> {
    match ty {
        MOUSE_MOVED | LEFT_MOUSE_DRAGGED | RIGHT_MOUSE_DRAGGED | OTHER_MOUSE_DRAGGED => {
            let at = ev.location();
            Some(LocalInput::Motion {
                dx: ev.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_X) as f64,
                dy: ev.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y) as f64,
                abs: (at.x, at.y),
            })
        }
        LEFT_MOUSE_DOWN | RIGHT_MOUSE_DOWN | OTHER_MOUSE_DOWN => Some(LocalInput::Button {
            button: button_for(ty, ev),
            down: true,
        }),
        LEFT_MOUSE_UP | RIGHT_MOUSE_UP | OTHER_MOUSE_UP => Some(LocalInput::Button {
            button: button_for(ty, ev),
            down: false,
        }),
        SCROLL_WHEEL => Some(LocalInput::Scroll {
            dx: ev.get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_2)
                as f64,
            dy: ev.get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1)
                as f64,
        }),
        KEY_DOWN | KEY_UP => {
            let keycode = ev.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            keymap::usage_for_keycode(keycode).map(|usage| LocalInput::Key {
                usage,
                down: ty == KEY_DOWN,
            })
        }
        FLAGS_CHANGED => {
            let keycode = ev.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            let usage = keymap::usage_for_keycode(keycode)?;
            let bit = keymap::modifier_down_bit(keycode)?;
            Some(LocalInput::Key {
                usage,
                down: ev.get_flags().bits() & bit != 0,
            })
        }
        _ => None,
    }
}

/// The tap callback body: deliver the event, then swallow it while grabbed.
/// A motion event at a position Mouser itself just set.
fn is_own_echo(state: &State, input: &LocalInput) -> bool {
    match input {
        LocalInput::Motion { abs, .. } => state.echo.is_echo(abs.0, abs.1, Instant::now()),
        _ => false,
    }
}

pub(crate) fn handle(state: &State, ty: u32, ev: &CGEvent) -> Verdict {
    if ev.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == MOUSER_TAG {
        return Verdict::Keep;
    }
    if let Some(input) = translate(ty, ev)
        && !is_own_echo(state, &input)
        && let Some(tx) = state.tx.lock().unwrap().as_ref()
    {
        // A receiver that has gone away just means nobody is listening.
        let _ = tx.send(input);
    }
    if state.grabbed.load(Ordering::SeqCst) {
        Verdict::Drop
    } else {
        Verdict::Keep
    }
}

pub(crate) struct MacCapture {
    cursor: Arc<dyn Cursor>,
    create: Create,
    state: Arc<State>,
    tap: Option<Tap>,
    grabbed: bool,
}

impl MacCapture {
    pub(crate) fn new(cursor: Arc<dyn Cursor>, create: Create, echo: Arc<Echo>) -> Self {
        MacCapture {
            cursor,
            create,
            state: Arc::new(State::new(echo)),
            tap: None,
            grabbed: false,
        }
    }

    pub(crate) fn system(echo: Arc<Echo>) -> Self {
        Self::new(Arc::new(SystemCursor), tap::create_system, echo)
    }
}

impl InputCapture for MacCapture {
    fn start(&mut self, tx: Sender<LocalInput>) -> Result<()> {
        *self.state.tx.lock().unwrap() = Some(tx);
        let state = self.state.clone();
        self.tap = Some(Tap::start(
            self.create,
            event_mask(),
            Box::new(move |ty, ev| handle(&state, ty, ev)),
        )?);
        Ok(())
    }

    fn set_grabbed(&mut self, grabbed: bool) -> Result<()> {
        if grabbed == self.grabbed {
            return Ok(());
        }
        self.grabbed = grabbed;
        self.state.grabbed.store(grabbed, Ordering::SeqCst);
        if grabbed {
            let (x, y) = self.cursor.position();
            self.cursor.warp(x, y);
            self.cursor.associate(false);
            self.cursor.hide();
        } else {
            self.cursor.associate(true);
            self.cursor.show();
        }
        Ok(())
    }

    fn warp(&mut self, x: f64, y: f64) -> Result<()> {
        self.state.echo.record(x, y);
        self.cursor.warp(x, y);
        Ok(())
    }

    fn stop(&mut self) {
        self.tap = None;
        *self.state.tx.lock().unwrap() = None;
        self.set_grabbed(false).unwrap();
    }
}

impl Drop for MacCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
