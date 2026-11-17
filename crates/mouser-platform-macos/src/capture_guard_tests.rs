//! Story 1 end to end on the `Cursor` seam: a grabbed capture with a scripted
//! "other app" that captures and releases the Mac mouse. (Not an integration
//! test file because `Cursor` is crate-private.)

use crate::capture::{MacCapture, handle};
use crate::cursor_guard::TICK;
use crate::cursor_guard_tests::Scripted;
use crate::diag::heard_during;
use core_graphics::display::CGPoint;
use core_graphics::event::{CGEvent, CGEventType, CGMouseButton, EventField};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use mouser_platform::{InputCapture, LocalInput};
use std::sync::Arc;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

fn capture_over(cursor: &Arc<Scripted>) -> MacCapture {
    MacCapture::new(cursor.clone(), crate::tap::create_system, Arc::default())
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn motion(source_state: i64) -> CGEvent {
    let source = CGEventSource::new(CGEventSourceStateID::Private).unwrap();
    let ev = CGEvent::new_mouse_event(
        source,
        CGEventType::MouseMoved,
        CGPoint::new(960.0, 540.0),
        CGMouseButton::Left,
    )
    .unwrap();
    ev.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, 600);
    ev.set_integer_value_field(EventField::EVENT_SOURCE_STATE_ID, source_state);
    ev
}

#[test]
fn a_foreign_visible_flip_right_after_the_grab_is_corrected_and_release_is_clean() {
    let heard = heard_during(|| {
        let cursor = Scripted::untouched();
        let mut capture = capture_over(&cursor);
        capture.set_grabbed(true).unwrap();
        assert!(!cursor.is_visible());
        // S5: the game makes the cursor visible; the position does not move.
        cursor.foreign_release();
        wait_until("one tick to hide it again", || !cursor.is_visible());
        assert_eq!(cursor.depth(), 1);
        assert!(!cursor.is_associated());
        capture.set_grabbed(false).unwrap();
        assert!(cursor.is_visible());
        assert!(cursor.is_associated());
        assert_eq!(cursor.depth(), 0, "hides balanced");
        // Nothing further after the release.
        cursor.take_calls();
        std::thread::sleep(TICK * 4);
        assert!(cursor.take_calls().is_empty());
    });
    assert!(!heard.is_empty());
}

#[test]
fn the_s1_jump_to_centre_is_restored_and_ungrab_lands_normally() {
    let heard = heard_during(|| {
        let cursor = Scripted::untouched();
        let mut capture = capture_over(&cursor);
        capture.set_grabbed(true).unwrap();
        let park = cursor.at();
        cursor.foreign_warp(960.0, 540.0);
        cursor.foreign_disassociate();
        wait_until("the cursor back at the park point", || cursor.at() == park);
        capture.set_grabbed(false).unwrap();
        assert!(cursor.is_visible() && cursor.is_associated());
        assert_eq!(cursor.depth(), 0);
    });
    assert!(!heard.is_empty());
}

#[test]
fn foreign_motion_while_grabbed_is_not_forwarded_but_hardware_motion_is() {
    // Encodes UNVERIFIED assumption (probe S2 unobserved): the scripted
    // foreign actor models the assumed source-state behaviour and cannot
    // prove the real game behaves that way; the owner check in T035 is the
    // real verification.
    let cursor = Scripted::untouched();
    let mut capture = capture_over(&cursor);
    let (tx, rx) = channel();
    *capture.state.tx.lock().unwrap() = Some(tx);
    capture.set_grabbed(true).unwrap();
    let verdict = handle(&capture.state, 5, &motion(0));
    assert_eq!(verdict, crate::tap::Verdict::Drop);
    assert!(rx.try_recv().is_err(), "foreign motion forwarded");
    handle(&capture.state, 5, &motion(1));
    assert!(matches!(rx.try_recv(), Ok(LocalInput::Motion { .. })));
    // The same foreign motion is untouched once ungrabbed.
    capture.set_grabbed(false).unwrap();
    handle(&capture.state, 5, &motion(0));
    assert!(matches!(rx.try_recv(), Ok(LocalInput::Motion { .. })));
}

