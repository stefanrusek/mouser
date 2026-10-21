//! Conformance rule 5 (clipboard write/read) against the real macOS backend.
//!
//! These tests REPLACE THE REAL GENERAL PASTEBOARD. Run only with the owner's
//! permission. The original contents (every type, as raw bytes) are saved first
//! and put back afterwards, even if an assertion fails.

use clipboard_rs::{Clipboard as _, ClipboardContent, ClipboardContext};
use mouser_platform::conformance as rules;
use mouser_platform::{ClipRep, Platform};
use mouser_platform_macos::MacPlatform;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// The pasteboard as it was: every type with its bytes.
struct Saved(Vec<(String, Vec<u8>)>);

impl Saved {
    fn take() -> Saved {
        let ctx = ClipboardContext::new().unwrap();
        let types = ctx.available_formats().unwrap_or_default();
        Saved(
            types
                .into_iter()
                .filter_map(|t| ctx.get_buffer(&t).ok().map(|b| (t, b)))
                .collect(),
        )
    }
}

impl Drop for Saved {
    fn drop(&mut self) {
        let ctx = ClipboardContext::new().unwrap();
        let contents: Vec<ClipboardContent> = std::mem::take(&mut self.0)
            .into_iter()
            .map(|(t, b)| ClipboardContent::Other(t, b))
            .collect();
        if contents.is_empty() {
            ctx.clear().unwrap();
        } else {
            ctx.set(contents).unwrap();
        }
    }
}

#[test]
fn rule_5_clipboard() {
    let _serial = serial();
    let _saved = Saved::take();
    rules::rule_5_clipboard(&MacPlatform::new());
}

#[test]
fn files_round_trip_and_change_count_advances() {
    let _serial = serial();
    let _saved = Saved::take();
    let dir = std::env::temp_dir().join("mouser_us6_files");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("one.txt");
    std::fs::write(&file, b"x").unwrap();
    let mut clipboard = MacPlatform::new().clipboard().unwrap();
    let before = clipboard.change_count();
    clipboard
        .write(vec![
            ClipRep::Text("one.txt".into()),
            ClipRep::Files(vec![file.clone()]),
        ])
        .unwrap();
    assert!(clipboard.change_count() > before);
    let read = clipboard.read().unwrap();
    let files = read
        .iter()
        .find_map(|r| match r {
            ClipRep::Files(f) => Some(f.clone()),
            _ => None,
        })
        .expect("files read back");
    // The pasteboard reports /var where the temp dir may say /private/var.
    let expected: PathBuf = file.strip_prefix("/private").unwrap_or(&file).into();
    assert_eq!(files, [expected]);
    assert!(read.contains(&ClipRep::Text("one.txt".into())));
    std::fs::remove_dir_all(dir).unwrap();
}
