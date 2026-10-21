//! Conformance rules 1 (grab), 2 (release on drop), 3 (HID usage independent
//! of layout), 4 (displays) and 7 (no feedback of injected events) against the
//! real macOS backend, plus the OS-side half of rules 1 and 7 that the trait
//! cannot observe.
//!
//! These tests drive real input and need Accessibility and Input Monitoring:
//! without them tap creation fails and every test below fails loudly.
//!
//! Safety: the conformance rules post letters, Return, Cmd/Ctrl, clicks and
//! scroll. Every such event carries Mouser's user-data tag, and `Swallow`
//! (a tail-of-chain HID tap installed first) drops every tagged event except
//! pointer moves, so nothing reaches the focused app. Pointer moves are
//! restored by `PointerRestore`. Tests are serialised and each grab lasts
//! well under a second and is released on drop.

use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes, kCFRunLoopDefaultMode};
use core_graphics::display::{CGDisplay, CGPoint};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CGMouseButton, CallbackResult, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use mouser_platform::conformance as rules;
use mouser_platform::{LocalInput, Platform};
use mouser_platform_macos::MacPlatform;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel, sync_channel};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

const MOUSER_TAG: i64 = 0x4D4F_5553_4552;

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// Event types as the tail tap saw them: `(type, keycode, user data)`.
type Seen = Arc<Mutex<Vec<(u32, i64, i64)>>>;

/// A tail-of-chain HID filtering tap on its own run-loop thread. It records
/// every event that survives the taps ahead of it and drops Mouser-tagged
/// events other than pointer moves, so injected test input never reaches
/// applications.
struct Swallow {
    seen: Seen,
    swallowed: Arc<Mutex<u64>>,
    run_loop: CFRunLoop,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Swallow {
    fn new() -> Swallow {
        let seen: Seen = Arc::default();
        let swallowed = Arc::new(Mutex::new(0u64));
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = sync_channel(1);
        let (seen2, swallowed2, stop2) = (seen.clone(), swallowed.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            let tap = CGEventTap::new(
                CGEventTapLocation::HID,
                CGEventTapPlacement::TailAppendEventTap,
                CGEventTapOptions::Default,
                vec![
                    CGEventType::LeftMouseDown,
                    CGEventType::LeftMouseUp,
                    CGEventType::RightMouseDown,
                    CGEventType::RightMouseUp,
                    CGEventType::OtherMouseDown,
                    CGEventType::OtherMouseUp,
                    CGEventType::MouseMoved,
                    CGEventType::KeyDown,
                    CGEventType::KeyUp,
                    CGEventType::FlagsChanged,
                    CGEventType::ScrollWheel,
                ],
                move |_proxy, ty, ev| {
                    if matches!(
                        ty,
                        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
                    ) {
                        return CallbackResult::Keep;
                    }
                    let tag = ev.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA);
                    let keycode = ev.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
                    seen2.lock().unwrap().push((ty as u32, keycode, tag));
                    if tag == MOUSER_TAG && !matches!(ty, CGEventType::MouseMoved) {
                        *swallowed2.lock().unwrap() += 1;
                        CallbackResult::Drop
                    } else {
                        CallbackResult::Keep
                    }
                },
            )
            .expect("CGEventTapCreate refused: grant Accessibility and Input Monitoring");
            let source = tap.mach_port().create_runloop_source(0).unwrap();
            let run_loop = CFRunLoop::get_current();
            run_loop.add_source(&source, unsafe { kCFRunLoopCommonModes });
            tap.enable();
            ready_tx.send(run_loop).unwrap();
            while !stop2.load(Ordering::SeqCst) {
                let _ = CFRunLoop::run_in_mode(
                    unsafe { kCFRunLoopDefaultMode },
                    Duration::from_millis(100),
                    false,
                );
            }
        });
        let run_loop = ready_rx.recv().expect("swallow tap thread");
        Swallow {
            seen,
            swallowed,
            run_loop,
            stop,
            thread: Some(thread),
        }
    }

    fn swallowed(&self) -> u64 {
        std::thread::sleep(Duration::from_millis(150));
        *self.swallowed.lock().unwrap()
    }

    /// F19 presses that reached the tail of the chain (i.e. would reach apps).
    fn f19_seen(&self) -> usize {
        std::thread::sleep(Duration::from_millis(150));
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|(ty, keycode, tag)| {
                *tag != MOUSER_TAG && matches!(*ty, 10 | 11) && *keycode == 0x50
            })
            .count()
    }
}

impl Drop for Swallow {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.run_loop.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn pointer() -> CGPoint {
    let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).unwrap();
    CGEvent::new(source).unwrap().location()
}

/// Puts the pointer back where the test found it, even on panic.
struct PointerRestore(CGPoint);

impl PointerRestore {
    fn new() -> Self {
        PointerRestore(pointer())
    }
}

impl Drop for PointerRestore {
    fn drop(&mut self) {
        CGDisplay::warp_mouse_cursor_position(self.0).unwrap();
        CGDisplay::associate_mouse_and_mouse_cursor_position(true).unwrap();
        // The OS emits settling motion events after a warp; let them pass
        // before the next test (or process) starts capturing.
        std::thread::sleep(Duration::from_millis(400));
    }
}

