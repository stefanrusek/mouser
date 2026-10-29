use crate::capture::*;
use crate::inject::WinInject;
use crate::watchdog::{Hook, Outcome, Surroundings};
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
use windows::Win32::UI::WindowsAndMessaging::HHOOK;
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
fn motion_without_a_grab_is_the_change_from_the_cursor() {
    assert_eq!(
        mouse_raw(WM_MOUSEMOVE, (103, 48), 0, (100, 50), false),
        Some(Raw::Motion {
            dx: 3,
            dy: -2,
            x: 103,
            y: 48
        })
    );
    // However far the cursor was placed since the last move (Mouser places
    // it for a computer driving this one), only this move counts.
    assert_eq!(
        mouse_raw(WM_MOUSEMOVE, (900, 701), 0, (900, 700), false),
        Some(Raw::Motion {
            dx: 0,
            dy: 1,
            x: 900,
            y: 701
        })
    );
}

#[test]
fn motion_while_grabbed_is_the_distance_from_the_held_cursor() {
    // The hook point is not clamped to the screen, so a push past the edge counts.
    assert_eq!(
        mouse_raw(WM_MOUSEMOVE, (2578, 590), 0, (2559, 600), true),
        Some(Raw::Motion {
            dx: 19,
            dy: -10,
            x: 2559,
            y: 600
        })
    );
    assert_eq!(
        mouse_raw(WM_MOUSEMOVE, (2559, 600), 0, (2559, 600), true),
        None
    );
}

/// Where the cursor of a test's hook context is.
const CURSOR: (i32, i32) = (500, 300);

fn hook_ctx(cursor: fn() -> Option<(i32, i32)>) -> (HookCtx, Receiver<Raw>) {
    let (raw, forwarded) = channel();
    let ctx = HookCtx {
        raw,
        shared: Arc::new(shared()),
        cursor,
    };
    (ctx, forwarded)
}

fn moved_to(x: i32, y: i32) -> windows::Win32::UI::WindowsAndMessaging::MSLLHOOKSTRUCT {
    windows::Win32::UI::WindowsAndMessaging::MSLLHOOKSTRUCT {
        pt: POINT { x, y },
        ..Default::default()
    }
}

#[test]
fn the_hooks_count_every_event_they_see_not_only_probes() {
    use std::sync::atomic::Ordering::SeqCst;
    use windows::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT;
    let (ctx, _forwarded) = &mut hook_ctx(|| Some(CURSOR));
    // Real input, Mouser's own and the watchdog's probe alike: any event a
    // hook sees shows that it still sees input.
    for tag in [0, crate::inject::MOUSER_TAG, PROBE_TAG] {
        let mouse = windows::Win32::UI::WindowsAndMessaging::MSLLHOOKSTRUCT {
            dwExtraInfo: tag,
            ..moved_to(500, 300)
        };
        on_mouse(ctx, WM_MOUSEMOVE, &mouse);
        let key = KBDLLHOOKSTRUCT {
            scanCode: 0x1E,
            dwExtraInfo: tag,
            ..Default::default()
        };
        on_key(ctx, &key);
    }
    let seen = (
        ctx.shared.seen_mouse.load(SeqCst),
        ctx.shared.seen_key.load(SeqCst),
    );
    assert_eq!(seen, (3, 3));
}

#[test]
fn a_grabbed_move_counts_from_where_the_cursor_is_not_from_where_it_was_parked() {
    // The cursor got away from where the grab began (a move just before the
    // grab took hold, or one the system let past a slow hook): every move
    // after it is still only itself, so the other computer's pointer does
    // not keep running in that direction.
    let (ctx, forwarded) = &mut hook_ctx(|| Some(CURSOR));
    ctx.shared
        .grabbed
        .store(true, std::sync::atomic::Ordering::SeqCst);
    for _ in 0..3 {
        assert!(on_mouse(ctx, WM_MOUSEMOVE, &moved_to(500, 304)));
        assert_eq!(
            forwarded.try_recv(),
            Ok(Raw::Motion {
                dx: 0,
                dy: 4,
                x: 500,
                y: 300
            })
        );
    }
}

