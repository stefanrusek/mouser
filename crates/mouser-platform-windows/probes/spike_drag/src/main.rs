//! Hardware probe 3 (T368): DoDragDrop started by an injected button-down; virtual-file
//! drop into Explorer with IDataObjectAsyncCapability.
//!
//! `spike_drag own`        part A: injected drag from a source window to our own drop target
//! `spike_drag explorer`   part B: virtual-file drop into an Explorer window, async then sync
//!
//! All mouse input is injected with SendInput; the probe opens and closes its own
//! Explorer windows on fresh folders under %TEMP%\spike_drag.

use std::cell::{Cell, RefCell};
use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering::SeqCst};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::*;
use windows::Win32::System::Ole::*;
use windows::Win32::System::SystemServices::*;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, HRESULT, Interface, PCWSTR, Ref, Result, w};
use windows_core::implement;

static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
static MAIN_TID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn log(s: &str) {
    let t = T0.get_or_init(Instant::now).elapsed().as_millis();
    let tid = unsafe { GetCurrentThreadId() };
    let which = if tid == MAIN_TID.load(SeqCst) { "main" } else { "other" };
    println!("[{t:>6} ms tid={tid} {which}] {s}");
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn format_name(cf: u16) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClipboardFormatNameW(cf as u32, &mut buf) };
    if n > 0 {
        String::from_utf16_lossy(&buf[..n as usize])
    } else {
        format!("CF#{cf}")
    }
}

fn cf(name: PCWSTR) -> u16 {
    unsafe { RegisterClipboardFormatW(name) as u16 }
}

// ---------------------------------------------------------------- data object

struct VFile {
    name: &'static str,
    data: Vec<u8>,
}

#[implement(IDataObject, IDataObjectAsyncCapability)]
struct VirtualFiles {
    files: Vec<VFile>,
    cf_desc: u16,
    cf_contents: u16,
    offer_async: bool,
    async_mode: Cell<bool>,
    in_op: Cell<bool>,
    ended: std::sync::Arc<AtomicBool>,
}

impl VirtualFiles {
    fn formats(&self) -> Vec<FORMATETC> {
        let mut v = vec![FORMATETC {
            cfFormat: self.cf_desc,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        }];
        v.push(FORMATETC {
            cfFormat: self.cf_contents,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_ISTREAM.0 as u32,
        });
        v
    }

    fn descriptor(&self) -> HGLOBAL {
        let size = size_of::<u32>() + self.files.len() * size_of::<FILEDESCRIPTORW>();
        unsafe {
            let h = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, size + 8).unwrap();
            let p = GlobalLock(h) as *mut u8;
            *(p as *mut u32) = self.files.len() as u32;
            let arr = p.add(size_of::<u32>()) as *mut FILEDESCRIPTORW;
            for (i, f) in self.files.iter().enumerate() {
                let mut d = FILEDESCRIPTORW {
                    dwFlags: (FD_FILESIZE.0 | FD_PROGRESSUI.0) as u32,
                    nFileSizeLow: f.data.len() as u32,
                    ..Default::default()
                };
                for (j, c) in f.name.encode_utf16().enumerate() {
                    d.cFileName[j] = c;
                }
                std::ptr::write_unaligned(arr.add(i), d);
            }
            let _ = GlobalUnlock(h);
            h
        }
    }
}

