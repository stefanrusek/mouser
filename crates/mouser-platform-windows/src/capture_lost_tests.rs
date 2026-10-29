use crate::capture::{Shared, WinCapture};
use crate::capture_lost::*;
use crate::diag::{DiagLevel, DiagValue, heard_during};
use crate::inject::{MOUSER_TAG, key_input, send};
use crate::watchdog::{Skip, Surroundings};
use mouser_platform::{CaptureLossReason, ForegroundApp, InputCapture, LocalInput};
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, SetForegroundWindow, WINDOW_EX_STYLE, WS_POPUP, WS_VISIBLE,
};
use windows::core::w;

fn shared(grabbed: bool) -> Shared {
    Shared {
        grabbed: AtomicBool::new(grabbed),
        ..Shared::default()
    }
}

/// The `capture.lost` lines written while `f` ran about `foreground`,
/// which marks this test's own (other tests' captures may write meanwhile).
fn lost_lines(foreground: &str, f: impl FnOnce()) -> Vec<Vec<(&'static str, DiagValue)>> {
    let ours = ("foreground", DiagValue::Str(foreground.into()));
    heard_during(f)
        .into_iter()
        .filter(|(level, event, fields)| {
            *level == DiagLevel::Warn && event == "capture.lost" && fields.contains(&ours)
        })
        .map(|(_, _, fields)| fields)
        .collect()
}

fn app(outranks_mouser: bool) -> Option<ForegroundApp> {
    Some(ForegroundApp {
        name: "app.exe".into(),
        admin: outranks_mouser,
        outranks_mouser,
    })
}

#[test]
fn losing_a_grabbed_capture_shows_the_pointer_before_core_is_told() {
    let shared = shared(true);
    let done = RefCell::new(Vec::new());
    let mut lost = false;
    let lines = lost_lines("first.exe", || {
        lost = lose(
            &shared,
            CaptureLossReason::HooksSilent,
            Some("first.exe".into()),
            || done.borrow_mut().push("pointer shown".to_string()),
            |input| done.borrow_mut().push(format!("{input:?}")),
        );
    });
    assert!(lost);
    // Logged with the program in front, by name only.
    assert_eq!(
        lines,
        [vec![
            ("reason", DiagValue::Str("hooks_silent".into())),
            ("foreground", DiagValue::Str("first.exe".into()))
        ]]
    );
    assert!(!shared.grabbed.load(SeqCst), "ungrabbed");
    let told = format!(
        "{:?}",
        LocalInput::CaptureLost(CaptureLossReason::HooksSilent)
    );
    assert_eq!(*done.borrow(), ["pointer shown".to_string(), told]);
}

#[test]
fn a_capture_that_is_not_grabbed_has_nothing_to_lose() {
    let shared = shared(false);
    let mut lost = true;
    let lines = lost_lines("second.exe", || {
        lost = lose(
            &shared,
            CaptureLossReason::OtherDesktop,
            Some("second.exe".into()),
            || panic!("the pointer is already shown"),
            |_| panic!("core is not told"),
        );
    });
    assert!(!lost);
    assert!(lines.is_empty(), "nothing was lost, nothing is logged");
}

#[test]
fn another_desktop_or_an_app_above_mouser_takes_the_capture_away() {
    use CaptureLossReason::*;
    assert_eq!(reason_now(app(false), true), Some(OtherDesktop));
    assert_eq!(reason_now(app(true), true), Some(OtherDesktop));
    assert_eq!(reason_now(app(true), false), Some(ElevatedForeground));
    assert_eq!(reason_now(app(false), false), None);
    assert_eq!(reason_now(None, false), None);
}

#[test]
fn a_foreground_change_to_an_app_above_mouser_takes_the_capture_away_at_once() {
    let lost = RefCell::new(Vec::new());
    let lose = |reason, foreground| lost.borrow_mut().push((reason, foreground));
    on_foreground(app(false), &lose);
    on_foreground(None, &lose);
    on_foreground(app(true), &lose);
    let above = (
        CaptureLossReason::ElevatedForeground,
        Some("app.exe".into()),
    );
    assert_eq!(*lost.borrow(), [above]);
}

/// A PC on which another desktop has the input.
struct OtherDesktopHasTheInput;

impl Surroundings for OtherDesktopHasTheInput {
    fn skip(&mut self) -> Option<Skip> {
        Some(Skip::OtherDesktop)
    }