#[test]
fn a_move_when_the_cursor_cannot_be_read_counts_for_nothing() {
    let (ctx, forwarded) = &mut hook_ctx(|| None);
    assert!(!on_mouse(ctx, WM_MOUSEMOVE, &moved_to(10, 20)));
    assert_eq!(
        forwarded.try_recv(),
        Ok(Raw::Motion {
            dx: 0,
            dy: 0,
            x: 10,
            y: 20
        })
    );
    ctx.shared
        .grabbed
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(on_mouse(ctx, WM_MOUSEMOVE, &moved_to(10, 20)));
    assert!(forwarded.try_recv().is_err());
}

#[test]
fn buttons_are_numbered_one_to_five() {
    let at = (0, 0);
    let button = |msg, data| mouse_raw(msg, at, data, at, false);
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
    let wheel =
        |msg, delta: i16| mouse_raw(msg, (0, 0), u32::from(delta as u16) << 16, (0, 0), false);
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

/// The motion reported for the next untagged move straight down.
fn next_motion_down(rx: &Receiver<LocalInput>) -> (f64, f64) {
    mouse(MOUSEEVENTF_MOVE, 0, 4, 0);
    let LocalInput::Motion { dx, dy, .. } = next(rx, |i| matches!(i, LocalInput::Motion { .. }))
    else {
        unreachable!()
    };
    (dx, dy)
}

#[test]
fn a_move_that_got_past_the_grab_does_not_skew_the_ones_after() {
    let _live = crate::live_input();
    let home = cursor();
    put_cursor((400, 400));
    let (mut capture, rx) = started(no_problems());
    capture.set_grabbed(true).unwrap();
    // A move the hook did not swallow (one that came just before the grab
    // took hold, or one the system let past a slow hook) leaves the cursor
    // off the point it was parked at. `SetCursorPos` stands in for it: it
    // moves the cursor without reaching the hooks.
    put_cursor((460, 400));
    for _ in 0..3 {
        let (dx, dy) = next_motion_down(&rx);
        assert_eq!(dx, 0.0, "a move straight down goes straight down");
        assert!(dy > 0.0, "{dy}");
    }
    capture.stop();
    put_cursor(home);
}

#[test]
fn motion_after_the_cursor_was_placed_is_measured_from_where_it_is() {
    let _live = crate::live_input();
    let home = cursor();
    put_cursor((400, 400));
    let (mut capture, rx) = started(no_problems());
    next_motion_down(&rx);
    // Another computer drives this one: Mouser places the cursor, which the
    // hooks never see. The owner's own next move must not count the jump.
    put_cursor((400, 700));
    let (dx, dy) = next_motion_down(&rx);
    assert_eq!(dx, 0.0);
    assert!(dy > 0.0 && dy < 100.0, "{dy}");
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
fn grabbing_and_letting_go_write_no_pointer_lines_of_the_backends_own() {
    // Core writes pointer.hidden and pointer.shown with the catalog's
    // reasons; a second record from the backend would double them.
    let _live = crate::live_input();
    let lines = crate::diag::heard_during(|| {
        let (mut capture, _rx) = started(no_problems());
        capture.set_grabbed(true).unwrap();
        capture.set_grabbed(false).unwrap();
        capture.set_grabbed(true).unwrap();
        capture.stop();
    });
    let pointer: Vec<_> = lines
        .iter()
        .filter(|(_, event, _)| event.starts_with("pointer."))
        .collect();
    assert!(pointer.is_empty(), "{pointer:?}");
    assert!(
        lines.iter().any(|(_, event, _)| event == "capture.start"),
        "the capture's own lines were heard"
    );
}

#[test]
fn warping_to_a_point_on_no_display_is_an_error_and_leaves_the_cursor() {
    let _live = crate::live_input();
    let (mut capture, _rx) = started(no_problems());
    let before = cursor();
    let far = 1_000_000.0;
    let warped = capture.warp(far, far);
    let after = cursor();
    capture.stop();
    let Err(Error::Os(text)) = warped else {
        panic!("warp to a point on no display returned {warped:?}");
    };
    assert_eq!(text, "(1000000, 1000000) is on no display");
    assert_eq!(after, before, "the cursor did not move");
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
    let mut capture = WinCapture::with_watchdog(sink, Duration::from_millis(100), always);
    capture.start(txi).unwrap();
    std::thread::sleep(Duration::from_millis(700));
    capture.stop();
    assert!(problems.try_recv().is_err());
}

/// A PC on which the watchdog always probes.
struct Always;

impl Surroundings for Always {
    fn skip(&mut self) -> Option<crate::watchdog::Skip> {
        None
    }

    fn lost(&mut self) -> Option<mouser_platform::CaptureLossReason> {
        None
    }

    fn probed(&mut self) {}

    fn foreground(&self) -> String {
        "test.exe".into()
    }

    fn input_tick(&self) -> u32 {
        crate::watchdog::last_input()
    }
}

fn always() -> Box<dyn Surroundings + Send> {
    Box::new(Always)
}

/// Whether `rx` gets the input `send` makes within a few seconds of trying.
fn arrives(rx: &Receiver<LocalInput>, send: impl Fn(), want: impl Fn(&LocalInput) -> bool) {
    let end = Instant::now() + Duration::from_secs(5);
    loop {
        send();
        if rx
            .recv_timeout(Duration::from_millis(100))
            .is_ok_and(|got| want(&got))
        {
            return;
        }
        assert!(Instant::now() < end, "hooks were not reinstalled");
    }
}

#[test]
fn the_watchdog_quietly_reinstalls_a_hook_the_system_removed() {
    let _live = crate::live_input();
    let (ptx, prx) = channel();
    let sink: Problems = Arc::new(move |text| {
        let _ = ptx.send(text);
    });
    let (tx, rx) = channel();
    let mut capture = WinCapture::with_watchdog(sink, Duration::from_millis(100), always);
    capture.start(tx).unwrap();
    let shared = capture.running().unwrap().shared.clone();
    let counts = || {
        use std::sync::atomic::Ordering::SeqCst;
        (shared.seen_mouse.load(SeqCst), shared.seen_key.load(SeqCst))
    };
    let press = || {
        key(0x76, true);
        key(0x76, false);
    };
    capture.drop_hooks(&[Hook::Keyboard]);
    // The hook thread removes the hook when it reads that message: wait until
    // keys no longer reach it.
    let removed = (0..30).any(|_| {
        let before = counts().1;
        press();
        std::thread::sleep(Duration::from_millis(150));
        counts().1 == before
    });
    assert!(removed, "the keyboard hook was not removed");
    // Keys it saw before it went are not keys that reached it again.
    while rx.try_recv().is_ok() {}
    // Someone keeps typing, so the silent hook is missed and installed again:
    // keys reach it again.
    arrives(&rx, press, |i| matches!(i, LocalInput::Key { .. }));
    // Wait for a probe both hooks see after the reinstall: the miss is cured.
    let back = counts();
    let end = Instant::now() + Duration::from_secs(5);
    while !(counts().0 > back.0 && counts().1 > back.1) {
        assert!(
            Instant::now() < end,
            "no probe got through after the reinstall"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    // The mouse hook was left alone and still works.
    arrives(
        &rx,
        || mouse(MOUSEEVENTF_MOVE, 2, 0, 0),
        |i| matches!(i, LocalInput::Motion { .. }),
    );
    capture.stop();
    // One miss that a reinstall cured is not the owner's problem.
    assert!(prx.try_recv().is_err());
}

#[test]
fn the_watchdog_reinstalls_both_hooks_when_both_were_removed() {
    let _live = crate::live_input();
    let (tx, rx) = channel();
    let mut capture = WinCapture::with_watchdog(no_problems(), Duration::from_millis(100), always);
    capture.start(tx).unwrap();
    capture.drop_hooks(&Hook::BOTH);
    let press = || {
        key(0x76, true);
        key(0x76, false);
    };
    arrives(&rx, press, |i| matches!(i, LocalInput::Key { .. }));
    arrives(
        &rx,
        || mouse(MOUSEEVENTF_MOVE, 2, 0, 0),
        |i| matches!(i, LocalInput::Motion { .. }),
    );
    capture.stop();
}

/// Hook calls as a fake system takes them.
#[derive(Default)]
struct FakeApi {
    refuse: std::cell::Cell<bool>,
    next: std::cell::Cell<usize>,
    calls: std::cell::RefCell<Vec<String>>,
}

impl HookApi for &FakeApi {
    fn install(&self, hook: Hook) -> Option<HHOOK> {
        if self.refuse.get() {
            self.calls
                .borrow_mut()
                .push(format!("refuse {}", hook.name()));
            return None;
        }
        self.next.set(self.next.get() + 1);
        let handle = self.next.get();
        self.calls
            .borrow_mut()
            .push(format!("install {} {handle}", hook.name()));
        Some(HHOOK(handle as *mut _))
    }

    fn remove(&self, handle: HHOOK) -> bool {
        self.calls
            .borrow_mut()
            .push(format!("remove {}", handle.0 as usize));
        // The system already took hook 1 away.
        handle.0 as usize != 1
    }
}

#[test]
fn a_hook_is_installed_afresh_before_the_old_one_goes() {
    let api = FakeApi::default();
    let mut hooks = Hooks::install(&api);
    assert!(hooks.reinstall(Hook::Mouse));
    assert!(hooks.reinstall(Hook::Keyboard));
    assert_eq!(
        *api.calls.borrow(),
        [
            "install mouse 1",
            "install keyboard 2",
            "install mouse 3",
            // Already gone: counted as removed, no panic.
            "remove 1",
            "install keyboard 4",
            "remove 2",
        ]
    );
}

#[test]
fn a_fresh_hook_the_system_refuses_leaves_the_old_one_in() {
    let api = FakeApi::default();
    let mut hooks = Hooks::install(&api);
    api.refuse.set(true);
    assert!(!hooks.reinstall(Hook::Mouse));
    assert_eq!(hooks.mouse.map(|h| h.0 as usize), Some(1));
    hooks.remove(Hook::Mouse);
    hooks.remove(Hook::Mouse);
    assert_eq!(hooks.mouse, None);
    assert_eq!(
        api.calls.borrow()[2..],
        ["refuse mouse".to_string(), "remove 1".to_string()]
    );
}

#[test]
fn hooks_the_system_refuses_at_start_are_left_for_the_watchdog() {
    let api = FakeApi::default();
    api.refuse.set(true);
    let hooks = Hooks::install(&api);
    assert_eq!((hooks.mouse, hooks.keyboard), (None, None));
}

#[test]
fn the_system_hooks_install_and_a_removed_one_is_reported_gone() {
    let _live = crate::live_input();
    for hook in Hook::BOTH {
        let handle = System.install(hook).unwrap();
        assert!(System.remove(handle));
        assert!(!System.remove(handle), "already removed");
    }
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

#[test]
fn the_problem_text_is_unchanged() {
    assert_eq!(
        LOST_HOLD,
        "Mouser lost its hold on the keyboard and mouse and took it back; \
         input may have briefly leaked to this PC."
    );
}

fn shared() -> Shared {
    Shared::default()
}

#[test]
fn a_probe_is_seen_when_both_hooks_count_it() {
    use std::sync::atomic::Ordering::SeqCst;
    let s = shared();
    let out = probe_hooks_with(&s, Duration::from_secs(5), &mut || {
        s.seen_mouse.fetch_add(1, SeqCst);
        s.seen_key.fetch_add(1, SeqCst);
        Ok(())
    });
    assert_eq!(out, Outcome::Sent);
}

#[test]
fn a_probe_a_hook_does_not_count_in_time_is_sent_all_the_same() {
    use std::sync::atomic::Ordering::SeqCst;
    let s = shared();
    let out = probe_hooks_with(&s, Duration::from_millis(30), &mut || {
        s.seen_mouse.fetch_add(1, SeqCst);
        Ok(())
    });
    assert_eq!(out, Outcome::Sent);
}

#[test]
fn a_probe_the_system_refuses_is_not_sent() {
    let s = shared();
    let out = probe_hooks_with(&s, Duration::from_secs(5), &mut || {
        Err(Error::Os("SendInput: blocked".into()))
    });
    assert_eq!(out, Outcome::NotSent);
}

#[test]
fn the_real_probe_is_seen_by_working_hooks_and_leaves_the_cursor_alone() {
    let _live = crate::live_input();
    let home = cursor();
    let (mut capture, _rx) = started(no_problems());
    let s = &capture.running().unwrap().shared;
    for _ in 0..3 {
        let before = (
            s.seen_mouse.load(std::sync::atomic::Ordering::SeqCst),
            s.seen_key.load(std::sync::atomic::Ordering::SeqCst),
        );
        assert_eq!(probe_hooks(s, Duration::from_secs(5)), Outcome::Sent);
        let after = (
            s.seen_mouse.load(std::sync::atomic::Ordering::SeqCst),
            s.seen_key.load(std::sync::atomic::Ordering::SeqCst),
        );
        assert!(
            after.0 > before.0 && after.1 > before.1,
            "{before:?} {after:?}"
        );
    }
    assert_eq!(cursor(), home);
    capture.stop();
}

#[test]
fn a_mouse_message_that_means_nothing_is_not_forwarded_and_passes_through() {
    use windows::Win32::UI::WindowsAndMessaging::MSLLHOOKSTRUCT;
    let (ctx, forwarded) = &mut hook_ctx(|| Some(CURSOR));
    let info = MSLLHOOKSTRUCT::default();
    // Not grabbed: swallowed never, and a message with no meaning sends nothing.
    assert!(!on_mouse(ctx, 0x9999, &info));
    assert!(forwarded.try_recv().is_err());
    // Grabbed: swallowed, still nothing sent.
    ctx.shared
        .grabbed
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(on_mouse(ctx, 0x9999, &info));
    assert!(forwarded.try_recv().is_err());
}

// ---- the cursor guard (research V5) ----

#[test]
fn the_guard_is_this_executable_run_with_cursor_guard_and_the_parent_pid() {
    let exe = std::env::current_exe().unwrap();
    let command = guard_command(&exe, 4242);
    assert_eq!(command.get_program(), exe.as_os_str());
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args, ["--cursor-guard", "4242"]);
}

#[test]
fn capture_start_runs_this_executable_as_the_guard() {
    // Captures in this process share the guard's stop event: one at a time.
    let _live = crate::live_input();
    let (capture, _rx) = started(no_problems());
    let guard = capture.guard().expect("a guard was started");
    let image = image_of(guard);
    drop(capture);
    assert_eq!(image, std::env::current_exe().unwrap());
}

/// The executable file of `child`, read through its handle, so it can be
/// read after the child has ended.
fn image_of(child: &std::process::Child) -> std::path::PathBuf {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Threading::{PROCESS_NAME_WIN32, QueryFullProcessImageNameW};
    let mut path = [0u16; 1024];
    let mut len = path.len() as u32;
    let process = HANDLE(child.as_raw_handle());
    let name = windows::core::PWSTR(path.as_mut_ptr());
    unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, name, &mut len) }.unwrap();
    String::from_utf16_lossy(&path[..len as usize]).into()
}

/// Set in a copy of this test binary that stands in for the guard: the
/// parent's pid.
const GUARD_PARENT: &str = "MOUSER_TEST_GUARD_PARENT";

/// The file a stand-in guard writes just before it starts guarding `parent`.
fn ready_file(parent: u32) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("mouser-test-guard-{parent}.ready"))
}