impl IDataObject_Impl for VirtualFiles_Impl {
    fn GetData(&self, f: *const FORMATETC) -> Result<STGMEDIUM> {
        let f = unsafe { &*f };
        log(&format!("GetData {} lindex={} tymed=0x{:x}", format_name(f.cfFormat), f.lindex, f.tymed));
        if f.cfFormat == self.cf_desc && f.tymed & TYMED_HGLOBAL.0 as u32 != 0 {
            return Ok(STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: self.descriptor() },
                pUnkForRelease: ManuallyDrop::new(None),
            });
        }
        if f.cfFormat == self.cf_contents && f.tymed & TYMED_ISTREAM.0 as u32 != 0 {
            let i = f.lindex as usize;
            let stream = unsafe { SHCreateMemStream(Some(&self.files[i].data)) };
            return Ok(STGMEDIUM {
                tymed: TYMED_ISTREAM.0 as u32,
                u: STGMEDIUM_0 { pstm: ManuallyDrop::new(stream) },
                pUnkForRelease: ManuallyDrop::new(None),
            });
        }
        Err(DV_E_FORMATETC.into())
    }

    fn GetDataHere(&self, _f: *const FORMATETC, _m: *mut STGMEDIUM) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, f: *const FORMATETC) -> HRESULT {
        let f = unsafe { &*f };
        let ok = self.formats().iter().any(|x| x.cfFormat == f.cfFormat && x.tymed & f.tymed != 0);
        if ok { S_OK } else { DV_E_FORMATETC }
    }

    fn GetCanonicalFormatEtc(&self, _i: *const FORMATETC, o: *mut FORMATETC) -> HRESULT {
        unsafe { (*o).ptd = std::ptr::null_mut() };
        DATA_S_SAMEFORMATETC
    }

    fn SetData(&self, f: *const FORMATETC, m: *const STGMEDIUM, release: BOOL) -> Result<()> {
        let f = unsafe { &*f };
        let m = unsafe { &*m };
        let mut detail = String::new();
        if m.tymed == TYMED_HGLOBAL.0 as u32 {
            unsafe {
                let h = m.u.hGlobal;
                let n = GlobalSize(h).min(24);
                let p = GlobalLock(h) as *const u8;
                if !p.is_null() {
                    let bytes = std::slice::from_raw_parts(p, n);
                    detail = format!(" first bytes {:02x?}", bytes);
                    let _ = GlobalUnlock(h);
                }
            }
        }
        log(&format!("SetData {} tymed=0x{:x} release={}{detail}", format_name(f.cfFormat), m.tymed, release.as_bool()));
        if release.as_bool() {
            unsafe { ReleaseStgMedium(m as *const _ as *mut _) };
        }
        Ok(())
    }

    fn EnumFormatEtc(&self, dir: u32) -> Result<IEnumFORMATETC> {
        log(&format!("EnumFormatEtc dir={dir}"));
        if dir == DATADIR_GET.0 as u32 {
            unsafe { SHCreateStdEnumFmtEtc(&self.formats()) }
        } else {
            Err(E_NOTIMPL.into())
        }
    }

    fn DAdvise(&self, _f: *const FORMATETC, _a: u32, _s: Ref<IAdviseSink>) -> Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _c: u32) -> Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

impl IDataObjectAsyncCapability_Impl for VirtualFiles_Impl {
    fn SetAsyncMode(&self, on: BOOL) -> Result<()> {
        log(&format!("Async SetAsyncMode({})", on.as_bool()));
        self.async_mode.set(on.as_bool());
        Ok(())
    }
    fn GetAsyncMode(&self) -> Result<BOOL> {
        let v = self.offer_async && self.async_mode.get();
        log(&format!("Async GetAsyncMode -> {v}"));
        Ok(v.into())
    }
    fn StartOperation(&self, _b: Ref<IBindCtx>) -> Result<()> {
        log("Async StartOperation");
        self.in_op.set(true);
        Ok(())
    }
    fn InOperation(&self) -> Result<BOOL> {
        log(&format!("Async InOperation -> {}", self.in_op.get()));
        Ok(self.in_op.get().into())
    }
    fn EndOperation(&self, hr: HRESULT, _b: Ref<IBindCtx>, effects: u32) -> Result<()> {
        log(&format!("Async EndOperation hr=0x{:08x} effects={effects}", hr.0));
        self.in_op.set(false);
        self.ended.store(true, SeqCst);
        Ok(())
    }
}

// ---------------------------------------------------------------- drop source

#[implement(IDropSource)]
struct Source {
    first: Cell<bool>,
    calls: Cell<u32>,
}

