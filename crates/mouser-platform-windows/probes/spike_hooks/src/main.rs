//! Hardware probe 1 (T366): LL hooks vs Raw Input for swallowed events, LL hooks
//! with an elevated foreground window, ClipCursor across a desktop switch.
//!
//! `spike_hooks hooks`     phases P1-P4 (injected and real input, pass-through and swallow)
//! `spike_hooks elevated`  ClipCursor across the UAC secure desktop, then an elevated
//!                         foreground window (UAC prompt)
//! `spike_hooks target <log> <secs>`  the elevated window (started by `elevated`)
//!
//! Injected keys are F15 (scan 0x66) so nothing types into another window.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering::SeqCst};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Security::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::StationsAndDesktops::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Input::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

// Every line also goes to %TEMP%spike_hooks.log so runs started outside this shell are recorded.
macro_rules! println {
    ($($t:tt)*) => {{
        let s = format!($($t)*);
        std::println!("{s}");
        log_line(&s);
    }};
}

fn log_line(s: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("spike_hooks.log");
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
    writeln!(f, "{s}").unwrap();
}

static SWALLOW: AtomicBool = AtomicBool::new(false);
static HOOK_ENTRIES: AtomicU32 = AtomicU32::new(0);
static HOOK_TAGGED: AtomicU32 = AtomicU32::new(0);
/// dwExtraInfo marker on every injected event ("MOUS").
const TAG: usize = 0x4D4F_5553;
static TRACK_PID: AtomicU32 = AtomicU32::new(0);

#[derive(Default)]
struct Counters {
    hook_mouse_injected: AtomicU32,
    hook_mouse_real: AtomicU32,
    hook_key_injected: AtomicU32,
    hook_key_real: AtomicU32,
    hook_while_tracked_fg: AtomicU32,
    raw_mouse_null_dev: AtomicU32,
    raw_mouse_dev: AtomicU32,
    raw_key_null_dev: AtomicU32,
    raw_key_dev: AtomicU32,
}

static C: Counters = Counters {
    hook_mouse_injected: AtomicU32::new(0),
    hook_mouse_real: AtomicU32::new(0),
    hook_key_injected: AtomicU32::new(0),
    hook_key_real: AtomicU32::new(0),
    hook_while_tracked_fg: AtomicU32::new(0),
    raw_mouse_null_dev: AtomicU32::new(0),
    raw_mouse_dev: AtomicU32::new(0),
    raw_key_null_dev: AtomicU32::new(0),
    raw_key_dev: AtomicU32::new(0),
};

static STATUS_HWND: AtomicIsize = AtomicIsize::new(0);

fn reset() {
    HOOK_TAGGED.store(0, SeqCst);
    for a in [
        &C.hook_mouse_injected,
        &C.hook_mouse_real,
        &C.hook_key_injected,
        &C.hook_key_real,
        &C.hook_while_tracked_fg,
        &C.raw_mouse_null_dev,
        &C.raw_mouse_dev,
        &C.raw_key_null_dev,
        &C.raw_key_dev,
    ] {
        a.store(0, SeqCst);
    }
}

fn report(phase: &str) {
    println!(
        "{phase}: hook events with MOUS dwExtraInfo tag={} | hook mouse injected={} real={} | hook key injected={} real={} | hook calls while tracked pid foreground={} | raw mouse hDevice=NULL {} hDevice!=NULL {} | raw key hDevice=NULL {} hDevice!=NULL {}",
        HOOK_TAGGED.load(SeqCst),
        C.hook_mouse_injected.load(SeqCst),
        C.hook_mouse_real.load(SeqCst),
        C.hook_key_injected.load(SeqCst),
        C.hook_key_real.load(SeqCst),
        C.hook_while_tracked_fg.load(SeqCst),
        C.raw_mouse_null_dev.load(SeqCst),
        C.raw_mouse_dev.load(SeqCst),
        C.raw_key_null_dev.load(SeqCst),
        C.raw_key_dev.load(SeqCst),
    );
}

