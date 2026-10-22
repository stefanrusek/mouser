use crate::capture::{Cursor, MOUSER_TAG, MacCapture, State, event_mask, handle, translate};
use crate::tap::Verdict;
use core_graphics::display::CGPoint;
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventType, CGMouseButton, EventField, ScrollEventUnit,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use mouser_platform::{InputCapture, LocalInput};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::Instant;

fn source() -> CGEventSource {
    CGEventSource::new(CGEventSourceStateID::Private).unwrap()
}

fn mouse(ty: CGEventType, x: f64, y: f64) -> CGEvent {
    CGEvent::new_mouse_event(source(), ty, CGPoint::new(x, y), CGMouseButton::Left).unwrap()
}

fn key(keycode: u16, down: bool) -> CGEvent {
    CGEvent::new_keyboard_event(source(), keycode, down).unwrap()
}

fn flags_changed(keycode: u16, flags: u64) -> CGEvent {
    let ev = key(keycode, true);
    ev.set_type(CGEventType::FlagsChanged);
    ev.set_flags(CGEventFlags::from_bits_retain(flags));
    ev
}

#[derive(Default)]
struct FakeCursor {
    calls: Mutex<Vec<String>>,
}

impl FakeCursor {
    fn log(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }
}

impl Cursor for FakeCursor {
    fn position(&self) -> (f64, f64) {
        (300.0, 200.0)
    }
    fn warp(&self, x: f64, y: f64) {
        self.log(format!("warp {x} {y}"));
    }
    fn associate(&self, on: bool) {
        self.log(format!("associate {on}"));
    }
    fn hide(&self) {
        self.log("hide".into());
    }
    fn show(&self) {
        self.log("show".into());
    }
}

fn state_with_receiver() -> (State, std::sync::mpsc::Receiver<LocalInput>) {
    let state = State::new(Arc::default());
    let (tx, rx) = channel();
    *state.tx.lock().unwrap() = Some(tx);
    (state, rx)
}

#[test]
fn mask_covers_pointer_keys_sysdefined_scroll_and_gestures() {
    let mask = event_mask();
    for ty in [
        1, 2, 3, 4, 5, 6, 7, 10, 11, 12, 14, 22, 25, 26, 27, 29, 30, 31, 32, 33, 34,
    ] {
        assert!(mask & (1 << ty) != 0, "type {ty} not tapped");
    }
    assert_eq!(mask & (1 << 13), 0);
}

#[test]
fn pointer_motion_carries_deltas_and_position() {
    let ev = mouse(CGEventType::MouseMoved, 120.0, 340.0);
    ev.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, -7);
    ev.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_Y, 4);
    for ty in [5, 6, 7, 27] {
        assert_eq!(
            translate(ty, &ev),
            Some(LocalInput::Motion {
                dx: -7.0,
                dy: 4.0,
                abs: (120.0, 340.0)
            })
        );
    }
}

#[test]
fn buttons_use_one_based_hid_numbering() {
    let ev = mouse(CGEventType::LeftMouseDown, 0.0, 0.0);
    let button = |ty, b: u8| {
        Some(LocalInput::Button {
            button: b,
            down: ty % 2 == 1,
        })
    };
    assert_eq!(translate(1, &ev), button(1, 1));
    assert_eq!(translate(2, &ev), button(2, 1));
    assert_eq!(translate(3, &ev), button(3, 2));
    assert_eq!(translate(4, &ev), button(4, 2));
    // Other buttons: the CG button number plus one (middle = 3, back = 4).
    ev.set_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER, 2);
    assert_eq!(
        translate(25, &ev),
        Some(LocalInput::Button {
            button: 3,
            down: true
        })
    );
    ev.set_integer_value_field(EventField::MOUSE_EVENT_BUTTON_NUMBER, 3);
    assert_eq!(
        translate(26, &ev),
        Some(LocalInput::Button {
            button: 4,
            down: false
        })
    );
}

#[test]
fn scroll_reports_point_deltas() {
    let ev = CGEvent::new_scroll_event(source(), ScrollEventUnit::PIXEL, 2, 5, -3, 0).unwrap();
    assert_eq!(
        translate(22, &ev),
        Some(LocalInput::Scroll { dx: -3.0, dy: 5.0 })
    );
}