impl IDropSource_Impl for Source_Impl {
    fn QueryContinueDrag(&self, esc: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        self.calls.set(self.calls.get() + 1);
        let lbutton = keys.0 & MK_LBUTTON.0 != 0;
        if self.first.replace(false) {
            log(&format!("QueryContinueDrag first call: esc={} MK_LBUTTON={lbutton}", esc.as_bool()));
        }
        if esc.as_bool() {
            log("QueryContinueDrag -> cancel (escape)");
            return DRAGDROP_S_CANCEL;
        }
        if !lbutton {
            log(&format!("QueryContinueDrag -> drop (button released) after {} calls", self.calls.get()));
            return DRAGDROP_S_DROP;
        }
        S_OK
    }
    fn GiveFeedback(&self, _e: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

// ---------------------------------------------------------------- own drop target

#[implement(IDropTarget)]
struct Target {
    overs: Cell<u32>,
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(&self, obj: Ref<IDataObject>, _k: MODIFIERKEYS_FLAGS, pt: &POINTL, eff: *mut DROPEFFECT) -> Result<()> {
        let has_desc = obj.ok().map(|o| {
            let f = FORMATETC {
                cfFormat: cf(CFSTR_FILEDESCRIPTORW),
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
            };
            unsafe { o.QueryGetData(&f) == S_OK }
        });
        log(&format!("Target DragEnter at ({},{}) FileGroupDescriptorW offered={has_desc:?}", pt.x, pt.y));
        unsafe { *eff = DROPEFFECT_COPY };
        Ok(())
    }
    fn DragOver(&self, _k: MODIFIERKEYS_FLAGS, _pt: &POINTL, eff: *mut DROPEFFECT) -> Result<()> {
        self.overs.set(self.overs.get() + 1);
        unsafe { *eff = DROPEFFECT_COPY };
        Ok(())
    }
    fn DragLeave(&self) -> Result<()> {
        log("Target DragLeave");
        Ok(())
    }
    fn Drop(&self, _obj: Ref<IDataObject>, _k: MODIFIERKEYS_FLAGS, pt: &POINTL, eff: *mut DROPEFFECT) -> Result<()> {
        log(&format!("Target Drop at ({},{}) after {} DragOver", pt.x, pt.y, self.overs.get()));
        unsafe { *eff = DROPEFFECT_COPY };
        Ok(())
    }
}

// ---------------------------------------------------------------- windows + injection

thread_local! {
    static DRAG: RefCell<Option<(IDataObject, IDropSource)>> = const { RefCell::new(None) };
}
static RESULT: AtomicIsize = AtomicIsize::new(-1);

unsafe extern "system" fn source_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_LBUTTONDOWN => {
            let extra = unsafe { GetMessageExtraInfo().0 };
            log(&format!("source WM_LBUTTONDOWN (message extra info 0x{extra:x}) -> DoDragDrop"));
            let pair = DRAG.with(|d| d.borrow_mut().take());
            if let Some((obj, src)) = pair {
                let mut effect = DROPEFFECT(0);
                let hr = unsafe { DoDragDrop(&obj, &src, DROPEFFECT_COPY, &mut effect) };
                log(&format!("DoDragDrop returned hr=0x{:08x} effect={}", hr.0, effect.0));
                RESULT.store(hr.0 as isize, SeqCst);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            unsafe {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);
                let mut rc = RECT::default();
                GetClientRect(hwnd, &mut rc).unwrap();
                let mut t = wide("spike_drag source (drag starts here)");
                let n = t.len() - 1;
                DrawTextW(hdc, &mut t[..n], &mut rc, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
                let _ = EndPaint(hwnd, &ps);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn make_window(class: PCWSTR, title: &str, proc_: WNDPROC, r: RECT) -> HWND {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let wc = WNDCLASSW {
            lpfnWndProc: proc_,
            hInstance: hinst,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let t = wide(title);
        CreateWindowExW(
            WS_EX_TOPMOST,
            class,
            PCWSTR(t.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            r.left,
            r.top,
            r.right - r.left,
            r.bottom - r.top,
            None,
            None,
            Some(hinst),
            None,
        )
        .unwrap()
    }
}

unsafe extern "system" fn target_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn abs_move(x: i32, y: i32) -> INPUT {
    let w = unsafe { GetSystemMetrics(SM_CXSCREEN) } - 1;
    let h = unsafe { GetSystemMetrics(SM_CYSCREEN) } - 1;
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: x * 65535 / w,
                dy: y * 65535 / h,
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                dwExtraInfo: 0x4D4F_5553,
                ..Default::default()
            },
        },
    }
}

fn button(down: bool) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: if down { MOUSEEVENTF_LEFTDOWN } else { MOUSEEVENTF_LEFTUP },
                dwExtraInfo: 0x4D4F_5553,
                ..Default::default()
            },
        },
    }
}

