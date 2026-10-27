//! Probe P1 (T011): UIPI, foreground queries, WinEvent hook, cursor guard,
//! probe-swallowing hook, VM-style pointer capture.
//!
//! Nothing here elevates anything. Steps that need an elevated window in front
//! are done by the owner while `observe` logs.
//!
//! `spike_uipi observe <secs> <label> [noinject]`
//!     items 1-4: LL hooks (pass-through), an out-of-context EVENT_SYSTEM_FOREGROUND hook
//!     on the hook thread, a 1 s sampler (foreground pid / exe / elevation / integrity vs
//!     own, hook counts), and every 2 s a UIPI test (SetCursorPos, SendInput: return value,
//!     GetLastError, whether the hook saw it, whether the cursor moved).
//! `spike_uipi proc <pid>...`   item 3 helper: exe name, elevation and integrity of arbitrary pids.
//! `spike_uipi cursor-test`     item 5: victim hides cursors, `--cursor-guard <pid>` child,
//!     TerminateProcess(victim), check the arrow is restored.
//! `spike_uipi swallow <secs>`  item 6: probe hook A, then a probe-swallowing hook B.
//! `spike_uipi capture <secs> <label> [inject]`  item 7: ClipCursor + RIDEV_CAPTUREMOUSE,
//!     released by Ctrl+Alt (or after <secs>), LL hooks counting meanwhile.
//!
//! Every line goes to stdout and to %TEMP%\spike_uipi_<label>.log (timestamps are
//! seconds since start).

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::SeqCst};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Security::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Input::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PWSTR, w};

/// dwExtraInfo on probes ("MOUS").
const TAG: usize = 0x4D4F_5553;
/// dwExtraInfo on "foreign" injected input ("FORE").
const FOREIGN: usize = 0x464F_5245;

static START: OnceLock<Instant> = OnceLock::new();
static LOG: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

