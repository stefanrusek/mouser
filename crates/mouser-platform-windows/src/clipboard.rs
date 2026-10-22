//! `Clipboard` on the Win32 clipboard (Story 6).
//!
//! Representations: Text is `CF_UNICODETEXT`; Html is `HTML Format` (the
//! fragment is wrapped in the CF_HTML header and read back out of it); Rtf is
//! `Rich Text Format`; Png is the registered `PNG` format plus `CF_DIBV5` for
//! apps that only paste bitmaps; Files is `CF_HDROP` with a copy drop effect.
//!
//! Writes use delayed rendering: the clipboard is claimed for a hidden owner
//! window that lives on its own thread, and each format is produced when an
//! app first asks for it (`WM_RENDERFORMAT`), so a large image is only
//! converted to a bitmap if someone pastes it as one. When the owner goes
//! away the formats are rendered first (`WM_RENDERALLFORMATS`), so closing
//! Mouser does not empty what was copied.
//!
//! The change counter is `GetClipboardSequenceNumber`, which Windows advances
//! on every change by any app; the seam is polled, so no format listener
//! window is needed for it.

use crate::dpi;
use crate::inject::os;
use mouser_platform::{ClipRep, Clipboard, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardOwner,
    GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::{CF_DIBV5, CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, MSG, PM_NOREMOVE, PeekMessageW, PostMessageW, PostQuitMessage,
    RegisterClassW, SetWindowLongPtrW, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_DESTROYCLIPBOARD,
    WM_RENDERALLFORMATS, WM_RENDERFORMAT, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::{PCWSTR, w};

/// `DROPEFFECT_COPY`.
const DROP_EFFECT_COPY: u32 = 1;
/// How long `OpenClipboard` is retried: another app may hold it for a moment.
const OPEN_ATTEMPTS: u32 = 50;
const OPEN_PAUSE: Duration = Duration::from_millis(10);

const CF_UNICODE: u32 = CF_UNICODETEXT.0 as u32;
const CF_DIB_V5: u32 = CF_DIBV5.0 as u32;
const CF_DROP: u32 = CF_HDROP.0 as u32;

/// The registered (non-standard) clipboard format ids.
#[derive(Clone, Copy)]
pub(crate) struct Formats {
    pub(crate) html: u32,
    pub(crate) rtf: u32,
    pub(crate) png: u32,
    pub(crate) drop_effect: u32,
}

impl Formats {
    pub(crate) fn register() -> Self {
        let id = |name: PCWSTR| unsafe { RegisterClipboardFormatW(name) };
        Formats {
            html: id(w!("HTML Format")),
            rtf: id(w!("Rich Text Format")),
            png: id(w!("PNG")),
            drop_effect: id(w!("Preferred DropEffect")),
        }
    }
}

// ---- encodings (pure) -----------------------------------------------------

/// UTF-16LE with a terminating NUL, as `CF_UNICODETEXT` holds it.
pub(crate) fn utf16_z(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// The UTF-16LE code units in `bytes` (a trailing odd byte is dropped).
fn utf16_units(bytes: &[u8]) -> Vec<u16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect()
}

/// The text up to the first NUL.
pub(crate) fn from_utf16_z(bytes: &[u8]) -> String {
    let units: Vec<u16> = utf16_units(bytes)
        .into_iter()
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// `bytes` without the terminating NULs Windows apps append.
pub(crate) fn trim_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    &bytes[..end]
}

const HTML_PREFIX: &str = "<html><body>\r\n<!--StartFragment-->";
const HTML_SUFFIX: &str = "<!--EndFragment-->\r\n</body></html>";

/// `fragment` in a CF_HTML envelope (version 0.9, 10-digit byte offsets).
pub(crate) fn cf_html(fragment: &str) -> Vec<u8> {
    // The header is fixed-width, so offsets can be computed up front.
    const HEADER_LEN: usize = "Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:0000000000\r\nEndFragment:0000000000\r\n".len();
    let start_fragment = HEADER_LEN + HTML_PREFIX.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + HTML_SUFFIX.len();
    format!(
        "Version:0.9\r\nStartHTML:{HEADER_LEN:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n{HTML_PREFIX}{fragment}{HTML_SUFFIX}"
    )
    .into_bytes()
}

fn header_number(text: &str, key: &str) -> Option<usize> {
    let at = text.find(key)? + key.len();
    let digits: String = text[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

/// The fragment of a CF_HTML buffer; the whole body when the header does not
/// name one.
pub(crate) fn html_fragment(bytes: &[u8]) -> String {
    let bytes = trim_nul(bytes);
    let text = String::from_utf8_lossy(bytes);
    let range = header_number(&text, "StartFragment:")
        .zip(header_number(&text, "EndFragment:"))
        .and_then(|(start, end)| bytes.get(start..end));
    match range {
        Some(fragment) => String::from_utf8_lossy(fragment).into_owned(),
        None => {
            let body = header_number(&text, "StartHTML:").and_then(|at| bytes.get(at..));
            String::from_utf8_lossy(body.unwrap_or(bytes)).into_owned()
        }
    }
}

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

pub(crate) fn looks_like_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&PNG_SIGNATURE)
}

fn le32(out: &mut [u8], at: usize, value: u32) {
    out[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// A PNG as a `CF_DIBV5` block: 32-bit BGRA, bottom-up, with an alpha mask.
/// `None` when the PNG cannot be decoded.
pub(crate) fn dibv5_from_png(png: &[u8]) -> Option<Vec<u8>> {
    let (width, height, rgba) = decode_png(png)?;
    let mut out = vec![0u8; 124 + rgba.len()];
    le32(&mut out, 0, 124);
    le32(&mut out, 4, width);
    le32(&mut out, 8, height);
    out[12..14].copy_from_slice(&1u16.to_le_bytes());
    out[14..16].copy_from_slice(&32u16.to_le_bytes());
    le32(&mut out, 16, 3); // BI_BITFIELDS
    le32(&mut out, 20, rgba.len() as u32);
    le32(&mut out, 40, 0x00FF_0000);
    le32(&mut out, 44, 0x0000_FF00);
    le32(&mut out, 48, 0x0000_00FF);
    le32(&mut out, 52, 0xFF00_0000);
    le32(&mut out, 56, 0x7352_4742); // LCS_sRGB
    le32(&mut out, 108, 4); // LCS_GM_IMAGES
    let row = width as usize * 4;
    for (y, line) in rgba.chunks_exact(row).enumerate() {
        let at = 124 + (height as usize - 1 - y) * row;
        for (x, px) in line.as_chunks::<4>().0.iter().enumerate() {
            out[at + x * 4..at + x * 4 + 4].copy_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    Some(out)
}

/// `(width, height, RGBA8 rows top to bottom)`.
fn decode_png(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut raw = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut raw).ok()?;
    let raw = &raw[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => raw.to_vec(),
        png::ColorType::Rgb => raw
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => raw
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        _ => raw.iter().flat_map(|&g| [g, g, g, 255]).collect(),
    };
    Some((info.width, info.height, rgba))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// A PNG from a `CF_DIBV5` block of 24-bit or 32-bit pixels (BGR order, the
/// layout every screenshot tool produces); `None` for anything else.
pub(crate) fn png_from_dib(dib: &[u8]) -> Option<Vec<u8>> {
    let header = read_u32(dib, 0)? as usize;
    let width = read_u32(dib, 4)? as i32;
    let height = read_u32(dib, 8)? as i32;
    let bits = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?);
    let compression = read_u32(dib, 16)?;
    let colours = read_u32(dib, 32)? as usize;
    let bitfields_after_header = header == 40 && compression == 3;
    let at = header + colours * 4 + if bitfields_after_header { 12 } else { 0 };
    let (bytes_per_pixel, supported) = match (bits, compression) {
        (32, 0 | 3) => (4, true),
        (24, 0) => (3, true),
        _ => (0, false),
    };
    if !supported || width <= 0 || height == 0 {
        return None;
    }
    let (width, rows) = (width as usize, height.unsigned_abs() as usize);
    let stride = (width * bytes_per_pixel).div_ceil(4) * 4;
    let pixels = dib.get(at..at + stride * rows)?;
    let mut rgba = Vec::with_capacity(width * rows * 4);
    for y in 0..rows {
        let line = if height > 0 { rows - 1 - y } else { y };
        for px in pixels[line * stride..][..width * bytes_per_pixel].chunks_exact(bytes_per_pixel) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], px.get(3).copied().unwrap_or(255)]);
        }
    }
    // Bitmaps without a real alpha channel leave the byte zero.
    if bytes_per_pixel == 4 && rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        rgba.as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|p| p[3] = 255);
    }
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, width as u32, rows as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&rgba).ok()?;
    writer.finish().ok()?;
    Some(png)
}

