//! File drag (Story 7) against the real macOS backend.
//!
//! These tests START REAL DRAGS, inject REAL mouse events and open a Finder
//! window, so run them only with the owner's permission and nothing else using
//! the mouse. There is no libtest harness: a drag can only run while the main
//! thread runs AppKit's event loop, so `main` is that loop and the scenarios
//! run on a worker thread that drives the mouse. It exits non-zero with a
//! message when a scenario fails or hangs.

use core_graphics::display::CGDisplay;
use core_graphics::display::CGPoint;
use core_graphics::event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use dispatch2::DispatchQueue;
use mouser_platform::Platform;
use mouser_platform_macos::MacPlatform;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventType, NSPasteboard,
    NSPasteboardNameDrag,
};
use objc2_app_kit::{NSBackingStoreType, NSFloatingWindowLevel, NSWindow, NSWindowStyleMask};
use objc2_foundation::{NSArray, NSPoint, NSRect, NSSize, NSString, NSURL};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

fn outgoing_is_none_when_no_drag_is_in_progress() {
    let mut drag = MacPlatform::new().file_drag().expect("file drag supported");
    assert_eq!(drag.outgoing(), None);
    assert_eq!(drag.outgoing(), None);
}

fn source() -> CGEventSource {
    CGEventSource::new(CGEventSourceStateID::HIDSystemState).expect("event source")
}

fn cursor() -> CGPoint {
    CGEvent::new(source()).expect("event").location()
}

fn post(kind: CGEventType, at: CGPoint) {
    CGEvent::new_mouse_event(source(), kind, at, CGMouseButton::Left)
        .expect("mouse event")
        .post(CGEventTapLocation::HID);
}

