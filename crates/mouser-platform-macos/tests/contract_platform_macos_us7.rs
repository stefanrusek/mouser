//! File drag (Story 7) against the real macOS backend.
//!
//! These tests START A REAL DRAG and read the real drag pasteboard, and need a
//! person (or injected input) to complete a drop. Run only with the owner's
//! permission and nothing else using the mouse.

use mouser_platform::Platform;
use mouser_platform_macos::MacPlatform;
use std::sync::mpsc::channel;
use std::time::Duration;

#[test]
fn outgoing_is_none_when_no_drag_is_in_progress() {
    let mut drag = MacPlatform::new().file_drag().expect("file drag supported");
    assert_eq!(drag.outgoing(), None);
    assert_eq!(drag.outgoing(), None);
}

/// Needs the main thread free to run the drag, so the drag is requested from a
/// worker while this test thread only waits. Drop the promised file on a Finder
/// window (or a Desktop folder) within 30 seconds, holding the primary button.
#[test]
fn begin_incoming_reports_the_folder_the_files_were_dropped_in() {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut drag = MacPlatform::new().file_drag().unwrap();
        tx.send(drag.begin_incoming(vec!["mouser-drag-test.txt".into()]))
            .unwrap();
    });
    let target = rx
        .recv_timeout(Duration::from_secs(30))
        .expect("no drop within 30 s")
        .expect("drag started");
    assert!(target.0.is_dir(), "{:?} is not a folder", target.0);
    let placeholder = target.0.join("mouser-drag-test.txt");
    assert!(placeholder.exists());
    std::fs::remove_file(placeholder).unwrap();
}
