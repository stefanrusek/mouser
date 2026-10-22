//! Hiding the local pointer while control is on another computer.
//!
//! Windows has no per-process "hide the cursor" that works over other apps'
//! windows, so every system cursor shape is replaced with a blank one and
//! put back with `SPI_SETCURSORS`, which reloads the user's own scheme from
//! the registry. `SetSystemCursor` takes ownership of (and destroys) the
//! cursor it is given, so each shape gets its own copy of the blank one.
//!
//! Because the replacement outlives the process, [`recover`] runs when the
//! platform starts: a crash while the pointer was hidden is undone by the
//! next launch.

use crate::inject::os;
use mouser_platform::Result;
use std::sync::Mutex;
use std::sync::OnceLock;
use windows::Win32::Graphics::Gdi::{
    BITMAP, DeleteObject, GetBitmapBits, GetObjectW, HBITMAP, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CopyIcon, CreateCursor, GetIconInfo, HCURSOR, HICON, ICONINFO, IMAGE_CURSOR, LR_DEFAULTSIZE,
    LR_SHARED, LoadImageW, OCR_APPSTARTING, OCR_CROSS, OCR_HAND, OCR_HELP, OCR_IBEAM, OCR_NO,
    OCR_NORMAL, OCR_SIZEALL, OCR_SIZENESW, OCR_SIZENS, OCR_SIZENWSE, OCR_SIZEWE, OCR_UP, OCR_WAIT,
    SPI_SETCURSORS, SYSTEM_CURSOR_ID, SetSystemCursor, SystemParametersInfoW,
};
use windows::core::PCWSTR;

/// Every shape Windows shows for the system cursor.
const SHAPES: [SYSTEM_CURSOR_ID; 14] = [
    OCR_NORMAL,
    OCR_IBEAM,
    OCR_WAIT,
    OCR_CROSS,
    OCR_UP,
    OCR_SIZENWSE,
    OCR_SIZENESW,
    OCR_SIZEWE,
    OCR_SIZENS,
    OCR_SIZEALL,
    OCR_NO,
    OCR_HAND,
    OCR_APPSTARTING,
    OCR_HELP,
];

/// Whether this process has replaced the cursors and not yet restored them.
/// Held across each change so concurrent callers see whole states.
static HIDDEN: Mutex<bool> = Mutex::new(false);

/// The blank cursor's handle once made (a handle is not Sync, so as an address).
static BLANK: OnceLock<usize> = OnceLock::new();

/// The blank cursor, made once: 32 x 32, every pixel transparent (AND plane
/// set, XOR plane clear).
fn blank() -> Result<HCURSOR> {
    if let Some(&handle) = BLANK.get() {
        return Ok(HCURSOR(handle as _));
    }
    let and_plane = [0xFFu8; 32 * 32 / 8];
    let xor_plane = [0u8; 32 * 32 / 8];
    let made = unsafe {
        CreateCursor(
            None,
            0,
            0,
            32,
            32,
            and_plane.as_ptr().cast(),
            xor_plane.as_ptr().cast(),
        )
    };
    os("CreateCursor", made).map(|cursor| HCURSOR(*BLANK.get_or_init(|| cursor.0 as usize) as _))
}

/// Replaces every system cursor with the blank one; does nothing when already
/// hidden.
pub(crate) fn hide() -> Result<()> {
    hide_with(&mut replace)
}

/// [`hide`] with the per-shape replacement supplied, so a failure part way
/// through can be exercised.
pub(crate) fn hide_with(replace: &mut dyn FnMut(SYSTEM_CURSOR_ID) -> Result<()>) -> Result<()> {
    let mut hidden = HIDDEN.lock().unwrap();
    if *hidden {
        return Ok(());
    }
    let result = SHAPES.into_iter().try_for_each(replace);
    if result.is_err() {
        // Some shapes may have changed already; put them all back.
        let _ = recover();
    }
    *hidden = result.is_ok();
    result
}

/// Gives `shape` its own copy of the blank cursor: the system owns, and
/// destroys, what it is given.
fn replace(shape: SYSTEM_CURSOR_ID) -> Result<()> {
    let copy = blank().and_then(|blank| os("CopyIcon", unsafe { CopyIcon(HICON(blank.0)) }));
    copy.and_then(|copy| {
        os("SetSystemCursor", unsafe {
            SetSystemCursor(HCURSOR(copy.0), shape)
        })
    })
}

/// Puts the user's cursors back; does nothing when this process has not hidden
/// them.
pub(crate) fn show() -> Result<()> {
    let mut hidden = HIDDEN.lock().unwrap();
    if !*hidden {
        return Ok(());
    }
    *hidden = false;
    recover()
}

/// For platform start: undoes a hide a crashed earlier run left behind. A
/// process that is itself hiding the pointer leaves it alone.
pub(crate) fn recover_at_startup() -> Result<()> {
    if *HIDDEN.lock().unwrap() || !arrow_is_blank() {
        return Ok(());
    }
    recover()
}

/// Whether the system arrow is the blank cursor: monochrome, AND plane all
/// ones, XOR plane all zeros. That is what a crashed run leaves behind.
pub(crate) fn arrow_is_blank() -> bool {
    let shared = unsafe {
        LoadImageW(
            None,
            PCWSTR(OCR_NORMAL.0 as _),
            IMAGE_CURSOR,
            0,
            0,
            LR_SHARED | LR_DEFAULTSIZE,
        )
    };
    let mut info = ICONINFO::default();
    let known = shared.is_ok_and(|h| unsafe { GetIconInfo(HICON(h.0), &mut info) }.is_ok());
    let blank = known && info.hbmColor.is_invalid() && mask_is_blank(info.hbmMask);
    for bitmap in [info.hbmMask, info.hbmColor] {
        if !bitmap.is_invalid() {
            let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
        }
    }
    blank
}

fn mask_is_blank(mask: HBITMAP) -> bool {
    let mut bitmap = BITMAP::default();
    let size = size_of::<BITMAP>() as i32;
    unsafe { GetObjectW(HGDIOBJ(mask.0), size, Some((&raw mut bitmap).cast())) };
    let mut bits = vec![0u8; (bitmap.bmWidthBytes * bitmap.bmHeight) as usize];
    unsafe { GetBitmapBits(mask, bits.len() as i32, bits.as_mut_ptr().cast()) };
    let (and, xor) = bits.split_at(bits.len() / 2);
    and.iter().all(|&b| b == 0xFF) && xor.iter().all(|&b| b == 0)
}

/// Reloads the user's cursor scheme whatever state it is in, for startup.
pub(crate) fn recover() -> Result<()> {
    os("SystemParametersInfo(SPI_SETCURSORS)", unsafe {
        SystemParametersInfoW(
            SPI_SETCURSORS,
            0,
            None,
            windows::Win32::UI::WindowsAndMessaging::SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    })
}

/// For tests: forgets that this process hid the pointer, as a crash would,
/// leaving the replaced cursors in place.
#[cfg(test)]
pub(crate) fn forget_hidden() {
    *HIDDEN.lock().unwrap() = false;
}