fn fg_is_tracked() -> bool {
    let pid = TRACK_PID.load(SeqCst);
    if pid == 0 {
        return false;
    }
    let mut fg_pid = 0u32;
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut fg_pid)) };
    fg_pid == pid
}

unsafe extern "system" fn mouse_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    HOOK_ENTRIES.fetch_add(1, SeqCst);
    let slow = SLOW_MS.load(SeqCst);
    if slow > 0 {
        std::thread::sleep(Duration::from_millis(slow as u64));
    }
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
        if info.flags & LLMHF_INJECTED != 0 {
            C.hook_mouse_injected.fetch_add(1, SeqCst);
        } else {
            C.hook_mouse_real.fetch_add(1, SeqCst);
        }
        if info.dwExtraInfo == TAG {
            HOOK_TAGGED.fetch_add(1, SeqCst);
        }
        if fg_is_tracked() {
            C.hook_while_tracked_fg.fetch_add(1, SeqCst);
        }
        if SWALLOW.load(SeqCst) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

unsafe extern "system" fn key_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    HOOK_ENTRIES.fetch_add(1, SeqCst);
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        if info.flags.0 & LLKHF_INJECTED.0 != 0 {
            C.hook_key_injected.fetch_add(1, SeqCst);
        } else {
            C.hook_key_real.fetch_add(1, SeqCst);
        }
        if info.dwExtraInfo == TAG {
            HOOK_TAGGED.fetch_add(1, SeqCst);
        }
        if fg_is_tracked() {
            C.hook_while_tracked_fg.fetch_add(1, SeqCst);
        }
        if SWALLOW.load(SeqCst) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

unsafe extern "system" fn raw_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_INPUT {
        let mut raw = RAWINPUT::default();
        let mut size = size_of::<RAWINPUT>() as u32;
        let got = unsafe {
            GetRawInputData(
                HRAWINPUT(lp.0 as _),
                RID_INPUT,
                Some(&mut raw as *mut _ as *mut _),
                &mut size,
                size_of::<RAWINPUTHEADER>() as u32,
            )
        };
        if got != u32::MAX {
            let null_dev = raw.header.hDevice.is_invalid();
            match RID_DEVICE_INFO_TYPE(raw.header.dwType) {
                RIM_TYPEMOUSE if null_dev => C.raw_mouse_null_dev.fetch_add(1, SeqCst),
                RIM_TYPEMOUSE => C.raw_mouse_dev.fetch_add(1, SeqCst),
                RIM_TYPEKEYBOARD if null_dev => C.raw_key_null_dev.fetch_add(1, SeqCst),
                RIM_TYPEKEYBOARD => C.raw_key_dev.fetch_add(1, SeqCst),
                _ => 0,
            };
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

unsafe extern "system" fn status_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_PAINT {
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = RECT::default();
            GetClientRect(hwnd, &mut rc).unwrap();
            FillRect(hdc, &rc, HBRUSH(GetStockObject(BLACK_BRUSH).0));
            SetTextColor(hdc, COLORREF(0x0000_FFFF));
            SetBkMode(hdc, TRANSPARENT);
            let mut buf = [0u16; 512];
            let n = GetWindowTextW(hwnd, &mut buf) as usize;
            rc.left += 12;
            rc.right -= 12;
            DrawTextW(hdc, &mut buf[..n], &mut rc, DT_CENTER | DT_VCENTER | DT_WORDBREAK);
            let _ = EndPaint(hwnd, &ps);
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn status(text: &str) {
    println!("[status] {text}");
    let hwnd = HWND(STATUS_HWND.load(SeqCst) as _);
    if hwnd.is_invalid() {
        return;
    }
    let t = wide(text);
    unsafe {
        SetWindowTextW(hwnd, PCWSTR(t.as_ptr())).unwrap();
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
    pump_for(Duration::from_millis(30));
}

fn create_windows() -> HWND {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let raw_class = WNDCLASSW {
            lpfnWndProc: Some(raw_wndproc),
            hInstance: hinst,
            lpszClassName: w!("spike_hooks_raw"),
            ..Default::default()
        };
        RegisterClassW(&raw_class);
        let raw_hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("spike_hooks_raw"),
            w!("raw"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        )
        .unwrap();
        let devices = [
            RAWINPUTDEVICE {
                usUsagePage: 0x01,
                usUsage: 0x02,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: raw_hwnd,
            },
            RAWINPUTDEVICE {
                usUsagePage: 0x01,
                usUsage: 0x06,
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: raw_hwnd,
            },
        ];
        RegisterRawInputDevices(&devices, size_of::<RAWINPUTDEVICE>() as u32).unwrap();

        let status_class = WNDCLASSW {
            lpfnWndProc: Some(status_wndproc),
            hInstance: hinst,
            lpszClassName: w!("spike_hooks_status"),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            ..Default::default()
        };
        RegisterClassW(&status_class);
        let sw = GetSystemMetrics(SM_CXSCREEN);
        let status_hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("spike_hooks_status"),
            w!("Mouser probe starting"),
            WS_POPUP | WS_BORDER,
            sw / 2 - 450,
            20,
            900,
            110,
            None,
            None,
            Some(hinst),
            None,
        )
        .unwrap();
        let _ = ShowWindow(status_hwnd, SW_SHOWNOACTIVATE);
        STATUS_HWND.store(status_hwnd.0 as isize, SeqCst);
        raw_hwnd
    }
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
            let left = (end - now).as_millis() as u32;
            MsgWaitForMultipleObjects(None, false, left.min(50), QS_ALLINPUT);
        }
    }
}