/// A `CF_HDROP` block: `DROPFILES` then NUL-terminated UTF-16 paths and an
/// extra NUL.
pub(crate) fn dropfiles(paths: &[PathBuf]) -> Vec<u8> {
    let mut out = vec![0u8; 20];
    le32(&mut out, 0, 20);
    le32(&mut out, 16, 1); // fWide
    for path in paths {
        out.extend(utf16_z(&path.to_string_lossy()));
    }
    out.extend([0, 0]);
    out
}

/// The paths in a `CF_HDROP` block, wide or ANSI.
pub(crate) fn parse_dropfiles(bytes: &[u8]) -> Vec<PathBuf> {
    let (Some(start), Some(wide)) = (read_u32(bytes, 0), read_u32(bytes, 16)) else {
        return Vec::new();
    };
    let list = bytes.get(start as usize..).unwrap_or_default();
    if wide != 0 {
        utf16_units(list)
            .split(|&u| u == 0)
            .take_while(|name| !name.is_empty())
            .map(|name| PathBuf::from(String::from_utf16_lossy(name)))
            .collect()
    } else {
        list.split(|&b| b == 0)
            .take_while(|name| !name.is_empty())
            .map(|name| PathBuf::from(String::from_utf8_lossy(name).into_owned()))
            .collect()
    }
}