    fn lost(&mut self) -> Option<CaptureLossReason> {
        Some(CaptureLossReason::OtherDesktop)
    }

    fn probed(&mut self) {}

    fn foreground(&self) -> String {
        "LogonUI.exe".into()
    }

    fn input_tick(&self) -> u32 {
        0
    }
}

fn other_desktop_has_the_input() -> Box<dyn Surroundings + Send> {
    Box::new(OtherDesktopHasTheInput)
}

#[test]
fn the_real_capture_lost_while_grabbed_shows_the_pointer_before_core_is_told() {
    let _live = crate::live_input();
    let mut capture = WinCapture::with_watchdog(
        Arc::new(|_| {}),
        Duration::from_millis(50),
        other_desktop_has_the_input,
    );
    let (mut told, mut shown, mut grabbed) = (None, false, true);
    let lines = lost_lines("LogonUI.exe", || {
        let (tx, rx) = channel();
        capture.start(tx).unwrap();
        capture.set_grabbed(true).unwrap();
        // The watchdog's next cycle finds the other desktop.
        let end = Instant::now() + Duration::from_secs(5);
        told = loop {
            let left = end.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(LocalInput::CaptureLost(reason)) => break Some(reason),
                Ok(_) => {}
                Err(_) => break None,
            }
        };
        // By the time core hears of it the pointer is back (rule 10).
        shown = !crate::cursor::arrow_is_blank();
        grabbed = capture.running().unwrap().shared.grabbed.load(SeqCst);
        capture.stop();
    });
    assert_eq!(told, Some(CaptureLossReason::OtherDesktop));
    assert!(shown, "the pointer is shown");
    assert!(!grabbed, "ungrabbed");
    // Logged once, with the program in front by name only.
    assert_eq!(
        lines,
        [vec![
            ("reason", DiagValue::Str("other_desktop".into())),
            ("foreground", DiagValue::Str("LogonUI.exe".into()))
        ]]
    );
}

/// A PC on which nothing takes the capture away.
struct NothingLost;

impl Surroundings for NothingLost {
    fn skip(&mut self) -> Option<Skip> {
        None
    }

    fn lost(&mut self) -> Option<CaptureLossReason> {
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

fn nothing_lost() -> Box<dyn Surroundings + Send> {
    Box::new(NothingLost)
}

#[test]
fn the_real_capture_keeps_its_grab_when_an_app_not_above_mouser_comes_to_the_front() {
    let _live = crate::live_input();
    let own = std::env::current_exe().unwrap();
    let own = own.file_name().unwrap().to_string_lossy().into_owned();
    // The foreground changes the capture's hook heard of.
    let (heard_tx, heard) = channel();
    crate::capture_lost::heard::listen(Some(heard_tx));
    let mut capture =
        WinCapture::with_watchdog(Arc::new(|_| {}), Duration::from_millis(50), nothing_lost);
    let (tx, rx) = channel();
    capture.start(tx).unwrap();
    capture.set_grabbed(true).unwrap();
    let window = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("capture foreground test"),
            WS_POPUP | WS_VISIBLE,
            0,
            0,
            40,
            40,
            None,
            None,
            None,
            None,
        )
    }
    .unwrap();
    // Windows lets a program bring its window to the front just after it
    // sent input; a tap of Alt is the least a press can do.
    send(&[
        key_input(0x38, false, true, MOUSER_TAG),
        key_input(0x38, false, false, MOUSER_TAG),
    ])
    .unwrap();
    let _ = unsafe { SetForegroundWindow(window) };
    let ours = (own, false);
    let end = Instant::now() + Duration::from_secs(5);
    let heard_ours = loop {
        let left = end.saturating_duration_since(Instant::now());
        match heard.recv_timeout(left) {
            Ok(change) if change == ours => break true,
            Ok(_) => {}
            Err(_) => break false,
        }
    };
    let grabbed = capture.running().unwrap().shared.grabbed.load(SeqCst);
    capture.stop();
    crate::capture_lost::heard::listen(None);
    let _ = unsafe { DestroyWindow(window) };
    let lost = rx
        .try_iter()
        .any(|input| matches!(input, LocalInput::CaptureLost(_)));
    assert!(
        heard_ours,
        "the hook heard the test's window come to the front"
    );
    assert!(grabbed, "still grabbed");
    assert!(!lost, "core is not told the capture was lost");
}
