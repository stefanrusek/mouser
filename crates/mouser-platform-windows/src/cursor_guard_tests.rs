use crate::cursor_guard::*;
use crate::diag::set_diagnostics;
use mouser_platform::Error;
use std::cell::Cell;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The `pointer.shown` lines the guard wrote while `f` ran.
fn guard_notes(f: impl FnOnce()) -> Vec<Vec<(&'static str, String)>> {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = heard.clone();
    set_diagnostics(Some(Arc::new(move |event, fields| {
        // Other tests may write meanwhile; the guard's line is its own.
        if event == "pointer.shown" && fields.contains(&("reason", "guard".to_string())) {
            sink.lock().unwrap().push(fields);
        }
    })));
    f();
    set_diagnostics(None);
    std::mem::take(&mut *heard.lock().unwrap())
}

#[test]
fn a_blank_arrow_left_behind_is_restored_and_said() {
    let restored = Cell::new(false);
    let mut code = None;
    let notes = guard_notes(|| {
        code = Some(after_parent(
            || true,
            || {
                restored.set(true);
                Ok(())
            },
        ));
    });
    assert!(restored.get());
    assert_eq!(code, Some(ExitCode::SUCCESS));
    assert_eq!(notes, [vec![("reason", "guard".to_string())]]);
}

#[test]
fn an_arrow_that_is_not_blank_is_left_alone() {
    let mut code = None;
    let notes = guard_notes(|| {
        code = Some(after_parent(|| false, || panic!("nothing to restore")));
    });
    assert_eq!(code, Some(ExitCode::SUCCESS));
    assert!(notes.is_empty());
}

#[test]
fn cursors_that_cannot_be_restored_are_a_failure_and_not_said_to_be_shown() {
    let mut code = None;
    let notes = guard_notes(|| {
        code = Some(after_parent(
            || true,
            || Err(Error::Os("SystemParametersInfo: refused".into())),
        ));
    });
    assert_eq!(code, Some(ExitCode::FAILURE));
    assert!(notes.is_empty());
}

#[test]
fn a_parent_that_has_already_ended_is_not_waited_for() {
    // Process ids are multiples of four: this one cannot be opened.
    let started = Instant::now();
    wait_for(u32::MAX);
    assert!(started.elapsed() < Duration::from_secs(1));
}

/// A stand-in for Mouser: a process that runs until it is ended. Dropped,
/// it is ended and waited for.
struct Parent(std::process::Child);

impl Drop for Parent {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Puts the user's cursors back when dropped, whatever the test did.
struct Cursors;

impl Drop for Cursors {
    fn drop(&mut self) {
        let _ = crate::cursor::recover();
    }
}

#[test]
fn the_guard_puts_the_pointer_back_after_its_parent_is_ended() {
    let _live = crate::live_input();
    let _cursors = Cursors;
    let mut parent = Parent(
        std::process::Command::new("ping")
            .args(["-n", "60", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    // The parent hid the pointer and died without showing it again.
    crate::cursor::hide().unwrap();
    crate::cursor::forget_hidden();
    assert!(crate::cursor::arrow_is_blank());
    let pid = parent.0.id();
    let guard = std::thread::spawn(move || run(pid));
    std::thread::sleep(Duration::from_millis(200));
    assert!(!guard.is_finished(), "the guard waits for its parent");
    parent.0.kill().unwrap();
    let code = guard.join().unwrap();
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(!crate::cursor::arrow_is_blank(), "the pointer is back");
}
