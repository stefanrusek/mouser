use crate::capture::*;
use crate::inject::WinInject;
use mouser_platform::{
    Error, InputCapture, InputInject, LocalDisplay, LocalInput, PlatformProblem,
};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{LPARAM, POINT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
    MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP,
    MOUSEINPUT, SendInput,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, SetCursorPos, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDOWN,
    WM_XBUTTONUP,
};

const F24: u32 = 0x0007_0073;

fn no_problems() -> Problems {
    Arc::new(|_| {})
}

fn cursor() -> (i32, i32) {
    crate::dpi::physical(|| {
        let mut p = POINT::default();
        unsafe { GetCursorPos(&mut p).unwrap() };
        (p.x, p.y)
    })
}

fn put_cursor((x, y): (i32, i32)) {
    crate::dpi::physical(|| unsafe { SetCursorPos(x, y).unwrap() });
}

/// An untagged event, as a real device would send it.
fn mouse(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: i32) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data as u32,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    assert_eq!(unsafe { SendInput(&[input], size_of::<INPUT>() as i32) }, 1);
}

fn key(scan: u16, down: bool) {
    let mut input = crate::inject::key_input(scan, false, down, 0);
    input.Anonymous.ki.dwExtraInfo = 0;
    crate::inject::send(&[input]).unwrap();
}

fn started(problems: Problems) -> (WinCapture, Receiver<LocalInput>) {
    let (tx, rx) = channel();
    let mut capture = WinCapture::new(problems);
    capture.start(tx).unwrap();
    (capture, rx)
}

/// The first input satisfying `want`, within a few seconds.
fn next(rx: &Receiver<LocalInput>, want: impl Fn(&LocalInput) -> bool) -> LocalInput {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        let left = end.saturating_duration_since(Instant::now());
        let got = rx
            .recv_timeout(left)
            .expect("expected input did not arrive");
        if want(&got) {
            return got;
        }
    }
}

fn display(origin: f64, scale: f32) -> LocalDisplay {
    LocalDisplay {
        key: String::new(),
        name: String::new(),
        pixel_size: (1000, 1000),
        logical_size: (1000.0 / f64::from(scale), 1000.0 / f64::from(scale)),
        scale,
        physical_size_mm: None,
        origin: (origin, 0.0),
    }
}

#[test]
fn motion_without_a_grab_is_the_change_since_the_last_point() {
    let mut last = None;
    let m = |at, last: &mut _| mouse_raw(WM_MOUSEMOVE, at, 0, None, last);
    assert_eq!(
        m((100, 50), &mut last),
        Some(Raw::Motion {
            dx: 0,
            dy: 0,
            x: 100,
            y: 50
        })
    );
    assert_eq!(
        m((103, 48), &mut last),
        Some(Raw::Motion {
            dx: 3,
            dy: -2,
            x: 103,
            y: 48
        })
    );
}

#[test]
fn motion_while_grabbed_is_the_distance_from_the_parked_cursor() {
    let mut last = Some((1, 1));
    let parked = Some((2559, 600));
    // The hook point is not clamped to the screen, so a push past the edge counts.
    assert_eq!(
        mouse_raw(WM_MOUSEMOVE, (2578, 590), 0, parked, &mut last),
        Some(Raw::Motion {
            dx: 19,
            dy: -10,
            x: 2559,
            y: 600
        })
    );
    assert_eq!(last, None, "the next ungrabbed motion starts afresh");
    assert_eq!(
        mouse_raw(WM_MOUSEMOVE, (2559, 600), 0, parked, &mut last),
        None
    );
}

#[test]
fn buttons_are_numbered_one_to_five() {
    let at = (0, 0);
    let mut last = None;
    let mut button = |msg, data| mouse_raw(msg, at, data, None, &mut last);
    let b = |button, down| Some(Raw::Button { button, down });
    assert_eq!(button(WM_LBUTTONDOWN, 0), b(1, true));
    assert_eq!(button(WM_LBUTTONUP, 0), b(1, false));
    assert_eq!(button(WM_RBUTTONDOWN, 0), b(2, true));
    assert_eq!(button(WM_RBUTTONUP, 0), b(2, false));
    assert_eq!(button(WM_MBUTTONDOWN, 0), b(3, true));
    assert_eq!(button(WM_MBUTTONUP, 0), b(3, false));
    assert_eq!(button(WM_XBUTTONDOWN, 1 << 16), b(4, true));
    assert_eq!(button(WM_XBUTTONUP, 1 << 16), b(4, false));
    assert_eq!(button(WM_XBUTTONDOWN, 2 << 16), b(5, true));
    assert_eq!(button(WM_XBUTTONUP, 2 << 16), b(5, false));
    assert_eq!(button(WM_XBUTTONDOWN, 3 << 16), None, "no sixth button");
    assert_eq!(button(0x0200 + 0x40, 0), None, "some other message");
}

