use crate::clipboard::*;
use mouser_platform::{ClipRep, Clipboard};
use std::path::PathBuf;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND, LPARAM, WPARAM};
use windows::Win32::System::DataExchange::{
    EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardOwner, SetClipboardData,
};
use windows::Win32::System::Memory::GlobalSize;
use windows::Win32::UI::WindowsAndMessaging::{
    SendMessageW, WM_DESTROYCLIPBOARD, WM_RENDERALLFORMATS, WM_RENDERFORMAT,
};

const CF_UNICODE: u32 = 13;
const CF_DIBV5: u32 = 17;
const CF_HDROP: u32 = 15;

/// The system clipboard is one shared resource, so tests that touch it take
/// turns.
fn clip() -> std::sync::MutexGuard<'static, ()> {
    static CLIP: std::sync::Mutex<()> = std::sync::Mutex::new(());
    CLIP.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn png_of(color: png::ColorType, depth: png::BitDepth, w: u32, h: u32, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(color);
    encoder.set_depth(depth);
    if color == png::ColorType::Indexed {
        encoder.set_palette(vec![10, 20, 30, 200, 100, 50]);
    }
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(data).unwrap();
    writer.finish().unwrap();
    out
}

/// 2 x 2: red, green / blue, translucent white.
fn rgba_png() -> Vec<u8> {
    let data = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128,
    ];
    png_of(png::ColorType::Rgba, png::BitDepth::Eight, 2, 2, &data)
}

fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    (info.width, info.height, buf[..info.buffer_size()].to_vec())
}

// ---- encodings ---------------------------------------------------------------

#[test]
fn text_is_utf16_with_a_terminator_and_stops_at_the_first_nul() {
    let bytes = utf16_z("hi \u{1F600}");
    assert_eq!(&bytes[bytes.len() - 2..], [0, 0]);
    assert_eq!(from_utf16_z(&bytes), "hi \u{1F600}");
    let mut padded = utf16_z("ab");
    padded.extend(utf16_z("junk"));
    assert_eq!(from_utf16_z(&padded), "ab");
    assert_eq!(from_utf16_z(&[]), "");
}

#[test]
fn trailing_nuls_are_trimmed() {
    assert_eq!(trim_nul(b"abc\0\0"), b"abc");
    assert_eq!(trim_nul(b"a\0b"), b"a\0b");
    assert_eq!(trim_nul(b"\0\0"), b"");
}

#[test]
fn html_is_wrapped_with_byte_exact_offsets_and_comes_back_out() {
    let fragment = "<b>caf\u{e9}</b>";
    let bytes = cf_html(fragment);
    let text = String::from_utf8(bytes.clone()).unwrap();
    let number = |key: &str| -> usize {
        let at = text.find(key).unwrap() + key.len();
        text[at..at + 10].parse().unwrap()
    };
    assert_eq!(
        &bytes[number("StartFragment:")..number("EndFragment:")],
        fragment.as_bytes()
    );
    assert_eq!(number("EndHTML:"), bytes.len());
    assert!(text[number("StartHTML:")..].starts_with("<html>"));
    assert_eq!(html_fragment(&bytes), fragment);
    let mut with_nul = bytes;
    with_nul.push(0);
    assert_eq!(html_fragment(&with_nul), fragment);
}

#[test]
fn nested_envelopes_are_all_taken_off_and_never_added() {
    let fragment = "<p>caf\u{e9}</p>";
    // A tool that took CF_HTML for plain HTML and wrapped it again, twice.
    let once = cf_html(fragment);
    let nested = |html: Vec<u8>| -> Vec<u8> {
        let html = String::from_utf8(html).unwrap();
        let (prefix, suffix) = (
            "<html><body>\r\n<!--StartFragment-->",
            "<!--EndFragment-->\r\n</body></html>",
        );
        let start = 105 + prefix.len();
        let end = start + html.len();
        format!(
            "Version:0.9\r\nStartHTML:{:010}\r\nEndHTML:{:010}\r\nStartFragment:{start:010}\r\nEndFragment:{end:010}\r\n{prefix}{html}{suffix}",
            105,
            end + suffix.len()
        )
        .into_bytes()
    };
    let deep = nested(nested(once.clone()));
    assert_eq!(html_fragment(&deep), fragment);
    // Wrapping what is already an envelope gives exactly one envelope.
    let deep = String::from_utf8(deep).unwrap();
    assert_eq!(cf_html(&deep), once);
    assert_eq!(cf_html(&String::from_utf8(once.clone()).unwrap()), once);
    // Text that merely starts like a header is HTML like any other.
    assert_eq!(html_fragment(b"Version: two"), "Version: two");
}

