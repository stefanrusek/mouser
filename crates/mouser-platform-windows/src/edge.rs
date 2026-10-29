//! Noticing files being dragged out of this PC.
//!
//! An OLE drag only tells the window under the pointer what is being dragged.
//! So four nearly invisible strips along the outer border of the desktop are
//! registered as drop targets; when a drag of files reaches one, its
//! `DragEnter` reads the file list from the data object. The strips exist
//! (are shown) only while the left button is down, so they never get in the
//! way of ordinary clicks on a screen edge.
//!
//! The strips never accept a drop: they only watch.

use crate::clipboard::parse_dropfiles;
use crate::dpi;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, POINTL, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::Com::{
    DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, TYMED_HGLOBAL,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{
    DROPEFFECT, DROPEFFECT_NONE, IDropTarget, IDropTarget_Impl, OleInitialize, OleUninitialize,
    RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetSystemMetrics,
    HWND_TOPMOST, LWA_ALPHA, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
    RegisterClassW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SW_HIDE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{Ref, Result, implement, w};

/// Strip thickness in physical pixels: wide enough that a fast drag cannot
/// jump over it between two pointer samples.
pub(crate) const THICKNESS: i32 = 8;

/// How often the strips follow the button.
const POLL: u32 = 25;

/// How long `outgoing` waits for the drag's `DragEnter` when the button is
/// down but the strip has not seen the drag yet.
pub(crate) const ENTER_WAIT: Duration = Duration::from_millis(150);

/// The four strips along the outer border of `desktop`: left, top, right,
/// bottom (the top and bottom strips span the corners).
pub(crate) fn strips(desktop: RECT) -> [RECT; 4] {
    let t = THICKNESS;
    [
        RECT {
            right: desktop.left + t,
            ..desktop
        },
        RECT {
            bottom: desktop.top + t,
            ..desktop
        },
        RECT {
            left: desktop.right - t,
            ..desktop
        },
        RECT {
            top: desktop.bottom - t,
            ..desktop
        },
    ]
}

pub(crate) fn left_button_down() -> bool {
    let state = unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON.0)) };
    state as u16 & 0x8000 != 0
}

/// The files of the drag that reached a strip.
#[derive(Default)]
pub(crate) struct Pending {
    files: Mutex<Option<Vec<PathBuf>>>,
    changed: Condvar,
}

impl Pending {
    pub(crate) fn set(&self, files: Vec<PathBuf>) {
        *self.files.lock().unwrap() = (!files.is_empty()).then_some(files);
        self.changed.notify_all();
    }

    pub(crate) fn clear(&self) {
        *self.files.lock().unwrap() = None;
    }

    /// The files, once; waits up to `limit` for them to arrive.
    pub(crate) fn take(&self, limit: Duration) -> Option<Vec<PathBuf>> {
        let guard = self.files.lock().unwrap();
        let (mut guard, _) = self
            .changed
            .wait_timeout_while(guard, limit, |files| files.is_none())
            .unwrap();
        guard.take()
    }
}

/// The bytes of a global memory block.
fn block_bytes(block: windows::Win32::Foundation::HGLOBAL) -> Vec<u8> {
    let size = unsafe { GlobalSize(block) };
    let at = unsafe { GlobalLock(block) };
    let bytes = unsafe { std::slice::from_raw_parts(at.cast::<u8>(), size) }.to_vec();
    let _ = unsafe { GlobalUnlock(block) };
    bytes
}

