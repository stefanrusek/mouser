//! Displays (W8): Per-Monitor-V2 rectangles from `EnumDisplayMonitors` /
//! `GetMonitorInfoW`, scale from `GetDpiForMonitor(MDT_EFFECTIVE_DPI)`,
//! physical size from the monitor's EDID, and a watcher that re-enumerates on
//! `WM_DISPLAYCHANGE` / `WM_DPICHANGED`.
//!
//! Desktop coordinates: a display's `origin` is its physical top-left pixel and
//! it spans `logical_size` (pixels / scale) from there, so desktop point `p` on
//! display `d` is physical pixel `d.origin + (p - d.origin) * d.scale`. At
//! scale 1 that is the physical pixel itself, and displays never overlap.

use crate::dpi;
use mouser_platform::{LocalDisplay, Result, WatchHandle};
use std::sync::mpsc::Sender;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    DISPLAY_DEVICEW, EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR,
    MONITORINFOEXW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_BINARY, RegGetValueW};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, EDD_GET_DEVICE_INTERFACE_NAME,
    GWLP_USERDATA, GetMessageW, GetWindowLongPtrW, MSG, PostMessageW, PostQuitMessage,
    RegisterClassW, SetWindowLongPtrW, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_DISPLAYCHANGE,
    WM_DPICHANGED, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::{BOOL, HSTRING, w};

/// What Windows reports for one monitor, before conversion.
pub(crate) struct Monitor {
    /// Monitor device interface path (stable across reboots).
    pub(crate) path: String,
    /// The monitor driver's description ("Generic PnP Monitor").
    pub(crate) description: String,
    /// Physical-pixel rectangle.
    pub(crate) rect: RECT,
    pub(crate) dpi: u32,
    pub(crate) edid: Option<Vec<u8>>,
}

/// `(width, height)` in mm: the first detailed timing descriptor's image size,
/// else the basic block's centimetres; `None` when the EDID states neither.
pub(crate) fn edid_size_mm(edid: &[u8]) -> Option<(f64, f64)> {
    let dtd_w = u32::from(edid[66]) | (u32::from(edid[68] & 0xF0) << 4);
    let dtd_h = u32::from(edid[67]) | (u32::from(edid[68] & 0x0F) << 8);
    let (w, h) = if dtd_w > 0 && dtd_h > 0 {
        (f64::from(dtd_w), f64::from(dtd_h))
    } else {
        (f64::from(edid[21]) * 10.0, f64::from(edid[22]) * 10.0)
    };
    (w > 0.0 && h > 0.0).then_some((w, h))
}

/// The monitor name descriptor (tag 0xFC), if the EDID has one.
pub(crate) fn edid_name(edid: &[u8]) -> Option<String> {
    [54, 72, 90, 108].iter().find_map(|&at| {
        let d = &edid[at..at + 18];
        (d[..3] == [0, 0, 0] && d[3] == 0xFC).then(|| {
            let text = &d[5..];
            let end = text.iter().position(|&b| b == 0x0A).unwrap_or(text.len());
            String::from_utf8_lossy(&text[..end]).trim().to_string()
        })
    })
}

pub(crate) fn to_display(m: &Monitor) -> LocalDisplay {
    let scale = m.dpi as f32 / 96.0;
    let pixel_size = (
        (m.rect.right - m.rect.left) as u32,
        (m.rect.bottom - m.rect.top) as u32,
    );
    let edid = m.edid.as_deref();
    LocalDisplay {
        key: m.path.clone(),
        name: edid
            .and_then(edid_name)
            .unwrap_or_else(|| m.description.clone()),
        pixel_size,
        logical_size: (
            f64::from(pixel_size.0) / f64::from(scale),
            f64::from(pixel_size.1) / f64::from(scale),
        ),
        scale,
        physical_size_mm: edid.and_then(edid_size_mm),
        origin: (f64::from(m.rect.left), f64::from(m.rect.top)),
    }
}

