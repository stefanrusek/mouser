use crate::capture::{Shared, WinCapture};
use crate::capture_lost::*;
use crate::watchdog::{Skip, Surroundings};
use mouser_platform::{CaptureLossReason, ForegroundApp, InputCapture, LocalInput};
use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::SeqCst};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

fn shared(grabbed: bool) -> Shared {
    Shared {
        grabbed: AtomicBool::new(grabbed),
        seen_mouse: AtomicU32::new(0),
        seen_key: AtomicU32::new(0),
    }
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
    let lost = lose(
        &shared,
        CaptureLossReason::HooksSilent,
        || done.borrow_mut().push("pointer shown".to_string()),
        |input| done.borrow_mut().push(format!("{input:?}")),
    );
    assert!(lost);
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
    let lost = lose(
        &shared,
        CaptureLossReason::OtherDesktop,
        || panic!("the pointer is already shown"),
        |_| panic!("core is not told"),
    );
    assert!(!lost);
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
    let lose = |reason| lost.borrow_mut().push(reason);
    on_foreground(app(false), &lose);
    on_foreground(None, &lose);
    on_foreground(app(true), &lose);
    assert_eq!(*lost.borrow(), [CaptureLossReason::ElevatedForeground]);
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
    let (tx, rx) = channel();
    capture.start(tx).unwrap();
    capture.set_grabbed(true).unwrap();
    // The watchdog's next cycle finds the other desktop.
    let end = Instant::now() + Duration::from_secs(5);
    let told = loop {
        let left = end.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(LocalInput::CaptureLost(reason)) => break Some(reason),
            Ok(_) => {}
            Err(_) => break None,
        }
    };
    // By the time core hears of it the pointer is back (rule 10).
    let shown = !crate::cursor::arrow_is_blank();
    let grabbed = capture.running().unwrap().shared.grabbed.load(SeqCst);
    capture.stop();
    assert_eq!(told, Some(CaptureLossReason::OtherDesktop));
    assert!(shown, "the pointer is shown");
    assert!(!grabbed, "ungrabbed");
}