/// The stand-in guard: a copy of this test binary running only
/// `guard_host`, started as the real guard is, with no window.
fn spawn_guard_host(parent: u32) -> std::io::Result<std::process::Child> {
    let mut command = std::process::Command::new(std::env::current_exe()?);
    command
        .args(["--exact", "capture_tests::guard_host", "--test-threads=1"])
        .env(GUARD_PARENT, parent.to_string());
    windowless(&mut command).spawn()
}

/// In a stand-in guard, guards the parent with the real guard, which must
/// end well; run as an ordinary test, does nothing.
#[test]
fn guard_host() {
    let Ok(parent) = std::env::var(GUARD_PARENT) else {
        return;
    };
    let parent = parent.parse().unwrap();
    std::fs::write(ready_file(parent), "").unwrap();
    assert_eq!(
        crate::cursor_guard::run(parent),
        std::process::ExitCode::SUCCESS
    );
}

fn session_of(pid: u32) -> u32 {
    use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    let mut session = 0;
    unsafe { ProcessIdToSessionId(pid, &mut session) }.unwrap();
    session
}

/// The visible top-level windows of process `pid`.
fn visible_windows_of(pid: u32) -> usize {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
    };
    unsafe extern "system" fn each(window: HWND, found: LPARAM) -> windows::core::BOOL {
        let found = unsafe { &mut *(found.0 as *mut (u32, usize)) };
        let mut owner = 0;
        unsafe { GetWindowThreadProcessId(window, Some(&mut owner)) };
        if owner == found.0 && unsafe { IsWindowVisible(window) }.as_bool() {
            found.1 += 1;
        }
        true.into()
    }
    let mut found = (pid, 0usize);
    unsafe { EnumWindows(Some(each), LPARAM(&raw mut found as isize)) }.unwrap();
    found.1
}