#[test]
fn keys_map_to_hid_usages_and_unmapped_keys_are_not_delivered() {
    assert_eq!(
        translate(10, &key(0x00, true)),
        Some(LocalInput::Key {
            usage: 0x0007_0004,
            down: true
        })
    );
    assert_eq!(
        translate(11, &key(0x00, false)),
        Some(LocalInput::Key {
            usage: 0x0007_0004,
            down: false
        })
    );
    assert_eq!(translate(10, &key(0x3F, true)), None);
}

#[test]
fn modifier_flag_changes_become_key_down_and_up() {
    // Left Shift: device bit 0x2 set => down, clear => up.
    assert_eq!(
        translate(12, &flags_changed(0x38, 0x0002_0002)),
        Some(LocalInput::Key {
            usage: 0x0007_00E1,
            down: true
        })
    );
    assert_eq!(
        translate(12, &flags_changed(0x38, 0)),
        Some(LocalInput::Key {
            usage: 0x0007_00E1,
            down: false
        })
    );
    // Right Shift still down after Left Shift released.
    assert_eq!(
        translate(12, &flags_changed(0x38, 0x0002_0004)),
        Some(LocalInput::Key {
            usage: 0x0007_00E1,
            down: false
        })
    );
    // Fn is not a HID key; a keycode that is not a modifier is ignored.
    assert_eq!(translate(12, &flags_changed(0x3F, 0x0080_0000)), None);
    assert_eq!(translate(12, &flags_changed(0x00, 0)), None);
}

#[test]
fn other_event_types_are_not_delivered() {
    assert_eq!(translate(14, &key(0, true)), None);
    assert_eq!(translate(29, &key(0, true)), None);
}

#[test]
fn ungrabbed_events_are_delivered_and_kept() {
    let (state, rx) = state_with_receiver();
    assert_eq!(handle(&state, 10, &key(0x00, true)), Verdict::Keep);
    assert_eq!(
        rx.try_recv().unwrap(),
        LocalInput::Key {
            usage: 0x0007_0004,
            down: true
        }
    );
}

#[test]
fn grabbed_events_are_delivered_and_dropped_including_undelivered_classes() {
    let (state, rx) = state_with_receiver();
    state
        .grabbed
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(handle(&state, 10, &key(0x00, true)), Verdict::Drop);
    assert_eq!(
        rx.try_recv().unwrap(),
        LocalInput::Key {
            usage: 0x0007_0004,
            down: true
        }
    );
    let m = mouse(CGEventType::MouseMoved, 5.0, 6.0);
    m.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, 3);
    assert_eq!(handle(&state, 5, &m), Verdict::Drop);
    assert_eq!(
        rx.try_recv().unwrap(),
        LocalInput::Motion {
            dx: 3.0,
            dy: 0.0,
            abs: (5.0, 6.0)
        }
    );
    // NX_SYSDEFINED and gestures are swallowed without being forwarded.
    assert_eq!(handle(&state, 14, &key(0, true)), Verdict::Drop);
    assert_eq!(handle(&state, 29, &key(0, true)), Verdict::Drop);
    assert!(rx.try_recv().is_err());
}

#[test]
fn events_tagged_by_mouser_are_ignored_even_when_grabbed() {
    let (state, rx) = state_with_receiver();
    state
        .grabbed
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let ev = key(0x00, true);
    ev.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, MOUSER_TAG);
    assert_eq!(handle(&state, 10, &ev), Verdict::Keep);
    assert!(rx.try_recv().is_err());
}

#[test]
fn events_with_no_receiver_are_still_filtered() {
    let state = State::new(Arc::default());
    assert_eq!(handle(&state, 10, &key(0x00, true)), Verdict::Keep);
}

fn fake_capture() -> (Arc<FakeCursor>, MacCapture) {
    let cursor = Arc::new(FakeCursor::default());
    let capture = MacCapture::new(cursor.clone(), crate::tap::create_system, Arc::default());
    (cursor, capture)
}

#[test]
fn grabbing_parks_disassociates_and_hides_then_releasing_restores() {
    let (cursor, mut capture) = fake_capture();
    capture.set_grabbed(true).unwrap();
    assert_eq!(cursor.take(), ["warp 300 200", "associate false", "hide"]);
    // Grabbing again changes nothing.
    capture.set_grabbed(true).unwrap();
    assert!(cursor.take().is_empty());
    capture.set_grabbed(false).unwrap();
    assert_eq!(cursor.take(), ["associate true", "show"]);
    capture.set_grabbed(false).unwrap();
    assert!(cursor.take().is_empty());
}