fn install_hooks() -> (HHOOK, HHOOK) {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let m = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(hinst), 0).unwrap();
        let k = SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_proc), Some(hinst), 0).unwrap();
        (m, k)
    }
}

fn mouse_rel(dx: i32, dy: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                dwFlags: MOUSEEVENTF_MOVE,
                dwExtraInfo: TAG,
                ..Default::default()
            },
        },
    }
}

fn key_f15(up: bool) -> INPUT {
    let mut flags = KEYEVENTF_SCANCODE;
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wScan: 0x66,
                dwFlags: flags,
                dwExtraInfo: TAG,
                ..Default::default()
            },
        },
    }
}

/// Injects 20 relative moves and 5 F15 presses on the calling thread; returns
/// (inputs accepted by SendInput, GetLastError after the last failing call).
fn inject_batch_body() -> (u32, u32) {
    let mut accepted = 0;
    let mut err = 0;
    for i in 0..20 {
        let d = if i % 2 == 0 { 1 } else { -1 };
        let n = unsafe { SendInput(&[mouse_rel(d, 0)], size_of::<INPUT>() as i32) };
        accepted += n;
        if n == 0 {
            err = unsafe { GetLastError().0 };
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    for _ in 0..5 {
        let n = unsafe { SendInput(&[key_f15(false), key_f15(true)], size_of::<INPUT>() as i32) };
        accepted += n;
        if n == 0 {
            err = unsafe { GetLastError().0 };
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    (accepted, err)
}

/// The batch injected from another thread of this process while this thread pumps.
fn inject_batch() -> (u32, u32) {
    while_pumping(inject_batch_body)
}

fn cursor() -> POINT {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p).unwrap() };
    p
}

fn countdown(text: &str, secs: u64) {
    for s in (1..=secs).rev() {
        status(&format!("{text}\nstarts in {s} s"));
        pump_for(Duration::from_secs(1));
    }
}

fn run_hooks() {
    create_windows();
    let (mh, kh) = install_hooks();
    print_env();

    status("P0: pass-through hooks, input injected by this same process (do not touch mouse/keyboard)");
    pump_for(Duration::from_millis(500));
    reset();
    let (acc, err) = inject_batch();
    pump_for(Duration::from_millis(800));
    println!("P0 SendInput accepted={acc} lastError={err}");
    report("P0 pass-through injected by hook process");

    status("P1: pass-through hooks, input injected by a child process (do not touch mouse/keyboard)");
    pump_for(Duration::from_millis(500));
    reset();
    let (acc, err) = inject_child();
    pump_for(Duration::from_millis(800));
    println!("P1 SendInput accepted={acc} lastError={err}");
    report("P1 pass-through injected");

    status("P2: swallowing hooks, injected input (do not touch mouse/keyboard)");
    pump_for(Duration::from_millis(500));
    reset();
    SWALLOW.store(true, SeqCst);
    let before = cursor();
    let (acc, err) = inject_child();
    pump_for(Duration::from_millis(800));
    SWALLOW.store(false, SeqCst);
    let after = cursor();
    println!("P2 SendInput accepted={acc} lastError={err} cursor before=({},{}) after=({},{})", before.x, before.y, after.x, after.y);
    report("P2 swallow injected");

    countdown("P3: MOVE THE MOUSE and PRESS KEYS (e.g. Shift) for 6 s - input passes through", 3);
    reset();
    status("P3: MOVE THE MOUSE and PRESS Shift now (6 s, pass-through)");
    pump_for(Duration::from_secs(6));
    report("P3 pass-through real");

    countdown("P4: keep MOVING THE MOUSE and PRESSING Shift for 6 s - input will be FROZEN (swallowed) and comes back by itself", 4);
    reset();
    SWALLOW.store(true, SeqCst);
    let before = cursor();
    let t0 = Instant::now();
    status("P4: SWALLOWING - keep moving the mouse and pressing Shift (6 s)");
    pump_for(Duration::from_secs(6));
    SWALLOW.store(false, SeqCst);
    let after = cursor();
    println!("P4 swallowed for {} ms cursor before=({},{}) after=({},{})", t0.elapsed().as_millis(), before.x, before.y, after.x, after.y);
    report("P4 swallow real");

    status("Probe 1 hooks phases finished - input is back to normal");
    pump_for(Duration::from_secs(2));
    unsafe {
        UnhookWindowsHookEx(mh).unwrap();
        UnhookWindowsHookEx(kh).unwrap();
    }
}

fn integrity_level() -> u32 {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token).unwrap();
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut len);
        let mut buf = vec![0u8; len as usize];
        GetTokenInformation(token, TokenIntegrityLevel, Some(buf.as_mut_ptr() as _), len, &mut len).unwrap();
        let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let sid = label.Label.Sid;
        let count = *GetSidSubAuthorityCount(sid) as u32;
        let rid = *GetSidSubAuthority(sid, count - 1);
        let _ = CloseHandle(token);
        rid
    }
}