/// `\\?\DISPLAY#PRL5000#3&21de92e8&0&UID0#{e6f07b5f-...}` ->
/// `SYSTEM\CurrentControlSet\Enum\DISPLAY\PRL5000\3&21de92e8&0&UID0\Device Parameters`.
pub(crate) fn device_parameters_key(path: &str) -> String {
    let parts: Vec<&str> = path.trim_start_matches(r"\\?\").split('#').collect();
    format!(
        r"SYSTEM\CurrentControlSet\Enum\{}\{}\{}\Device Parameters",
        parts[0], parts[1], parts[2]
    )
}

pub(crate) fn read_edid(path: &str) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 512];
    let mut size = buf.len() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(device_parameters_key(path)),
            w!("EDID"),
            RRF_RT_REG_BINARY,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    buf.truncate(size as usize);
    (r.is_ok() && size >= 128).then_some(buf)
}

fn wide_to_string(w: &[u16]) -> String {
    String::from_utf16_lossy(&w[..w.iter().position(|&c| c == 0).unwrap_or(w.len())])
}

unsafe extern "system" fn collect(h: HMONITOR, _dc: HDC, _r: *mut RECT, list: LPARAM) -> BOOL {
    unsafe { (*(list.0 as *mut Vec<HMONITOR>)).push(h) };
    true.into()
}

fn monitor(h: HMONITOR) -> Monitor {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    unsafe { GetMonitorInfoW(h, &mut info.monitorInfo) }
        .ok()
        .expect("GetMonitorInfoW");
    let (mut dpi, mut dpi_y) = (0, 0);
    unsafe { GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y) }
        .expect("GetDpiForMonitor");
    let mut dev = DISPLAY_DEVICEW {
        cb: size_of::<DISPLAY_DEVICEW>() as u32,
        ..Default::default()
    };
    let device = HSTRING::from(wide_to_string(&info.szDevice));
    unsafe { EnumDisplayDevicesW(&device, 0, &mut dev, EDD_GET_DEVICE_INTERFACE_NAME) }
        .ok()
        .expect("EnumDisplayDevicesW");
    let path = wide_to_string(&dev.DeviceID);
    Monitor {
        edid: read_edid(&path),
        path,
        description: wide_to_string(&dev.DeviceString),
        rect: info.monitorInfo.rcMonitor,
        dpi,
    }
}

pub(crate) fn displays() -> Result<Vec<LocalDisplay>> {
    dpi::physical(|| {
        let mut handles: Vec<HMONITOR> = Vec::new();
        unsafe {
            EnumDisplayMonitors(None, None, Some(collect), LPARAM(&raw mut handles as isize))
        }
        .ok()
        .expect("EnumDisplayMonitors");
        Ok(handles
            .into_iter()
            .map(|h| to_display(&monitor(h)))
            .collect())
    })
}

unsafe extern "system" fn watch_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            let tx = unsafe {
                &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Sender<Vec<LocalDisplay>>)
            };
            // A receiver that has gone away just means nobody is listening.
            let _ = tx.send(displays().expect("displays"));
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// A hidden top-level window (message-only windows get no display broadcasts)
/// on its own thread; returns its handle with the watch.
pub(crate) fn start_watch(tx: Sender<Vec<LocalDisplay>>) -> (HWND, WatchHandle) {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        dpi::physical_thread();
        let hinst = unsafe { GetModuleHandleW(None) }.expect("module handle");
        let class = WNDCLASSW {
            lpfnWndProc: Some(watch_proc),
            hInstance: hinst.into(),
            lpszClassName: w!("MouserDisplayWatch"),
            ..Default::default()
        };
        unsafe { RegisterClassW(&class) };
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("MouserDisplayWatch"),
                w!("Mouser display watch"),
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
        .expect("display watch window");
        let tx = Box::into_raw(Box::new(tx));
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, tx as isize) };
        ready_tx.send(hwnd.0 as isize).expect("watch ready");
        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            unsafe { DispatchMessageW(&msg) };
        }
        drop(unsafe { Box::from_raw(tx) });
    });
    let hwnd = HWND(ready_rx.recv().expect("display watch thread") as _);
    let raw = hwnd.0 as isize;
    let handle = WatchHandle::new(move || {
        unsafe { PostMessageW(Some(HWND(raw as _)), WM_CLOSE, WPARAM(0), LPARAM(0)) }
            .expect("stop display watch");
        thread.join().expect("display watch thread");
    });
    (hwnd, handle)
}

/// `WM_CLOSE` goes to `DefWindowProcW`, which destroys the window; its
/// `WM_DESTROY` ends the thread's loop.
pub(crate) fn watch(tx: Sender<Vec<LocalDisplay>>) -> Result<WatchHandle> {
    Ok(start_watch(tx).1)
}