fn log_line(s: &str) {
    use std::io::Write;
    let t = START.get_or_init(Instant::now).elapsed().as_secs_f64();
    let line = format!("[+{t:8.3}s pid {}] {s}", std::process::id());
    std::println!("{line}");
    if let Some(path) = LOG.lock().unwrap().clone() {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
}
macro_rules! lg {
    ($($t:tt)*) => { log_line(&format!($($t)*)) };
}

fn set_log(label: &str) {
    let path = std::env::temp_dir().join(format!("spike_uipi_{label}.log"));
    lg!("log file: {}", path.display());
    *LOG.lock().unwrap() = Some(path);
}

// ---- counters seen by hook A (the probe's own hook) ----
static MOUSE_REAL: AtomicU32 = AtomicU32::new(0);
static MOUSE_FOREIGN: AtomicU32 = AtomicU32::new(0);
static MOUSE_TAGGED: AtomicU32 = AtomicU32::new(0);
static KEY_REAL: AtomicU32 = AtomicU32::new(0);
static KEY_FOREIGN: AtomicU32 = AtomicU32::new(0);
static KEY_TAGGED: AtomicU32 = AtomicU32::new(0);
static RAW_MOUSE: AtomicU32 = AtomicU32::new(0);
static FG_EVENTS: AtomicU32 = AtomicU32::new(0);
/// Hook B (swallower) is active and eats tagged events.
static B_SWALLOWING: AtomicBool = AtomicBool::new(false);
static B_EATEN: AtomicU32 = AtomicU32::new(0);
static HOOK_TID: AtomicU64 = AtomicU64::new(0);

fn snapshot() -> [u32; 6] {
    [
        MOUSE_REAL.load(SeqCst),
        MOUSE_FOREIGN.load(SeqCst),
        MOUSE_TAGGED.load(SeqCst),
        KEY_REAL.load(SeqCst),
        KEY_FOREIGN.load(SeqCst),
        KEY_TAGGED.load(SeqCst),
    ]
}

fn fmt_counts(c: [u32; 6]) -> String {
    format!(
        "hookA mouse real={} foreign-injected={} tagged={} | key real={} foreign-injected={} tagged={}",
        c[0], c[1], c[2], c[3], c[4], c[5]
    )
}

unsafe extern "system" fn mouse_a(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let i = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
        if i.dwExtraInfo == TAG {
            MOUSE_TAGGED.fetch_add(1, SeqCst);
        } else if i.flags & LLMHF_INJECTED != 0 {
            MOUSE_FOREIGN.fetch_add(1, SeqCst);
        } else {
            MOUSE_REAL.fetch_add(1, SeqCst);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

unsafe extern "system" fn key_a(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let i = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        if i.dwExtraInfo == TAG {
            KEY_TAGGED.fetch_add(1, SeqCst);
        } else if i.flags.0 & LLKHF_INJECTED.0 != 0 {
            KEY_FOREIGN.fetch_add(1, SeqCst);
        } else {
            KEY_REAL.fetch_add(1, SeqCst);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

// Hook B: installed after A, so it runs first; eats tagged probes only.
unsafe extern "system" fn mouse_b(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && B_SWALLOWING.load(SeqCst) {
        let i = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
        if i.dwExtraInfo == TAG {
            B_EATEN.fetch_add(1, SeqCst);
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

unsafe extern "system" fn key_b(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && B_SWALLOWING.load(SeqCst) {
        let i = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        if i.dwExtraInfo == TAG {
            B_EATEN.fetch_add(1, SeqCst);
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

unsafe extern "system" fn win_event(
    _h: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread: u32,
    _time: u32,
) {
    FG_EVENTS.fetch_add(1, SeqCst);
    lg!("WINEVENT EVENT_SYSTEM_FOREGROUND fired: {}", describe(hwnd));
}

/// Starts the hook thread: hook A (and hook B when `swallower`), plus the
/// WinEvent hook when `winevent`. Returns the thread id.
fn start_hook_thread(swallower: bool, winevent: bool) -> (u32, std::thread::JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let ma = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_a), Some(hinst), 0);
        let ka = SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_a), Some(hinst), 0);
        lg!("hook A installed: mouse={:?} key={:?}", ma.is_ok(), ka.is_ok());
        let mut b = None;
        if swallower {
            let mb = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_b), Some(hinst), 0);
            let kb = SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_b), Some(hinst), 0);
            lg!("hook B (swallower, installed after A) installed: mouse={:?} key={:?}", mb.is_ok(), kb.is_ok());
            b = Some((mb, kb));
        }
        let we = if winevent {
            let h = SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(win_event),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
            lg!("WinEvent hook (EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT) installed: {}", !h.is_invalid());
            Some(h)
        } else {
            None
        };
        tx.send(GetCurrentThreadId()).unwrap();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if let Some(h) = we {
            let _ = UnhookWinEvent(h);
        }
        if let Some((mb, kb)) = b {
            for h in [mb, kb].into_iter().flatten() {
                let _ = UnhookWindowsHookEx(h);
            }
        }
        for h in [ma, ka].into_iter().flatten() {
            let _ = UnhookWindowsHookEx(h);
        }
    });
    let tid = rx.recv().unwrap();
    HOOK_TID.store(tid as u64, SeqCst);
    (tid, handle)
}

fn stop_hook_thread(tid: u32, handle: std::thread::JoinHandle<()>) {
    unsafe { PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0)).ok() };
    handle.join().unwrap();
}

// ---- process / token queries ----

fn level_name(il: u32) -> &'static str {
    match il {
        0..0x1000 => "untrusted",
        0x1000..0x2000 => "low",
        0x2000..0x3000 => "medium",
        0x3000..0x4000 => "high",
        0x4000..0x5000 => "system",
        _ => "protected",
    }
}

struct ProcInfo {
    exe: String,
    elevated: String,
    il: Option<u32>,
    il_text: String,
}