fn input_desktop_name() -> String {
    unsafe {
        match OpenInputDesktop(DESKTOP_CONTROL_FLAGS(0), false, DESKTOP_ACCESS_FLAGS(DESKTOP_READOBJECTS.0)) {
            Ok(h) => {
                let mut buf = [0u16; 128];
                let mut need = 0u32;
                let name = if GetUserObjectInformationW(
                    HANDLE(h.0),
                    UOI_NAME,
                    Some(buf.as_mut_ptr() as _),
                    (buf.len() * 2) as u32,
                    Some(&mut need),
                )
                .is_ok()
                {
                    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                    String::from_utf16_lossy(&buf[..n])
                } else {
                    format!("<name error {}>", GetLastError().0)
                };
                let _ = CloseDesktop(h);
                name
            }
            Err(e) => format!("<OpenInputDesktop failed: {e}>"),
        }
    }
}

fn print_env() {
    let mut info = windows::Win32::System::SystemInformation::SYSTEM_INFO::default();
    unsafe { windows::Win32::System::SystemInformation::GetNativeSystemInfo(&mut info) };
    let arch = unsafe { info.Anonymous.Anonymous.wProcessorArchitecture.0 };
    let mut session = 0u32;
    unsafe {
        let _ = windows::Win32::System::RemoteDesktop::ProcessIdToSessionId(GetCurrentProcessId(), &mut session);
    }
    let thread_desk = unsafe { object_name(HANDLE(GetThreadDesktop(GetCurrentThreadId()).unwrap().0)) };
    let winsta = unsafe { object_name(HANDLE(GetProcessWindowStation().unwrap().0)) };
    println!(
        "env: native arch={} (12=ARM64, 9=x64) integrity RID=0x{:x} session={} winsta={} thread desktop={} input desktop={}",
        arch,
        integrity_level(),
        session,
        winsta,
        thread_desk,
        input_desktop_name()
    );
}