fn send(i: INPUT) {
    let n = unsafe { SendInput(&[i], size_of::<INPUT>() as i32) };
    assert_eq!(n, 1, "SendInput failed: {:?}", unsafe { GetLastError() });
}

/// Press at `from`, glide to `to` in 25 steps, release; on its own thread.
fn inject_drag(from: POINT, to: POINT) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        send(abs_move(from.x, from.y));
        std::thread::sleep(Duration::from_millis(150));
        log("inject: left button down at source");
        send(button(true));
        std::thread::sleep(Duration::from_millis(300));
        for s in 1..=25 {
            let x = from.x + (to.x - from.x) * s / 25;
            let y = from.y + (to.y - from.y) * s / 25;
            send(abs_move(x, y));
            std::thread::sleep(Duration::from_millis(40));
        }
        std::thread::sleep(Duration::from_millis(300));
        // A last wiggle so the target sees DragOver at the final point.
        send(abs_move(to.x + 2, to.y + 2));
        std::thread::sleep(Duration::from_millis(200));
        log("inject: left button up at target");
        send(button(false));
    })
}

fn pump_for(d: Duration) {
    let end = Instant::now() + d;
    unsafe {
        loop {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let now = Instant::now();
            if now >= end {
                break;
            }
            MsgWaitForMultipleObjects(None, false, ((end - now).as_millis() as u32).min(50), QS_ALLINPUT);
        }
    }
}

fn pump_until(limit: Duration, done: impl Fn() -> bool) -> bool {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        pump_for(Duration::from_millis(50));
        if done() {
            return true;
        }
    }
    false
}

fn center(r: RECT) -> POINT {
    POINT { x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2 }
}

fn setup() {
    T0.get_or_init(Instant::now);
    MAIN_TID.store(unsafe { GetCurrentThreadId() }, SeqCst);
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).unwrap();
        OleInitialize(None).unwrap();
    }
}