fn query_process(pid: u32) -> ProcInfo {
    unsafe {
        let process = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(p) => p,
            Err(e) => {
                let m = format!("OpenProcess(QUERY_LIMITED_INFORMATION) failed: {e}");
                return ProcInfo { exe: m.clone(), elevated: m.clone(), il: None, il_text: m };
            }
        };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let exe = match QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) {
            Ok(()) => {
                let full = String::from_utf16_lossy(&buf[..len as usize]);
                full.rsplit('\\').next().unwrap_or(&full).to_string() + &format!(" ({full})")
            }
            Err(e) => format!("QueryFullProcessImageNameW failed: {e}"),
        };
        let mut token = HANDLE::default();
        let (elevated, il, il_text);
        match OpenProcessToken(process, TOKEN_QUERY, &mut token) {
            Err(e) => {
                let m = format!("OpenProcessToken(TOKEN_QUERY) failed: {e}");
                elevated = m.clone();
                il = None;
                il_text = m;
            }
            Ok(()) => {
                let mut el = TOKEN_ELEVATION::default();
                let mut ret = 0;
                elevated = match GetTokenInformation(
                    token,
                    TokenElevation,
                    Some((&raw mut el).cast()),
                    size_of::<TOKEN_ELEVATION>() as u32,
                    &mut ret,
                ) {
                    Ok(()) => format!("TokenIsElevated={}", el.TokenIsElevated),
                    Err(e) => format!("TokenElevation failed: {e}"),
                };
                let mut b = [0u64; 64];
                let mut ret = 0;
                match GetTokenInformation(token, TokenIntegrityLevel, Some(b.as_mut_ptr().cast()), 512, &mut ret) {
                    Ok(()) => {
                        let sid = (*b.as_ptr().cast::<TOKEN_MANDATORY_LABEL>()).Label.Sid;
                        let n = *GetSidSubAuthorityCount(sid);
                        let v = *GetSidSubAuthority(sid, u32::from(n) - 1);
                        il = Some(v);
                        il_text = format!("IL=0x{v:X} ({})", level_name(v));
                    }
                    Err(e) => {
                        il = None;
                        il_text = format!("TokenIntegrityLevel failed: {e}");
                    }
                }
                let _ = CloseHandle(token);
            }
        }
        let _ = CloseHandle(process);
        ProcInfo { exe, elevated, il, il_text }
    }
}

fn own_il() -> u32 {
    query_process(std::process::id()).il.expect("own integrity level")
}

fn describe(hwnd: HWND) -> String {
    if hwnd.0.is_null() {
        return "no window (NULL)".into();
    }
    let mut pid = 0u32;
    let tid = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let mut title = [0u16; 128];
    let n = unsafe { GetWindowTextW(hwnd, &mut title) };
    let mut class = [0u16; 128];
    let cn = unsafe { GetClassNameW(hwnd, &mut class) };
    let p = query_process(pid);
    let own = own_il();
    let outranks = match p.il {
        Some(l) => format!("{}", l > own),
        None => "unreadable => treated as true".to_string(),
    };
    format!(
        "hwnd=0x{:X} class='{}' title='{}' tid={tid} pid={pid} exe={} | {} | {} | own IL=0x{own:X} ({}) | outranks_own={outranks}",
        hwnd.0 as usize,
        String::from_utf16_lossy(&class[..cn as usize]),
        String::from_utf16_lossy(&title[..n as usize]),
        p.exe,
        p.elevated,
        p.il_text,
        level_name(own)
    )
}

fn header(mode: &str) {
    let me = query_process(std::process::id());
    lg!("=== spike_uipi {mode} === own: {} | {} | {}", me.exe, me.elevated, me.il_text);
    let (mut bits, mut ver) = (String::new(), String::new());
    let _ = write!(bits, "{}", std::env::consts::ARCH);
    if let Ok(o) = std::process::Command::new("cmd").args(["/c", "ver"]).output() {
        ver = String::from_utf8_lossy(&o.stdout).trim().to_string();
    }
    lg!("arch={bits} os={ver}");
}

// ---- injection ----

fn mouse_rel(dx: i32, dy: i32, extra: usize) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dx, dy, dwFlags: MOUSEEVENTF_MOVE, dwExtraInfo: extra, ..Default::default() },
        },
    }
}

fn key_f15(up: bool, extra: usize) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wScan: 0x66,
                dwFlags: if up { KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP } else { KEYEVENTF_SCANCODE },
                dwExtraInfo: extra,
                ..Default::default()
            },
        },
    }
}

fn cursor() -> (i32, i32) {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p).ok() };
    (p.x, p.y)
}

/// One SendInput call: (returned count, GetLastError).
fn send(input: INPUT) -> (u32, u32) {
    unsafe {
        SetLastError(WIN32_ERROR(0));
        let n = SendInput(&[input], size_of::<INPUT>() as i32);
        (n, GetLastError().0)
    }
}