fn object_name(h: HANDLE) -> String {
    let mut buf = [0u16; 128];
    let mut need = 0u32;
    match unsafe {
        GetUserObjectInformationW(h, UOI_NAME, Some(buf.as_mut_ptr() as _), (buf.len() * 2) as u32, Some(&mut need))
    } {
        Ok(()) => {
            let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            String::from_utf16_lossy(&buf[..n])
        }
        Err(e) => format!("<{e}>"),
    }
}

fn shell_runas(args: &str) -> Option<HANDLE> {
    let exe = std::env::current_exe().unwrap();
    let file = wide(exe.to_str().unwrap());
    let params = wide(args);
    let mut sei = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    match unsafe { ShellExecuteExW(&mut sei) } {
        Ok(()) => Some(sei.hProcess),
        Err(e) => {
            println!("ShellExecuteExW runas failed: {e}");
            None
        }
    }
}

fn clip_rect() -> RECT {
    let mut r = RECT::default();
    unsafe { GetClipCursor(&mut r).unwrap() };
    r
}

fn fmt_rect(r: RECT) -> String {
    format!("({},{})-({},{})", r.left, r.top, r.right, r.bottom)
}

fn run_elevated() {
    create_windows();
    print_env();

    // ClipCursor across the UAC secure-desktop switch.
    let p = cursor();
    let confine = RECT { left: p.x - 200, top: p.y - 150, right: p.x + 200, bottom: p.y + 150 };
    unsafe { ClipCursor(Some(&confine)).unwrap() };
    println!("clip: set {} read back {}", fmt_rect(confine), fmt_rect(clip_rect()));
    status("CLIP + ELEVATED: a UAC prompt will appear - click YES (the cursor is confined to a box until then)");
    pump_for(Duration::from_secs(2));

    let log = std::env::temp_dir().join("spike_hooks_target.log");
    let _ = std::fs::remove_file(&log);
    let stop = std::sync::Arc::new(AtomicBool::new(false));
    let watcher = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            let mut last = String::new();
            let t0 = Instant::now();
            while !stop.load(SeqCst) {
                let now = format!("clip={} input desktop={}", fmt_rect(clip_rect()), input_desktop_name());
                if now != last {
                    println!("clip watch t={}ms {now}", t0.elapsed().as_millis());
                    last = now;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        })
    };
    let target_secs = 30;
    let child = shell_runas(&format!("target \"{}\" {target_secs}", log.display()));
    // Pump until the elevated window exists (UAC answered) or 60 s.
    let t0 = Instant::now();
    let mut target = HWND::default();
    while t0.elapsed() < Duration::from_secs(60) {
        pump_for(Duration::from_millis(100));
        if let Ok(h) = unsafe { FindWindowW(w!("spike_hooks_target"), None) } {
            target = h;
            break;
        }
    }
    pump_for(Duration::from_millis(500));
    stop.store(true, SeqCst);
    watcher.join().unwrap();
    println!("clip: after UAC read back {}", fmt_rect(clip_rect()));
    unsafe { ClipCursor(None).unwrap() };

    if child.is_none() || target.is_invalid() {
        println!("elevated: target window not found (UAC declined?)");
        status("UAC declined - elevated part skipped");
        pump_for(Duration::from_secs(2));
        return;
    }
    let mut tpid = 0u32;
    unsafe { GetWindowThreadProcessId(target, Some(&mut tpid)) };
    TRACK_PID.store(tpid, SeqCst);
    let (mh, kh) = install_hooks();
    pump_for(Duration::from_millis(300));
    println!("elevated: target pid={tpid} foreground is target={}", fg_is_tracked());

    reset();
    status("E1: injecting into the elevated window (do not touch mouse/keyboard)");
    let (acc, err) = inject_child();
    pump_for(Duration::from_millis(800));
    println!("E1 SendInput accepted={acc} lastError={err} foreground is target={}", fg_is_tracked());
    report("E1 injected, elevated foreground");

    countdown("E2: click the ELEVATED window (title 'spike_hooks ELEVATED target'), then MOVE THE MOUSE over it and PRESS Shift for 6 s", 4);
    reset();
    status("E2: move the mouse over the elevated window and press Shift (6 s)");
    pump_for(Duration::from_secs(6));
    println!("E2 foreground is target={}", fg_is_tracked());
    report("E2 real input, elevated foreground");

    status("Elevated part finished - waiting for the elevated window to close");
    unsafe {
        UnhookWindowsHookEx(mh).unwrap();
        UnhookWindowsHookEx(kh).unwrap();
        PostMessageW(Some(target), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
        WaitForSingleObject(child.unwrap(), 15_000);
    }
    match std::fs::read_to_string(&log) {
        Ok(s) => print!("{s}"),
        Err(e) => println!("target log unreadable: {e}"),
    }
}

