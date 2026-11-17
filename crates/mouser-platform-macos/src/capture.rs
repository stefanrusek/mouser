//! `InputCapture`: one HID-level filtering event tap delivering `LocalInput`.
//!
//! Grabbed ("remote mode", FINDINGS-macos-input section 2): the tap drops
//! keys, buttons, scroll, gestures and `NX_SYSDEFINED`; the cursor is
//! disassociated from the mouse, parked and hidden; motion is still reported
//! from `kCGMouseEventDeltaX/Y`. Events carrying Mouser's own
//! `kCGEventSourceUserData` tag were injected by this process and are ignored
//! (conformance rule 7).
//!
//! Every hardware event carries the modifier flags held at that moment. A
//! modifier the tap saw go down whose flag is gone from a later event was
//! released while the tap did not see it (the OS turned the tap off for a
//! moment, or Secure Input held the keyboard): its release is reported then,
//! so the computer being controlled is never left holding it (Story 3, no
//! stuck keys).

use crate::activity::Activity;
use crate::cursor::{Cursor, SystemCursor};
use crate::cursor_guard::{Guard, Watcher};
use crate::echo::Echo;
use crate::keymap;
use crate::tap::{self, Create, Tap, Verdict};
use core_graphics::event::{CGEvent, EventField};
use mouser_platform::{InputCapture, LocalInput, Result};
use std::collections::BTreeSet;
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

/// State shared with the tap callback.
pub(crate) struct State {
    pub(crate) grabbed: AtomicBool,
    pub(crate) tx: Mutex<Option<Sender<LocalInput>>>,
    pub(crate) echo: Arc<Echo>,
    /// Keycodes of the modifiers the tap last saw down.
    pub(crate) modifiers: Mutex<BTreeSet<u16>>,
}