/// Item 2: SetCursorPos and SendInput return values and GetLastError, plus
/// whether the hook saw the probe and whether the cursor moved.
fn uipi_test() {
    let fg = unsafe { GetForegroundWindow() };
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(fg, Some(&mut pid)) };
    let il = query_process(pid).il;
    let own = own_il();
    let rel = match il {
        Some(l) if l > own => "FG OUTRANKS probe",
        Some(l) if l == own => "fg same IL",
        Some(_) => "fg lower IL",
        None => "fg IL unreadable",
    };
    let (x0, y0) = cursor();
    let before = snapshot();
    // SetCursorPos
    let (r_scp, e_scp) = unsafe {
        SetLastError(WIN32_ERROR(0));
        let r = SetCursorPos(x0 + 1, y0);
        (r.is_ok(), GetLastError().0)
    };
    let (x1, y1) = cursor();
    unsafe { SetCursorPos(x0, y0).ok() };
    // SendInput mouse (+1 then -1) and key F15 down/up, all tagged
    let m1 = send(mouse_rel(1, 0, TAG));
    let m2 = send(mouse_rel(-1, 0, TAG));
    let k1 = send(key_f15(false, TAG));
    let k2 = send(key_f15(true, TAG));
    std::thread::sleep(Duration::from_millis(150));
    let after = snapshot();
    let (x2, y2) = cursor();
    lg!(
        "UIPI-TEST [{rel}] SetCursorPos(x+1): ok={r_scp} GetLastError={e_scp} cursor ({x0},{y0})->({x1},{y1}) moved={} | SendInput mouse(+1)=({},err {}) mouse(-1)=({},err {}) key F15 down=({},err {}) up=({},err {}) [count,GetLastError] | hook saw tagged mouse +{} key +{} (sent 2 mouse, 2 key) | cursor after SendInput pair ({x2},{y2})",
        x1 != x0 || y1 != y0,
        m1.0, m1.1, m2.0, m2.1, k1.0, k1.1, k2.0, k2.1,
        after[2] - before[2],
        after[5] - before[5],
    );
}

// ---- mode: observe ----

fn observe(secs: u64, label: &str, inject: bool) {
    set_log(label);
    header(&format!("observe {secs}s label={label} inject={inject}"));
    lg!("START. Foreground at start: {}", describe(unsafe { GetForegroundWindow() }));
    let (tid, h) = start_hook_thread(false, true);
    let end = Instant::now() + Duration::from_secs(secs);
    let mut last_fg = 0usize;
    let mut last = snapshot();
    let mut tick = 0u64;
    while Instant::now() < end {
        std::thread::sleep(Duration::from_millis(1000));
        tick += 1;
        let fg = unsafe { GetForegroundWindow() };
        let now = snapshot();
        let delta: Vec<u32> = now.iter().zip(last.iter()).map(|(a, b)| a - b).collect();
        let fgnote = if fg.0 as usize != last_fg {
            last_fg = fg.0 as usize;
            format!("FOREGROUND CHANGED (poll): {}", describe(fg))
        } else {
            "fg same".to_string()
        };
        lg!(
            "sample: hookA delta/s mouse real={} foreign={} tagged={} | key real={} foreign={} tagged={} | winevents so far={} | {fgnote}",
            delta[0], delta[1], delta[2], delta[3], delta[4], delta[5], FG_EVENTS.load(SeqCst)
        );
        last = now;
        if inject && tick % 2 == 0 {
            uipi_test();
        }
    }
    lg!("END. totals: {} | WinEvent foreground events={}", fmt_counts(snapshot()), FG_EVENTS.load(SeqCst));
    stop_hook_thread(tid, h);
}

// ---- mode: swallow ----