#[test]
fn the_plausibility_fallback_drops_motion_at_a_position_mouser_set() {
    let cursor = Scripted::untouched();
    let capture = capture_over(&cursor);
    let input = LocalInput::Motion {
        dx: 1.0,
        dy: 0.0,
        abs: (5.0, 5.0),
    };
    capture.state.echo.record(5.0, 5.0);
    assert!(crate::capture::by_plausibility(
        &capture.state,
        &motion(1),
        &input
    ));
    let elsewhere = LocalInput::Motion {
        dx: 1.0,
        dy: 0.0,
        abs: (500.0, 5.0),
    };
    assert!(!crate::capture::by_plausibility(
        &capture.state,
        &motion(1),
        &elsewhere
    ));
}

#[test]
fn a_capture_dropped_while_remote_leaves_the_cursor_shown_and_balanced() {
    let heard = heard_during(|| {
        let cursor = Scripted::untouched();
        let mut capture = capture_over(&cursor);
        capture.set_grabbed(true).unwrap();
        cursor.foreign_release();
        wait_until("a correction", || !cursor.is_visible());
        drop(capture);
        assert!(cursor.is_visible());
        assert!(cursor.is_associated());
        assert_eq!(cursor.depth(), 0);
    });
    assert!(!heard.is_empty());
}

#[test]
fn a_refused_show_during_a_correction_does_not_leave_the_release_unbalanced() {
    let heard = heard_during(|| {
        let cursor = Scripted::untouched();
        let mut capture = capture_over(&cursor);
        capture.set_grabbed(true).unwrap();
        cursor.refuse("show", 1);
        cursor.foreign_release();
        wait_until("recovery after the refusal", || {
            !cursor.is_visible() && cursor.depth() == 1
        });
        capture.set_grabbed(false).unwrap();
        assert_eq!(cursor.depth(), 0);
    });
    assert!(!heard.is_empty());
}

#[test]
fn the_mac_as_the_controlled_computer_is_never_guarded() {
    let heard = heard_during(|| {
        let cursor = Scripted::untouched();
        let mut capture = capture_over(&cursor);
        let (tx, rx) = channel();
        *capture.state.tx.lock().unwrap() = Some(tx);
        // Never grabbed: another app captures and moves the cursor freely.
        cursor.foreign_hide();
        cursor.foreign_warp(960.0, 540.0);
        cursor.foreign_disassociate();
        std::thread::sleep(TICK * 5);
        assert!(capture.watcher.is_none(), "no guard thread");
        assert!(cursor.take_calls().is_empty(), "no correction");
        assert_eq!(cursor.at(), (960.0, 540.0));
        assert!(rx.try_recv().is_err());
        // Releasing a grab that never was changes nothing either.
        capture.set_grabbed(false).unwrap();
        assert!(cursor.take_calls().is_empty());
    });
    assert!(heard.is_empty(), "no log record: {heard:?}");
}

#[test]
fn ungrab_against_a_recapturing_game_leaves_the_cursor_shown_and_associated() {
    let heard = heard_during(|| {
        let cursor = Scripted::untouched();
        let mut capture = capture_over(&cursor);
        capture.set_grabbed(true).unwrap();
        // The game takes the mouse again on the focus change.
        cursor.foreign_hide();
        cursor.foreign_disassociate();
        capture.set_grabbed(false).unwrap();
        // Within one tick the take-back holds.
        wait_until("the cursor shown and associated", || {
            cursor.is_visible() && cursor.is_associated()
        });
        cursor.foreign_hide();
        wait_until("a re-hide to be corrected within the window", || {
            cursor.is_visible()
        });
        drop(capture);
        assert!(cursor.is_visible());
    });
    assert!(!heard.is_empty());
}

#[test]
fn the_guard_thread_ends_by_itself_after_the_settle_window() {
    let heard = heard_during(|| {
        let cursor = Scripted::untouched();
        let mut capture = capture_over(&cursor);
        capture.set_grabbed(true).unwrap();
        capture.set_grabbed(false).unwrap();
        std::thread::sleep(crate::cursor_guard::SETTLE + TICK * 5);
        cursor.foreign_hide();
        std::thread::sleep(TICK * 4);
        assert!(!cursor.is_visible(), "the other app wins after the window");
    });
    let _ = heard;
}