#[test]
fn the_guard_runs_in_this_session_with_no_window_until_a_normal_stop_ends_it() {
    // Captures in this process share the guard's stop event: one at a time.
    let _live = crate::live_input();
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
        WaitForSingleObject,
    };
    let own = std::process::id();
    let ready = ready_file(own);
    let _ = std::fs::remove_file(&ready);
    let (tx, _rx) = channel();
    let mut capture = WinCapture::new(no_problems()).with_guard(spawn_guard_host);
    capture.start(tx).unwrap();
    let pid = capture.guard().expect("a guard was started").id();
    let access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE;
    let process = unsafe { OpenProcess(access, false, pid) }.unwrap();
    // The stand-in was asked to guard this process: it names its file so.
    let end = Instant::now() + Duration::from_secs(20);
    while !ready.exists() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    let was_ready = ready.exists();
    let session = session_of(pid);
    let windows = visible_windows_of(pid);
    let waiting = unsafe { WaitForSingleObject(process, 0) } == WAIT_TIMEOUT;
    capture.stop();
    let ended = unsafe { WaitForSingleObject(process, 0) } == WAIT_OBJECT_0;
    let mut code = 0;
    unsafe { GetExitCodeProcess(process, &mut code) }.unwrap();
    let _ = unsafe { CloseHandle(process) };
    let _ = std::fs::remove_file(&ready);
    assert!(was_ready, "the guard started for this process");
    assert_eq!(session, session_of(own), "in this session");
    assert_eq!(windows, 0, "with no window");
    assert!(waiting, "the guard waits while the capture runs");
    assert!(ended, "a normal stop ends the guard");
    assert_eq!(code, 0, "and the guard ended well");
}

fn no_guard(_: u32) -> std::io::Result<std::process::Child> {
    Err(std::io::Error::other("refused"))
}

#[test]
fn a_guard_that_cannot_start_is_a_problem_and_the_capture_runs_on() {
    // Captures in this process share the guard's stop event: one at a time.
    let _live = crate::live_input();
    let (ptx, prx) = channel();
    let problems: Problems = Arc::new(move |problem| {
        let _ = ptx.send(problem);
    });
    let (tx, _rx) = channel();
    let mut capture = WinCapture::new(problems).with_guard(no_guard);
    capture.start(tx).unwrap();
    let guard = capture.guard().is_some();
    let running = capture.running().is_ok();
    capture.stop();
    assert!(!guard);
    assert!(running, "the capture runs on");
    assert_eq!(
        prx.try_iter().collect::<Vec<_>>(),
        [PlatformProblem::Raised(
            "Mouser could not start its cursor guard: refused".into()
        )]
    );
}
