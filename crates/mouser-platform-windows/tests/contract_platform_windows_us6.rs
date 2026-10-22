//! Conformance rule 5 (clipboard) against the real Windows clipboard, and the
//! Files representation the rule does not cover.

#![cfg(windows)]

use mouser_platform::conformance::rule_5_clipboard;
use mouser_platform::{ClipRep, Platform};
use mouser_platform_windows::WinPlatform;
use std::sync::Mutex;

/// The system clipboard is one shared resource.
static CLIPBOARD: Mutex<()> = Mutex::new(());

fn clipboard() -> std::sync::MutexGuard<'static, ()> {
    CLIPBOARD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
fn rule_5_text_html_rtf_and_png_round_trip() {
    let _guard = clipboard();
    rule_5_clipboard(&WinPlatform::new());
}

#[test]
fn files_round_trip_as_paths() {
    let _guard = clipboard();
    let mut clip = WinPlatform::new().clipboard().unwrap();
    let before = clip.change_count();
    let files = vec![
        std::path::PathBuf::from(r"C:\Windows\notepad.exe"),
        std::path::PathBuf::from(r"C:\Windows\System32\cmd.exe"),
    ];
    clip.write(vec![ClipRep::Files(files.clone())]).unwrap();
    assert!(clip.change_count() > before);
    assert_eq!(clip.read().unwrap(), [ClipRep::Files(files)]);
}

#[test]
fn each_call_gets_its_own_handle_onto_the_same_clipboard() {
    let _guard = clipboard();
    let platform = WinPlatform::new();
    let mut writer = platform.clipboard().unwrap();
    writer.write(vec![ClipRep::Text("shared".into())]).unwrap();
    let mut reader = platform.clipboard().unwrap();
    assert_eq!(reader.read().unwrap(), [ClipRep::Text("shared".into())]);
}