#[test]
fn html_without_fragment_offsets_falls_back_to_the_body_or_everything() {
    let only_html = b"Version:0.9\r\nStartHTML:0000000035\r\n<p>x</p>";
    assert_eq!(html_fragment(only_html), "<p>x</p>");
    assert_eq!(html_fragment(b"<p>bare</p>"), "<p>bare</p>");
    let out_of_range = b"StartFragment:0000000900\r\nEndFragment:0000001000\r\n<p>y</p>";
    assert_eq!(
        html_fragment(out_of_range),
        String::from_utf8_lossy(out_of_range)
    );
}

#[test]
fn png_becomes_a_bottom_up_bgra_dibv5_block() {
    let dib = dibv5_from_png(&rgba_png()).unwrap();
    assert_eq!(dib.len(), 124 + 16);
    let u32_at = |at: usize| u32::from_le_bytes(dib[at..at + 4].try_into().unwrap());
    assert_eq!((u32_at(0), u32_at(4), u32_at(8)), (124, 2, 2));
    assert_eq!(u32_at(16), 3, "BI_BITFIELDS");
    assert_eq!(u32_at(52), 0xFF00_0000, "alpha mask");
    // The bottom image row (blue, white) comes first, as BGRA.
    assert_eq!(&dib[124..128], [255, 0, 0, 255]);
    assert_eq!(&dib[128..132], [255, 255, 255, 128]);
    assert_eq!(&dib[132..136], [0, 0, 255, 255]);
}

#[test]
fn every_png_colour_type_converts() {
    use png::BitDepth::{Eight, Sixteen};
    use png::ColorType::{Grayscale, GrayscaleAlpha, Indexed, Rgb};
    let cases = [
        png_of(Rgb, Eight, 1, 1, &[1, 2, 3]),
        png_of(Grayscale, Eight, 1, 1, &[7]),
        png_of(GrayscaleAlpha, Eight, 1, 1, &[7, 9]),
        png_of(Indexed, Eight, 1, 1, &[1]),
        png_of(Rgb, Sixteen, 1, 1, &[0, 1, 0, 2, 0, 3]),
    ];
    for png in cases {
        assert_eq!(dibv5_from_png(&png).unwrap().len(), 128);
    }
    let rgb = dibv5_from_png(&png_of(Rgb, Eight, 1, 1, &[1, 2, 3])).unwrap();
    assert_eq!(&rgb[124..128], [3, 2, 1, 255]);
    let grey = dibv5_from_png(&png_of(GrayscaleAlpha, Eight, 1, 1, &[7, 9])).unwrap();
    assert_eq!(&grey[124..128], [7, 7, 7, 9]);
}

const PNG_HEAD: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

#[test]
fn bytes_that_are_not_a_png_do_not_convert() {
    assert!(dibv5_from_png(b"nope").is_none());
    assert!(dibv5_from_png(&PNG_HEAD).is_none());
    assert!(looks_like_png(&rgba_png()));
    assert!(!looks_like_png(b"\x89PNG"));
}

#[test]
fn a_dibv5_block_round_trips_to_the_same_pixels() {
    let dib = dibv5_from_png(&rgba_png()).unwrap();
    let (w, h, pixels) = decode(&png_from_dib(&dib).unwrap());
    assert_eq!((w, h), (2, 2));
    assert_eq!(pixels, decode(&rgba_png()).2);
}

/// A `BITMAPINFOHEADER` (40 bytes), then `extra`, then `pixels`.
fn dib40(
    width: i32,
    height: i32,
    bits: u16,
    compression: u32,
    extra: &[u8],
    pixels: &[u8],
) -> Vec<u8> {
    let mut d = vec![0u8; 40];
    d[0..4].copy_from_slice(&40u32.to_le_bytes());
    d[4..8].copy_from_slice(&width.to_le_bytes());
    d[8..12].copy_from_slice(&height.to_le_bytes());
    d[12..14].copy_from_slice(&1u16.to_le_bytes());
    d[14..16].copy_from_slice(&bits.to_le_bytes());
    d[16..20].copy_from_slice(&compression.to_le_bytes());
    d.extend_from_slice(extra);
    d.extend_from_slice(pixels);
    d
}