static T_MOVES: AtomicU32 = AtomicU32::new(0);
static T_KEYS: AtomicU32 = AtomicU32::new(0);
static T_F15: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn target_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEMOVE => {
            T_MOVES.fetch_add(1, SeqCst);
        }
        WM_KEYDOWN => {
            T_KEYS.fetch_add(1, SeqCst);
            if wp.0 == VK_F15.0 as usize {
                T_F15.fetch_add(1, SeqCst);
            }
        }
        WM_CLOSE => {
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn run_target(log: &str, secs: u64) {
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let class = WNDCLASSW {
            lpfnWndProc: Some(target_wndproc),
            hInstance: hinst,
            lpszClassName: w!("spike_hooks_target"),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hbrBackground: HBRUSH(GetStockObject(WHITE_BRUSH).0),
            ..Default::default()
        };
        RegisterClassW(&class);
        let sw = GetSystemMetrics(SM_CXSCREEN);
        let sh = GetSystemMetrics(SM_CYSCREEN);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("spike_hooks_target"),
            w!("spike_hooks ELEVATED target"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            sw / 2 - 400,
            sh / 2 - 250,
            800,
            500,
            None,
            None,
            Some(hinst),
            None,
        )
        .unwrap();
        let fg = SetForegroundWindow(hwnd).as_bool();
        let start = Instant::now();
        let mut msg = MSG::default();
        let timer = SetTimer(Some(hwnd), 1, (secs * 1000) as u32, None);
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_TIMER && msg.wParam.0 == timer {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let out = format!(
            "target: integrity RID=0x{:x} SetForegroundWindow={} ran {} ms WM_MOUSEMOVE={} WM_KEYDOWN={} (F15={})\n",
            integrity_level(),
            fg,
            start.elapsed().as_millis(),
            T_MOVES.load(SeqCst),
            T_KEYS.load(SeqCst),
            T_F15.load(SeqCst)
        );
        std::fs::write(log, out).unwrap();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("env") => print_env(),
        Some("hooktest") => run_hooktest(),
        Some("listen") => run_listen(),
        Some("stall") => run_stall(false),
        Some("setcursor") => run_setcursor(),
        Some("edge") => run_edge(),
        Some("slow") => run_slow(),
        Some("stall-self") => run_stall(true),
        Some("inject") => std::println!("inject accepted/err={:?}", inject_batch_body()),
        Some("hooks") => run_hooks(),
        Some("elevated") => run_elevated(),
        Some("target") => run_target(&args[2], args[3].parse().unwrap()),
        _ => eprintln!("usage: spike_hooks hooks | elevated"),
    }
}

fn run_hooktest() {
    create_windows();
    print_env();
    let (mh, kh) = install_hooks();
    pump_for(Duration::from_millis(300));
    reset();
    let (acc, err) = inject_batch();
    pump_for(Duration::from_millis(800));
    println!("hooktest SendInput accepted={acc} lastError={err}");
    report("hooktest pass-through injected");
    println!("hooktest hook proc entries (any code)={} hooks={:?} {:?}", HOOK_ENTRIES.load(SeqCst), mh, kh);
    unsafe {
        UnhookWindowsHookEx(mh).unwrap();
        UnhookWindowsHookEx(kh).unwrap();
    }
}

fn run_listen() {
    create_windows();
    let (mh, kh) = install_hooks();
    reset();
    pump_for(Duration::from_secs(4));
    report("listen 4 s");
    println!("listen hook proc entries (any code)={}", HOOK_ENTRIES.load(SeqCst));
    unsafe {
        UnhookWindowsHookEx(mh).unwrap();
        UnhookWindowsHookEx(kh).unwrap();
    }
}

/// Same batch as `inject_batch`, injected by a separate `spike_hooks inject` process
/// (LL hooks are not called for input injected by the hook-owning process).
fn inject_child() -> (u32, u32) {
    let out = while_pumping(|| std::process::Command::new(std::env::current_exe().unwrap()).arg("inject").output().unwrap());
    let s = String::from_utf8(out.stdout).unwrap();
    let inner = s.trim().trim_start_matches("inject accepted/err=(").trim_end_matches(')');
    let (a, e) = inner.split_once(", ").unwrap();
    (a.parse().unwrap(), e.parse().unwrap())
}

/// Runs `f` on another thread while this (hook) thread keeps pumping messages:
/// a hook thread that stops pumping gets its LL hooks timed out and silently removed.
fn while_pumping<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let h = std::thread::spawn(f);
    while !h.is_finished() {
        pump_for(Duration::from_millis(10));
    }
    h.join().unwrap()
}