fn swallow(secs: u64) {
    set_log("swallow");
    header(&format!("swallow {secs}s"));
    let (tid, h) = start_hook_thread(true, false);
    let probe_round = || {
        // tagged probes (mouse +1/-1, F15) and foreign injected (untagged-by-us, FORE) mouse
        let _ = send(mouse_rel(1, 0, TAG));
        let _ = send(mouse_rel(-1, 0, TAG));
        let _ = send(key_f15(false, TAG));
        let _ = send(key_f15(true, TAG));
        let _ = send(mouse_rel(1, 0, FOREIGN));
        let _ = send(mouse_rel(-1, 0, FOREIGN));
    };
    // Phase 1: control, B passes everything.
    B_SWALLOWING.store(false, SeqCst);
    let s0 = snapshot();
    for _ in 0..4 {
        probe_round();
        std::thread::sleep(Duration::from_millis(500));
    }
    let s1 = snapshot();
    lg!("PHASE 1 control (B passes): sent per kind: 4 rounds = 8 tagged mouse, 8 tagged key, 8 foreign mouse | hookA saw tagged mouse +{} tagged key +{} foreign mouse +{} | real mouse +{} real key +{}",
        s1[2] - s0[2], s1[5] - s0[5], s1[1] - s0[1], s1[0] - s0[0], s1[3] - s0[3]);
    // Phase 2: B eats tagged events.
    B_SWALLOWING.store(true, SeqCst);
    let rounds = secs * 2;
    for _ in 0..rounds {
        probe_round();
        std::thread::sleep(Duration::from_millis(500));
    }
    std::thread::sleep(Duration::from_millis(200));
    let s2 = snapshot();
    B_SWALLOWING.store(false, SeqCst);
    lg!("PHASE 2 swallow (B eats tagged): {rounds} rounds = {} tagged mouse, {} tagged key, {} foreign mouse sent | hookB ate {} | hookA saw tagged mouse +{} tagged key +{} foreign mouse +{} | real mouse +{} real key +{}",
        rounds * 2, rounds * 2, rounds * 2, B_EATEN.load(SeqCst),
        s2[2] - s1[2], s2[5] - s1[5], s2[1] - s1[1], s2[0] - s1[0], s2[3] - s1[3]);
    lg!("(real input only arrives if a person moves the mouse / types while this runs)");
    stop_hook_thread(tid, h);
}

// ---- mode: cursor-test / victim / --cursor-guard ----

fn arrow_is_blank() -> bool {
    unsafe {
        let shared = LoadImageW(None, windows::core::PCWSTR(OCR_NORMAL.0 as _), IMAGE_CURSOR, 0, 0, LR_SHARED | LR_DEFAULTSIZE);
        let mut info = ICONINFO::default();
        let known = shared.is_ok_and(|h| GetIconInfo(HICON(h.0), &mut info).is_ok());
        let mut blank = false;
        if known && info.hbmColor.is_invalid() {
            let mut bm = BITMAP::default();
            GetObjectW(HGDIOBJ(info.hbmMask.0), size_of::<BITMAP>() as i32, Some((&raw mut bm).cast()));
            let mut bits = vec![0u8; (bm.bmWidthBytes * bm.bmHeight) as usize];
            GetBitmapBits(info.hbmMask, bits.len() as i32, bits.as_mut_ptr().cast());
            let (and, xor) = bits.split_at(bits.len() / 2);
            blank = and.iter().all(|&b| b == 0xFF) && xor.iter().all(|&b| b == 0);
        }
        for b in [info.hbmMask, info.hbmColor] {
            if !b.is_invalid() {
                let _ = DeleteObject(HGDIOBJ(b.0));
            }
        }
        blank
    }
}

const SHAPES: [SYSTEM_CURSOR_ID; 13] = [
    OCR_NORMAL, OCR_IBEAM, OCR_WAIT, OCR_CROSS, OCR_UP, OCR_SIZENWSE, OCR_SIZENESW, OCR_SIZEWE,
    OCR_SIZENS, OCR_SIZEALL, OCR_NO, OCR_HAND, OCR_APPSTARTING,
];

fn hide_cursors() {
    unsafe {
        let and = [0xFFu8; 128];
        let xor = [0u8; 128];
        let blank = CreateCursor(None, 0, 0, 32, 32, and.as_ptr().cast(), xor.as_ptr().cast()).unwrap();
        for s in SHAPES {
            let copy = CopyIcon(HICON(blank.0)).unwrap();
            SetSystemCursor(HCURSOR(copy.0), s).unwrap();
        }
    }
}