/// The display holding physical pixel `(x, y)`; the first when none does.
fn display_at_pixel(all: &[LocalDisplay], x: f64, y: f64) -> &LocalDisplay {
    all.iter()
        .find(|d| {
            x >= d.origin.0
                && x < d.origin.0 + f64::from(d.pixel_size.0)
                && y >= d.origin.1
                && y < d.origin.1 + f64::from(d.pixel_size.1)
        })
        .unwrap_or(&all[0])
}

/// A physical pixel as desktop logical coordinates, with the scale of the
/// display it is on (the module header gives the mapping).
pub(crate) fn pixel_to_logical(all: &[LocalDisplay], x: i32, y: i32) -> ((f64, f64), f64) {
    let d = display_at_pixel(all, f64::from(x), f64::from(y));
    let scale = f64::from(d.scale);
    (
        (
            d.origin.0 + (f64::from(x) - d.origin.0) / scale,
            d.origin.1 + (f64::from(y) - d.origin.1) / scale,
        ),
        scale,
    )
}

/// Desktop logical coordinates as a physical pixel; the first display stands
/// in for a point outside every display.
pub(crate) fn logical_to_pixel(all: &[LocalDisplay], x: f64, y: f64) -> (i32, i32) {
    let d = all.iter().find(|d| contains(d, x, y)).unwrap_or(&all[0]);
    let scale = f64::from(d.scale);
    (
        (d.origin.0 + (x - d.origin.0) * scale).round() as i32,
        (d.origin.1 + (y - d.origin.1) * scale).round() as i32,
    )
}

/// Whether logical point (`x`, `y`) is on display `d`.
fn contains(d: &LocalDisplay, x: f64, y: f64) -> bool {
    x >= d.origin.0
        && x < d.origin.0 + d.logical_size.0
        && y >= d.origin.1
        && y < d.origin.1 + d.logical_size.1
}

/// Whether logical point (`x`, `y`) is on one of the displays `all`.
pub(crate) fn on_a_display(all: &[LocalDisplay], x: f64, y: f64) -> bool {
    all.iter().any(|d| contains(d, x, y))
}

/// Read access to a [`DisplayCache`] from another thread.
#[derive(Clone)]
pub(crate) struct DisplayView(std::sync::Arc<std::sync::Mutex<Vec<LocalDisplay>>>);

impl DisplayView {
    pub(crate) fn snapshot(&self) -> Vec<LocalDisplay> {
        self.0.lock().unwrap().clone()
    }
}

/// The current displays, kept fresh by a watch, for code that converts
/// coordinates on every input event.
pub(crate) struct DisplayCache {
    shown: std::sync::Arc<std::sync::Mutex<Vec<LocalDisplay>>>,
    #[cfg_attr(not(test), allow(dead_code))]
    updates: std::sync::Arc<std::sync::atomic::AtomicU32>,
    #[cfg_attr(not(test), allow(dead_code))]
    hwnd: isize,
    _watch: WatchHandle,
}

impl DisplayCache {
    pub(crate) fn new() -> Self {
        let shown = std::sync::Arc::new(std::sync::Mutex::new(displays().expect("displays")));
        let updates = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let (tx, rx) = std::sync::mpsc::channel();
        let (hwnd, watch) = start_watch(tx);
        let (into, count) = (shown.clone(), updates.clone());
        // Ends when the watch drops its sender.
        std::thread::spawn(move || {
            while let Ok(next) = rx.recv() {
                *into.lock().unwrap() = next;
                count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
        });
        DisplayCache {
            shown,
            updates,
            hwnd: hwnd.0 as isize,
            _watch: watch,
        }
    }

    /// The watch window, for tests to post display messages to.
    #[cfg(test)]
    pub(crate) fn hwnd(&self) -> HWND {
        HWND(self.hwnd as _)
    }

    pub(crate) fn snapshot(&self) -> Vec<LocalDisplay> {
        self.shown.lock().unwrap().clone()
    }

    /// A handle other threads can read the displays through.
    pub(crate) fn view(&self) -> DisplayView {
        DisplayView(self.shown.clone())
    }

    /// How many watch updates have been applied.
    #[cfg(test)]
    pub(crate) fn updates(&self) -> u32 {
        self.updates.load(std::sync::atomic::Ordering::SeqCst)
    }
}