#[test]
fn wheel_deltas_are_signed_per_axis() {
    let mut last = None;
    let mut wheel =
        |msg, delta: i16| mouse_raw(msg, (0, 0), u32::from(delta as u16) << 16, None, &mut last);
    assert_eq!(
        wheel(WM_MOUSEWHEEL, 120),
        Some(Raw::Scroll { dx: 0, dy: 120 })
    );
    assert_eq!(
        wheel(WM_MOUSEWHEEL, -240),
        Some(Raw::Scroll { dx: 0, dy: -240 })
    );
    assert_eq!(
        wheel(WM_MOUSEHWHEEL, 120),
        Some(Raw::Scroll { dx: 120, dy: 0 })
    );
}

#[test]
fn keys_are_hid_usages_by_scan_code_and_extended_flag() {
    // Flags: 0x01 extended, 0x80 key up.
    assert_eq!(
        key_raw(0x1E, 0),
        Some(Raw::Key {
            usage: 0x0007_0004,
            down: true
        })
    );
    assert_eq!(
        key_raw(0x1D, 0x01 | 0x80),
        Some(Raw::Key {
            usage: 0x0007_00E4,
            down: false
        })
    );
    assert_eq!(key_raw(0x6F, 0), None, "not a key Mouser forwards");
}

#[test]
fn raw_input_becomes_local_input_in_logical_coordinates() {
    let all = [display(0.0, 1.0), display(1000.0, 2.0)];
    assert_eq!(
        convert(
            &all,
            Raw::Motion {
                dx: 10,
                dy: -4,
                x: 1400,
                y: 200
            }
        ),
        LocalInput::Motion {
            dx: 5.0,
            dy: -2.0,
            abs: (1200.0, 100.0)
        }
    );
    assert_eq!(
        convert(&all, Raw::Scroll { dx: -120, dy: 240 }),
        LocalInput::Scroll {
            dx: -15.0,
            dy: 30.0
        }
    );
    assert_eq!(
        convert(
            &all,
            Raw::Button {
                button: 2,
                down: true
            }
        ),
        LocalInput::Button {
            button: 2,
            down: true
        }
    );
    assert_eq!(
        convert(
            &all,
            Raw::Key {
                usage: 5,
                down: false
            }
        ),
        LocalInput::Key {
            usage: 5,
            down: false
        }
    );
}

#[test]
fn a_capture_must_be_started_before_it_is_grabbed_or_warped() {
    let mut capture = WinCapture::new(no_problems());
    let not_started = Err(Error::Os("capture has not been started".into()));
    assert_eq!(capture.set_grabbed(true), not_started);
    assert_eq!(capture.warp(1.0, 1.0), not_started);
    capture.stop();
}

#[test]
fn a_capture_starts_once_and_stops_as_often_as_asked() {
    let _live = crate::live_input();
    let (mut capture, _rx) = started(no_problems());
    let (tx, _rx2) = channel();
    assert_eq!(
        capture.start(tx),
        Err(Error::Os("capture is already started".into()))
    );
    capture.stop();
    capture.stop();
    let (tx, _rx3): (Sender<LocalInput>, _) = channel();
    capture.start(tx).unwrap();
}

#[test]
fn input_passes_through_and_is_reported_until_grabbed() {
    let _live = crate::live_input();
    let home = cursor();
    put_cursor((400, 400));
    let (mut capture, rx) = started(no_problems());
    // The first motion has nothing to be relative to.
    mouse(MOUSEEVENTF_MOVE, 2, 0, 0);
    next(
        &rx,
        |i| matches!(i, LocalInput::Motion { dx, .. } if *dx == 0.0),
    );
    mouse(MOUSEEVENTF_MOVE, 8, 0, 0);
    let right = next(
        &rx,
        |i| matches!(i, LocalInput::Motion { dx, .. } if *dx > 0.0),
    );
    mouse(MOUSEEVENTF_MOVE, -8, 0, 0);
    let left = next(
        &rx,
        |i| matches!(i, LocalInput::Motion { dx, .. } if *dx < 0.0),
    );
    let (LocalInput::Motion { abs: a, .. }, LocalInput::Motion { abs: b, .. }) = (right, left)
    else {
        unreachable!()
    };
    assert!(a.0 > b.0, "{a:?} {b:?}");
    capture.stop();
    put_cursor(home);
}