fn restore_cursors() -> bool {
    unsafe { SystemParametersInfoW(SPI_SETCURSORS, 0, None, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_ok() }
}

fn cursor_guard(parent: u32) {
    set_log("cursor");
    lg!("guard: watching parent pid {parent}");
    unsafe {
        match OpenProcess(PROCESS_SYNCHRONIZE, false, parent) {
            Ok(p) => {
                WaitForSingleObject(p, INFINITE);
                lg!("guard: parent exited, calling SPI_SETCURSORS");
            }
            Err(e) => lg!("guard: OpenProcess(parent) failed ({e}); parent already gone? restoring anyway"),
        }
    }
    let ok = restore_cursors();
    lg!("guard: SPI_SETCURSORS ok={ok}; arrow blank now={}", arrow_is_blank());
}

fn victim() {
    set_log("cursor");
    hide_cursors();
    let exe = std::env::current_exe().unwrap();
    let guard = std::process::Command::new(exe)
        .args(["--cursor-guard", &std::process::id().to_string()])
        .spawn()
        .unwrap();
    lg!("victim: cursors hidden (arrow blank={}), guard child pid {} started; waiting to be killed", arrow_is_blank(), guard.id());
    std::thread::sleep(Duration::from_secs(120));
}

fn cursor_test() {
    set_log("cursor");
    header("cursor-test");
    lg!("baseline arrow blank={}", arrow_is_blank());
    let exe = std::env::current_exe().unwrap();
    let mut v = std::process::Command::new(exe).arg("victim").spawn().unwrap();
    let t0 = Instant::now();
    while !arrow_is_blank() && t0.elapsed() < Duration::from_secs(8) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let hidden = arrow_is_blank();
    lg!("after victim started: arrow blank={hidden} (after {:?})", t0.elapsed());
    std::thread::sleep(Duration::from_millis(1500)); // guard is running
    let kill = v.kill(); // std uses TerminateProcess
    lg!("TerminateProcess(victim pid {}) -> {kill:?}", v.id());
    let t1 = Instant::now();
    while arrow_is_blank() && t1.elapsed() < Duration::from_secs(8) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let restored = !arrow_is_blank();
    lg!("RESULT: hidden-before-kill={hidden} restored-by-guard={restored} within {:?}", t1.elapsed());
    if !restored {
        lg!("guard did not restore; restoring manually: ok={}", restore_cursors());
    }
    std::thread::sleep(Duration::from_millis(300));
}

// ---- mode: capture ----

static CAP_DEADLINE: Mutex<Option<Instant>> = Mutex::new(None);
static CAP_LAST: Mutex<[u32; 6]> = Mutex::new([0; 6]);
static CAP_RAW_LAST: AtomicU32 = AtomicU32::new(0);

fn release_capture(hwnd: HWND, why: &str) {
    unsafe {
        let _ = ClipCursor(None);
        let dev = [RAWINPUTDEVICE { usUsagePage: 1, usUsage: 2, dwFlags: RIDEV_REMOVE, hwndTarget: HWND::default() }];
        let _ = RegisterRawInputDevices(&dev, size_of::<RAWINPUTDEVICE>() as u32);
        let mut r = RECT::default();
        let _ = GetClipCursor(&mut r);
        lg!("CAPTURE RELEASED ({why}). clip rect now ({},{})-({},{})", r.left, r.top, r.right, r.bottom);
        let _ = DestroyWindow(hwnd);
    }
}

unsafe extern "system" fn cap_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_INPUT => {
            RAW_MOUSE.fetch_add(1, SeqCst);
        }
        WM_TIMER => {
            let ctrl = unsafe { GetAsyncKeyState(VK_CONTROL.0 as i32) } as u16 & 0x8000 != 0;
            let alt = unsafe { GetAsyncKeyState(VK_MENU.0 as i32) } as u16 & 0x8000 != 0;
            let expired = CAP_DEADLINE.lock().unwrap().is_some_and(|d| Instant::now() >= d);
            if ctrl && alt {
                release_capture(hwnd, "Ctrl+Alt pressed");
            } else if expired {
                release_capture(hwnd, "timeout");
            } else if wp.0 == 2 {
                // 1 s status tick
                let now = snapshot();
                let mut last = CAP_LAST.lock().unwrap();
                let d: Vec<u32> = now.iter().zip(last.iter()).map(|(a, b)| a - b).collect();
                *last = now;
                let raw = RAW_MOUSE.load(SeqCst);
                let rd = raw - CAP_RAW_LAST.swap(raw, SeqCst);
                let mut r = RECT::default();
                unsafe { GetClipCursor(&mut r).ok() };
                let (x, y) = cursor();
                lg!(
                    "capturing: hookA delta/s mouse real={} foreign={} tagged={} | key real={} foreign={} tagged={} | raw WM_INPUT mouse +{rd} | cursor ({x},{y}) clip ({},{})-({},{})",
                    d[0], d[1], d[2], d[3], d[4], d[5], r.left, r.top, r.right, r.bottom
                );
            }
        }
        WM_DESTROY => unsafe { PostQuitMessage(0) },
        _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
    LRESULT(0)
}