/// LL hook survival when the hook thread stops pumping for `block` ms while input is
/// injected (by a child process, or by another thread of this process when
/// `self_inject`): afterwards a child injection (with pumping) shows whether the hook
/// is still installed.
fn run_stall(self_inject: bool) {
    create_windows();
    for block in [100u64, 250, 500, 1000, 2000, 5000] {
        let (mh, kh) = install_hooks();
        pump_for(Duration::from_millis(200));
        reset();
        let t0 = Instant::now();
        let injector = if self_inject {
            std::thread::spawn(|| {
                let t = Instant::now();
                inject_batch_body();
                t.elapsed().as_millis()
            })
        } else {
            let exe = std::env::current_exe().unwrap();
            std::thread::spawn(move || {
                let t = Instant::now();
                std::process::Command::new(exe).arg("inject").output().unwrap();
                t.elapsed().as_millis()
            })
        };
        std::thread::sleep(Duration::from_millis(block));
        pump_for(Duration::from_millis(600));
        let inject_ms = while_pumping(move || injector.join().unwrap());
        pump_for(Duration::from_millis(300));
        let after_stall = C.hook_mouse_injected.load(SeqCst) + C.hook_key_injected.load(SeqCst);
        reset();
        let (acc, _) = inject_child();
        pump_for(Duration::from_millis(300));
        let second = C.hook_mouse_injected.load(SeqCst) + C.hook_key_injected.load(SeqCst);
        println!(
            "stall {block} ms ({} injector, injector took {inject_ms} ms, total {} ms): stalled batch delivered {after_stall} of 30; next child injection (pumping) delivered {second} of {acc}",
            if self_inject { "same-process thread" } else { "child process" },
            t0.elapsed().as_millis()
        );
        unsafe {
            let _ = UnhookWindowsHookEx(mh);
            let _ = UnhookWindowsHookEx(kh);
        }
    }
}

static SLOW_MS: AtomicU32 = AtomicU32::new(0);