// ---- what each representation offers and renders --------------------------

/// The formats `reps` are offered under.
pub(crate) fn offered(reps: &[ClipRep], f: &Formats) -> Vec<u32> {
    let mut out = Vec::new();
    for rep in reps {
        match rep {
            ClipRep::Text(_) => out.push(CF_UNICODE),
            ClipRep::Html(_) => out.push(f.html),
            ClipRep::Rtf(_) => out.push(f.rtf),
            ClipRep::Png(png) => {
                out.push(f.png);
                if looks_like_png(png) {
                    out.push(CF_DIB_V5);
                }
            }
            ClipRep::Files(_) => out.extend([CF_DROP, f.drop_effect]),
        }
    }
    out
}

/// The bytes of `format` from `reps`; `None` when no representation produces
/// it.
pub(crate) fn render(reps: &[ClipRep], format: u32, f: &Formats) -> Option<Vec<u8>> {
    reps.iter().find_map(|rep| match rep {
        ClipRep::Text(text) if format == CF_UNICODE => Some(utf16_z(text)),
        ClipRep::Html(html) if format == f.html => Some(cf_html(html)),
        ClipRep::Rtf(rtf) if format == f.rtf => Some([rtf.as_slice(), &[0]].concat()),
        ClipRep::Png(png) if format == f.png => Some(png.clone()),
        ClipRep::Png(png) if format == CF_DIB_V5 => dibv5_from_png(png),
        ClipRep::Files(paths) if format == CF_DROP => Some(dropfiles(paths)),
        ClipRep::Files(_) if format == f.drop_effect => {
            Some(DROP_EFFECT_COPY.to_le_bytes().to_vec())
        }
        _ => None,
    })
}

// ---- Win32 ------------------------------------------------------------------

/// A global memory block holding `bytes`, ready for `SetClipboardData`.
pub(crate) fn global(bytes: &[u8]) -> Result<HGLOBAL> {
    let alloc = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) };
    let block = os("GlobalAlloc", alloc)?;
    let at = unsafe { GlobalLock(block) };
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), at.cast(), bytes.len()) };
    let _ = unsafe { GlobalUnlock(block) };
    Ok(block)
}