fn capture(secs: u64, label: &str, inject: bool) {
    set_log(label);
    header(&format!("capture {secs}s label={label} inject={inject}"));
    let (tid, h) = start_hook_thread(false, false);
    unsafe {
        let hinst: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let class = WNDCLASSW { lpfnWndProc: Some(cap_wndproc), hInstance: hinst, lpszClassName: w!("spike_uipi_cap"), ..Default::default() };
        RegisterClassW(&class);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(WS_EX_TOPMOST.0),
            w!("spike_uipi_cap"),
            w!("spike_uipi capture - press Ctrl+Alt to release"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            200, 200, 640, 400,
            None, None, Some(hinst), None,
        )
        .unwrap();
        let _ = SetForegroundWindow(hwnd);
        let mut rc = RECT::default();
        GetClientRect(hwnd, &mut rc).unwrap();
        let mut tl = POINT { x: rc.left, y: rc.top };
        let mut br = POINT { x: rc.right, y: rc.bottom };
        let _ = ClientToScreen(hwnd, &mut tl);
        let _ = ClientToScreen(hwnd, &mut br);
        let clip = RECT { left: tl.x, top: tl.y, right: br.x, bottom: br.y };
        let c = ClipCursor(Some(&clip));
        let dev = [RAWINPUTDEVICE { usUsagePage: 1, usUsage: 2, dwFlags: RIDEV_CAPTUREMOUSE | RIDEV_NOLEGACY, hwndTarget: hwnd }];
        let r = RegisterRawInputDevices(&dev, size_of::<RAWINPUTDEVICE>() as u32);
        lg!(
            "CAPTURE STARTED: ClipCursor ok={} to ({},{})-({},{}); RegisterRawInputDevices(RIDEV_CAPTUREMOUSE|RIDEV_NOLEGACY) ok={}; release with Ctrl+Alt, auto-release after {secs}s",
            c.is_ok(), clip.left, clip.top, clip.right, clip.bottom, r.is_ok()
        );
        *CAP_DEADLINE.lock().unwrap() = Some(Instant::now() + Duration::from_secs(secs));
        *CAP_LAST.lock().unwrap() = snapshot();
        SetTimer(Some(hwnd), 1, 50, None); // release poll
        SetTimer(Some(hwnd), 2, 1000, None); // status tick
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let inj = inject.then(|| {
            let stop = stop.clone();
            std::thread::spawn(move || {
                while !stop.load(SeqCst) {
                    let _ = send(mouse_rel(1, 0, TAG));
                    let _ = send(mouse_rel(-1, 0, TAG));
                    std::thread::sleep(Duration::from_millis(250));
                }
            })
        });
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        stop.store(true, SeqCst);
        if let Some(j) = inj {
            j.join().unwrap();
        }
        let _ = ClipCursor(None);
    }
    lg!("END. totals: {} | raw WM_INPUT mouse={}", fmt_counts(snapshot()), RAW_MOUSE.load(SeqCst));
    stop_hook_thread(tid, h);
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n = |i: usize, d: u64| a.get(i).and_then(|s| s.parse().ok()).unwrap_or(d);
    let s = |i: usize, d: &str| a.get(i).cloned().unwrap_or(d.to_string());
    match a.get(1).map(String::as_str) {
        Some("observe") => observe(n(2, 30), &s(3, "observe"), a.get(4).map(String::as_str) != Some("noinject")),
        Some("swallow") => swallow(n(2, 6)),
        Some("cursor-test") => cursor_test(),
        Some("victim") => victim(),
        Some("proc") => {
            header("proc");
            for p in &a[2..] {
                let pid: u32 = p.parse().unwrap();
                let q = query_process(pid);
                lg!("pid {pid}: exe={} | {} | {}", q.exe, q.elevated, q.il_text);
            }
        }
        Some("--cursor-guard") => cursor_guard(n(2, 0) as u32),
        Some("capture") => capture(n(2, 30), &s(3, "capture"), a.get(4).map(String::as_str) == Some("inject")),
        _ => std::println!("usage: spike_uipi observe <secs> <label> [noinject] | swallow <secs> | cursor-test | capture <secs> <label> [inject]"),
    }
}