#[test]
fn warp_moves_the_cursor() {
    let (cursor, mut capture) = fake_capture();
    capture.warp(10.0, 20.0).unwrap();
    assert_eq!(cursor.take(), ["warp 10 20"]);
}

#[test]
fn dropping_a_grabbing_capture_releases_the_grab() {
    let (cursor, mut capture) = fake_capture();
    capture.set_grabbed(true).unwrap();
    cursor.take();
    drop(capture);
    assert_eq!(cursor.take(), ["associate true", "show"]);
}

#[test]
fn start_refused_by_the_os_reports_the_permissions_to_grant() {
    fn refuse(
        _: crate::tap::TapCallback,
        _: *mut std::ffi::c_void,
        _: u64,
    ) -> core_foundation::mach_port::CFMachPortRef {
        std::ptr::null_mut()
    }
    let mut capture = MacCapture::new(Arc::new(FakeCursor::default()), refuse, Arc::default());
    let (tx, _rx) = channel();
    assert!(capture.start(tx).is_err());
    assert!(capture.activity.is_none(), "nothing held without a tap");
}

#[test]
fn a_started_capture_runs_a_real_tap_and_stops_it() {
    // Needs Accessibility / Input Monitoring: fails loudly without them.
    let cursor = Arc::new(FakeCursor::default());
    let mut capture = MacCapture::new(cursor.clone(), crate::tap::create_system, Arc::default());
    let (tx, _rx) = channel();
    capture.start(tx).unwrap();
    capture.stop();
    assert!(cursor.take().is_empty());
}

#[test]
fn untagged_motion_at_a_position_mouser_just_set_is_an_echo_not_input() {
    // Measured on hardware: the OS hands the tap an untagged copy of an
    // injected absolute move, located exactly where it was injected.
    let echo = Arc::new(crate::echo::Echo::default());
    let state = State::new(echo.clone());
    let (tx, rx) = channel();
    *state.tx.lock().unwrap() = Some(tx);
    echo.record(20.0, 20.0);
    let copy = mouse(CGEventType::MouseMoved, 20.0, 20.0);
    copy.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, -1093);
    assert_eq!(handle(&state, 5, &copy), Verdict::Keep);
    assert!(rx.try_recv().is_err());
    // Motion elsewhere is still real input, and clicks at that spot are too.
    assert_eq!(
        handle(&state, 5, &mouse(CGEventType::MouseMoved, 40.0, 20.0)),
        Verdict::Keep
    );
    assert!(matches!(rx.try_recv(), Ok(LocalInput::Motion { .. })));
    assert_eq!(
        handle(&state, 1, &mouse(CGEventType::LeftMouseDown, 20.0, 20.0)),
        Verdict::Keep
    );
    assert!(matches!(rx.try_recv(), Ok(LocalInput::Button { .. })));
}

#[test]
fn injecting_and_warping_record_the_position_for_echo_detection() {
    let echo = Arc::new(crate::echo::Echo::default());
    let mut inj =
        crate::inject::MacInject::new(Box::new(crate::inject_tests::NullPoster), echo.clone());
    mouser_platform::InputInject::motion_abs(&mut inj, 7.0, 8.0).unwrap();
    assert!(echo.is_echo(7.0, 8.0, Instant::now()));
    let echo = Arc::new(crate::echo::Echo::default());
    let mut capture = MacCapture::new(
        Arc::new(FakeCursor::default()),
        crate::tap::create_system,
        echo.clone(),
    );
    capture.warp(3.0, 4.0).unwrap();
    assert!(echo.is_echo(3.0, 4.0, Instant::now()));
}

#[test]
fn the_window_server_lets_mouser_hide_the_cursor_from_the_background() {
    // Without this, a grab leaves the cursor showing on the computer the
    // user left (Mouser is never the frontmost app). It changes no state
    // other than this process's permission to hide the cursor.
    crate::capture::allow_hiding_in_background();
    crate::capture::allow_hiding_in_background();
}