#[test]
fn a_24_bit_bitmap_pads_rows_and_is_opaque() {
    // 1 x 2, bottom-up: bottom pixel (B,G,R) = 1,2,3; top = 4,5,6; rows padded to 4.
    let dib = dib40(1, 2, 24, 0, &[], &[1, 2, 3, 0, 4, 5, 6, 0]);
    let (w, h, px) = decode(&png_from_dib(&dib).unwrap());
    assert_eq!((w, h), (1, 2));
    assert_eq!(px, [6, 5, 4, 255, 3, 2, 1, 255]);
}

#[test]
fn a_top_down_32_bit_bitmap_without_alpha_is_made_opaque() {
    let dib = dib40(1, -2, 32, 0, &[], &[1, 2, 3, 0, 4, 5, 6, 0]);
    let (_, _, px) = decode(&png_from_dib(&dib).unwrap());
    assert_eq!(px, [3, 2, 1, 255, 6, 5, 4, 255]);
}

#[test]
fn a_32_bit_bitmap_keeps_real_alpha_and_skips_the_masks_after_a_small_header() {
    let masks = [0u8; 12];
    let dib = dib40(1, 1, 32, 3, &masks, &[1, 2, 3, 77]);
    let (_, _, px) = decode(&png_from_dib(&dib).unwrap());
    assert_eq!(px, [3, 2, 1, 77]);
}

#[test]
fn unsupported_or_truncated_bitmaps_do_not_convert() {
    assert!(png_from_dib(&[]).is_none());
    assert!(png_from_dib(&dib40(1, 1, 8, 0, &[], &[0, 0, 0, 0])).is_none());
    assert!(png_from_dib(&dib40(1, 1, 24, 1, &[], &[0, 0, 0, 0])).is_none());
    assert!(png_from_dib(&dib40(0, 1, 24, 0, &[], &[0, 0, 0, 0])).is_none());
    assert!(png_from_dib(&dib40(1, 0, 24, 0, &[], &[0, 0, 0, 0])).is_none());
    assert!(png_from_dib(&dib40(2, 2, 32, 0, &[], &[0; 4])).is_none());
}

#[test]
fn file_lists_round_trip_wide_and_ansi() {
    let paths = vec![
        PathBuf::from(r"C:\a b\c.txt"),
        PathBuf::from("D:\\caf\u{e9}.png"),
    ];
    assert_eq!(parse_dropfiles(&dropfiles(&paths)), paths);
    let mut ansi = vec![0u8; 20];
    ansi[0] = 20;
    ansi.extend(b"C:\\x.txt\0C:\\y.txt\0\0");
    assert_eq!(
        parse_dropfiles(&ansi),
        [PathBuf::from(r"C:\x.txt"), PathBuf::from(r"C:\y.txt")]
    );
    assert!(parse_dropfiles(&[]).is_empty());
    assert!(parse_dropfiles(&dropfiles(&[])).is_empty());
    let mut past_end = dropfiles(&paths);
    past_end[0..4].copy_from_slice(&9999u32.to_le_bytes());
    assert!(parse_dropfiles(&past_end).is_empty());
}

#[test]
fn each_representation_is_offered_under_its_formats_and_renders_them() {
    let f = Formats::register();
    let reps = vec![
        ClipRep::Text("t".into()),
        ClipRep::Html("<i>h</i>".into()),
        ClipRep::Rtf(b"{\\rtf1}".to_vec()),
        ClipRep::Png(rgba_png()),
        ClipRep::Files(vec![PathBuf::from(r"C:\f.txt")]),
    ];
    let formats = offered(&reps, &f);
    assert_eq!(
        formats,
        [
            CF_UNICODE,
            f.html,
            f.rtf,
            f.png,
            CF_DIBV5,
            CF_HDROP,
            f.drop_effect
        ]
    );
    for format in formats {
        assert!(render(&reps, format, &f).is_some(), "{format}");
    }
    assert_eq!(render(&reps, f.rtf, &f).unwrap(), b"{\\rtf1}\0");
    assert_eq!(render(&reps, f.drop_effect, &f).unwrap(), [1, 0, 0, 0]);
    assert_eq!(render(&reps, f.png, &f).unwrap(), rgba_png());
    assert!(render(&reps, 0xBEEF, &f).is_none());
}