impl State {
    pub(crate) fn new(echo: Arc<Echo>) -> Self {
        State {
            echo,
            grabbed: AtomicBool::new(false),
            tx: Mutex::new(None),
            modifiers: Mutex::new(BTreeSet::new()),
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

/// `kCGEventSourceStateHIDSystemState`: events from real hardware.
const HID_SYSTEM_STATE: i64 = 1;

/// Decides whether a motion event came from another app rather than the user.
type ForeignFilter = fn(&State, &CGEvent, &LocalInput) -> bool;

/// Research D2 (default): hardware motion carries the HID system state; an
/// app's synthesised or warped motion does not. UNVERIFIED on a real game
/// (probe S2 unobserved).
fn by_source_state(_: &State, ev: &CGEvent, _: &LocalInput) -> bool {
    ev.get_integer_value_field(EventField::EVENT_SOURCE_STATE_ID) != HID_SYSTEM_STATE
}

/// Research D2 fallback: motion located where Mouser or the guard last put
/// the pointer (the park point and foreign corrections are recorded in the
/// echo ring) is not the user's.
pub(crate) fn by_plausibility(state: &State, _: &CGEvent, input: &LocalInput) -> bool {
    is_own_echo(state, input)
}

/// The filters, and which one is in force: switching after the S2 result
/// (T035) is changing `FILTER_IN_FORCE` to 1.
const FILTERS: [ForeignFilter; 2] = [by_source_state, by_plausibility];
const FILTER_IN_FORCE: usize = 0;

/// Motion that, while grabbed, another app produced (FR-002, FR-003).
fn is_foreign_motion(state: &State, ev: &CGEvent, input: &LocalInput) -> bool {
    matches!(input, LocalInput::Motion { .. })
        && state.grabbed.load(Ordering::SeqCst)
        && FILTERS[FILTER_IN_FORCE](state, ev, input)
}

/// A motion event at a position Mouser itself just set.
fn is_own_echo(state: &State, input: &LocalInput) -> bool {
    match input {
        LocalInput::Motion { abs, .. } => state.echo.is_echo(abs.0, abs.1, Instant::now()),
        _ => false,
    }
}

/// Releases of held modifiers whose flag `ev` no longer carries, then the
/// event's own input, with the held modifiers brought up to date.
fn with_missed_releases(
    state: &State,
    ty: u32,
    ev: &CGEvent,
    input: LocalInput,
) -> Vec<LocalInput> {
    let keycode = ev.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
    let own = (ty == FLAGS_CHANGED).then_some(keycode);
    let flags = ev.get_flags().bits();
    let mut held = state.modifiers.lock().unwrap();
    let missed: Vec<u16> = held
        .iter()
        .copied()
        .filter(|k| Some(*k) != own && flags & keymap::modifier_flag(*k).unwrap() == 0)
        .collect();
    let mut out: Vec<LocalInput> = missed
        .iter()
        .map(|k| {
            held.remove(k);
            LocalInput::Key {
                usage: keymap::usage_for_keycode(*k).unwrap(),
                down: false,
            }
        })
        .collect();
    if let (Some(k), LocalInput::Key { down, .. }) = (own, input) {
        if down {
            held.insert(k);
        } else {
            held.remove(&k);
        }
    }
    out.push(input);
    out
}

/// The tap callback body: deliver the event, then swallow it while grabbed.
pub(crate) fn handle(state: &State, ty: u32, ev: &CGEvent) -> Verdict {
    if ev.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == MOUSER_TAG {
        return Verdict::Keep;
    }
    if let Some(input) = translate(ty, ev)
        && !is_own_echo(state, &input)
        && !is_foreign_motion(state, ev, &input)
    {
        let inputs = with_missed_releases(state, ty, ev, input);
        if let Some(tx) = state.tx.lock().unwrap().as_ref() {
            for input in inputs {
                // A receiver that has gone away just means nobody is listening.
                let _ = tx.send(input);
            }
        }
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
    pub(crate) state: Arc<State>,
    /// What the cursor should be; shared with the watcher thread only.
    pub(crate) guard: Arc<Mutex<Guard>>,
    /// Runs from grab to release.
    pub(crate) watcher: Option<Watcher>,
    tap: Option<Tap>,
    /// Held while capturing so App Nap cannot throttle the event tap.
    pub(crate) activity: Option<Activity>,
}

impl MacCapture {
    pub(crate) fn new(cursor: Arc<dyn Cursor>, create: Create, echo: Arc<Echo>) -> Self {
        MacCapture {
            guard: Arc::new(Mutex::new(Guard::new(cursor.clone(), echo.clone()))),
            watcher: None,
            cursor,
            create,
            state: Arc::new(State::new(echo)),
            tap: None,
            activity: None,
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
        self.activity = Some(Activity::begin(
            "Mouser is capturing keyboard and mouse input",
        ));
        self.tap = Some(
            Tap::start(
                self.create,
                event_mask(),
                Box::new(move |ty, ev| handle(&state, ty, ev)),
            )
            .inspect_err(|_| self.activity = None)?,
        );
        Ok(())
    }

    fn set_grabbed(&mut self, grabbed: bool) -> Result<()> {
        if grabbed == self.state.grabbed.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.state.grabbed.store(grabbed, Ordering::SeqCst);
        // The guard thread stops first, so it cannot undo what follows.
        self.watcher = None;
        if grabbed {
            let grab = || -> Result<(f64, f64)> {
                let (x, y) = self.cursor.position();
                self.cursor.warp(x, y)?;
                self.cursor.associate(false)?;
                self.cursor.hide()?;
                Ok((x, y))
            };
            let park = match grab() {
                Ok(park) => park,
                Err(refused) => {
                    // Not grabbed after all: put the pointer back as far as the
                    // OS lets us, and let the caller see the refusal.
                    self.state.grabbed.store(false, Ordering::SeqCst);
                    let _ = self.cursor.associate(true);
                    return Err(refused);
                }
            };
            self.state.echo.record(park.0, park.1);
            self.guard
                .lock()
                .unwrap()
                .begin_remote(park, Instant::now());
            self.watcher = Some(Watcher::spawn(self.guard.clone()));
            Ok(())
        } else {
            // The caller's `warp` has put the pointer at the landing point.
            let landing = self.cursor.position();
            let hides = self.guard.lock().unwrap().arrive(landing, Instant::now());
            // All are tried, so one refusal does not leave the others undone.
            let associated = self.cursor.associate(true);
            let mut shown = Ok(());
            for _ in 0..hides {
                shown = shown.and(self.cursor.show());
            }
            // The guard holds the take-back through the settle window.
            self.watcher = Some(Watcher::spawn(self.guard.clone()));
            associated.and(shown)
        }
    }

    fn warp(&mut self, x: f64, y: f64) -> Result<()> {
        self.state.echo.record(x, y);
        self.cursor.warp(x, y)
    }

    fn stop(&mut self) {
        self.tap = None;
        self.activity = None;
        *self.state.tx.lock().unwrap() = None;
        // `stop` also runs from `Drop`, which cannot return the refusal.
        if let Err(refused) = self.set_grabbed(false) {
            eprintln!("mouser: releasing the cursor at stop failed: {refused}");
        }
        // No settle window at stop: the guard ends with the capture.
        self.watcher = None;
        self.guard.lock().unwrap().end();
    }
}

impl Drop for MacCapture {
    fn drop(&mut self) {
        self.stop();
    }
}
