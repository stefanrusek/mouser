use crate::cursor_guard::*;
use crate::diag::{DiagLevel, DiagValue, set_diagnostics};
use mouser_platform::Error;
use std::cell::Cell;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The `pointer.shown` lines the guard wrote while `f` ran, with their level.
type Lines = Vec<(DiagLevel, Vec<(&'static str, DiagValue)>)>;

fn guard_notes(f: impl FnOnce()) -> Lines {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let sink = heard.clone();
    let guard = ("reason", DiagValue::Str("guard".into()));
    set_diagnostics(Some(Arc::new(move |level, event, fields| {
        // Other tests may write meanwhile; the guard's line is its own.
        if event == "pointer.shown" && fields.contains(&guard) {
            sink.lock().unwrap().push((level, fields));
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
        code = Some(after_parent(&|| true, &|| {
            restored.set(true);
            Ok(())
        }));
    });
    assert!(restored.get());
    assert_eq!(code, Some(ExitCode::SUCCESS));
    assert_eq!(
        notes,
        [(
            DiagLevel::Info,
            vec![("reason", DiagValue::Str("guard".into()))]
        )]
    );
}

#[test]
fn an_arrow_that_is_not_blank_is_left_alone() {
    let mut code = None;
    let notes = guard_notes(|| {
        code = Some(after_parent(&|| false, &|| panic!("nothing to restore")));
    });
    assert_eq!(code, Some(ExitCode::SUCCESS));
    assert!(notes.is_empty());
}

#[test]
fn cursors_that_cannot_be_restored_are_a_failure_and_not_said_to_be_shown() {
    let mut code = None;
    let notes = guard_notes(|| {
        code = Some(after_parent(&|| true, &|| {
            Err(Error::Os("SystemParametersInfo: refused".into()))
        }));
    });
    assert_eq!(code, Some(ExitCode::FAILURE));
    assert!(notes.is_empty());
}

#[test]
fn a_parent_that_has_already_ended_is_not_waited_for() {
    // Process ids are multiples of four: this one cannot be opened.
    let started = Instant::now();
    assert_eq!(wait_for(u32::MAX), Ended::Parent);
    assert!(started.elapsed() < Duration::from_secs(1));
}

/// A stand-in parent that runs until it is ended.
fn living_parent() -> Parent {
    Parent(
        std::process::Command::new("ping")
            .args(["-n", "60", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    )
}

#[test]
fn a_stop_set_before_the_guard_starts_ends_it_at_once() {
    // The parent makes the event before it starts the guard, so a guard
    // that starts after the stop still sees it.
    let parent = living_parent();
    let stop = StopEvent::create(parent.0.id()).unwrap();
    stop.set();
    let started = Instant::now();
    assert_eq!(wait_for(parent.0.id()), Ended::Stopped);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn a_stop_event_whose_name_another_kind_of_object_has_is_an_error() {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::CreateMutexW;
    // Names no process: only this test uses it.
    let pid = u32::MAX - 2;
    let name = windows::core::HSTRING::from(format!(r"Local\MouserCursorGuard.{pid}"));
    let mutex = unsafe { CreateMutexW(None, false, &name) }.unwrap();
    let made = StopEvent::create(pid);
    let _ = unsafe { CloseHandle(mutex) };
    let Err(Error::Os(text)) = made else {
        panic!("the event was made");
    };
    assert!(text.starts_with("CreateEventW: "), "{text}");
}

#[test]
fn a_parent_that_set_its_stop_and_then_ended_leaves_the_cursors_alone() {
    let _live = crate::live_input();
    let _cursors = Cursors;
    let mut gone = std::process::Command::new("ping")
        .args(["-n", "1", "127.0.0.1"])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pid = gone.id();
    let stop = StopEvent::create(pid).unwrap();
    stop.set();
    gone.wait().unwrap();
    // Not ours to restore: a parent that stopped showed the pointer itself.
    crate::cursor::hide().unwrap();
    crate::cursor::forget_hidden();
    assert_eq!(run(pid), ExitCode::SUCCESS);
    assert!(
        crate::cursor::arrow_is_blank(),
        "the cursors were left alone"
    );
}

#[test]
fn a_stop_while_the_guard_waits_ends_it_and_leaves_the_cursors_alone() {
    let _live = crate::live_input();
    let _cursors = Cursors;
    let parent = living_parent();
    let pid = parent.0.id();
    let stop = StopEvent::create(pid).unwrap();
    crate::cursor::hide().unwrap();
    crate::cursor::forget_hidden();
    let guard = std::thread::spawn(move || run(pid));
    std::thread::sleep(Duration::from_millis(200));
    assert!(!guard.is_finished(), "the guard waits until it is stopped");
    stop.set();
    assert_eq!(guard.join().unwrap(), ExitCode::SUCCESS);
    assert!(
        crate::cursor::arrow_is_blank(),
        "the cursors were left alone"
    );
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
    let mut parent = living_parent();
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