#[test]
fn a_png_that_is_not_one_is_offered_only_as_png() {
    let f = Formats::register();
    let reps = vec![ClipRep::Png(vec![1, 2, 3])];
    assert_eq!(offered(&reps, &f), [f.png]);
    // Even asked for directly, a broken image renders nothing.
    assert!(render(&reps, CF_DIBV5, &f).is_none());
    assert!(render(&[], CF_UNICODE, &f).is_none());
}

#[test]
fn global_blocks_can_be_made_for_any_bytes() {
    assert!(global(b"abc").is_ok());
    assert!(global(b"").is_ok());
}

// ---- the system clipboard ----------------------------------------------------

fn all_reps() -> Vec<ClipRep> {
    vec![
        ClipRep::Text("mouser \u{e9} text".into()),
        ClipRep::Html("<b>mouser</b>".into()),
        ClipRep::Rtf(b"{\\rtf1 mouser}".to_vec()),
        ClipRep::Png(rgba_png()),
        ClipRep::Files(vec![PathBuf::from(r"C:\Windows\notepad.exe")]),
    ]
}

fn formats_on_clipboard() -> Vec<u32> {
    let _open = open(None, 50).unwrap();
    let mut out = Vec::new();
    let mut format = 0;
    loop {
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            return out;
        }
        out.push(format);
    }
}

/// What another app does: put finished data on the clipboard.
fn foreign(items: &[(u32, Vec<u8>)]) {
    let _open = open(None, 50).unwrap();
    unsafe { EmptyClipboard() }.unwrap();
    for (format, bytes) in items {
        let block = global(bytes).unwrap();
        unsafe { SetClipboardData(*format, Some(HANDLE(block.0))) }.unwrap();
    }
}

#[test]
fn everything_written_is_read_back_and_the_counter_advances() {
    let _guard = clip();
    let mut c = WinClipboard::new();
    let before = c.change_count();
    c.write(all_reps()).unwrap();
    assert!(c.change_count() > before);
    assert_eq!(c.read().unwrap(), all_reps());
}

#[test]
fn formats_are_rendered_on_demand_by_the_owner_window() {
    let _guard = clip();
    let mut c = WinClipboard::new();
    c.write(all_reps()).unwrap();
    assert_eq!(unsafe { GetClipboardOwner() }.unwrap(), c.hwnd());
    let formats = formats_on_clipboard();
    for expected in [CF_UNICODE, CF_DIBV5, CF_HDROP] {
        assert!(formats.contains(&expected), "{expected}");
    }
    // An app that only pastes bitmaps gets the image.
    let _open = open(None, 50).unwrap();
    let handle = unsafe { GetClipboardData(CF_DIBV5) }.unwrap();
    assert_eq!(unsafe { GlobalSize(HGLOBAL(handle.0)) }, 124 + 16);
}

#[test]
fn asking_the_owner_to_render_something_it_does_not_have_does_nothing() {
    let _guard = clip();
    let mut c = WinClipboard::new();
    c.write(vec![ClipRep::Text("only text".into())]).unwrap();
    unsafe {
        SendMessageW(
            c.hwnd(),
            WM_RENDERFORMAT,
            Some(WPARAM(0xBEEF)),
            Some(LPARAM(0)),
        )
    };
    assert_eq!(c.read().unwrap(), [ClipRep::Text("only text".into())]);
}

#[test]
fn render_all_formats_keeps_the_data_readable() {
    let _guard = clip();
    let mut c = WinClipboard::new();
    c.write(all_reps()).unwrap();
    unsafe { SendMessageW(c.hwnd(), WM_RENDERALLFORMATS, None, None) };
    assert_eq!(c.read().unwrap(), all_reps());
}

#[test]
fn render_all_formats_is_ignored_when_another_app_owns_the_clipboard() {
    let _guard = clip();
    let mut c = WinClipboard::new();
    foreign(&[(CF_UNICODE, utf16_z("theirs"))]);
    unsafe { SendMessageW(c.hwnd(), WM_RENDERALLFORMATS, None, None) };
    assert_eq!(c.read().unwrap(), [ClipRep::Text("theirs".into())]);
}

#[test]
fn what_was_copied_survives_closing_mouser() {
    let _guard = clip();
    let mut c = WinClipboard::new();
    c.write(all_reps()).unwrap();
    drop(c);
    let mut later = WinClipboard::new();
    assert_eq!(later.read().unwrap(), all_reps());
}