fn data_object(offer_async: bool, ended: std::sync::Arc<AtomicBool>) -> IDataObject {
    let big: Vec<u8> = (0..8 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    VirtualFiles {
        files: vec![
            VFile { name: "hello.txt", data: b"hello from the mouser drag probe\r\n".to_vec() },
            VFile { name: "big.bin", data: big },
        ],
        cf_desc: cf(CFSTR_FILEDESCRIPTORW),
        cf_contents: cf(CFSTR_FILECONTENTS),
        offer_async,
        async_mode: Cell::new(offer_async),
        in_op: Cell::new(false),
        ended,
    }
    .into()
}

fn source(drop_src: &mut Option<IDropSource>) -> IDropSource {
    let s: IDropSource = Source { first: Cell::new(true), calls: Cell::new(0) }.into();
    *drop_src = Some(s.clone());
    s
}

fn run_own() {
    setup();
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let src_rect = RECT { left: 80, top: 200, right: 480, bottom: 500 };
    let dst_rect = RECT { left: sw - 520, top: 200, right: sw - 120, bottom: 500 };
    let src_hwnd = make_window(w!("spike_drag_source"), "spike_drag source", Some(source_proc), src_rect);
    let dst_hwnd = make_window(w!("spike_drag_target"), "spike_drag target", Some(target_proc), dst_rect);
    let target: IDropTarget = Target { overs: Cell::new(0) }.into();
    unsafe { RegisterDragDrop(dst_hwnd, &target).unwrap() };
    let ended = std::sync::Arc::new(AtomicBool::new(false));
    let mut keep = None;
    DRAG.with(|d| *d.borrow_mut() = Some((data_object(false, ended), source(&mut keep))));
    pump_for(Duration::from_millis(500));

    let mut sr = RECT::default();
    let mut dr = RECT::default();
    unsafe {
        GetWindowRect(src_hwnd, &mut sr).unwrap();
        GetWindowRect(dst_hwnd, &mut dr).unwrap();
    }
    let inj = inject_drag(center(sr), center(dr));
    let finished = pump_until(Duration::from_secs(15), || RESULT.load(SeqCst) != -1);
    inj.join().unwrap();
    log(&format!("part A finished={finished} DoDragDrop hr=0x{:08x}", RESULT.load(SeqCst) as i32));
    unsafe {
        let _ = RevokeDragDrop(dst_hwnd);
        let _ = DestroyWindow(src_hwnd);
        let _ = DestroyWindow(dst_hwnd);
    }
}

fn find_explorer(folder_name: &str, limit: Duration) -> Option<HWND> {
    let end = Instant::now() + limit;
    let title = wide(&format!("{folder_name} - File Explorer"));
    while Instant::now() < end {
        if let Ok(h) = unsafe { FindWindowW(w!("CabinetWClass"), PCWSTR(title.as_ptr())) } {
            return Some(h);
        }
        pump_for(Duration::from_millis(100));
    }
    None
}

/// Folder shown by the Explorer window that is the root ancestor of `hwnd`, via
/// IShellWindows -> IShellBrowser -> IFolderView -> IPersistFolder2.
fn explorer_folder_at(hwnd: HWND) -> String {
    unsafe {
        let root = GetAncestor(hwnd, GA_ROOT);
        let windows: IShellWindows = match CoCreateInstance(&ShellWindows, None, CLSCTX_ALL) {
            Ok(w) => w,
            Err(e) => return format!("<ShellWindows: {e}>"),
        };
        let mut cls = [0u16; 64];
        let n = GetClassNameW(root, &mut cls) as usize;
        log(&format!("drop point root window {:?} class {}", root, String::from_utf16_lossy(&cls[..n])));
        let count = windows.Count().unwrap_or(0);
        log(&format!("IShellWindows count={count}"));
        for i in 0..count {
            let Ok(disp) = windows.Item(&windows::Win32::System::Variant::VARIANT::from(i)) else {
                log(&format!("  item {i}: Item failed"));
                continue;
            };
            let app_hwnd = disp.cast::<IWebBrowserApp>().and_then(|a| a.HWND()).map(|h| h.0);
            let Ok(sp) = disp.cast::<IServiceProvider>() else { continue };
            let Ok(browser) = sp.QueryService::<IShellBrowser>(&SID_STopLevelBrowser) else {
                log(&format!("  item {i}: IWebBrowserApp.HWND={app_hwnd:x?} no IShellBrowser"));
                continue;
            };
            let bw = browser.GetWindow();
            log(&format!("  item {i}: IWebBrowserApp.HWND={app_hwnd:x?} IShellBrowser.GetWindow={bw:?}"));
            let matches = bw.as_ref().is_ok_and(|w| *w == root) || app_hwnd.as_ref().is_ok_and(|h| *h == root.0 as isize);
            if !matches {
                continue;
            }
            let Ok(view) = browser.QueryActiveShellView() else { return "<no view>".into() };
            let Ok(fv) = view.cast::<IFolderView>() else { return "<no IFolderView>".into() };
            let Ok(pf) = fv.GetFolder::<IPersistFolder2>() else { return "<no IPersistFolder2>".into() };
            let Ok(pidl) = pf.GetCurFolder() else { return "<no pidl>".into() };
            let mut buf = [0u16; 260];
            let ok = SHGetPathFromIDListW(pidl, &mut buf).as_bool();
            CoTaskMemFree(Some(pidl as _));
            if !ok {
                return "<not a file system folder>".into();
            }
            let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            return String::from_utf16_lossy(&buf[..n]);
        }
        format!("<root window {:?} is not a shell window>", root)
    }
}

fn run_explorer_once(offer_async: bool) {
    let base = std::env::temp_dir().join("spike_drag");
    let name = format!("drop-{}-{}", if offer_async { "async" } else { "sync" }, std::process::id());
    let dir = base.join(&name);
    std::fs::create_dir_all(&dir).unwrap();
    log(&format!("=== part B ({}) into {}", if offer_async { "async offered" } else { "sync" }, dir.display()));
    std::process::Command::new("explorer.exe").arg(&dir).spawn().unwrap();
    let Some(exp) = find_explorer(&name, Duration::from_secs(15)) else {
        log("Explorer window did not appear");
        return;
    };
    unsafe {
        let _ = SetWindowPos(exp, None, 600, 150, 1000, 700, SWP_NOZORDER | SWP_SHOWWINDOW);
        let _ = SetForegroundWindow(exp);
    }
    pump_for(Duration::from_millis(1500));

    let src_rect = RECT { left: 60, top: 300, right: 460, bottom: 600 };
    let src_hwnd = make_window(w!("spike_drag_source"), "spike_drag source", Some(source_proc), src_rect);
    let ended = std::sync::Arc::new(AtomicBool::new(false));
    let mut keep = None;
    DRAG.with(|d| *d.borrow_mut() = Some((data_object(offer_async, ended.clone()), source(&mut keep))));
    RESULT.store(-1, SeqCst);
    pump_for(Duration::from_millis(500));

    let mut sr = RECT::default();
    let mut er = RECT::default();
    unsafe {
        GetWindowRect(src_hwnd, &mut sr).unwrap();
        GetWindowRect(exp, &mut er).unwrap();
    }
    // Into the file list, clear of the navigation pane and the ribbon.
    let drop_pt = POINT { x: er.left + (er.right - er.left) * 7 / 10, y: er.top + (er.bottom - er.top) * 6 / 10 };
    let inj = inject_drag(center(sr), drop_pt);
    let finished = pump_until(Duration::from_secs(20), || RESULT.load(SeqCst) != -1);
    inj.join().unwrap();
    let returned_at = T0.get().unwrap().elapsed().as_millis();
    let under = unsafe { WindowFromPoint(drop_pt) };
    log(&format!(
        "DoDragDrop finished={finished} at {returned_at} ms; folder of the window under the drop point: {}",
        explorer_folder_at(under)
    ));

    let hello = dir.join("hello.txt");
    let big = dir.join("big.bin");
    let complete = pump_until(Duration::from_secs(30), || {
        std::fs::metadata(&big).map(|m| m.len() == 8 * 1024 * 1024).unwrap_or(false) && hello.exists()
    });
    let ended_ok = if offer_async { pump_until(Duration::from_secs(10), || ended.load(SeqCst)) } else { false };
    let hello_ok = std::fs::read(&hello).map(|b| b == b"hello from the mouser drag probe\r\n").unwrap_or(false);
    let big_ok = std::fs::read(&big)
        .map(|b| b.len() == 8 * 1024 * 1024 && b.iter().enumerate().all(|(i, &v)| v == (i % 251) as u8))
        .unwrap_or(false);
    log(&format!(
        "files present={complete} hello.txt content ok={hello_ok} big.bin content ok={big_ok} EndOperation seen={ended_ok}"
    ));
    drop(keep);
    DRAG.with(|d| d.borrow_mut().take());
    unsafe {
        let _ = DestroyWindow(src_hwnd);
        let _ = PostMessageW(Some(exp), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    pump_for(Duration::from_millis(800));
    let _ = std::fs::remove_dir_all(&dir);
}

fn run_explorer() {
    setup();
    run_explorer_once(true);
    run_explorer_once(false);
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("own") => run_own(),
        Some("explorer") => run_explorer(),
        _ => eprintln!("usage: spike_drag own | explorer"),
    }
}