/// Puts `bytes` on the open clipboard under `format`.
pub(crate) fn set(format: u32, bytes: &[u8]) -> Result<()> {
    let block = global(bytes)?;
    let result = os("SetClipboardData", unsafe {
        SetClipboardData(format, Some(HANDLE(block.0)))
    });
    if result.is_err() {
        // Ownership passes to the clipboard only on success.
        let _ = unsafe { GlobalFree(Some(block)) };
    }
    result.map(|_| ())
}

/// Offers `format` without data. A null handle is the success answer here,
/// which the bindings report as an error whose code is "success".
pub(crate) fn delayed(format: u32) -> Result<()> {
    match unsafe { SetClipboardData(format, None) } {
        Err(e) if e.code().is_err() => os("SetClipboardData", Err::<(), _>(e)),
        _ => Ok(()),
    }
}

/// The open clipboard; closed on drop.
pub(crate) struct Open;

impl Drop for Open {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

/// Opens the clipboard for `owner` (or no window), retrying while another app
/// holds it.
pub(crate) fn open(owner: Option<HWND>, attempts: u32) -> Result<Open> {
    open_pumping(owner, attempts, || std::thread::sleep(OPEN_PAUSE))
}

/// Waits between attempts with `pause`. A window procedure must keep answering
/// sent messages while it waits: the app holding the clipboard may be waiting
/// for this very window to render a format.
pub(crate) fn open_pumping(owner: Option<HWND>, attempts: u32, pause: impl Fn()) -> Result<Open> {
    let mut last = Err(windows::core::Error::empty());
    for _ in 0..attempts {
        last = unsafe { OpenClipboard(owner) };
        if last.is_ok() {
            break;
        }
        pause();
    }
    os("OpenClipboard", last).map(|()| Open)
}

/// The bytes the open clipboard holds under `format`.
fn get(format: u32) -> Option<Vec<u8>> {
    unsafe { IsClipboardFormatAvailable(format) }.ok()?;
    let handle = unsafe { GetClipboardData(format) }.ok()?;
    let block = HGLOBAL(handle.0);
    let size = unsafe { GlobalSize(block) };
    let at = unsafe { GlobalLock(block) };
    let bytes = unsafe { std::slice::from_raw_parts(at.cast::<u8>(), size) }.to_vec();
    let _ = unsafe { GlobalUnlock(block) };
    Some(bytes)
}

/// Reads every representation the open clipboard holds.
pub(crate) fn read_all(f: &Formats) -> Vec<ClipRep> {
    let mut out = Vec::new();
    if let Some(bytes) = get(CF_UNICODE) {
        out.push(ClipRep::Text(from_utf16_z(&bytes)));
    }
    if let Some(bytes) = get(f.html) {
        out.push(ClipRep::Html(html_fragment(&bytes)));
    }
    if let Some(bytes) = get(f.rtf) {
        out.push(ClipRep::Rtf(trim_nul(&bytes).to_vec()));
    }
    let png = get(f.png).or_else(|| get(CF_DIB_V5).and_then(|dib| png_from_dib(&dib)));
    if let Some(png) = png {
        out.push(ClipRep::Png(png));
    }
    let files = get(CF_DROP).map(|bytes| parse_dropfiles(&bytes));
    if let Some(files) = files.filter(|files| !files.is_empty()) {
        out.push(ClipRep::Files(files));
    }
    out
}

type Offered = Arc<Mutex<Vec<ClipRep>>>;

/// What the owner window's procedure needs.
struct Owner {
    reps: Offered,
    formats: Formats,
}

fn owner_of(hwnd: HWND) -> &'static Owner {
    unsafe { &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Owner) }
}

/// Lets sent messages (a render request from the app holding the clipboard)
/// through, then pauses.
fn answer_sent_messages() {
    let mut msg = MSG::default();
    let _ = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE) };
    std::thread::sleep(OPEN_PAUSE);
}