/// Runs `body` with the swallow tap and pointer restore in place.
fn safely(injects: bool, body: impl FnOnce()) {
    let _serial = serial();
    let _pointer = PointerRestore::new();
    let swallow = Swallow::new();
    body();
    if injects {
        assert!(swallow.swallowed() > 0, "injected input was not swallowed");
    }
}

/// Everything the capture delivered within `window`. Real input from a person
/// at the keyboard may be mixed in, so callers look only for their own events.
fn drain(rx: &Receiver<LocalInput>, window: Duration) -> Vec<LocalInput> {
    let mut got = Vec::new();
    while let Ok(input) = rx.recv_timeout(window) {
        got.push(input);
    }
    got
}

const F19: u32 = 0x0007_006E;
const LEFT_SHIFT: u32 = 0x0007_00E1;

fn f19(down: bool) -> LocalInput {
    LocalInput::Key { usage: F19, down }
}

fn raw(source: &CGEventSource, ty: CGEventType, at: CGPoint) -> CGEvent {
    CGEvent::new_mouse_event(source.clone(), ty, at, CGMouseButton::Left).unwrap()
}

fn untagged_f19(down: bool) -> CGEvent {
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
    CGEvent::new_keyboard_event(source, 0x50, down).unwrap()
}

#[test]
fn rule_1_grab() {
    safely(false, || rules::rule_1_grab(&MacPlatform::new()));
}

#[test]
fn rule_2_release_on_drop() {
    safely(true, || rules::rule_2_release(&MacPlatform::new()));
}

#[test]
fn rule_3_hid_usage_independent_of_layout() {
    safely(true, || rules::rule_3_hid_usage(&MacPlatform::new()));
}

#[test]
fn rule_4_displays() {
    rules::rule_4_displays(&MacPlatform::new());
}

#[test]
fn rule_7_no_self_capture() {
    safely(true, || rules::rule_7_no_self_capture(&MacPlatform::new()));
}

#[test]
fn a_grab_swallows_local_keys_and_reports_pointer_motion_until_released() {
    let _serial = serial();
    let _pointer = PointerRestore::new();
    let swallow = Swallow::new();
    let platform = MacPlatform::new();
    let mut capture = platform.capture().unwrap();
    let (tx, rx) = channel();
    capture.start(tx).unwrap();
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
    let window = Duration::from_millis(300);

    // Ungrabbed: an (unassigned) F19 press is delivered and reaches apps.
    untagged_f19(true).post(CGEventTapLocation::HID);
    untagged_f19(false).post(CGEventTapLocation::HID);
    let got = drain(&rx, window);
    assert!(got.contains(&f19(true)) && got.contains(&f19(false)));
    assert_eq!(swallow.f19_seen(), 2);

    // Grabbed: F19 and pointer motion are still reported but F19 never reaches
    // apps. (A physical mouse cannot move a disassociated pointer; a posted
    // absolute move can, so pointer position is not asserted here.)
    capture.set_grabbed(true).unwrap();
    let parked = pointer();
    untagged_f19(true).post(CGEventTapLocation::HID);
    untagged_f19(false).post(CGEventTapLocation::HID);
    let moved = raw(
        &source,
        CGEventType::MouseMoved,
        CGPoint::new(parked.x + 25.0, parked.y),
    );
    moved.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, 25);
    moved.post(CGEventTapLocation::HID);
    let got = drain(&rx, window);
    assert!(got.contains(&f19(true)));
    assert!(
        got.iter()
            .any(|i| matches!(i, LocalInput::Motion { dx, .. } if *dx == 25.0))
    );
    assert_eq!(swallow.f19_seen(), 2, "grabbed input reached apps");

    // Released: local input flows to apps again.
    capture.set_grabbed(false).unwrap();
    untagged_f19(true).post(CGEventTapLocation::HID);
    untagged_f19(false).post(CGEventTapLocation::HID);
    assert_eq!(swallow.f19_seen(), 4);
    capture.stop();
}

#[test]
fn injected_modifier_and_function_keys_do_not_loop_back_but_local_ones_do() {
    let _serial = serial();
    let _pointer = PointerRestore::new();
    let swallow = Swallow::new();
    let platform = MacPlatform::new();
    let mut capture = platform.capture().unwrap();
    let mut inject = platform.inject().unwrap();
    let (tx, rx) = channel();
    capture.start(tx).unwrap();
    // Shift and F19 are harmless even if something went wrong.
    inject.key(LEFT_SHIFT, true).unwrap();
    inject.key(F19, true).unwrap();
    inject.key(F19, false).unwrap();
    inject.key(LEFT_SHIFT, false).unwrap();
    let looped_back: Vec<LocalInput> = drain(&rx, Duration::from_millis(300))
        .into_iter()
        .filter(
            |i| matches!(i, LocalInput::Key { usage, .. } if *usage == F19 || *usage == LEFT_SHIFT),
        )
        .collect();
    assert_eq!(looped_back, []);
    assert_eq!(swallow.swallowed(), 4);
    untagged_f19(true).post(CGEventTapLocation::HID);
    untagged_f19(false).post(CGEventTapLocation::HID);
    assert!(drain(&rx, Duration::from_millis(300)).contains(&f19(true)));
    capture.stop();
}