/// LL hook removal when the callback itself is slow: the mouse callback sleeps
/// `slow` ms per event for one child-injected batch, then (callback fast again) a
/// second child batch shows whether the hook is still installed.
fn run_slow() {
    create_windows();
    for slow in [100u32, 250, 400, 600, 1000, 1500] {
        let (mh, kh) = install_hooks();
        pump_for(Duration::from_millis(200));
        reset();
        SLOW_MS.store(slow, SeqCst);
        let t = Instant::now();
        let (acc1, _) = inject_child();
        let inject_ms = t.elapsed().as_millis();
        SLOW_MS.store(0, SeqCst);
        pump_for(Duration::from_millis(300));
        let first = C.hook_mouse_injected.load(SeqCst);
        reset();
        let (acc2, _) = inject_child();
        pump_for(Duration::from_millis(300));
        println!(
            "slow callback {slow} ms: batch took {inject_ms} ms, mouse hook calls {first} of 20 (accepted {acc1}); next batch with fast callback: mouse {} of 20, key {} of 10 (accepted {acc2})",
            C.hook_mouse_injected.load(SeqCst),
            C.hook_key_injected.load(SeqCst)
        );
        unsafe {
            let mr = UnhookWindowsHookEx(mh).is_ok();
            let kr = UnhookWindowsHookEx(kh).is_ok();
            println!("slow callback {slow} ms: UnhookWindowsHookEx mouse ok={mr} key ok={kr}");
        }
    }
}

/// Does `SetCursorPos` reach LL mouse hooks (and with which flags/extra info)?
fn run_setcursor() {
    create_windows();
    let (mh, kh) = install_hooks();
    pump_for(Duration::from_millis(200));
    reset();
    let start = cursor();
    let calls = while_pumping(move || {
        for i in 0..10 {
            unsafe { SetCursorPos(start.x + (i % 2) * 5, start.y).unwrap() };
            std::thread::sleep(Duration::from_millis(20));
        }
        unsafe { SetCursorPos(start.x, start.y).unwrap() };
    });
    let () = calls;
    pump_for(Duration::from_millis(300));
    report("setcursor 11 SetCursorPos calls");
    unsafe {
        UnhookWindowsHookEx(mh).unwrap();
        UnhookWindowsHookEx(kh).unwrap();
    }
}

static LAST_PT: std::sync::Mutex<Vec<(i32, i32, i32, i32)>> = std::sync::Mutex::new(Vec::new());

unsafe extern "system" fn edge_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && wp.0 as u32 == WM_MOUSEMOVE {
        let info = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
        let mut now = POINT::default();
        unsafe { GetCursorPos(&mut now).unwrap() };
        LAST_PT.lock().unwrap().push((info.pt.x, info.pt.y, now.x, now.y));
        if SWALLOW.load(SeqCst) {
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

/// At the right edge of the virtual screen, a relative +10 px move: is the hook's
/// `pt` clamped, and what does GetCursorPos say inside the hook? Pass-through,
/// then swallowed (the cursor parked at the edge).
fn run_edge() {
    create_windows();
    let hinst: HINSTANCE = unsafe { GetModuleHandleW(None).unwrap().into() };
    let h = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(edge_proc), Some(hinst), 0).unwrap() };
    let right = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN) } - 1;
    let start = cursor();
    for swallow in [false, true] {
        SWALLOW.store(swallow, SeqCst);
        unsafe { SetCursorPos(right - 2, start.y).unwrap() };
        pump_for(Duration::from_millis(100));
        LAST_PT.lock().unwrap().clear();
        while_pumping(|| {
            for _ in 0..3 {
                unsafe { SendInput(&[mouse_rel(10, 0)], size_of::<INPUT>() as i32) };
                std::thread::sleep(Duration::from_millis(30));
            }
        });
        pump_for(Duration::from_millis(200));
        println!(
            "edge (swallow={swallow}): screen right pixel={right}; hook (pt.x, pt.y, cursor.x, cursor.y) per +10 move: {:?}; cursor after: {:?}",
            LAST_PT.lock().unwrap(),
            (cursor().x, cursor().y)
        );
    }
    SWALLOW.store(false, SeqCst);
    unsafe {
        SetCursorPos(start.x, start.y).unwrap();
        UnhookWindowsHookEx(h).unwrap();
    }
}
