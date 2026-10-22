//! `FileDrag` (Story 7): files dragged out of this PC are noticed by
//! [`crate::edge`]; a drag of files from another computer is started here
//! from a helper window and its drop folder reported.
//!
//! `DoDragDrop` has to be called with the left button down, from a window
//! under the pointer, so `begin_incoming` makes a small near-invisible window
//! centred on the pointer, presses the left button (tagged, so Mouser's own
//! capture ignores it) and starts the drag from that window's
//! `WM_LBUTTONDOWN` (hardware probe 3). The button is released by whoever
//! injects the pointer's movement; the drag ends with the drop.

use crate::dpi;
use crate::dropdir;
use crate::edge::{ENTER_WAIT, Edge, left_button_down};
use crate::inject::{MOUSER_TAG, mouse_input, send};
use crate::vfiles::{Ended, new_data_object};
use mouser_platform::{DropTarget, Error, FileDrag, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{
    DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, HWND, LPARAM, LRESULT, POINT,
    S_OK, WPARAM,
};
use windows::Win32::System::Com::IDataObject;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Ole::{
    DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE, DoDragDrop, IDropSource, IDropSource_Impl,
    OleInitialize, OleUninitialize,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTDOWN;
use windows::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTUP;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos,
    GetWindowLongPtrW, LWA_ALPHA, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW,
    QS_ALLINPUT, RegisterClassW, SW_SHOWNOACTIVATE, SetLayeredWindowAttributes, SetWindowLongPtrW,
    ShowWindow, WM_LBUTTONDOWN, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{BOOL, HRESULT, implement, w};

/// Side of the helper window under the pointer, in physical pixels.
const HELPER_SIDE: i32 = 32;
/// How long the helper window gets to receive the press that starts the drag.
const START_LIMIT: Duration = Duration::from_secs(5);
/// How long the shell gets to finish creating the dropped files.
const COPY_LIMIT: Duration = Duration::from_secs(10);

/// The Win32 file drag.
pub(crate) struct WinFileDrag {
    edge: Edge,
}

impl WinFileDrag {
    pub(crate) fn new() -> Self {
        WinFileDrag { edge: Edge::new() }
    }

    #[cfg(test)]
    pub(crate) fn edge(&self) -> &Edge {
        &self.edge
    }
}

impl FileDrag for WinFileDrag {
    fn outgoing(&mut self) -> Option<Vec<PathBuf>> {
        // The drag may reach a strip a moment after the pointer got there.
        let wait = if left_button_down() {
            ENTER_WAIT
        } else {
            Duration::ZERO
        };
        self.edge.take(wait)
    }

    fn begin_incoming(&mut self, names: Vec<String>) -> Result<DropTarget> {
        if names.is_empty() {
            return Err(Error::Os("no files to drag".into()));
        }
        let drag = std::thread::spawn(move || run(names));
        drag.join().expect("drag thread").map(DropTarget)
    }
}

/// What ending the drag looked like.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Ending {
    Dropped,
    Cancelled,
}

/// `QueryContinueDrag`'s answer: Escape cancels, releasing the left button
/// drops, otherwise the drag goes on.
pub(crate) fn continue_drag(escape: bool, left_down: bool) -> HRESULT {
    if escape {
        DRAGDROP_S_CANCEL
    } else if left_down {
        S_OK
    } else {
        DRAGDROP_S_DROP
    }
}

/// How `DoDragDrop` ended.
pub(crate) fn ending(result: HRESULT, effect: DROPEFFECT) -> Result<Ending> {
    if result == DRAGDROP_S_DROP && effect != DROPEFFECT_NONE {
        Ok(Ending::Dropped)
    } else if result == DRAGDROP_S_DROP || result == DRAGDROP_S_CANCEL {
        Ok(Ending::Cancelled)
    } else {
        Err(Error::Os(format!("DoDragDrop failed: {result:?}")))
    }
}

/// Packs a point so it can be stored atomically by the drop source.
pub(crate) fn pack(p: POINT) -> i64 {
    (i64::from(p.x) << 32) | i64::from(p.y as u32)
}

pub(crate) fn unpack(v: i64) -> POINT {
    POINT {
        x: (v >> 32) as i32,
        y: v as i32,
    }
}

/// Where the pointer was when the button came up.
#[derive(Default)]
pub(crate) struct DropPoint(pub(crate) AtomicI64);

#[implement(IDropSource)]
pub(crate) struct Source {
    pub(crate) at: Arc<DropPoint>,
}

impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        let answer = continue_drag(escape.as_bool(), keys.0 & MK_LBUTTON.0 != 0);
        if answer == DRAGDROP_S_DROP {
            let mut at = POINT::default();
            let _ = unsafe { GetCursorPos(&mut at) };
            self.at.0.store(pack(at), Ordering::SeqCst);
        }
        answer
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// What the helper window needs to start the drag from its press.
pub(crate) struct Pending {
    pub(crate) data: IDataObject,
    pub(crate) source: IDropSource,
    pub(crate) result: Mutex<Option<(HRESULT, DROPEFFECT)>>,
}

unsafe extern "system" fn helper_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_LBUTTONDOWN {
        let pending = unsafe { &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Pending) };
        let result = pending.result.lock().unwrap();
        if result.is_none() {
            // Only the first press starts a drag.
            drop(result);
            let mut effect = DROPEFFECT_NONE;
            let hr =
                unsafe { DoDragDrop(&pending.data, &pending.source, DROPEFFECT_COPY, &mut effect) };
            *pending.result.lock().unwrap() = Some((hr, effect));
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

pub(crate) fn pump() {
    let mut msg = MSG::default();
    while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
        unsafe { DispatchMessageW(&msg) };
    }
}

/// The helper window, centred on the pointer, and where the pointer is.
pub(crate) fn helper_window(pending: &Pending) -> (HWND, POINT) {
    let mut at = POINT::default();
    let _ = unsafe { GetCursorPos(&mut at) };
    let hinst = unsafe { GetModuleHandleW(None) }.expect("module handle");
    let class = WNDCLASSW {
        lpfnWndProc: Some(helper_proc),
        hInstance: hinst.into(),
        lpszClassName: w!("MouserDragSource"),
        ..Default::default()
    };
    unsafe { RegisterClassW(&class) };
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("MouserDragSource"),
            w!("Mouser drag"),
            WS_POPUP,
            at.x - HELPER_SIDE / 2,
            at.y - HELPER_SIDE / 2,
            HELPER_SIDE,
            HELPER_SIDE,
            None,
            None,
            Some(hinst.into()),
            None,
        )
    }
    .expect("drag helper window");
    unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, std::ptr::from_ref(pending) as isize) };
    // Nearly invisible, but still hit by the press.
    unsafe { SetLayeredWindowAttributes(hwnd, Default::default(), 1, LWA_ALPHA) }
        .expect("drag helper opacity");
    let _ = unsafe { ShowWindow(hwnd, SW_SHOWNOACTIVATE) };
    (hwnd, at)
}