#[test]
fn losing_the_clipboard_to_another_owner_releases_what_was_offered() {
    let _guard = clip();
    let mut first = WinClipboard::new();
    first.write(all_reps()).unwrap();
    let mut second = WinClipboard::new();
    second.write(vec![ClipRep::Text("second".into())]).unwrap();
    assert!(first.reps().is_empty());
    assert_eq!(second.read().unwrap(), [ClipRep::Text("second".into())]);
    // The message the system sends is harmless to repeat.
    unsafe { SendMessageW(first.hwnd(), WM_DESTROYCLIPBOARD, None, None) };
}

#[test]
fn writing_nothing_empties_the_clipboard() {
    let _guard = clip();
    let mut c = WinClipboard::new();
    c.write(all_reps()).unwrap();
    c.write(Vec::new()).unwrap();
    assert!(c.read().unwrap().is_empty());
}

#[test]
fn a_screenshot_from_another_app_reads_as_a_png() {
    let _guard = clip();
    foreign(&[(CF_DIBV5, dib40(1, 1, 24, 0, &[], &[1, 2, 3, 0]))]);
    let mut c = WinClipboard::new();
    let read = c.read().unwrap();
    let [ClipRep::Png(png)] = read.as_slice() else {
        panic!("{read:?}")
    };
    assert_eq!(decode(png).2, [3, 2, 1, 255]);
}

#[test]
fn another_apps_html_and_rich_text_are_read_as_their_content() {
    let _guard = clip();
    let f = Formats::register();
    foreign(&[
        (f.html, cf_html("<u>theirs</u>")),
        (f.rtf, b"{\\rtf1 x}\0".to_vec()),
    ]);
    let mut c = WinClipboard::new();
    assert_eq!(
        c.read().unwrap(),
        [
            ClipRep::Html("<u>theirs</u>".into()),
            ClipRep::Rtf(b"{\\rtf1 x}".to_vec())
        ]
    );
}

#[test]
fn an_empty_file_list_from_another_app_is_not_a_files_representation() {
    let _guard = clip();
    foreign(&[(CF_HDROP, dropfiles(&[]))]);
    let mut c = WinClipboard::new();
    assert!(c.read().unwrap().is_empty());
}

#[test]
fn opening_waits_for_another_app_but_not_forever() {
    let _guard = clip();
    let (held_tx, held_rx) = std::sync::mpsc::channel();
    let holder_window = WinClipboard::new();
    let owner = holder_window.hwnd().0 as isize;
    let holder = std::thread::spawn(move || {
        let _open = open(Some(HWND(owner as _)), 50).unwrap();
        held_tx.send(()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
    });
    held_rx.recv().unwrap();
    assert!(open(None, 1).is_err(), "held by another thread");
    assert!(open(None, 100).is_ok(), "freed after a moment");
    holder.join().unwrap();
}

#[test]
fn data_for_an_invalid_format_is_refused_and_named() {
    let _guard = clip();
    let c = WinClipboard::new();
    let _open = open(Some(c.hwnd()), 50).unwrap();
    let mouser_platform::Error::Os(text) = set(0, b"x").unwrap_err() else {
        panic!("not an OS error")
    };
    assert!(text.starts_with("SetClipboardData"), "{text}");
}

#[test]
fn offering_an_invalid_format_is_refused_and_named() {
    let _guard = clip();
    let c = WinClipboard::new();
    let _open = open(Some(c.hwnd()), 50).unwrap();
    let mouser_platform::Error::Os(text) = delayed(0).unwrap_err() else {
        panic!("not an OS error")
    };
    assert!(text.starts_with("SetClipboardData"), "{text}");
}

#[test]
fn rendering_everything_waits_for_an_app_that_has_the_clipboard_open() {
    let _guard = clip();
    let mut c = WinClipboard::new();
    c.write(all_reps()).unwrap();
    let other = WinClipboard::new();
    let (held_tx, held_rx) = std::sync::mpsc::channel();
    let holder_window = other.hwnd().0 as isize;
    let holder = std::thread::spawn(move || {
        let _open = open(Some(HWND(holder_window as _)), 50).unwrap();
        held_tx.send(()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
    });
    held_rx.recv().unwrap();
    drop(c);
    holder.join().unwrap();
    let mut later = WinClipboard::new();
    assert_eq!(later.read().unwrap(), all_reps());
}