/// Moves the cursor in small steps, with the primary button held.
fn drag_to(from: CGPoint, to: CGPoint) {
    let steps = 40;
    for i in 1..=steps {
        let t = f64::from(i) / f64::from(steps);
        let at = CGPoint::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        post(CGEventType::LeftMouseDragged, at);
        std::thread::sleep(Duration::from_millis(15));
    }
    // Hover a moment so the destination accepts the drag.
    for i in 0..50 {
        post(
            CGEventType::LeftMouseDragged,
            CGPoint::new(to.x + f64::from(i % 2), to.y),
        );
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// A small window of the test's own, where the button goes down: the window
/// server sends a drag's events to the window that took the press, and the
/// drag session only advances if that is one of ours (not Finder's).
struct Pad(Retained<NSWindow>);

// The window is only touched from the main queue.
unsafe impl Send for Pad {}

impl Pad {
    /// `at` is the pad's centre in global top-left coordinates.
    fn show(at: CGPoint) -> Pad {
        let (tx, rx) = channel();
        DispatchQueue::main().exec_async(move || {
            let mtm = MainThreadMarker::new().unwrap();
            let height = CGDisplay::main().bounds().size.height;
            let frame = NSRect::new(
                NSPoint::new(at.x - 40.0, height - at.y - 40.0),
                NSSize::new(80.0, 80.0),
            );
            let window = unsafe {
                NSWindow::initWithContentRect_styleMask_backing_defer(
                    NSWindow::alloc(mtm),
                    frame,
                    NSWindowStyleMask::Borderless,
                    NSBackingStoreType::Buffered,
                    false,
                )
            };
            unsafe { window.setReleasedWhenClosed(false) };
            window.setLevel(NSFloatingWindowLevel);
            window.orderFrontRegardless();
            tx.send(Pad(window)).unwrap();
        });
        let pad = rx.recv().unwrap();
        std::thread::sleep(Duration::from_millis(500));
        pad
    }
}

impl Drop for Pad {
    fn drop(&mut self) {
        let window = Pad(self.0.clone());
        DispatchQueue::main().exec_async(move || window.0.close());
    }
}

fn osascript(script: &str) -> Option<String> {
    let out = Command::new("osascript")
        .args(["-e", script])
        .output()
        .expect("run osascript");
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// A temp folder shown in a Finder window; the window is closed and the folder
/// removed on drop, even when an assertion fails.
struct Shown {
    dir: PathBuf,
    name: String,
    /// Window centre in global top-left coordinates.
    centre: CGPoint,
}

impl Shown {
    fn open(tag: &str) -> Shown {
        let name = format!("mouser_us7_{tag}_{}", std::process::id());
        let dir = std::env::temp_dir().join(&name);
        std::fs::create_dir_all(&dir).unwrap();
        SHOWN.lock().unwrap().push((name.clone(), dir.clone()));
        assert!(Command::new("open").arg(&dir).status().unwrap().success());
        let script = format!(
            "tell application \"Finder\" to get bounds of (first window whose name is \"{name}\")"
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        let bounds = loop {
            if let Some(b) = osascript(&script) {
                break b;
            }
            assert!(Instant::now() < deadline, "Finder window never appeared");
            std::thread::sleep(Duration::from_millis(300));
        };
        let n: Vec<f64> = bounds
            .split(", ")
            .map(|s| s.parse().expect("bounds number"))
            .collect();
        // Bring it to the front so nothing covers it, and let it settle.
        osascript("tell application \"Finder\" to activate");
        std::thread::sleep(Duration::from_secs(1));
        Shown {
            dir,
            name,
            centre: CGPoint::new((n[0] + n[2]) / 2.0, (n[1] + n[3]) / 2.0),
        }
    }
}

/// Folders whose Finder window is currently open, so the watchdog can clean up
/// too (it exits the process, which skips destructors).
static SHOWN: std::sync::Mutex<Vec<(String, PathBuf)>> = std::sync::Mutex::new(Vec::new());

/// Closes every Finder window named `name` (unique per process and scenario, so
/// it is only ever ours), retrying until none is left, then removes the folder.
/// The window goes first: removing the folder under an open window would make
/// Finder fall back to showing the parent ($TMPDIR, titled "T") instead.
fn close_shown(name: &str, dir: &Path) {
    let count = format!("count of (every window whose name is \"{name}\")");
    let close = format!(
        "tell application \"Finder\"\nif ({count}) > 0 then close (first window whose name is \"{name}\")\nreturn {count}\nend tell"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while osascript(&close).as_deref() != Some("0") && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    std::fs::remove_dir_all(dir).ok();
}

impl Drop for Shown {
    fn drop(&mut self) {
        SHOWN.lock().unwrap().retain(|(n, _)| *n != self.name);
        close_shown(&self.name, &self.dir);
    }
}

/// Presses the button at `from`, starts the incoming drag, moves to `to` and
/// releases. Returns what `begin_incoming` returned.
fn drag_scenario(
    from: CGPoint,
    to: CGPoint,
) -> mouser_platform::Result<mouser_platform::DropTarget> {
    let board = NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameDrag });
    let before = board.changeCount();
    let _pad = Pad::show(from);
    post(CGEventType::MouseMoved, from);
    std::thread::sleep(Duration::from_millis(300));
    post(CGEventType::LeftMouseDown, from);
    std::thread::sleep(Duration::from_millis(200));
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut drag = MacPlatform::new().file_drag().unwrap();
        tx.send(drag.begin_incoming(vec!["mouser-drag-test.txt".into()]))
            .unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while board.changeCount() == before {
        assert!(Instant::now() < deadline, "the drag never started");
        std::thread::sleep(Duration::from_millis(50));
    }
    std::thread::sleep(Duration::from_millis(500));
    // A watcher that has not seen the pasteboard idle reads it mid-drag; a
    // drag of promises carries no file URLs, so there is nothing to report.
    let mut watcher = MacPlatform::new().file_drag().unwrap();
    assert_eq!(watcher.outgoing(), None);
    drag_to(from, to);
    post(CGEventType::LeftMouseUp, to);
    wait(&rx)
}

fn wait<T>(rx: &Receiver<T>) -> T {
    rx.recv_timeout(Duration::from_secs(20))
        .expect("no result within 20 s of releasing")
}

fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap()
}

fn begin_incoming_reports_the_folder_the_files_were_dropped_in() {
    let shown = Shown::open("drop");
    let start = CGPoint::new(shown.centre.x - 60.0, shown.centre.y);
    let target = drag_scenario(start, shown.centre).expect("drag started and dropped");
    assert_eq!(canonical(&target.0), canonical(&shown.dir));
    let placeholder = target.0.join("mouser-drag-test.txt");
    assert!(placeholder.exists(), "Finder did not write the promise");
    std::fs::remove_file(placeholder).unwrap();
}

fn outgoing_reports_the_files_on_the_drag_pasteboard_while_the_button_is_down() {
    let dir = std::env::temp_dir().join(format!("mouser_us7_out_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("dragged.txt");
    std::fs::write(&file, b"x").unwrap();
    let at = cursor();
    let _pad = Pad::show(at);
    let mut watcher = MacPlatform::new().file_drag().unwrap();
    assert_eq!(watcher.outgoing(), None, "button is up");
    post(CGEventType::LeftMouseDown, at);
    std::thread::sleep(Duration::from_millis(300));
    let board = NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameDrag });
    board.clearContents();
    let url = NSURL::fileURLWithPath(&NSString::from_str(file.to_str().unwrap()));
    assert!(board.writeObjects(&NSArray::from_retained_slice(&[
        ProtocolObject::from_retained(url)
    ])));
    let reported = watcher.outgoing();
    post(CGEventType::LeftMouseUp, at);
    board.clearContents();
    assert_eq!(
        reported.map(|f| f.iter().map(|p| canonical(p)).collect::<Vec<_>>()),
        Some(vec![canonical(&file)])
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

fn a_drag_released_over_the_menu_bar_is_cancelled() {
    let shown = Shown::open("cancel");
    let start = CGPoint::new(shown.centre.x - 60.0, shown.centre.y);
    let result = drag_scenario(start, CGPoint::new(shown.centre.x, 4.0));
    assert!(result.is_err(), "a refused drop must cancel the drag");
    assert!(!shown.dir.join("mouser-drag-test.txt").exists());
}

fn scenarios() {
    let home = cursor();
    let run = || {
        begin_incoming_reports_the_folder_the_files_were_dropped_in();
        a_drag_released_over_the_menu_bar_is_cancelled();
        outgoing_reports_the_files_on_the_drag_pasteboard_while_the_button_is_down();
    };
    let outcome = std::panic::catch_unwind(run);
    // Release the button and put the pointer back whatever happened.
    post(CGEventType::LeftMouseUp, cursor());
    post(CGEventType::MouseMoved, home);
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

fn main() {
    outgoing_is_none_when_no_drag_is_in_progress();
    println!("ok outgoing_is_none_when_no_drag_is_in_progress");

    let mtm = MainThreadMarker::new().expect("main runs on the main thread");
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(150));
        eprintln!("FAILED: the drag scenarios did not finish within 150 s");
        let open: Vec<_> = SHOWN.lock().unwrap().drain(..).collect();
        for (name, dir) in open {
            close_shown(&name, &dir);
        }
        std::process::exit(1);
    });

    let (done, finished) = channel();
    std::thread::spawn(move || {
        let outcome = std::thread::spawn(scenarios).join();
        done.send(outcome.is_ok()).unwrap();
        DispatchQueue::main().exec_async(|| {
            let mtm = MainThreadMarker::new().unwrap();
            let app = NSApplication::sharedApplication(mtm);
            app.stop(None);
            // stop() only takes effect once another event has been handled.
            let wake = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                NSEventType::ApplicationDefined,
                NSPoint::new(0.0, 0.0),
                objc2_app_kit::NSEventModifierFlags::empty(),
                0.0,
                0,
                None,
                0,
                0,
                0,
            )
            .expect("wake event");
            app.postEvent_atStart(&wake, true);
        });
    });
    app.run();
    if finished.recv().unwrap() {
        println!("ok begin_incoming_reports_the_folder_the_files_were_dropped_in");
        println!("ok a_drag_released_over_the_menu_bar_is_cancelled");
        println!("ok outgoing_reports_the_files_on_the_drag_pasteboard_while_the_button_is_down");
    } else {
        eprintln!("FAILED: a drag scenario panicked (message above)");
        std::process::exit(1);
    }
}