/// What `begin_incoming` reports for how the drag ended: the folder for a drop
/// (once the shell has made the files), an error otherwise.
pub(crate) fn conclude(outcome: Result<Ending>, ended: &Ended, at: POINT) -> Result<PathBuf> {
    match outcome {
        Ok(Ending::Dropped) => {
            // The shell creates the files after `DoDragDrop` has returned;
            // wait for it so the transfer does not race the empty files.
            if ended.started() {
                ended.wait(COPY_LIMIT);
            }
            dropdir::folder_at(at)
        }
        Ok(Ending::Cancelled) => Err(Error::Os("drag cancelled".into())),
        Err(e) => Err(e),
    }
}

/// Starts the drag on this thread and returns the folder it was dropped in.
fn run(names: Vec<String>) -> Result<PathBuf> {
    dpi::physical_thread();
    crate::inject::os("OleInitialize", unsafe { OleInitialize(None) })?;
    let outcome = drag(names);
    unsafe { OleUninitialize() };
    outcome
}

fn drag(names: Vec<String>) -> Result<PathBuf> {
    let press = || send(&[mouse_input(MOUSEEVENTF_LEFTDOWN, 0, MOUSER_TAG)]);
    drag_with(names, &press, START_LIMIT)
}

/// [`drag`] with the press that starts it and the time allowed for it supplied.
pub(crate) fn drag_with(
    names: Vec<String>,
    press: &dyn Fn() -> Result<()>,
    start_limit: Duration,
) -> Result<PathBuf> {
    let ended = Arc::new(Ended::default());
    let dropped_at = Arc::new(DropPoint::default());
    let pending = Pending {
        data: new_data_object(names, ended.clone()),
        source: Source {
            at: dropped_at.clone(),
        }
        .into(),
        result: Mutex::new(None),
    };
    let (hwnd, _) = helper_window(&pending);
    let pressed = press();
    let begun = Instant::now();
    while pressed.is_ok()
        && pending.result.lock().unwrap().is_none()
        && begun.elapsed() < start_limit
    {
        pump();
        unsafe { MsgWaitForMultipleObjects(None, false, 25, QS_ALLINPUT) };
    }
    let finished = pending.result.lock().unwrap().take();
    let outcome = match (pressed, finished) {
        (Err(e), _) => Err(e),
        (Ok(()), None) => {
            // Never got the press; do not leave the button down.
            let _ = send(&[mouse_input(MOUSEEVENTF_LEFTUP, 0, MOUSER_TAG)]);
            Err(Error::Os("the drag could not be started".into()))
        }
        (Ok(()), Some((result, effect))) => ending(result, effect),
    };
    let folder = conclude(outcome, &ended, unpack(dropped_at.0.load(Ordering::SeqCst)));
    let _ = unsafe { DestroyWindow(hwnd) };
    drop(pending);
    folder
}