pub(crate) unsafe extern "system" fn owner_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    match msg {
        WM_RENDERFORMAT => {
            let owner = owner_of(hwnd);
            let reps = owner.reps.lock().unwrap();
            if let Some(bytes) = render(&reps, wp.0 as u32, &owner.formats) {
                let _ = set(wp.0 as u32, &bytes);
            }
            LRESULT(0)
        }
        WM_RENDERALLFORMATS => {
            let owner = owner_of(hwnd);
            if let Ok(_open) = open_pumping(Some(hwnd), OPEN_ATTEMPTS, answer_sent_messages)
                && unsafe { GetClipboardOwner() }.ok() == Some(hwnd)
            {
                let reps = owner.reps.lock().unwrap();
                for format in offered(&reps, &owner.formats) {
                    if let Some(bytes) = render(&reps, format, &owner.formats) {
                        let _ = set(format, &bytes);
                    }
                }
            }
            LRESULT(0)
        }
        WM_DESTROYCLIPBOARD => {
            owner_of(hwnd).reps.lock().unwrap().clear();
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// The clipboard owner window on its own thread.
pub(crate) struct WinClipboard {
    hwnd: isize,
    reps: Offered,
    formats: Formats,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl WinClipboard {
    pub(crate) fn new() -> Self {
        let reps: Offered = Arc::default();
        let formats = Formats::register();
        let owner = Box::into_raw(Box::new(Owner {
            reps: reps.clone(),
            formats,
        }));
        let owner_addr = owner as isize;
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            dpi::physical_thread();
            let hinst = unsafe { GetModuleHandleW(None) }.expect("module handle");
            let class = WNDCLASSW {
                lpfnWndProc: Some(owner_proc),
                hInstance: hinst.into(),
                lpszClassName: w!("MouserClipboardOwner"),
                ..Default::default()
            };
            unsafe { RegisterClassW(&class) };
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("MouserClipboardOwner"),
                    w!("Mouser clipboard"),
                    WS_OVERLAPPED,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    Some(hinst.into()),
                    None,
                )
            }
            .expect("clipboard owner window");
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, owner_addr) };
            ready_tx.send(hwnd.0 as isize).expect("owner ready");
            let mut msg = MSG::default();
            while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
                unsafe { DispatchMessageW(&msg) };
            }
            drop(unsafe { Box::from_raw(owner_addr as *mut Owner) });
        });
        WinClipboard {
            hwnd: ready_rx.recv().expect("clipboard owner thread"),
            reps,
            formats,
            thread: Some(thread),
        }
    }

    /// What is currently offered, for tests.
    #[cfg(test)]
    pub(crate) fn reps(&self) -> Vec<ClipRep> {
        self.reps.lock().unwrap().clone()
    }

    /// The owner window, for tests to post clipboard messages to.
    #[cfg(test)]
    pub(crate) fn hwnd(&self) -> HWND {
        HWND(self.hwnd as _)
    }
}

impl Drop for WinClipboard {
    fn drop(&mut self) {
        // WM_CLOSE destroys the window; if it still owns the clipboard,
        // Windows asks it to render every format first.
        unsafe { PostMessageW(Some(HWND(self.hwnd as _)), WM_CLOSE, WPARAM(0), LPARAM(0)) }
            .expect("stop clipboard owner");
        self.thread
            .take()
            .expect("owner thread")
            .join()
            .expect("clipboard owner thread");
    }
}

impl Clipboard for WinClipboard {
    fn read(&mut self) -> Result<Vec<ClipRep>> {
        // A real window, not NULL: openers that pass NULL do not exclude each
        // other, and another app closing the clipboard then closes ours.
        let _open = open(Some(HWND(self.hwnd as _)), OPEN_ATTEMPTS)?;
        Ok(read_all(&self.formats))
    }

    fn write(&mut self, reps: Vec<ClipRep>) -> Result<()> {
        let hwnd = HWND(self.hwnd as _);
        let _open = open(Some(hwnd), OPEN_ATTEMPTS)?;
        // Emptying tells the previous owner (possibly this window) to let go
        // of what it offered, so the new representations are stored after.
        os("EmptyClipboard", unsafe { EmptyClipboard() })?;
        let formats = offered(&reps, &self.formats);
        *self.reps.lock().unwrap() = reps;
        for format in formats {
            // No data: Windows asks this window for it when an app pastes.
            delayed(format)?;
        }
        Ok(())
    }

    fn change_count(&mut self) -> u64 {
        u64::from(unsafe { GetClipboardSequenceNumber() })
    }
}