/// The paths a drag's data object offers as `CF_HDROP`; empty when it offers
/// none (text, or virtual files that have no path).
pub(crate) fn dragged_paths(data: &IDataObject) -> Vec<PathBuf> {
    let format = FORMATETC {
        cfFormat: 15, // CF_HDROP
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    let medium: Option<STGMEDIUM> = unsafe { data.GetData(&format) }.ok();
    let paths = medium
        .as_ref()
        .map(|m| parse_dropfiles(&block_bytes(unsafe { m.u.hGlobal })));
    if let Some(mut medium) = medium {
        unsafe { ReleaseStgMedium(&mut medium) };
    }
    paths.unwrap_or_default()
}

#[implement(IDropTarget)]
pub(crate) struct Watcher {
    pub(crate) pending: Arc<Pending>,
}

impl IDropTarget_Impl for Watcher_Impl {
    fn DragEnter(
        &self,
        data: Ref<windows::Win32::System::Com::IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _at: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        self.pending
            .set(data.as_ref().map(dragged_paths).unwrap_or_default());
        unsafe { *effect = DROPEFFECT_NONE };
        Ok(())
    }

    fn DragOver(
        &self,
        _keys: MODIFIERKEYS_FLAGS,
        _at: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        unsafe { *effect = DROPEFFECT_NONE };
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        self.pending.clear();
        Ok(())
    }

    fn Drop(
        &self,
        _data: Ref<windows::Win32::System::Com::IDataObject>,
        _keys: MODIFIERKEYS_FLAGS,
        _at: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        self.pending.clear();
        unsafe { *effect = DROPEFFECT_NONE };
        Ok(())
    }
}

/// The desktop's bounding rectangle in physical pixels.
fn desktop() -> RECT {
    let metric = |m| unsafe { GetSystemMetrics(m) };
    let (left, top) = (metric(SM_XVIRTUALSCREEN), metric(SM_YVIRTUALSCREEN));
    RECT {
        left,
        top,
        right: left + metric(SM_CXVIRTUALSCREEN),
        bottom: top + metric(SM_CYVIRTUALSCREEN),
    }
}

fn show_strips(windows: &[HWND; 4]) {
    for (hwnd, r) in windows.iter().zip(strips(desktop())) {
        let _ = unsafe {
            SetWindowPos(
                *hwnd,
                Some(HWND_TOPMOST),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        };
    }
}

fn hide_strips(windows: &[HWND; 4]) {
    for hwnd in windows {
        let _ = unsafe { ShowWindow(*hwnd, SW_HIDE) };
    }
}

/// Creates the (hidden) strips and registers each as a drop target.
fn create_strips(pending: &Arc<Pending>) -> [HWND; 4] {
    let hinst = unsafe { GetModuleHandleW(None) }.expect("module handle");
    let class = WNDCLASSW {
        lpfnWndProc: Some(strip_proc),
        hInstance: hinst.into(),
        lpszClassName: w!("MouserEdgeStrip"),
        hbrBackground: HBRUSH::default(),
        ..Default::default()
    };
    unsafe { RegisterClassW(&class) };
    std::array::from_fn(|_| {
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("MouserEdgeStrip"),
                w!("Mouser edge"),
                WS_POPUP,
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
        .expect("edge strip");
        // Nearly invisible, but not so transparent that clicks fall through.
        unsafe { SetLayeredWindowAttributes(hwnd, Default::default(), 1, LWA_ALPHA) }
            .expect("edge strip opacity");
        let target: IDropTarget = Watcher {
            pending: pending.clone(),
        }
        .into();
        unsafe { RegisterDragDrop(hwnd, &target) }.expect("edge strip drop target");
        hwnd
    })
}

unsafe extern "system" fn strip_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn pump() {
    let mut msg = MSG::default();
    while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
        unsafe { DispatchMessageW(&msg) };
    }
}

/// The strips on a thread of their own, and what they have seen.
pub(crate) struct Edge {
    pub(crate) pending: Arc<Pending>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    #[cfg_attr(not(test), allow(dead_code))]
    strips: [isize; 4],
}

impl Edge {
    pub(crate) fn new() -> Self {
        let pending = Arc::new(Pending::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let thread = {
            let (pending, stop) = (pending.clone(), stop.clone());
            std::thread::spawn(move || {
                dpi::physical_thread();
                unsafe { OleInitialize(None) }.expect("OLE on the edge thread");
                let windows = create_strips(&pending);
                ready_tx
                    .send(windows.map(|h| h.0 as isize))
                    .expect("edge ready");
                let mut shown = false;
                while !stop.load(Ordering::SeqCst) {
                    pump();
                    let down = left_button_down();
                    if down != shown {
                        if down {
                            show_strips(&windows);
                        } else {
                            hide_strips(&windows);
                            pending.clear();
                        }
                        shown = down;
                    }
                    unsafe { MsgWaitForMultipleObjects(None, false, POLL, QS_ALLINPUT) };
                }
                for hwnd in windows {
                    let _ = unsafe { RevokeDragDrop(hwnd) };
                    let _ = unsafe { DestroyWindow(hwnd) };
                }
                unsafe { OleUninitialize() };
            })
        };
        Edge {
            pending,
            stop,
            thread: Some(thread),
            strips: ready_rx.recv().expect("edge strips"),
        }
    }

    /// The files of the drag now at an edge, once.
    pub(crate) fn take(&self, wait: Duration) -> Option<Vec<PathBuf>> {
        self.pending.take(wait)
    }

    #[cfg(test)]
    pub(crate) fn strips(&self) -> [HWND; 4] {
        self.strips.map(|h| HWND(h as _))
    }
}

/// The processes other than `own` that have edge strips: another Mouser
/// beside this one, whose strips lie over the same border as ours.
#[cfg(test)]
pub(crate) fn others_with_strips(own: u32) -> Vec<u32> {
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetWindowThreadProcessId};
    use windows::core::PCWSTR;
    let mut pids = Vec::new();
    let mut after = None;
    while let Ok(strip) =
        unsafe { FindWindowExW(None, after, w!("MouserEdgeStrip"), PCWSTR::null()) }
    {
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(strip, Some(&mut pid)) };
        if pid != own && !pids.contains(&pid) {
            pids.push(pid);
        }
        after = Some(strip);
    }
    pids
}

impl Drop for Edge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.thread
            .take()
            .expect("edge thread")
            .join()
            .expect("edge thread");
    }
}