#[test]
fn while_grabbed_input_is_swallowed_and_the_cursor_stays_parked() {
    let _live = crate::live_input();
    let home = cursor();
    let (mut capture, rx) = started(no_problems());
    capture.set_grabbed(true).unwrap();
    assert_eq!(cursor(), home);

    mouse(MOUSEEVENTF_MOVE, 10, 5, 0);
    let LocalInput::Motion { dx, dy, abs } = next(&rx, |i| matches!(i, LocalInput::Motion { .. }))
    else {
        unreachable!()
    };
    assert!(dx > 0.0 && dy > 0.0, "{dx} {dy}");
    let (logical, _) =
        crate::displays::pixel_to_logical(&crate::displays::displays().unwrap(), home.0, home.1);
    assert_eq!(abs, logical);
    assert_eq!(cursor(), home, "the swallowed move did not move the cursor");

    mouse(MOUSEEVENTF_LEFTDOWN, 0, 0, 0);
    mouse(MOUSEEVENTF_LEFTUP, 0, 0, 0);
    assert_eq!(
        next(&rx, |i| matches!(i, LocalInput::Button { .. })),
        LocalInput::Button {
            button: 1,
            down: true
        }
    );
    assert_eq!(
        next(&rx, |i| matches!(i, LocalInput::Button { .. })),
        LocalInput::Button {
            button: 1,
            down: false
        }
    );
    mouse(MOUSEEVENTF_XDOWN, 0, 0, 1);
    mouse(MOUSEEVENTF_XUP, 0, 0, 1);
    assert_eq!(
        next(&rx, |i| matches!(i, LocalInput::Button { .. })),
        LocalInput::Button {
            button: 4,
            down: true
        }
    );
    assert_eq!(
        next(&rx, |i| matches!(i, LocalInput::Button { .. })),
        LocalInput::Button {
            button: 4,
            down: false
        }
    );

    mouse(MOUSEEVENTF_WHEEL, 0, 0, 120);
    assert_eq!(
        next(&rx, |i| matches!(i, LocalInput::Scroll { .. })),
        LocalInput::Scroll { dx: 0.0, dy: 15.0 }
    );
    mouse(MOUSEEVENTF_HWHEEL, 0, 0, -120);
    assert_eq!(
        next(&rx, |i| matches!(i, LocalInput::Scroll { .. })),
        LocalInput::Scroll { dx: -15.0, dy: 0.0 }
    );

    key(0x76, true);
    key(0x76, false);
    assert_eq!(
        next(&rx, |i| matches!(i, LocalInput::Key { .. })),
        LocalInput::Key {
            usage: F24,
            down: true
        }
    );
    assert_eq!(
        next(&rx, |i| matches!(i, LocalInput::Key { .. })),
        LocalInput::Key {
            usage: F24,
            down: false
        }
    );
    // A key Mouser does not forward is swallowed without a report.
    key(0x6F, true);
    key(0x6F, false);
    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());

    capture.set_grabbed(false).unwrap();
    key(0x76, true);
    key(0x76, false);
    assert_eq!(
        next(&rx, |i| matches!(i, LocalInput::Key { .. })),
        LocalInput::Key {
            usage: F24,
            down: true
        }
    );
    capture.stop();
    put_cursor(home);
}

#[test]
fn warping_moves_and_re_parks_the_cursor() {
    let _live = crate::live_input();
    let home = cursor();
    let (mut capture, rx) = started(no_problems());
    capture.set_grabbed(true).unwrap();
    let all = crate::displays::displays().unwrap();
    let primary = all.iter().find(|d| d.origin == (0.0, 0.0)).unwrap();
    capture.warp(100.0, 100.0).unwrap();
    let scale = f64::from(primary.scale);
    let parked = (
        (100.0 * scale).round() as i32,
        (100.0 * scale).round() as i32,
    );
    assert_eq!(cursor(), parked);
    mouse(MOUSEEVENTF_MOVE, 6, 0, 0);
    let LocalInput::Motion { dx, abs, .. } = next(&rx, |i| matches!(i, LocalInput::Motion { .. }))
    else {
        unreachable!()
    };
    assert!(dx > 0.0);
    assert_eq!(abs, (100.0, 100.0));
    capture.stop();
    put_cursor(home);
}