const LEFT_META: u32 = 0x0007_00E3;
const RIGHT_META: u32 = 0x0007_00E7;
const LEFT_SHIFT_USAGE: u32 = 0x0007_00E1;
const CMD: u64 = 0x0010_0000;
const LEFT_CMD: u64 = CMD | 0x08;
const RIGHT_CMD: u64 = CMD | 0x10;

fn with_flags(ev: CGEvent, flags: u64) -> CGEvent {
    ev.set_flags(CGEventFlags::from_bits_retain(flags));
    ev
}

fn moved(flags: u64) -> CGEvent {
    let ev = mouse(CGEventType::MouseMoved, 10.0, 10.0);
    ev.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, 1);
    with_flags(ev, flags)
}

fn meta(usage: u32, down: bool) -> LocalInput {
    LocalInput::Key { usage, down }
}

fn is_motion(input: &LocalInput) -> bool {
    matches!(input, LocalInput::Motion { .. })
}

#[test]
fn a_missed_command_release_is_reported_from_the_next_events_flags() {
    let (state, rx) = state_with_receiver();
    state
        .grabbed
        .store(true, std::sync::atomic::Ordering::SeqCst);
    handle(&state, 12, &flags_changed(0x37, LEFT_CMD));
    handle(&state, 5, &moved(LEFT_CMD));
    // The release never reached the tap (it was off, or Secure Input held
    // the keyboard); the next event no longer has Command down.
    handle(&state, 5, &moved(0));
    handle(&state, 5, &moved(0));
    let got: Vec<LocalInput> = rx.try_iter().collect();
    assert_eq!(got[0], meta(LEFT_META, true));
    assert!(is_motion(&got[1]));
    assert_eq!(got[2], meta(LEFT_META, false));
    assert!(is_motion(&got[3]) && is_motion(&got[4]));
    assert_eq!(got.len(), 5, "{got:?}");
}

#[test]
fn a_missed_release_comes_before_the_key_that_revealed_it() {
    let (state, rx) = state_with_receiver();
    handle(&state, 12, &flags_changed(0x37, LEFT_CMD));
    handle(&state, 10, &with_flags(key(0x00, true), 0));
    handle(&state, 12, &flags_changed(0x38, 0x0002_0002));
    let got: Vec<LocalInput> = rx.try_iter().collect();
    assert_eq!(
        got,
        [
            meta(LEFT_META, true),
            meta(LEFT_META, false),
            meta(0x0007_0004, true),
            meta(LEFT_SHIFT_USAGE, true),
        ]
    );
}

#[test]
fn held_modifiers_are_not_released_while_the_flags_still_show_them() {
    let (state, rx) = state_with_receiver();
    handle(&state, 12, &flags_changed(0x37, LEFT_CMD));
    handle(&state, 12, &flags_changed(0x36, LEFT_CMD | RIGHT_CMD));
    handle(
        &state,
        10,
        &with_flags(key(0x08, true), LEFT_CMD | RIGHT_CMD),
    );
    // Left Command up while right Command stays down: only the left goes.
    handle(&state, 12, &flags_changed(0x37, RIGHT_CMD));
    handle(&state, 5, &moved(RIGHT_CMD));
    handle(&state, 12, &flags_changed(0x36, 0));
    handle(&state, 5, &moved(0));
    let keys: Vec<LocalInput> = rx.try_iter().filter(|i| !is_motion(i)).collect();
    assert_eq!(
        keys,
        [
            meta(LEFT_META, true),
            meta(RIGHT_META, true),
            meta(0x0007_0006, true),
            meta(LEFT_META, false),
            meta(RIGHT_META, false),
        ]
    );
}

#[test]
fn own_injected_events_and_pointer_echoes_do_not_release_held_modifiers() {
    let (state, rx) = state_with_receiver();
    handle(&state, 12, &flags_changed(0x37, LEFT_CMD));
    let injected = moved(0);
    injected.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, MOUSER_TAG);
    handle(&state, 5, &injected);
    state.echo.record(10.0, 10.0);
    handle(&state, 5, &moved(0));
    // Nor do events the capture does not report (NX_SYSDEFINED, gestures).
    handle(&state, 14, &moved(0));
    let got: Vec<LocalInput> = rx.try_iter().collect();
    assert_eq!(got, [meta(LEFT_META, true)]);
}
