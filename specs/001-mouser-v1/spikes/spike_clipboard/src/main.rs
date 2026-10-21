//! Throwaway probe: clipboard format round-trips with arboard and clipboard-rs on this OS.
//! Writes test content, reads it back, then restores the original plain text.
use std::borrow::Cow;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::Duration;

fn report(name: &str, ok: bool, detail: impl std::fmt::Display) {
    println!("{:<44} {} {}", name, if ok { "OK  " } else { "FAIL" }, detail);
}

fn main() {
    let tmp = std::env::temp_dir().join("mouser_spike_clip.txt");
    std::fs::write(&tmp, b"x").unwrap();
    let tmp_s = tmp.to_string_lossy().to_string();

    let mut ab = arboard::Clipboard::new().expect("arboard init");
    let original = ab.get_text().ok();
    println!("original clipboard text saved: {} chars", original.as_ref().map(|s| s.len()).unwrap_or(0));

    // ---------- arboard 3.7.0 ----------
    ab.set_text("mouser-arboard-text").unwrap();
    let t = ab.get_text();
    report("arboard text", t.as_deref().ok() == Some("mouser-arboard-text"), format!("{t:?}"));

    ab.set_html("<b>mouser-arboard-html</b>", Some("alt-text")).unwrap();
    let h = ab.get().html();
    let alt = ab.get_text();
    report("arboard html (get().html)", h.as_ref().map(|s| s.contains("mouser-arboard-html")).unwrap_or(false), format!("{h:?}"));
    report("arboard html alt-text as plain", alt.as_deref().ok() == Some("alt-text"), format!("{alt:?}"));

    let px: Vec<u8> = (0..16 * 16).flat_map(|i| [(i % 256) as u8, 0, 255, 255]).collect();
    ab.set_image(arboard::ImageData { width: 16, height: 16, bytes: Cow::Borrowed(&px) }).unwrap();
    let img = ab.get_image();
    report("arboard image", img.as_ref().map(|i| i.width == 16 && i.height == 16).unwrap_or(false),
        img.as_ref().map(|i| format!("{}x{} {} bytes", i.width, i.height, i.bytes.len())).unwrap_or_else(|e| e.to_string()));

    let r = ab.set().file_list(&[&tmp]);
    let fl = ab.get().file_list();
    report("arboard file_list", fl.as_ref().map(|v| v.iter().any(|p| p.ends_with("mouser_spike_clip.txt"))).unwrap_or(false), format!("set={r:?} get={fl:?}"));
    println!("arboard RTF: no API (only text/html/image/file_list); no change-watch API");

    // ---------- clipboard-rs 0.3.5 ----------
    use clipboard_rs::{Clipboard, ClipboardContent, ClipboardContext, ClipboardHandler, ClipboardWatcher,
        ClipboardWatcherContext, ContentFormat, common::RustImage, RustImageData};
    let cr = ClipboardContext::new().expect("clipboard-rs init");
    cr.set_text("mouser-crs-text".into()).unwrap();
    report("clipboard-rs text", cr.get_text().ok().as_deref() == Some("mouser-crs-text"), format!("{:?}", cr.get_text()));
    cr.set_html("<i>mouser-crs-html</i>".into()).unwrap();
    report("clipboard-rs html", cr.get_html().map(|s| s.contains("mouser-crs-html")).unwrap_or(false), format!("{:?}", cr.get_html()));
    cr.set_rich_text(r"{\rtf1\ansi mouser-crs-rtf}".into()).unwrap();
    report("clipboard-rs rtf", cr.get_rich_text().map(|s| s.contains("mouser-crs-rtf")).unwrap_or(false), format!("{:?}", cr.get_rich_text()));
    let im = RustImageData::from_bytes(include_bytes!("test.png")).unwrap();
    cr.set_image(im).unwrap();
    let gi = cr.get_image();
    report("clipboard-rs image", gi.as_ref().map(|i| i.get_size() == (32, 32)).unwrap_or(false), format!("{:?}", gi.as_ref().map(|i| i.get_size()).map_err(|e| e.to_string())));
    cr.set_files(vec![tmp_s.clone()]).unwrap();
    let gf = cr.get_files();
    report("clipboard-rs files", gf.as_ref().map(|v| v.iter().any(|p| p.contains("mouser_spike_clip.txt"))).unwrap_or(false), format!("{gf:?}"));
    // Multi-format atomic set
    cr.set(vec![ClipboardContent::Text("multi-plain".into()), ClipboardContent::Html("<b>multi</b>".into()),
        ClipboardContent::Rtf(r"{\rtf1 multi}".into())]).unwrap();
    report("clipboard-rs multi-format set()", cr.has(ContentFormat::Text) && cr.has(ContentFormat::Html) && cr.has(ContentFormat::Rtf),
        format!("formats={:?}", cr.available_formats().unwrap_or_default()));

    // Watcher
    struct H(Arc<AtomicUsize>);
    impl ClipboardHandler for H { fn on_clipboard_change(&mut self) { self.0.fetch_add(1, Ordering::SeqCst); } }
    let hits = Arc::new(AtomicUsize::new(0));
    let mut w = ClipboardWatcherContext::new_with_interval(Duration::from_millis(100)).unwrap();
    w.add_handler(H(hits.clone()));
    let stop = w.get_shutdown_channel();
    let th = std::thread::spawn(move || w.start_watch());
    std::thread::sleep(Duration::from_millis(300));
    let c2 = ClipboardContext::new().unwrap();
    c2.set_text("watch-1".into()).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    c2.set_text("watch-2".into()).unwrap();
    std::thread::sleep(Duration::from_millis(400));
    stop.stop();
    th.join().unwrap();
    report("clipboard-rs watcher (changeCount poll)", hits.load(Ordering::SeqCst) >= 2, format!("callbacks={}", hits.load(Ordering::SeqCst)));

    // Restore
    let mut ab = arboard::Clipboard::new().unwrap();
    match original { Some(s) => { ab.set_text(s).unwrap(); println!("restored original text"); } None => { ab.clear().ok(); println!("original had no text; cleared"); } }
    let _ = std::fs::remove_file(tmp);
}