#[test]
fn mousers_own_injected_input_is_never_captured() {
    let _live = crate::live_input();
    let (mut capture, rx) = started(no_problems());
    capture.set_grabbed(true).unwrap();
    let mut inject = WinInject::new(None);
    inject.button(4, true).unwrap();
    inject.button(4, false).unwrap();
    inject.scroll(1.0, 1.0).unwrap();
    inject.key(F24, true).unwrap();
    inject.key(F24, false).unwrap();
    let got = rx.recv_timeout(Duration::from_millis(300));
    assert!(got.is_err(), "{got:?}");
    capture.stop();
}

#[test]
fn capture_survives_a_receiver_that_went_away() {
    let _live = crate::live_input();
    let (mut capture, rx) = started(no_problems());
    drop(rx);
    capture.set_grabbed(true).unwrap();
    mouse(MOUSEEVENTF_MOVE, 3, 0, 0);
    key(0x76, true);
    key(0x76, false);
    std::thread::sleep(Duration::from_millis(100));
    capture.stop();
}

#[test]
fn the_watchdog_leaves_working_hooks_alone() {
    let _live = crate::live_input();
    let (tx, problems) = channel();
    let sink: Problems = Arc::new(move |text| {
        let _ = tx.send(text);
    });
    let (txi, _rx) = channel();
    let mut capture = WinCapture::with_interval(sink, Duration::from_millis(100));
    capture.start(txi).unwrap();
    std::thread::sleep(Duration::from_millis(700));
    capture.stop();
    assert!(problems.try_recv().is_err());
}

#[test]
fn the_watchdog_reinstalls_a_hook_the_system_removed_and_raises_a_problem() {
    let _live = crate::live_input();
    let (ptx, prx) = channel();
    let sink: Problems = Arc::new(move |text| {
        let _ = ptx.send(text);
    });
    let (tx, rx) = channel();
    let mut capture = WinCapture::with_interval(sink, Duration::from_millis(100));
    capture.start(tx).unwrap();
    capture.drop_hooks();
    let PlatformProblem::Raised(problem) = prx.recv_timeout(Duration::from_secs(5)).unwrap() else {
        panic!("a lost hook is raised");
    };
    assert!(problem.contains("Mouser lost its hold"), "{problem}");
    // The hooks are back: input reaches them again.
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        key(0x76, true);
        key(0x76, false);
        if rx.recv_timeout(Duration::from_millis(100)).is_ok() {
            break;
        }
        assert!(Instant::now() < end, "hooks were not reinstalled");
    }
    capture.stop();
}

#[test]
fn hook_calls_that_must_not_be_processed_are_passed_on() {
    // A negative code means the hook must call the next one without looking.
    unsafe {
        mouse_proc(-1, WPARAM(0), LPARAM(0));
        key_proc(-1, WPARAM(0), LPARAM(0));
    }
}

#[test]
fn the_hook_thread_ignores_messages_it_does_not_know() {
    let _live = crate::live_input();
    let (mut capture, rx) = started(no_problems());
    capture.poke();
    key(0x76, true);
    key(0x76, false);
    next(&rx, |i| matches!(i, LocalInput::Key { .. }));
    capture.stop();
}

#[test]
fn the_pointer_is_hidden_while_grabbed_and_back_when_released() {
    use crate::cursor::arrow_is_blank;
    let _live = crate::live_input();
    crate::cursor::recover().unwrap();
    let (mut capture, _rx) = started(no_problems());
    assert!(!arrow_is_blank());
    capture.set_grabbed(true).unwrap();
    assert!(arrow_is_blank());
    capture.set_grabbed(false).unwrap();
    assert!(!arrow_is_blank());
    capture.stop();
}

#[test]
fn stopping_while_grabbed_brings_the_pointer_back() {
    use crate::cursor::arrow_is_blank;
    let _live = crate::live_input();
    crate::cursor::recover().unwrap();
    let (mut capture, _rx) = started(no_problems());
    capture.set_grabbed(true).unwrap();
    assert!(arrow_is_blank());
    capture.stop();
    assert!(!arrow_is_blank());
}

#[test]
fn a_pointer_that_cannot_be_hidden_is_a_problem_not_a_failed_grab() {
    use crate::capture::set_pointer_hidden;
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = seen.clone();
    let problems: Problems = Arc::new(move |text| sink.lock().unwrap().push(text));
    set_pointer_hidden(true, &problems, |hidden| {
        assert!(hidden);
        Err(Error::Os("CreateCursor: denied".into()))
    });
    set_pointer_hidden(false, &problems, |_| Ok(()));
    assert_eq!(
        *seen.lock().unwrap(),
        [PlatformProblem::Raised(
            "Mouser could not change the pointer on this PC: CreateCursor: denied".into()
        )]
    );
}
